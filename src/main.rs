extern crate r2;
use storage_proofs::drgraph::new_seed;
use r2::{NODES, BASE_PARENTS, EXP_PARENTS, file_backed_mmap_from_zeroes, replicate, id_from_str, graph,};
use storage_proofs::hasher::{Blake2sHasher, Hasher};

fn main() {
    // Load the graph from memory or generate a new one
    let gg = graph::Graph::new_cached(NODES, BASE_PARENTS, EXP_PARENTS, new_seed());
    // Compute replica_id
    let replica_id = id_from_str::<<Blake2sHasher as Hasher>::Domain>("aaaa");
    // Generate a file full of zeroes to be replicated
    let mut data = file_backed_mmap_from_zeroes(NODES, true);
    // Start replication
    println!("Starting replication");
    replicate::r2::<Blake2sHasher>(&replica_id, &mut data, &gg)
}
