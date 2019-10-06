extern crate r2;

use r2::{
    commit, file_backed_mmap_from_zeroes, graph, id_from_str, replicate, BASE_PARENTS, EXP_PARENTS,
    LAYERS, NODES,
};

use storage_proofs::drgraph::new_seed;
use storage_proofs::hasher::{Blake2sHasher, Hasher, PedersenHasher};

fn main() {
    // Load the graph from memory or generate a new one
    let gg = graph::Graph::new_cached(NODES, BASE_PARENTS, EXP_PARENTS, new_seed());

    // Compute replica_id
    let replica_id = id_from_str::<<Blake2sHasher as Hasher>::Domain>("aaaa");

    // Generate a file full of zeroes to be replicated
    println!("Generating CommD");
    let mut original_data = file_backed_mmap_from_zeroes(NODES, 1, false, "data");
    let tree_d = commit::single::<PedersenHasher>(&mut original_data, 0);

    // Start replication
    let mut replica = file_backed_mmap_from_zeroes(NODES, LAYERS, false, "replica");
    println!("Starting replication");
    replicate::r2::<Blake2sHasher>(&replica_id, &mut replica, &gg);

    // Generate CommR
    println!("Generating CommC");
    let tree_c = commit::columns::<PedersenHasher>(&mut replica).expect("t_c failed");
    println!("Generating CommRlast");
    let tree_rl = commit::single::<PedersenHasher>(&mut replica, LAYERS - 1).expect("t_rl failed");

    println!("Generating CommR");
    let comm_r = commit::r(tree_c.root(), tree_rl.root());
}
