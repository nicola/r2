#![feature(async_await, async_closure)]

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::SeekFrom;
use std::sync::{Arc, Mutex};

use chrono::Utc;
use futures::channel::mpsc;
use futures::future::poll_fn;
use futures_util::future::FutureExt;
use futures_util::sink::SinkExt;
use futures_util::stream::StreamExt;
use futures_util::try_future::TryFutureExt;
use memmap::{MmapMut, MmapOptions};
use tokio;
use tokio::fs;
use tokio::io::{AsyncRead, AsyncWriteExt};
use tokio::prelude::*;

use storage_proofs::hasher::Domain;
use tempfile;

use crate::graph::{Parents, ParentsIter, ParentsIterRev};

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
    ($parents:expr, $index:expr) => {
        // safe as we statically know this is fine. compiler, why don't you?
        *unsafe { $parents.exp_parents().get_unchecked($index - BASE_PARENTS) }
    };
}

/// Generate a tmp file full of zeros
pub fn file_backed_mmap_from_zeroes(n: usize, use_tmp: bool) -> MmapMut {
    let file: File = if use_tmp {
        tempfile::tempfile().unwrap()
    } else {
        OpenOptions::new()
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

pub async fn create_empty_file(file_path: &'static str, size: usize) -> Result<(), failure::Error> {
    let mut file = fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .read(true)
        .open(file_path)
        .await?;
    poll_fn(|_cx| file.poll_set_len(size as u64)).await?;
    Ok(())
}

pub struct AsyncData {
    receiver: mpsc::Receiver<(fs::File, HashMap<usize, [u8; NODE_SIZE]>)>,
    sender: mpsc::Sender<(fs::File, HashMap<usize, [u8; NODE_SIZE]>)>,
    nodes_map: Option<HashMap<usize, [u8; NODE_SIZE]>>,
    file: Option<fs::File>,
}

impl AsyncData {
    pub async fn new(file_path: &'static str) -> Result<Self, failure::Error> {
        let file = fs::OpenOptions::new()
            .write(true)
            .read(true)
            .open(file_path)
            .await?;
        let (sender, receiver) = mpsc::channel(2);

        Ok(AsyncData {
            receiver,
            sender,
            nodes_map: None,
            file: Some(file),
        })
    }

    pub fn prefetch(&mut self, node: usize, parents: ParentsIter) {
        // trigger async read into internal cache of
        // - node
        // - parents

        let mut sender = self.sender.clone();
        self.nodes_map.take();
        tokio::spawn(
            PrefetchNodeFuture::new(self.file.take().unwrap(), node, parents)
                .and_then(async move |res| {
                    sender.send(res).await.unwrap();
                    Ok(())
                })
                .map(|_| ()),
        );
    }

    pub fn prefetch_rev(&mut self, node: usize, parents: ParentsIterRev) {
        // trigger async read into internal cache of
        // - node
        // - parents

        let mut sender = self.sender.clone();
        self.nodes_map.take();
        tokio::spawn(
            PrefetchNodeFuture::new(self.file.take().unwrap(), node, parents)
                .and_then(async move |res| {
                    sender.send(res).await.unwrap();
                    Ok(())
                })
                .map(|_| ()),
        );
    }

    async fn fetch_node(&mut self) {
        if self.nodes_map.is_none() {
            let (file, nodes) = self.receiver.next().await.expect("failed to fetch");

            self.file = Some(file);
            self.nodes_map = Some(nodes);
        }
    }

    pub async fn get_node(&mut self, node: usize) -> &[u8] {
        // println!("fetching node: {}", node);
        self.fetch_node().await;

        self.nodes_map
            .as_ref()
            .unwrap()
            .get(&node)
            .map(|v| &v[..])
            .unwrap()
    }

    pub async fn get_node_mut(&mut self, node: usize) -> &mut [u8] {
        self.fetch_node().await;

        self.nodes_map
            .as_mut()
            .unwrap()
            .get_mut(&node)
            .map(|v| &mut v[..])
            .unwrap()
    }

    /// Write the node to disk, has to be called __after__ `get_node_mut` to this node.
    pub async fn write_node(&mut self, node: usize) {
        let data = self
            .nodes_map
            .as_ref()
            .unwrap()
            .get(&node)
            .map(|v| &v[..])
            .unwrap();
        let offset = node * NODE_SIZE;
        // println!("Writing {} - {:?} - {}", node, data, offset);

        let file = self.file.take().unwrap();
        let (mut file, _) = file.seek(SeekFrom::Start(offset as u64)).await.unwrap();
        file.write(data).await.unwrap();
        self.file = Some(file);
    }

    pub async fn flush(&mut self) {
        let mut file = self.file.take().unwrap();
        poll_fn(|_cx| file.poll_sync_data()).await.unwrap();
        self.file = Some(file);
    }
}

#[derive(Debug)]
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct PrefetchNodeFuture<T: Parents> {
    inner: Option<fs::File>,
    node: usize,
    parents: T,
    to_encode: Option<[usize; 14]>,
    nodes: Option<HashMap<usize, [u8; NODE_SIZE]>>,
    buf: [u8; NODE_SIZE],
}

impl<T: Parents> PrefetchNodeFuture<T> {
    pub fn new(file: fs::File, node: usize, parents: T) -> Self {
        Self {
            buf: [0u8; NODE_SIZE],
            node,
            to_encode: None,
            inner: Some(file),
            parents,
            nodes: Some(HashMap::default()),
        }
    }
}

impl<T: Parents + std::marker::Unpin> Future for PrefetchNodeFuture<T> {
    type Output = Result<(fs::File, HashMap<usize, [u8; NODE_SIZE]>), failure::Error>;

    fn poll(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> Poll<Self::Output> {
        let inner_self = std::pin::Pin::get_mut(self);
        match inner_self.nodes {
            Some(ref mut nodes) => {
                if inner_self.to_encode.is_none() {
                    let mut p = inner_self.parents.get_all(inner_self.node);
                    p.sort_unstable();
                    inner_self.to_encode = Some(p);
                }

                let mut seek_current = 0;
                let mut seek: SeekFrom;
                let mut offset: u64;
                for node in inner_self.to_encode.as_ref().unwrap() {
                    if !nodes.contains_key(node) {
                        offset = (node * NODE_SIZE) as u64;
                        // compute relative seeking offset, to improve seeking speed
                        seek = SeekFrom::Current(offset as i64 - seek_current as i64);

                        let f = inner_self.inner.as_mut().expect("fail after resolve");

                        seek_current = futures::ready!(f.poll_seek(seek))?;

                        pin_utils::pin_mut!(f);
                        let pf: std::pin::Pin<&mut _> = f;
                        futures::ready!(AsyncRead::poll_read(pf, _cx, &mut inner_self.buf[..]))?;
                        nodes.insert(*node, inner_self.buf.clone());
                    }
                }

                // done
                let inner = inner_self.inner.take().unwrap();
                let nodes = inner_self.nodes.take().unwrap();
                Poll::Ready(Ok((inner, nodes)))
            }
            None => {
                panic!("already resolved");
            }
        }
    }
}

#[inline(always)]
fn data_at_node_offset(v: usize) -> usize {
    v * NODE_SIZE
}
