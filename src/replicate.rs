use std::time::Instant;

use blake2s_simd::{Params as Blake2s, State};
use ff::PrimeFieldRepr;
use paired::bls12_381::FrRepr;
use storage_proofs::fr32::bytes_into_fr_repr_safe;
use storage_proofs::hasher::Hasher;

use crate::graph::{Graph, ParentsIter, ParentsIterRev};
use crate::{BASE_PARENTS, NODES, NODE_SIZE};
use crate::tsc;
use blake2s_filecoin;	

macro_rules! replicate_layer {
    ($graph:expr, $replica_id:expr, $layer:expr, $data:expr) => {
        print!("Replicating layer {}", $layer);
        let start = Instant::now();

        let tsc0 = tsc::rdtsc();
        let mut tot_bytes : u64 = 0;

        let mut hasher = Blake2s::new().hash_length(NODE_SIZE).to_state();
        hasher.update($replica_id.as_ref());

        let modulus: FrRepr = FrRepr([0xffffffff00000001,
                                      0x53bda402fffe5bfe,
                                      0x3339d80809a1d805,
                                      0x73eda753299d7d48]);

        for node in 0..NODES {
            let parents = ParentsIter::new($graph, node);
            // Compute `key` from `parents`
            let (key, count) = create_key::<H>(&parents, node, $data, 
                                               hasher.clone(), 
                                               $replica_id.as_ref());
            tot_bytes += count;

            // Get the `unencoded` node
            let start = data_at_node_offset(node);
            let end = start + NODE_SIZE;
            let mut br = FrRepr::default();
            br.read_le(&$data[start..end]).unwrap();
            br.add_nocarry(&key);
            if br >= modulus {
               br.sub_noborrow(&modulus);
            }
            br.write_le(&mut $data[start..end]).unwrap();
        }
        let tsc1 = tsc::rdtsc();
        let total_cycles = tsc1-tsc0;
        let cyc_per_byte = (total_cycles as f64) / (tot_bytes as f64);
        println!(" encoding tsc cyc/byte {cb:>width$}", cb=cyc_per_byte, width=12);
        println!(" ... took {:0.4}ms", start.elapsed().as_millis());
    };
}

macro_rules! replicate_layer_rev {
    ($graph:expr, $replica_id:expr, $layer:expr, $data:expr) => {
        print!("Replicating layer {}", $layer);
        let start = Instant::now();

        let tsc0 = tsc::rdtsc();
        let mut tot_bytes : u64 = 0;

        let mut hasher = Blake2s::new().hash_length(NODE_SIZE).to_state();
        hasher.update($replica_id.as_ref());

        let modulus: FrRepr = FrRepr([0xffffffff00000001,
                                      0x53bda402fffe5bfe,
                                      0x3339d80809a1d805,
                                      0x73eda753299d7d48]);
        for node in 0..NODES {
            let parents = ParentsIterRev::new($graph, node);
            // Compute `key` from `parents`
            let (key, count) = create_key_rev::<H>(&parents, node, $data, 
                                                   hasher.clone(), 
                                                   $replica_id.as_ref());

            tot_bytes += count;

            // Get the `unencoded` node
            let start = data_at_node_offset(node);
            let end = start + NODE_SIZE;
            let mut br = FrRepr::default();
            br.read_le(&$data[start..end]).unwrap();
            br.add_nocarry(&key);
            if br >= modulus {
               br.sub_noborrow(&modulus);
            }
            br.write_le(&mut $data[start..end]).unwrap();
        }
        let tsc1 = tsc::rdtsc();
        let total_cycles = tsc1-tsc0;
        let cyc_per_byte = (total_cycles as f64) / (tot_bytes as f64);
        println!("encoding tsc cyc/byte {cb:>width$}", cb=cyc_per_byte, width=12);
        println!(" ... took {:0.4}ms", start.elapsed().as_millis());
    };
}

/// Generates a ZigZag replicated sector.
#[inline(never)]
pub fn r2<'a, H>(replica_id: &'a H::Domain, data: &'a mut [u8], g: &'a Graph)
where
    H: Hasher,
{
    // Generate a replica at each layer of the 10 layers

    replicate_layer!(g, replica_id, 0, data);
    replicate_layer_rev!(g, replica_id, 1, data);

    replicate_layer!(g, replica_id, 2, data);
    replicate_layer_rev!(g, replica_id, 3, data);

    replicate_layer!(g, replica_id, 4, data);
    replicate_layer_rev!(g, replica_id, 5, data);

    replicate_layer!(g, replica_id, 6, data);
    replicate_layer_rev!(g, replica_id, 7, data);

    replicate_layer!(g, replica_id, 8, data);
    replicate_layer_rev!(g, replica_id, 9, data);
}

macro_rules! next_base {
    ($parents:expr, $index:expr) => {
        // safe as we statically know this is fine. compiler, why don't you?
        *unsafe { $parents.base_parents.get_unchecked($index) }
    };
}

macro_rules! next_base_rev {
    ($parents:expr, $index:expr) => {
        // safe as we statically know this is fine. compiler, why don't you?
        NODES - *unsafe { $parents.base_parents.get_unchecked($index) } - 1
    };
}

macro_rules! next_exp {
    ($parents:expr, $index:expr) => {
        // safe as we statically know this is fine. compiler, why don't you?
        *unsafe { $parents.exp_parents.get_unchecked($index - BASE_PARENTS) }
    };
}

fn create_key<H: Hasher>(
    parents: &ParentsIter,
    node: usize,
    data: &[u8],
    hasher: State,
    replica_id: &[u8],
) -> (FrRepr, u64) {
    // compile time fixed at 5 + 8 = 13 parents
    // The hash is about the parents, skip if a node doesn't have any parents
    let p0 = next_base!(parents, 0);
    if node != p0 {
        let offset    = data_at_node_offset(p0);
        let offset_1  = data_at_node_offset(next_base!(parents, 1));
        let offset_2  = data_at_node_offset(next_base!(parents, 2));
        let offset_3  = data_at_node_offset(next_base!(parents, 3));
        let offset_4  = data_at_node_offset(next_base!(parents, 4));
        let offset_5  = data_at_node_offset(next_exp!(parents, 5));
        let offset_6  = data_at_node_offset(next_exp!(parents, 6));
        let offset_7  = data_at_node_offset(next_exp!(parents, 7));
        let offset_8  = data_at_node_offset(next_exp!(parents, 8));
        let offset_9  = data_at_node_offset(next_exp!(parents, 9));
        let offset_10 = data_at_node_offset(next_exp!(parents, 10));
        let offset_11 = data_at_node_offset(next_exp!(parents, 11));
        let offset_12 = data_at_node_offset(next_exp!(parents, 12));
        let all_parents: [&[u8]; 14] = [
               replica_id,
               &unsafe { data.get_unchecked(offset..offset + NODE_SIZE) },
               &unsafe { data.get_unchecked(offset_1..offset_1 + NODE_SIZE) },
               &unsafe { data.get_unchecked(offset_2..offset_2 + NODE_SIZE) },
               &unsafe { data.get_unchecked(offset_3..offset_3 + NODE_SIZE) },
               &unsafe { data.get_unchecked(offset_4..offset_4 + NODE_SIZE) },
               &unsafe { data.get_unchecked(offset_5..offset_5 + NODE_SIZE) },
               &unsafe { data.get_unchecked(offset_6..offset_6 + NODE_SIZE) },
               &unsafe { data.get_unchecked(offset_7..offset_7 + NODE_SIZE) },
               &unsafe { data.get_unchecked(offset_8..offset_8 + NODE_SIZE) },
               &unsafe { data.get_unchecked(offset_9..offset_9 + NODE_SIZE) },
               &unsafe { data.get_unchecked(offset_10..offset_10 + NODE_SIZE) },
               &unsafe { data.get_unchecked(offset_11..offset_11 + NODE_SIZE) },
               &unsafe { data.get_unchecked(offset_12..offset_12 + NODE_SIZE) }
               ];

        let hash = blake2s_filecoin::hash_nodes_14(&all_parents);

        (bytes_into_fr_repr_safe(hash.as_ref()), 448)
    }
    else {
        let count = hasher.count();
        let hash = hasher.finalize();
        (bytes_into_fr_repr_safe(hash.as_ref()), count)
    }
}

fn create_key_rev<H: Hasher>(
    parents: &ParentsIterRev,
    node: usize,
    data: &[u8],
    hasher: State,
    replica_id: &[u8],
) -> (FrRepr, u64) {
    // compile time fixed at 5 + 8 = 13 parents
    // The hash is about the parents, skip if a node doesn't have any parents
    let p0 = next_base_rev!(parents, 0);
    if node != p0 {
        let offset    = data_at_node_offset(p0);
        let offset_1  = data_at_node_offset(next_base_rev!(parents, 1));
        let offset_2  = data_at_node_offset(next_base_rev!(parents, 2));
        let offset_3  = data_at_node_offset(next_base_rev!(parents, 3));
        let offset_4  = data_at_node_offset(next_base_rev!(parents, 4));
        let offset_5  = data_at_node_offset(next_exp!(parents, 5));
        let offset_6  = data_at_node_offset(next_exp!(parents, 6));
        let offset_7  = data_at_node_offset(next_exp!(parents, 7));
        let offset_8  = data_at_node_offset(next_exp!(parents, 8));
        let offset_9  = data_at_node_offset(next_exp!(parents, 9));
        let offset_10 = data_at_node_offset(next_exp!(parents, 10));
        let offset_11 = data_at_node_offset(next_exp!(parents, 11));
        let offset_12 = data_at_node_offset(next_exp!(parents, 12));
        let all_parents: [&[u8]; 14] = [
               replica_id,
               &unsafe { data.get_unchecked(offset..offset + NODE_SIZE) },
               &unsafe { data.get_unchecked(offset_1..offset_1 + NODE_SIZE) },
               &unsafe { data.get_unchecked(offset_2..offset_2 + NODE_SIZE) },
               &unsafe { data.get_unchecked(offset_3..offset_3 + NODE_SIZE) },
               &unsafe { data.get_unchecked(offset_4..offset_4 + NODE_SIZE) },
               &unsafe { data.get_unchecked(offset_5..offset_5 + NODE_SIZE) },
               &unsafe { data.get_unchecked(offset_6..offset_6 + NODE_SIZE) },
               &unsafe { data.get_unchecked(offset_7..offset_7 + NODE_SIZE) },
               &unsafe { data.get_unchecked(offset_8..offset_8 + NODE_SIZE) },
               &unsafe { data.get_unchecked(offset_9..offset_9 + NODE_SIZE) },
               &unsafe { data.get_unchecked(offset_10..offset_10 + NODE_SIZE) },
               &unsafe { data.get_unchecked(offset_11..offset_11 + NODE_SIZE) },
               &unsafe { data.get_unchecked(offset_12..offset_12 + NODE_SIZE) }
               ];

        let hash = blake2s_filecoin::hash_nodes_14(&all_parents);
        (bytes_into_fr_repr_safe(hash.as_ref()), 448)
    }
    else {
        let count = hasher.count();
        let hash = hasher.finalize();
        (bytes_into_fr_repr_safe(hash.as_ref()), count)
    }
}

#[inline(always)]
fn data_at_node_offset(v: usize) -> usize {
    v * NODE_SIZE
}
