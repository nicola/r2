extern crate r2;

use blake2s_simd::Params as Blake2s;
use r2::{commit, file_backed_mmap_from_zeroes, graph, replicate};
use r2::{BASE_PARENTS, EXP_PARENTS, LAYERS, NODES, REPLICA_ID_SIZE};
use storage_proofs::drgraph::new_seed;
use storage_proofs::hasher::{Blake2sHasher, PedersenHasher};
use rand::{rngs::OsRng};

fn main() {
    // Load the graph from memory or generate a new one
    let gg = graph::Graph::new_cached(NODES, BASE_PARENTS, EXP_PARENTS, new_seed());

    // Generate a file full of zeroes to be replicated
    println!("Generating CommD");
    let mut original_data = file_backed_mmap_from_zeroes(NODES, 1, false, "data");
    let tree_d = commit::single::<PedersenHasher>(&mut original_data, 0).expect("fail to commD");
    let comm_d = tree_d.root();
    println!("CommD is: {:02x?}", &comm_d);

    // Compute replica_id
    println!("Generating ReplicaId");
    let mut replica_id_hasher = Blake2s::new().hash_length(REPLICA_ID_SIZE).to_state();
    let miner_id = hex::decode("0000").expect("invalid hex for minerId");
    let seed = hex::decode("0000").expect("invalid hex for seed");
    replica_id_hasher.update(miner_id.as_ref());
    replica_id_hasher.update(comm_d.as_ref());
    replica_id_hasher.update(seed.as_ref());
    let replica_id = replica_id_hasher.finalize();
    println!("ReplicaId is: {:02x?}", &replica_id);

    // Start replication
    println!("Starting replication");
    let mut stack = file_backed_mmap_from_zeroes(NODES, LAYERS, false, "stack");
    replicate::r2::<Blake2sHasher>(replica_id.as_ref(), &original_data, &mut stack, &gg);

    println!("Generating CommR");
    let (comm_r, _tree_rl, _tree_c) = commit::commit::<PedersenHasher>(&stack);

    println!("CommR is: {:02x?}", &comm_r)
}
