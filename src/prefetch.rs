use crate::NODE_SIZE;
use runtime;
use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::io::Write;
use std::io::Seek;
use std::io::SeekFrom;

pub struct DataPrefetch {
    pub map: HashMap<usize, runtime::task::JoinHandle<[u8; 32]>>,
}

impl DataPrefetch {
    pub async fn await_all(&mut self) -> HashMap<usize, [u8; 32]> {
        let mut map2 = HashMap::new();

        let map = std::mem::replace(&mut self.map, Default::default());
        for (key, value) in map.into_iter() {
            map2.insert(key, value.await);
        }

        map2
    }
    pub fn write(&mut self, index: usize, buf: &[u8; 32]) {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .open("./hello")
            .unwrap();

        file.seek(SeekFrom::Start((index * NODE_SIZE) as u64)).unwrap();
        file.write(buf).unwrap();
    }
    pub fn prefetch(&mut self, index: usize) {
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
                buf
            });
            self.map.insert(index, handle);
        }
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

//     println!("{:?}", String::from_utf8_lossy(&buf1));
//     println!("{:?}", String::from_utf8_lossy(&buf2));
//     Ok(())
// }
