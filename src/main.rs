use blake2s_simd::Params as Blake2s;
use chrono::Utc;
use ff::Field;
use memmap::{MmapMut, MmapOptions};
use paired::bls12_381::{Bls12, Fr, FrRepr};
use paired::Engine;
use std::fs::{File, OpenOptions};
use storage_proofs::drgraph::new_seed;
use storage_proofs::drgraph::Graph;
use storage_proofs::error::Result;
use storage_proofs::fr32::bytes_into_fr_repr_safe;
use storage_proofs::hasher::{Blake2sHasher, Domain, Hasher};
use storage_proofs::util::{data_at_node, data_at_node_offset};
use storage_proofs::vde::create_key;
use storage_proofs::zigzag_graph::{ZigZag, ZigZagBucketGraph};
use tempfile;

// mod graph;

const DATA_SIZE: usize = 1 * 1024 * 1024;
const NODE_SIZE: usize = 32;
const LAYERS: usize = 10;
const NODES: usize = DATA_SIZE / NODE_SIZE;
const BASE_PARENTS: usize = 5;
const EXP_PARENTS: usize = 8;
const PARENT_SIZE: usize = BASE_PARENTS + EXP_PARENTS;

fn r<'a, H, G>(
    graph: &'a G,
    replica_id: &'a H::Domain,
    layer: usize,
    data: &'a mut [u8],
) -> Result<()>
where
    H: Hasher,
    G: Graph<H>,
{
    let mut parents = vec![0; PARENT_SIZE];
    for n in 0..NODES {
        let node = if graph.forward() { n } else { (NODES - n) - 1 };
        graph.parents(node, &mut parents);

        let key = create_key::<H>(replica_id, node, &parents, data)?;
        let start = data_at_node_offset(node);
        let end = start + NODE_SIZE;

        let node_data = H::Domain::try_from_bytes(&data[start..end])?;
        let mut node_fr: Fr = node_data.into();
        node_fr.add_assign(&key.into());
        let encoded: H::Domain = node_fr.into();

        encoded.write_bytes(&mut data[start..end])?;
    }

    Ok(())
}

fn r2<'a, G, H>(replica_id: &'a H::Domain, data: &'a mut [u8], g: &'a G)
where
    H: Hasher,
    G: Graph<H>,
{
    for l in 0..LAYERS {
        println!("Replica {} starting", l);
        let replica = r(g, replica_id, l, data);
        println!("Replica {} done", l);
        if let Ok(_) = replica {
            println!("replica is correct!");
        }
    }
}

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
    let g = ZigZagBucketGraph::<Blake2sHasher>::new_zigzag(NODES, 5, 8, new_seed());
    let replica_id = id_from_str::<<Blake2sHasher as Hasher>::Domain>("aaaa");
    let use_tmp = true;
    let mut data = file_backed_mmap_from_zeroes(NODES, use_tmp);
    println!("Starting replication");
    r2(&replica_id, &mut data, &g)
}
