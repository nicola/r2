use ff::Field;
use ff::PrimeField;
use paired::bls12_381::Fr;

use blake2s_simd::Params as Blake2s;
use storage_proofs::error::Result;
use storage_proofs::fr32::bytes_into_fr_repr_safe;
use storage_proofs::hasher::Domain;
use storage_proofs::hasher::Hasher;

use crate::{data_at_node, data_at_node_offset, graph};
use crate::{LAYERS, NODES, NODE_SIZE};

/// Generates an SDR replicated sector
pub fn r2<'a, H>(replica_id: &'a [u8], data: &'a [u8], stack: &'a mut [u8], g: &'a graph::Graph)
where
    H: Hasher,
{
    // Generate a replica at each layer
    for l in 0..LAYERS {
        println!("Replica {} starting", l);
        let replica = r::<H>(g, replica_id, l, stack);
    }

    for i in 0..NODES {
        let raw_node = data_at_node(&data, 0, i);
        let raw_fr: Fr = Fr::from_repr(bytes_into_fr_repr_safe(&raw_node)).expect("failed");
        let mut stack_node = data_at_node(&stack, LAYERS - 1, i);
        let mut stack_fr: Fr = Fr::from_repr(bytes_into_fr_repr_safe(&stack_node)).expect("failed");
        stack_fr.add_assign(&raw_fr);

        let encoded: H::Domain = stack_fr.into();
        let (start, end) = data_at_node_offset(LAYERS - 1, i);
        encoded.write_bytes(&mut stack[start..end]).expect("failed");
    }
}

/// Encoding of a single layer
pub fn r<'a, H>(
    graph: &'a graph::Graph,
    replica_id: &'a [u8],
    layer: usize,
    data: &'a mut [u8],
) -> Result<()>
where
    H: Hasher,
{
    // Optimization
    // instead of allocating a new vector memory every time, re-use this one
    let mut parents = vec![0; graph.degree()];

    for node in 0..graph.nodes {
        // Get the `parents`
        graph::Graph::parents(&graph, node, &mut parents);

        // Compute `label` from `parents`
        let mut hasher = Blake2s::new().hash_length(NODE_SIZE).to_state();
        hasher.update(replica_id.as_ref());
        for parent in parents.iter() {
            hasher.update(data_at_node(&data, layer, *parent));
        }
        let label = hasher.finalize();

        // Store the `encoded` label
        let (start, end) = data_at_node_offset(layer, node);
        data[start..end].copy_from_slice(label.as_ref());
    }

    Ok(())
}
