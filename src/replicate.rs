use blake2s_simd::{Params as Blake2s, State};
use ff::Field;
use paired::bls12_381::Fr;
use storage_proofs::error::Result;
use storage_proofs::fr32::bytes_into_fr_repr_safe;
use storage_proofs::hasher::{Domain, Hasher};
use storage_proofs::util::data_at_node_offset;

use crate::graph::{Graph, Parents, ParentsIter, ParentsIterRev};
use crate::{LAYERS, NODES, NODE_SIZE};

macro_rules! replicate_layer {
    ($graph:expr, $replica_id:expr, $layer:expr, $data:expr, $parents:ty) => {
        println!("Replicating layer {}", $layer);

        let mut hasher = Blake2s::new().hash_length(NODE_SIZE).to_state();
        hasher.update($replica_id.as_ref());

        for node in 0..NODES {
            let parents = <$parents>::new($graph, node);
            // Compute `key` from `parents`
            let key = create_key::<H, _>(&parents, node, $data, hasher.clone());

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
    };
}

/// Generates a ZigZag replicated sector.
#[inline(never)]
pub fn r2<'a, H>(replica_id: &'a H::Domain, data: &'a mut [u8], g: &'a Graph)
where
    H: Hasher,
{
    // Generate a replica at each layer of the 10 layers

    replicate_layer!(g, replica_id, 0, data, ParentsIter);
    replicate_layer!(g, replica_id, 1, data, ParentsIterRev);
    replicate_layer!(g, replica_id, 2, data, ParentsIter);
    replicate_layer!(g, replica_id, 3, data, ParentsIterRev);
    replicate_layer!(g, replica_id, 4, data, ParentsIter);

    replicate_layer!(g, replica_id, 5, data, ParentsIterRev);
    replicate_layer!(g, replica_id, 6, data, ParentsIter);
    replicate_layer!(g, replica_id, 7, data, ParentsIterRev);
    replicate_layer!(g, replica_id, 8, data, ParentsIter);
    replicate_layer!(g, replica_id, 9, data, ParentsIterRev);
}

macro_rules! hash {
    ($parents:expr, $hasher:expr, $data:expr, $parent_id:expr) => {
        let parent = $parents.next($parent_id);
        let offset = data_at_node_offset(parent);
        $hasher.update(&$data[offset..offset + NODE_SIZE]);
    };
}

#[inline]
pub fn create_key<H: Hasher, I: Parents>(
    parents: &I,
    node: usize,
    data: &[u8],
    mut hasher: State,
) -> H::Domain {
    // compile time fixed at 5 + 8 = 13 parents

    // The hash is about the parents, hence skip if a node doesn't have any parents
    let p1 = parents.next(0);
    if node != p1 {
        // hash first parent
        let offset = data_at_node_offset(p1);
        hasher.update(&data[offset..offset + NODE_SIZE]);

        // hash other 12 parents
        hash!(parents, hasher, data, 1);
        hash!(parents, hasher, data, 2);
        hash!(parents, hasher, data, 3);
        hash!(parents, hasher, data, 4);
        hash!(parents, hasher, data, 5);

        hash!(parents, hasher, data, 6);
        hash!(parents, hasher, data, 7);
        hash!(parents, hasher, data, 8);
        hash!(parents, hasher, data, 9);
        hash!(parents, hasher, data, 10);

        hash!(parents, hasher, data, 12);
        hash!(parents, hasher, data, 11);
    }

    let hash = hasher.finalize();
    bytes_into_fr_repr_safe(hash.as_ref()).into()
}
