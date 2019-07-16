#![feature(async_await, async_closure)]

use std::collections::HashMap;
use std::io::{Read, SeekFrom};

use r2::{file_backed_mmap_from_zeroes, graph, id_from_str, replicate, NODES, NODE_SIZE};
use storage_proofs::hasher::{Blake2sHasher, Hasher};

use futures::channel::oneshot;
use futures::future::BoxFuture;
use futures_util::future::FutureExt;
use futures_util::try_future::TryFutureExt;
use tokio;
use tokio::fs;
use tokio::io::{AsyncRead, AsyncWriteExt};
use tokio::prelude::*;

#[tokio::main]
pub async fn main() -> Result<(), failure::Error> {
    let file = fs::File::create("tmp.txt").await?;
    let file = set_len(file, (NODES * NODE_SIZE) as u64).await?;

    let mut data = AsyncData::new(file);
    let nodes = vec![(0, [0, 1, 4]), (1, [2, 4, 10])];

    for (node, parents) in nodes.into_iter() {
        data.prefetch(node, &parents);
        std::thread::sleep(std::time::Duration::from_millis(10));

        for parent in &parents {
            let n = data.get_node(*parent).await;
            println!("n{} - {}: {:?}", node, parent, n);
        }
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
    pub fn new(file: fs::File) -> Self {
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

    pub async fn get_node(&mut self, node: usize) -> &[u8] {
        println!("fetching node: {}", node);
        if self.nodes.is_some() {
            let f = self.nodes.take().expect("missing nodes");
            let (file, nodes) = f.await.expect("failed to fetch");

            self.file = Some(file);
            self.nodes_map = Some(nodes);
        }

        self.nodes_map
            .as_ref()
            .unwrap()
            .get(&node)
            .map(|v| &v[..])
            .unwrap()
    }

    pub async fn get_node_mut(&mut self, node: usize) -> &mut [u8] {
        unimplemented!()
    }
}

#[derive(Debug)]
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct PrefetchNodeFuture {
    inner: Option<fs::File>,
    parents: Vec<usize>,
    nodes: Option<HashMap<usize, Vec<u8>>>,
}

impl PrefetchNodeFuture {
    pub fn new(file: fs::File, node: usize, parents: &[usize]) -> Self {
        let mut p = vec![node];
        p.extend(parents);

        Self {
            inner: Some(file),
            parents: p,
            nodes: Some(HashMap::default()),
        }
    }
}

impl Future for PrefetchNodeFuture {
    type Output = std::io::Result<(fs::File, HashMap<usize, Vec<u8>>)>;

    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        let inner_self = std::pin::Pin::get_mut(self);

        match inner_self.nodes {
            Some(ref mut nodes) => {
                // TODO: figure out if this loop works as expected
                for node in &inner_self.parents {
                    let offset = node * NODE_SIZE;
                    let mut buf = vec![0u8; NODE_SIZE];

                    futures::ready!(inner_self
                        .inner
                        .as_mut()
                        .expect("fail after resolve")
                        .poll_seek(SeekFrom::Start(offset as u64)))?;

                    let f = inner_self.inner.as_mut().expect("fail after resolve");
                    pin_utils::pin_mut!(f);

                    match std::pin::Pin::get_mut(f).read(&mut buf[..]) {
                        Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            return Poll::Pending
                        }
                        other => {
                            nodes.insert(*node, buf);
                        }
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

/// Future returned by `set_len`.
#[derive(Debug)]
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct SetLenFuture {
    inner: Option<fs::File>,
    len: u64,
}

impl SetLenFuture {
    pub(crate) fn new(file: fs::File, len: u64) -> Self {
        Self {
            len,
            inner: Some(file),
        }
    }
}

impl Future for SetLenFuture {
    type Output = std::io::Result<fs::File>;

    fn poll(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> Poll<Self::Output> {
        let inner_self = std::pin::Pin::get_mut(self);
        futures::ready!(inner_self
            .inner
            .as_mut()
            .expect("Cannot poll `SetLenFuture` after it resolves")
            .poll_set_len(inner_self.len))?;
        let inner = inner_self.inner.take().unwrap();
        Poll::Ready(Ok(inner))
    }
}

fn set_len(file: fs::File, len: u64) -> SetLenFuture {
    SetLenFuture::new(file, len)
}
