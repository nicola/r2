use blake2s_simd::Params as Blake2s;
use ff::Field;
use paired::bls12_381::Fr;
use storage_proofs::error::Result;
use storage_proofs::hasher::{Domain, Hasher};

use crate::data_at_node_offset;
use crate::graph;
use crate::DATA_SIZE;
use crate::LAYERS;
use crate::NODE_SIZE;

/// Generates an SDR replicated sector
pub fn r2<'a, H>(replica_id: &'a H::Domain, data: &'a mut [u8], g: &'a graph::Graph)
where
    H: Hasher,
{
    // Generate a replica at each layer
    for l in 0..LAYERS {
        println!("Replica {} starting", l);
        let replica = r::<H>(g, replica_id, l, data);
        println!("Replica {} done", l);

        if let Ok(_) = replica {
            println!("replica is correct!");
        }
    }
}

/// Encoding of a single layer
pub fn r<'a, H>(
    graph: &'a graph::Graph,
    replica_id: &'a H::Domain,
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
            let offset = data_at_node_offset(layer, *parent);
            hasher.update(&data[offset..offset + NODE_SIZE]);
        }
        let label = hasher.finalize();

        // Store the `encoded` label
        let start = data_at_node_offset(layer, node);
        let end = start + NODE_SIZE;
        data[start..end].copy_from_slice(label.as_ref());
    }

    Ok(())
}
