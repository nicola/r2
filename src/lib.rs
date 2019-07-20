use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::fs::FileExt;
use std::sync::mpsc;
use std::sync::{Arc, Condvar, Mutex, RwLock};
use std::time::{Duration, Instant};

use cached::Cached;
use chrono::Utc;
use crossbeam::channel;
use memmap::{MmapMut, MmapOptions};

use storage_proofs::hasher::Domain;
use tempfile;

use crate::graph::{Graph, Parents, ParentsIter, ParentsIterRev};

pub mod graph;
pub mod replicate;

/// Size of the data to encode
pub const DATA_SIZE: usize = 1 * 1024 * 1024; // * 1024;
/// Size of each node in the graph
pub const NODE_SIZE: usize = 32;
/// Number of layers in ZigZag
pub const LAYERS: usize = 10;
/// Number of nodes in each layer DRG graph
pub const NODES: usize = DATA_SIZE / NODE_SIZE;
/// In-degree of the DRG graph
pub const BASE_PARENTS: usize = 5;
/// Degree of the Expander graph
pub const EXP_PARENTS: usize = 8;
/// Number of parents for each node in the graph
pub const PARENT_SIZE: usize = BASE_PARENTS + EXP_PARENTS;

pub const SEED: [u32; 7] = [0, 1, 2, 3, 4, 5, 6];

#[macro_export]
macro_rules! next_base {
    ($parents:expr, $index:expr) => {
        // safe as we statically know this is fine. compiler, why don't you?
        *unsafe { $parents.base_parents().get_unchecked($index) }
    };
}

#[macro_export]
macro_rules! next_base_rev {
    ($parents:expr, $index:expr) => {
        // safe as we statically know this is fine. compiler, why don't you?
        NODES - *unsafe { $parents.base_parents().get_unchecked($index) } - 1
    };
}

#[macro_export]
macro_rules! next_exp {
    ($parents:expr, $index:expr) => {{
        // safe as we statically know this is fine. compiler, why don't you?
        *unsafe { $parents.exp_parents().get_unchecked($index - BASE_PARENTS) }
    }};
}

/// Generate a tmp file full of zeros
pub fn file_backed_mmap_from_zeroes(n: usize, use_tmp: bool) -> MmapMut {
    let file = if use_tmp {
        tempfile::tempfile().unwrap()
    } else {
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(format!("./zigzag-data-{:?}", Utc::now()))
            .unwrap()
    };

    file.set_len((32 * n * (LAYERS + 1)) as u64).unwrap();

    unsafe { MmapOptions::new().map_mut(&file).unwrap() }
}

/// Compute replica id from string
pub fn id_from_str<T: Domain>(raw: &str) -> T {
    let replica_id_raw = hex::decode(raw).expect("invalid hex for replica id seed");
    let mut replica_id_bytes = vec![0u8; 32];
    let len = ::std::cmp::min(32, replica_id_raw.len());
    replica_id_bytes[..len].copy_from_slice(&replica_id_raw[..len]);
    T::try_from_bytes(&replica_id_bytes).expect("invalid replica id")
}

pub fn create_empty_file(file_path: &'static str, size: usize) -> Result<(), failure::Error> {
    let file = fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .read(true)
        .open(file_path)?;
    file.set_len((size*(LAYERS + 1)) as u64)?;
    Ok(())
}

pub struct AsyncData {
    sender: channel::Sender<Request>,
    handle: std::thread::JoinHandle<()>,
    queue: channel::Receiver<(usize, usize, [u8; NODE_SIZE])>,
    blocked: Duration,
}

enum Request {
    Read(usize, usize, bool),
    Write(usize, usize, [u8; NODE_SIZE]),
    Sync,
}

const MAX_SIZE: usize = 1024 * 1024;

fn load(
    sender: &channel::Sender<(usize, usize, [u8; NODE_SIZE])>,
    file: &mut fs::File,
    cache2: &mut lru::LruCache<(usize, usize), [u8; NODE_SIZE]>,
    buf: &mut [u8; NODE_SIZE],
    n: usize,
    l: usize,
    stats: &mut Stats,
    seek_pos: &mut u64,
) {
    // println!("loading {}", n);

    let start = Instant::now();
    if let Some(d) = cache2.get(&(n,l)) {
        stats.cache_hits += 1;
        // println!("cache hit");
        sender.send((n, l, *d)).unwrap();
        stats.cache_reads += start.elapsed();
    } else {
        stats.cache_misses += 1;
        let offset = (n * NODE_SIZE + (l+1) * NODES * NODE_SIZE) as u64;
        // println!("seek to: {} - {}", n, offset);
        // let target = offset as i64 - *seek_pos as i64;
        // let start = Instant::now();
        // *seek_pos = file.seek(SeekFrom::Current(target)).unwrap();
        // stats.seeks += start.elapsed();

        let start = Instant::now();
        file.read_exact_at(&mut buf[..], offset).unwrap();
        stats.reads += start.elapsed();

        sender.send((n, l, *buf)).unwrap();

        cache2.put((n, l), *buf);

        // println!("> wrote node {}", n);
    };

    // signal the arrival of a new element
    // println!("> signal node {}", n);
}

#[derive(Debug, Default)]
struct Stats {
    cache_hits: usize,
    cache_misses: usize,
    reads: Duration,
    seeks: Duration,
    cache_reads: Duration,
}

impl AsyncData {
    pub fn new(file_path: &'static str, graph: Arc<Graph>) -> Result<Self, failure::Error> {
        let (sender_req, receiver_req) = channel::bounded(512);
        let (sender_res, receiver_res) = channel::bounded(1024);

        let handle = std::thread::spawn(move || {
            let mut cache = lru::LruCache::new(MAX_SIZE);

            let mut file = fs::OpenOptions::new()
                .write(true)
                .read(true)
                .open(file_path)
                .expect("invalid file");

            let mut buf = [0u8; NODE_SIZE];
            let mut seek_pos = 0;

            let mut stats = Stats::default();

            let mut reads = Duration::new(0, 0);
            let mut reads_cnt = 0;

            while let Ok(req) = receiver_req.recv() {
                match req {
                    Request::Read(node, layer, rev) => {
                        let start = Instant::now();
                        reads_cnt += 1;
                        // println!("prefetch started for {}", node);
                        // WARNING: these loads must match exactly the usage pattern on the
                        // replication side

                        if rev {
                            // base parents
                            let parents = ParentsIterRev::new(graph.clone(), node);
                            load(
                                &sender_res,
                                &mut file,
                                &mut cache,
                                &mut buf,
                                next_base_rev!(parents, 0),
                                layer,
                                &mut stats,
                                &mut seek_pos,
                            );
                            load(
                                &sender_res,
                                &mut file,
                                &mut cache,
                                &mut buf,
                                next_base_rev!(parents, 1),
                                layer,
                                &mut stats,
                                &mut seek_pos,
                            );
                            load(
                                &sender_res,
                                &mut file,
                                &mut cache,
                                &mut buf,
                                next_base_rev!(parents, 2),
                                layer,
                                &mut stats,
                                &mut seek_pos,
                            );
                            load(
                                &sender_res,
                                &mut file,
                                &mut cache,
                                &mut buf,
                                next_base_rev!(parents, 3),
                                layer,
                                &mut stats,
                                &mut seek_pos,
                            );
                            load(
                                &sender_res,
                                &mut file,
                                &mut cache,
                                &mut buf,
                                next_base_rev!(parents, 4),
                                layer,
                                &mut stats,
                                &mut seek_pos,
                            );

                            // exp parents
                            load(
                                &sender_res,
                                &mut file,
                                &mut cache,
                                &mut buf,
                                next_exp!(parents, 5),
                                layer,
                                &mut stats,
                                &mut seek_pos,
                            );
                            load(
                                &sender_res,
                                &mut file,
                                &mut cache,
                                &mut buf,
                                next_exp!(parents, 6),
                                layer,
                                &mut stats,
                                &mut seek_pos,
                            );
                            load(
                                &sender_res,
                                &mut file,
                                &mut cache,
                                &mut buf,
                                next_exp!(parents, 7),
                                layer,
                                &mut stats,
                                &mut seek_pos,
                            );
                            load(
                                &sender_res,
                                &mut file,
                                &mut cache,
                                &mut buf,
                                next_exp!(parents, 8),
                                layer,
                                &mut stats,
                                &mut seek_pos,
                            );
                            load(
                                &sender_res,
                                &mut file,
                                &mut cache,
                                &mut buf,
                                next_exp!(parents, 9),
                                layer,
                                &mut stats,
                                &mut seek_pos,
                            );
                            load(
                                &sender_res,
                                &mut file,
                                &mut cache,
                                &mut buf,
                                next_exp!(parents, 10),
                                layer,
                                &mut stats,
                                &mut seek_pos,
                            );
                            load(
                                &sender_res,
                                &mut file,
                                &mut cache,
                                &mut buf,
                                next_exp!(parents, 11),
                                layer,
                                &mut stats,
                                &mut seek_pos,
                            );
                            load(
                                &sender_res,
                                &mut file,
                                &mut cache,
                                &mut buf,
                                next_exp!(parents, 12),
                                layer,
                                &mut stats,
                                &mut seek_pos,
                            );
                        } else {
                            let parents = ParentsIter::new(graph.clone(), node);

                            load(
                                &sender_res,
                                &mut file,
                                &mut cache,
                                &mut buf,
                                next_base!(parents, 0),
                                layer,
                                &mut stats,
                                &mut seek_pos,
                            );
                            load(
                                &sender_res,
                                &mut file,
                                &mut cache,
                                &mut buf,
                                next_base!(parents, 1),
                                layer,
                                &mut stats,
                                &mut seek_pos,
                            );
                            load(
                                &sender_res,
                                &mut file,
                                &mut cache,
                                &mut buf,
                                next_base!(parents, 2),
                                layer,
                                &mut stats,
                                &mut seek_pos,
                            );
                            load(
                                &sender_res,
                                &mut file,
                                &mut cache,
                                &mut buf,
                                next_base!(parents, 3),
                                layer,
                                &mut stats,
                                &mut seek_pos,
                            );
                            load(
                                &sender_res,
                                &mut file,
                                &mut cache,
                                &mut buf,
                                next_base!(parents, 4),
                                layer,
                                &mut stats,
                                &mut seek_pos,
                            );
                            // exp parents
                            load(
                                &sender_res,
                                &mut file,
                                &mut cache,
                                &mut buf,
                                next_exp!(parents, 5),
                                layer,
                                &mut stats,
                                &mut seek_pos,
                            );
                            load(
                                &sender_res,
                                &mut file,
                                &mut cache,
                                &mut buf,
                                next_exp!(parents, 6),
                                layer,
                                &mut stats,
                                &mut seek_pos,
                            );
                            load(
                                &sender_res,
                                &mut file,
                                &mut cache,
                                &mut buf,
                                next_exp!(parents, 7),
                                layer,
                                &mut stats,
                                &mut seek_pos,
                            );
                            load(
                                &sender_res,
                                &mut file,
                                &mut cache,
                                &mut buf,
                                next_exp!(parents, 8),
                                layer,
                                &mut stats,
                                &mut seek_pos,
                            );
                            load(
                                &sender_res,
                                &mut file,
                                &mut cache,
                                &mut buf,
                                next_exp!(parents, 9),
                                layer,
                                &mut stats,
                                &mut seek_pos,
                            );
                            load(
                                &sender_res,
                                &mut file,
                                &mut cache,
                                &mut buf,
                                next_exp!(parents, 10),
                                layer,
                                &mut stats,
                                &mut seek_pos,
                            );
                            load(
                                &sender_res,
                                &mut file,
                                &mut cache,
                                &mut buf,
                                next_exp!(parents, 11),
                                layer,
                                &mut stats,
                                &mut seek_pos,
                            );
                            load(
                                &sender_res,
                                &mut file,
                                &mut cache,
                                &mut buf,
                                next_exp!(parents, 12),
                                layer,
                                &mut stats,
                                &mut seek_pos,
                            );
                        }
                        // node itself
                        load(
                            &sender_res,
                            &mut file,
                            &mut cache,
                            &mut buf,
                            node,
                            layer-1,
                            &mut stats,
                            &mut seek_pos,
                        );
                        reads += start.elapsed();
                    }
                    Request::Write(node, layer, data) => {
                        // update cache
                        cache.put((node, layer), data);

                        // write to disk
                        let offset = (node * NODE_SIZE + (layer+1)*NODES*NODE_SIZE) as u64;
                        // let target = offset as i64 - seek_pos as i64;
                        // seek_pos = file.seek(SeekFrom::Current(target)).unwrap();
                        file.write_all_at(&data, offset).unwrap();
                    }
                    Request::Sync => {
                        file.sync_data().unwrap();
                        break;
                    }
                }
            }
            println!(
                "reads took {:0.4}ms for {} reads",
                reads.as_millis(),
                reads_cnt
            );
            println!("cache_hits: {}", stats.cache_hits);
            println!("cache_misses: {}", stats.cache_misses);
            println!("disk reads took {:0.4}ms", stats.reads.as_millis(),);
            println!("disk seeks took {:0.4}ms", stats.seeks.as_millis(),);
            println!(
                "disk cache reads took {:0.4}ms",
                stats.cache_reads.as_millis(),
            );
        });

        Ok(AsyncData {
            sender: sender_req,
            handle,
            queue: receiver_res,
            blocked: Duration::new(0, 0),
        })
    }

    pub fn prefetch(&mut self, node: usize, layer: usize, rev: bool) {
        //println!("< prefetch for node ({})", node);
        self.sender.send(Request::Read(node, layer, rev)).unwrap();
    }

    pub fn get_node(&mut self, node: usize, layer: usize) -> [u8; NODE_SIZE] {
        // println!("< get node ({})", node);
        let start = Instant::now();
        let (n, l, d) = self.queue.recv().unwrap();
        assert_eq!(node, n);
        self.blocked += start.elapsed();
        d
    }

    pub fn write_node(&mut self, node: usize, layer: usize, data: [u8; NODE_SIZE]) {
        self.sender.send(Request::Write(node, layer, data)).unwrap();
    }

    pub fn flush(self) {
        println!("blocked {:0.4}ms", self.blocked.as_millis());
        self.sender.send(Request::Sync).unwrap();
        self.handle.join().unwrap();
    }
}

#[inline(always)]
fn data_at_node_offset(v: usize) -> usize {
    v * NODE_SIZE
}
