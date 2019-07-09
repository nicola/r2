use ff::Field;
use paired::bls12_381::Fr;
use storage_proofs::error::Result;
use storage_proofs::hasher::{Domain, Hasher};
use storage_proofs::util::data_at_node_offset;
use storage_proofs::vde::create_key;

use crate::graph;
use crate::LAYERS;
use crate::NODE_SIZE;

pub fn r2<'a, H>(replica_id: &'a H::Domain, data: &'a mut [u8], g: &'a graph::Graph)
where
    H: Hasher,
{
    for l in 0..LAYERS {
        println!("Replica {} starting", l);
        let replica = r::<H>(g, replica_id, l, data);
        println!("Replica {} done", l);
        if let Ok(_) = replica {
            println!("replica is correct!");
        }
    }
}

pub fn r<'a, H>(
    graph: &'a graph::Graph,
    replica_id: &'a H::Domain,
    layer: usize,
    data: &'a mut [u8],
) -> Result<()>
where
    H: Hasher,
{
    let mut parents = vec![0; graph.degree()];
    for n in 0..graph.nodes {
        let node = if layer % 2 == 0 {
            n
        } else {
            (graph.nodes - n) - 1
        };
        graph.parents(node, layer, &mut parents);

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
