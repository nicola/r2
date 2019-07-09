use chrono::Utc;
use memmap::{MmapMut, MmapOptions};
use std::fs::{File, OpenOptions};
use storage_proofs::drgraph::new_seed;
use storage_proofs::hasher::{Blake2sHasher, Domain, Hasher};
use tempfile;

mod graph;
mod replicate;

pub const DATA_SIZE: usize = 1024 * 1024 * 1024;
pub const NODE_SIZE: usize = 32;
pub const LAYERS: usize = 10;
pub const NODES: usize = DATA_SIZE / NODE_SIZE;
pub const BASE_PARENTS: usize = 5;
pub const EXP_PARENTS: usize = 8;
pub const PARENT_SIZE: usize = BASE_PARENTS + EXP_PARENTS;

fn file_backed_mmap_from_zeroes(n: usize, use_tmp: bool) -> MmapMut {
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

pub fn id_from_str<T: Domain>(raw: &str) -> T {
    let replica_id_raw = hex::decode(raw).expect("invalid hex for replica id seed");
    let mut replica_id_bytes = vec![0u8; 32];
    let len = ::std::cmp::min(32, replica_id_raw.len());
    replica_id_bytes[..len].copy_from_slice(&replica_id_raw[..len]);
    T::try_from_bytes(&replica_id_bytes).expect("invalid replica id")
}

fn main() {
    let gg = graph::Graph::new_cached(NODES, BASE_PARENTS, EXP_PARENTS, new_seed());
    let replica_id = id_from_str::<<Blake2sHasher as Hasher>::Domain>("aaaa");
    let use_tmp = true;
    let mut data = file_backed_mmap_from_zeroes(NODES, use_tmp);
    println!("Starting replication");

    replicate::r2::<Blake2sHasher>(&replica_id, &mut data, &gg)
}
