extern crate r2;

use r2::commit::MerkleTree;
use r2::{
    commit, file_backed_mmap_from_zeroes, graph, id_from_str, replicate, BASE_PARENTS, EXP_PARENTS,
    NODES,
};

use storage_proofs::drgraph::new_seed;
use storage_proofs::hasher::{Blake2sHasher, Hasher, PedersenHasher};

fn main() {
    // Load the graph from memory or generate a new one
    let gg = graph::Graph::new_cached(NODES, BASE_PARENTS, EXP_PARENTS, new_seed());
    // Compute replica_id
    let replica_id = id_from_str::<<Blake2sHasher as Hasher>::Domain>("aaaa");
    // Generate a file full of zeroes to be replicated
    let mut data = file_backed_mmap_from_zeroes(NODES, false);
    // Start replication
    println!("Starting replication");
    replicate::r2::<Blake2sHasher>(&replica_id, &mut data, &gg);

    println!("Committing Merkle Tree");
    let mtree = commit::commit::<PedersenHasher>(&mut data, 0);
}
