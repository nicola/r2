use crate::graph::{Graph, ParentsIter};
use crate::{BASE_PARENTS, EXP_PARENTS, NODE_SIZE};
use runtime;
use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::io::Seek;
use std::io::SeekFrom;
use std::io::Write;

macro_rules! next_base {
    ($parents:expr, $index:expr) => {
        // safe as we statically know this is fine. compiler, why don't you?
        *unsafe { $parents.base_parents.get_unchecked($index) }
    };
}

macro_rules! next_exp {
    ($parents:expr, $index:expr) => {
        // safe as we statically know this is fine. compiler, why don't you?
        *unsafe { $parents.exp_parents.get_unchecked($index - BASE_PARENTS) }
    };
}

pub struct DataPrefetch {
    pub map: HashMap<usize, runtime::task::JoinHandle<[u8; 32]>>,
    pub cache: HashMap<usize, [u8; 32]>,
}

impl DataPrefetch {
    pub fn new() -> Self {
        DataPrefetch {
            map: HashMap::new(),
            cache: HashMap::new(),
        }
    }
    pub fn get_node(&mut self, node: usize, layer: usize) -> &[u8; 32] {
        // println!("getting {} for layer {} (requested)", node, layer);
        if !self.cache.contains_key(&node) {
            self.prefetch_node(node, layer);
            let future = self.map.remove(&node).unwrap();
            let data = futures::executor::block_on(future);
            // println!("getting {} for layer {} (block)", node, layer);
            self.cache.insert(node, data);
        }
        // println!("getting {} for layer {} (cached)", node, layer);
        self.cache.get(&node).unwrap()
    }
    pub async fn await_all(&mut self) -> HashMap<usize, [u8; 32]> {
        let mut map2 = HashMap::new();

        let map = std::mem::replace(&mut self.map, Default::default());
        for (key, value) in map.into_iter() {
            map2.insert(key, value.await);
        }

        map2
    }
    pub fn clear(&mut self) {
        self.map.clear();
        self.cache.clear();
    }
    pub fn write_node(&mut self, index: usize, buf: &[u8; 32]) {
        let mut file = fs::OpenOptions::new().write(true).open("./hello").unwrap();

        file.seek(SeekFrom::Start((index * NODE_SIZE) as u64))
            .unwrap();
        file.write(buf).unwrap();
    }
    pub fn prefetch_node(&mut self, index: usize, layer: usize) {
        if !self.map.contains_key(&index) {
            let handle = runtime::spawn(async move {
                let mut file = fs::OpenOptions::new()
                    .read(true)
                    .open("./hello")
                    // .await
                    .unwrap();
                let mut buf: [u8; 32] = [0; 32];
                file.seek(SeekFrom::Start((index * NODE_SIZE) as u64))
                    .unwrap();
                file.read(&mut buf).unwrap();
                // println!("prefetching {} for layer {} (disk-done)", index, layer);
                buf
            });
            self.map.insert(index, handle);
            // println!("prefetching {} for layer {} (disk-requested)", index, layer);
        } else {
            // // println!("prefetching {} for layer {} (prefetched)", index, layer);
        }
    }
    pub fn prefetch<'a>(&mut self, g: &'a Graph, node: usize, layer: usize) {
        let parents = ParentsIter::new(g, node);
        // println!("---- calling prefetch on {}", node);
        // println!(
        //     "base parents: {:?}, exp parents {:?}",
        //     parents.base_parents, parents.exp_parents
        // );

        self.prefetch_node(next_base!(parents, 0), layer);
        self.prefetch_node(next_base!(parents, 1), layer);
        self.prefetch_node(next_base!(parents, 2), layer);
        self.prefetch_node(next_base!(parents, 3), layer);
        self.prefetch_node(next_base!(parents, 4), layer);
        self.prefetch_node(next_exp!(parents, 5), layer);
        self.prefetch_node(next_exp!(parents, 6), layer);
        self.prefetch_node(next_exp!(parents, 7), layer);
        self.prefetch_node(next_exp!(parents, 8), layer);
        self.prefetch_node(next_exp!(parents, 9), layer);
        self.prefetch_node(next_exp!(parents, 10), layer);
        self.prefetch_node(next_exp!(parents, 11), layer);
        self.prefetch_node(next_exp!(parents, 12), layer);
    }
}

// #[runtime::main]
// async fn main() -> Result<(), failure::Error> {
//     // PREFETCH
//     let mut p = DataPrefetch {
//         map: HashMap::new(),
//     };
//     p.prefetch(0);
//     p.prefetch(1);
//     // SLOW COMPUTATION

//     // Read this
//     let handle1 = p.map.remove(&0).unwrap();
//     let handle2 = p.map.remove(&1).unwrap();
//     let buf1: [u8; 32] = handle1.await;
//     let buf2: [u8; 32] = handle2.await;

//     // println!("{:?}", String::from_utf8_lossy(&buf1));
//     // println!("{:?}", String::from_utf8_lossy(&buf2));
//     Ok(())
// }
