#![feature(async_await, async_closure)]

use std::collections::HashMap;
use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::sync::mpsc;
use std::sync::Arc;

use chrono::Utc;
use memmap::{MmapMut, MmapOptions};

use storage_proofs::hasher::Domain;
use tempfile;

use crate::graph::{Graph, Parents, ParentsIter, ParentsIterRev};

pub mod graph;
pub mod replicate;

/// Size of the data to encode
pub const DATA_SIZE: usize = 10 * 1024 * 1024; // * 1024;
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

    file.set_len(32 * n as u64).unwrap();

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
    file.set_len(size as u64)?;
    Ok(())
}

pub struct AsyncData {
    receiver: mpsc::Receiver<(usize, [u8; NODE_SIZE])>,
    sender: mpsc::Sender<Request>,
    handle: std::thread::JoinHandle<()>,
}

enum Request {
    Read(usize, bool),
    Write(usize, [u8; NODE_SIZE]),
    Sync,
}

impl AsyncData {
    pub fn new(file_path: &'static str, graph: Arc<Graph>) -> Result<Self, failure::Error> {
        let (sender_req, receiver_req) = mpsc::channel();
        let (sender_res, receiver_res) = mpsc::channel();

        let handle = std::thread::spawn(move || {
            let mut file = fs::OpenOptions::new()
                .write(true)
                .read(true)
                .open(file_path)
                .expect("invalid file");

            let mut cache: lru::LruCache<usize, [u8; NODE_SIZE]> = lru::LruCache::new(40);

            let mut buf = [0u8; NODE_SIZE];

            let load = |file: &mut fs::File,
                        buf: &mut [u8; NODE_SIZE],
                        cache: &mut lru::LruCache<usize, [u8; NODE_SIZE]>,
                        n: usize| {
                // println!("loading {}", n);
                let res = if let Some(data) = cache.get(&n) {
                    *data
                } else {
                    let offset = n * NODE_SIZE;
                    file.seek(SeekFrom::Start(offset as u64)).unwrap();
                    file.read_exact(&mut buf[..])
                        .expect(&format!("invalid read at {} - {}", n, offset));
                    *buf
                };

                cache.put(n, res);
                sender_res.send((n, res)).expect("failed to send");
            };

            while let Ok(req) = receiver_req.recv() {
                match req {
                    Request::Read(node, rev) => {
                        // WARNING: these loads must match exactly the usage pattern on the
                        // replication side

                        if rev {
                            // base parents
                            let parents = ParentsIterRev::new(graph.clone(), node);
                            load(&mut file, &mut buf, &mut cache, next_base_rev!(parents, 0));
                            load(&mut file, &mut buf, &mut cache, next_base_rev!(parents, 1));
                            load(&mut file, &mut buf, &mut cache, next_base_rev!(parents, 2));
                            load(&mut file, &mut buf, &mut cache, next_base_rev!(parents, 3));
                            load(&mut file, &mut buf, &mut cache, next_base_rev!(parents, 4));

                            // exp parents
                            load(&mut file, &mut buf, &mut cache, next_exp!(parents, 5));
                            load(&mut file, &mut buf, &mut cache, next_exp!(parents, 6));
                            load(&mut file, &mut buf, &mut cache, next_exp!(parents, 7));
                            load(&mut file, &mut buf, &mut cache, next_exp!(parents, 8));
                            load(&mut file, &mut buf, &mut cache, next_exp!(parents, 9));
                            load(&mut file, &mut buf, &mut cache, next_exp!(parents, 10));
                            load(&mut file, &mut buf, &mut cache, next_exp!(parents, 11));
                            load(&mut file, &mut buf, &mut cache, next_exp!(parents, 12));
                        } else {
                            let parents = ParentsIter::new(graph.clone(), node);

                            load(&mut file, &mut buf, &mut cache, next_base!(parents, 0));
                            load(&mut file, &mut buf, &mut cache, next_base!(parents, 1));
                            load(&mut file, &mut buf, &mut cache, next_base!(parents, 2));
                            load(&mut file, &mut buf, &mut cache, next_base!(parents, 3));
                            load(&mut file, &mut buf, &mut cache, next_base!(parents, 4));
                            // exp parents
                            load(&mut file, &mut buf, &mut cache, next_exp!(parents, 5));
                            load(&mut file, &mut buf, &mut cache, next_exp!(parents, 6));
                            load(&mut file, &mut buf, &mut cache, next_exp!(parents, 7));
                            load(&mut file, &mut buf, &mut cache, next_exp!(parents, 8));
                            load(&mut file, &mut buf, &mut cache, next_exp!(parents, 9));
                            load(&mut file, &mut buf, &mut cache, next_exp!(parents, 10));
                            load(&mut file, &mut buf, &mut cache, next_exp!(parents, 11));
                            load(&mut file, &mut buf, &mut cache, next_exp!(parents, 12));
                        }
                        // node itself
                        load(&mut file, &mut buf, &mut cache, node);
                    }
                    Request::Write(node, data) => {
                        // update cache
                        cache.pop(&node);
                        cache.put(node, data);

                        // write to disk
                        let offset = node * NODE_SIZE;
                        file.seek(SeekFrom::Start(offset as u64)).unwrap();
                        assert_eq!(file.write(&data).unwrap(), NODE_SIZE);
                    }
                    Request::Sync => {
                        file.sync_data().unwrap();
                        break;
                    }
                }
            }
        });

        Ok(AsyncData {
            receiver: receiver_res,
            sender: sender_req,
            handle,
        })
    }

    pub fn prefetch(&mut self, node: usize, rev: bool) {
        // println!("prefetching {}", node);
        self.sender.send(Request::Read(node, rev)).unwrap();
    }

    pub fn get_node(&mut self, _node: usize) -> [u8; NODE_SIZE] {
        // println!("get node {}", node);

        let (_node_recv, buf) = self.receiver.recv().unwrap();
        // assert_eq!(node, node_recv);
        buf
    }

    pub fn write_node(&mut self, node: usize, data: [u8; NODE_SIZE]) {
        self.sender.send(Request::Write(node, data)).unwrap();
    }

    pub fn flush(self) {
        self.sender.send(Request::Sync).unwrap();
        self.handle.join().unwrap();
    }
}

#[inline(always)]
fn data_at_node_offset(v: usize) -> usize {
    v * NODE_SIZE
}
