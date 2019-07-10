use blake2s_simd::Params as Blake2s;
use ff::Field;
use paired::bls12_381::Fr;
use storage_proofs::error::Result;
use storage_proofs::fr32::bytes_into_fr_repr_safe;
use storage_proofs::hasher::{Domain, Hasher};
use storage_proofs::util::data_at_node_offset;

use crate::graph;
use crate::LAYERS;
use crate::NODE_SIZE;

/// Generates a ZigZag replicated sector
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
    // instead of checking the parity of the layer per node,
    // check that per layer.

    if layer % 2 == 0 {
        r_even::<H>(graph, replica_id, data)
    } else {
        r_odd::<H>(graph, replica_id, data)
    }
}

pub fn r_even<'a, H>(
    graph: &'a graph::Graph,
    replica_id: &'a H::Domain,
    data: &'a mut [u8],
) -> Result<()>
where
    H: Hasher,
{
    for node in 0..graph.nodes {
        let parents = graph::Graph::parents_even(graph, node);
        // Compute `key` from `parents`
        let key = create_key::<H, _>(replica_id, node, parents, data)?;

        // Get the `unencoded` node
        let start = data_at_node_offset(node);
        let end = start + NODE_SIZE;
        let node_data = H::Domain::try_from_bytes(&data[start..end])?;
        let mut node_fr: Fr = node_data.into();

        // Compute the `encoded` node by adding the `key` to it
        node_fr.add_assign(&key.into());
        let encoded: H::Domain = node_fr.into();

        // Store the `encoded` data
        encoded.write_bytes(&mut data[start..end])?;
    }

    Ok(())
}

pub fn r_odd<'a, H>(
    graph: &'a graph::Graph,
    replica_id: &'a H::Domain,
    data: &'a mut [u8],
) -> Result<()>
where
    H: Hasher,
{
    for node in 0..graph.nodes {
        let parents = graph::Graph::parents_odd(graph, node);
        // Compute `key` from `parents`
        let key = create_key::<H, _>(replica_id, node, parents, data)?;

        // Get the `unencoded` node
        let start = data_at_node_offset(node);
        let end = start + NODE_SIZE;
        let node_data = H::Domain::try_from_bytes(&data[start..end])?;
        let mut node_fr: Fr = node_data.into();

        // Compute the `encoded` node by adding the `key` to it
        node_fr.add_assign(&key.into());
        let encoded: H::Domain = node_fr.into();

        // Store the `encoded` data
        encoded.write_bytes(&mut data[start..end])?;
    }

    Ok(())
}

#[inline]
pub fn create_key<H: Hasher, I>(
    id: &H::Domain,
    node: usize,
    parents: I,
    data: &[u8],
) -> Result<H::Domain>
where
    I: IntoIterator<Item = usize>,
{
    let mut hasher = Blake2s::new().hash_length(NODE_SIZE).to_state();
    hasher.update(id.as_ref());

    let mut parents = parents.into_iter();

    // The hash is about the parents, hence skip if a node doesn't have any parents
    if node != parents.next().unwrap() {
        for node in parents {
            let offset = data_at_node_offset(node);
            hasher.update(&data[offset..offset + NODE_SIZE]);
        }
    }

    let hash = hasher.finalize();
    Ok(bytes_into_fr_repr_safe(hash.as_ref()).into())
}
