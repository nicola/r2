use std::time::Instant;

use blake2s_simd::{Params as Blake2s, State};
use ff::Field;
use paired::bls12_381::Fr;
use storage_proofs::error::Result;
use storage_proofs::fr32::bytes_into_fr_repr_safe;
use storage_proofs::hasher::{Domain, Hasher};

use crate::graph::{Graph, ParentsIter, ParentsIterRev};
use crate::prefetch::DataPrefetch;
use crate::{BASE_PARENTS, LAYERS, NODES, NODE_SIZE};

macro_rules! replicate_layer {
    ($graph:expr, $replica_id:expr, $layer:expr, $data:expr) => {
        dbg!("Replicating layer {}", $layer);
        let start = Instant::now();

        let mut hasher = Blake2s::new().hash_length(NODE_SIZE).to_state();
        hasher.update($replica_id.as_ref());

        // Var that has current fetched nodes
        let mut data_curr = &mut DataPrefetch::new();
        let mut data_next = &mut DataPrefetch::new();

        data_curr.prefetch($graph, 0, $layer);
        for node in 0..NODES {
            dbg!("encode node {}", node);
            if node < NODES - 1 {
                data_next.prefetch($graph, node + 1, $layer);
            }
            let parents = ParentsIter::new($graph, node);
            // Compute `key` from `parents`
            let key = create_key::<H>(&parents, node, data_curr, hasher.clone(), $layer);

            // Get the `unencoded` node
            let start = data_at_node_offset(node);
            let end = start + NODE_SIZE;
            let node_data = H::Domain::try_from_bytes(&$data[start..end]).expect("invalid data");
            let mut node_fr: Fr = node_data.into();

            // Compute the `encoded` node by adding the `key` to it
            node_fr.add_assign(&key.into());
            let encoded: H::Domain = node_fr.into();

            // Store the `encoded` data
            encoded
                .write_bytes(&mut $data[start..end])
                .expect("failed to write");

            let tmp1 = data_curr;
            let tmp2 = data_next;
            data_curr = tmp2;
            data_next = tmp1;
            data_next.clear();
        }
        dbg!(" ... took {:0.4}ms", start.elapsed().as_millis());
    };
}

macro_rules! replicate_layer_rev {
    ($graph:expr, $replica_id:expr, $layer:expr, $data:expr) => {
        dbg!("Replicating layer {}", $layer);
        let start = Instant::now();

        let mut hasher = Blake2s::new().hash_length(NODE_SIZE).to_state();
        hasher.update($replica_id.as_ref());

        for node in 0..NODES {
            let parents = ParentsIterRev::new($graph, node);
            // Compute `key` from `parents`
            let key = create_key_rev::<H>(&parents, node, $data, hasher.clone());

            // Get the `unencoded` node
            let start = data_at_node_offset(node);
            let end = start + NODE_SIZE;
            let node_data = H::Domain::try_from_bytes(&$data[start..end]).expect("invalid data");
            let mut node_fr: Fr = node_data.into();

            // Compute the `encoded` node by adding the `key` to it
            node_fr.add_assign(&key.into());
            let encoded: H::Domain = node_fr.into();

            // Store the `encoded` data
            encoded
                .write_bytes(&mut $data[start..end])
                .expect("failed to write");
        }

        dbg!(" ... took {:0.4}ms", start.elapsed().as_millis());
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

macro_rules! hash {
    ($parent:expr, $hasher:expr, $data:expr) => {
        let offset = data_at_node_offset($parent);
        $hasher.update(&unsafe { $data.get_unchecked(offset..offset + NODE_SIZE) });
    };
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
    data: &mut DataPrefetch,
    mut hasher: State,
    layer: usize,
) -> H::Domain {
    // compile time fixed at 5 + 8 = 13 parents

    // The hash is about the parents, hence skip if a node doesn't have any parents
    let p0 = next_base!(parents, 0);
    if node != p0 {
        // hash first parentget_node
        dbg!("  soon to fetch {}", node);
        hasher.update(data.get_node(node, layer));

        // base parents
        dbg!("  soon to fetch {}", next_base!(parents, 1));
        hasher.update(data.get_node(next_base!(parents, 1), layer));
        dbg!("  soon to fetch {}", next_base!(parents, 2));
        hasher.update(data.get_node(next_base!(parents, 2), layer));
        dbg!("  soon to fetch {}", next_base!(parents, 3));
        hasher.update(data.get_node(next_base!(parents, 3), layer));
        dbg!("  soon to fetch {}", next_base!(parents, 4));
        hasher.update(data.get_node(next_base!(parents, 4), layer));

        // exp parents
        hasher.update(data.get_node(next_exp!(parents, 5), layer));
        hasher.update(data.get_node(next_exp!(parents, 6), layer));
        hasher.update(data.get_node(next_exp!(parents, 7), layer));
        hasher.update(data.get_node(next_exp!(parents, 8), layer));
        hasher.update(data.get_node(next_exp!(parents, 9), layer));
        hasher.update(data.get_node(next_exp!(parents, 10), layer));
        hasher.update(data.get_node(next_exp!(parents, 11), layer));
        hasher.update(data.get_node(next_exp!(parents, 12), layer));
    }

    let hash = hasher.finalize();
    bytes_into_fr_repr_safe(hash.as_ref()).into()
}

fn create_key_rev<H: Hasher>(
    parents: &ParentsIterRev,
    node: usize,
    data: &[u8],
    mut hasher: State,
) -> H::Domain {
    // compile time fixed at 5 + 8 = 13 parents

    // The hash is about the parents, hence skip if a node doesn't have any parents
    let p0 = next_base_rev!(parents, 0);
    if node != p0 {
        // hash first parent
        let offset = data_at_node_offset(p0);
        hasher.update(&unsafe { data.get_unchecked(offset..offset + NODE_SIZE) });

        // base parents
        hash!(next_base_rev!(parents, 1), hasher, data);
        hash!(next_base_rev!(parents, 2), hasher, data);
        hash!(next_base_rev!(parents, 3), hasher, data);
        hash!(next_base_rev!(parents, 4), hasher, data);

        // exp parents
        hash!(next_exp!(parents, 5), hasher, data);
        hash!(next_exp!(parents, 6), hasher, data);
        hash!(next_exp!(parents, 7), hasher, data);
        hash!(next_exp!(parents, 8), hasher, data);
        hash!(next_exp!(parents, 9), hasher, data);
        hash!(next_exp!(parents, 10), hasher, data);
        hash!(next_exp!(parents, 11), hasher, data);
        hash!(next_exp!(parents, 12), hasher, data);
    }

    let hash = hasher.finalize();
    bytes_into_fr_repr_safe(hash.as_ref()).into()
}

#[inline(always)]
fn data_at_node_offset(v: usize) -> usize {
    v * NODE_SIZE
}
