#![feature(async_await)]

extern crate r2;
use r2::{file_backed_mmap_from_zeroes, graph, id_from_str, replicate, NODES};
use storage_proofs::hasher::{Blake2sHasher, Hasher};

#[runtime::main]
async fn main() {
    // Load the graph from memory or generate a new one
    let gg = graph::Graph::new_cached();
    // Compute replica_id
    let replica_id = id_from_str::<<Blake2sHasher as Hasher>::Domain>("aaaa");
    // Generate a file full of zeroes to be replicated
    let mut data = file_backed_mmap_from_zeroes(NODES, true);
    // Start replication
    println!("Starting replication");
    replicate::r2::<Blake2sHasher>(&replica_id, &mut data, &gg)
}
