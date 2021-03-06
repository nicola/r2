use ff::Field;
use ff::PrimeField;
use paired::bls12_381::Fr;

// use blake2s_simd::Params as Blake2s;
use storage_proofs::error::Result;
use storage_proofs::fr32::bytes_into_fr_repr_safe;
use storage_proofs::hasher::Domain;
use storage_proofs::hasher::Hasher;
use sha2::{Digest, Sha256};

use crate::{data_at_node, data_at_node_offset, graph};
use crate::{LAYERS, NODES};

/// Generates an SDR replicated sector
pub fn r2<'a, H>(replica_id: &'a [u8], data: &'a [u8], stack: &'a mut [u8], g: &'a graph::Graph)
where
    H: Hasher,
{
    // Generate a replica at each layer
    for l in 0..LAYERS {
        println!("Replica {} starting", l);
        r::<H>(g, replica_id, l, stack).expect("some layers failed replicating");
    }

    for i in 0..NODES {
        let raw_node = data_at_node(&data, 0, i);
        let raw_fr: Fr = Fr::from_repr(bytes_into_fr_repr_safe(&raw_node)).expect("failed");
        let stack_node = data_at_node(&stack, LAYERS - 1, i);
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

    // Precompute first part of the hash used to hash the parents
    let mut base_hasher = Sha256::new();
    base_hasher.input(AsRef::<[u8]>::as_ref(replica_id));

    // On layer 0, only use DRG parents
    let get_parents = if layer == 0 {
        graph::Graph::parents_drg
    } else {
        graph::Graph::parents
    };

    for node in 0..NODES {
        // Get the `parents`
        get_parents(&graph, node, &mut parents);

        // Compute `label` from `parents`
        let mut hasher = base_hasher.clone();
        // prefix it with node id
        hasher.input(&(node as u64).to_be_bytes());

        for parent in parents.iter() {
            hasher.input(data_at_node(&data, layer, *parent));
        }
        let label = hasher.result();

        // Store the `encoded` label
        let (start, end) = data_at_node_offset(layer, node);
        data[start..end].copy_from_slice(label.as_ref());
    }

    Ok(())
}
