#![feature(async_await, async_closure)]

use blake2s_simd::Params as Blake2s;
use r2::{file_backed_mmap_from_zeroes, graph, id_from_str, replicate, NODES, NODE_SIZE};
use std::collections::HashMap;
use std::io::{Read, SeekFrom};
use storage_proofs::hasher::{Blake2sHasher, Hasher};

use futures::channel::oneshot;
use futures::future::poll_fn;
use futures::future::BoxFuture;
use futures_util::future::FutureExt;
use futures_util::try_future::TryFutureExt;
use tokio;
use tokio::fs;
use tokio::io::{AsyncRead, AsyncWriteExt};
use tokio::prelude::*;

#[tokio::main]
pub async fn main() -> Result<(), failure::Error> {
    let file_path = "tmp.txt";
    let mut file = fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .read(true)
        .open(file_path.clone())
        .await?;
    poll_fn(|_cx| file.poll_set_len((NODES * NODE_SIZE) as u64)).await?;

    let mut data = AsyncData::new(file_path.clone()).await;
    let nodes = vec![(0, [0, 1, 4]), (1, [0, 4, 10]), (2, [0, 1, 2])];

    for (node, parents) in nodes.into_iter() {
        data.prefetch(node, &parents);
        std::thread::sleep(std::time::Duration::from_millis(10));

        for parent in &parents {
            let p = data.get_node(*parent).await;
            println!("  {}: {:?}", parent, p);
        }

        let n = data.get_node_mut(node).await;
        println!("n{} - {:?}", node, n);

        // fancy encoding
        let mut hasher = Blake2s::new().hash_length(NODE_SIZE).to_state();
        hasher.update(n);
        n.copy_from_slice(hasher.finalize().as_ref());
        data.write_node(node).await;
    }

    Ok(())
}

// fn main() {
//     // Load the graph from memory or generate a new one
//     let gg = graph::Graph::new_cached();
//     // Compute replica_id
//     let replica_id = id_from_str::<<Blake2sHasher as Hasher>::Domain>("aaaa");
//     // Generate a file full of zeroes to be replicated
//     let mut data = file_backed_mmap_from_zeroes(NODES, true);
//     // Start replication
//     println!("Starting replication");
//     replicate::r2::<Blake2sHasher>(&replica_id, &mut data, &gg)
// }

pub struct AsyncData {
    nodes: Option<oneshot::Receiver<(fs::File, HashMap<usize, Vec<u8>>)>>,
    nodes_map: Option<HashMap<usize, Vec<u8>>>,
    file: Option<fs::File>,
}

impl AsyncData {
    pub async fn new(file_path: &'static str) -> Self {
        let file = fs::OpenOptions::new()
            .write(true)
            .read(true)
            .open(file_path)
            .await
            .unwrap();

        AsyncData {
            nodes: None,
            nodes_map: None,
            file: Some(file),
        }
    }

    pub fn prefetch(&mut self, node: usize, parents: &[usize]) {
        println!("prefetch start");
        // trigger async read into internal cache of
        // - node
        // - parents

        let (sender, receiver) = oneshot::channel();

        self.nodes = Some(receiver);
        let mut list = vec![node];
        list.extend(parents);

        tokio::spawn(
            PrefetchNodeFuture::new(self.file.take().unwrap(), node, parents).map(|res| {
                let (file, nodes) = res.unwrap();
                sender.send((file, nodes)).unwrap();
            }),
        );
    }

    async fn fetch_node(&mut self) {
        if self.nodes.is_some() {
            let f = self.nodes.take().expect("missing nodes");
            let (file, nodes) = f.await.expect("failed to fetch");

            self.file = Some(file);
            self.nodes_map = Some(nodes);
        }
    }

    pub async fn get_node(&mut self, node: usize) -> &[u8] {
        println!("fetching node: {}", node);
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
        println!("Writing {} - {:?} - {}", node, data, offset);

        let file = self.file.take().unwrap();
        let (mut file, _) = file.seek(SeekFrom::Start(offset as u64)).await.unwrap();
        file.write(data).await.unwrap();
        poll_fn(|_cx| file.poll_sync_data()).await.unwrap();
        self.file = Some(file);
    }
}

#[derive(Debug)]
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct PrefetchNodeFuture {
    inner: Option<fs::File>,
    parents: Vec<usize>,
    nodes: Option<HashMap<usize, Vec<u8>>>,
    buf: Vec<u8>,
}

impl PrefetchNodeFuture {
    pub fn new(file: fs::File, node: usize, parents: &[usize]) -> Self {
        let mut p = vec![node];
        p.extend(parents);

        Self {
            buf: vec![0u8; NODE_SIZE],
            inner: Some(file),
            parents: p,
            nodes: Some(HashMap::default()),
        }
    }
}

impl Future for PrefetchNodeFuture {
    type Output = std::io::Result<(fs::File, HashMap<usize, Vec<u8>>)>;

    fn poll(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> Poll<Self::Output> {
        let inner_self = std::pin::Pin::get_mut(self);

        match inner_self.nodes {
            Some(ref mut nodes) => {
                // TODO: figure out if this loop works as expected
                for node in &inner_self.parents {
                    let offset = node * NODE_SIZE;
                    let f = inner_self.inner.as_mut().expect("fail after resolve");

                    futures::ready!(f.poll_seek(SeekFrom::Start(offset as u64)))?;

                    pin_utils::pin_mut!(f);
                    let pf: std::pin::Pin<&mut _> = f;
                    futures::ready!(AsyncRead::poll_read(pf, _cx, &mut inner_self.buf[..]))?;
                    nodes.insert(*node, inner_self.buf.clone());
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
