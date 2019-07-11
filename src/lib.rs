use chrono::Utc;
use memmap::{MmapMut, MmapOptions};
use std::fs::{File, OpenOptions};
use storage_proofs::hasher::Domain;
use tempfile;

pub mod graph;
pub mod replicate;

/// Size of the data to encode
pub const DATA_SIZE: usize = 1 * 1024 * 1024 * 10;
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
