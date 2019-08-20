extern crate r2;
use r2::{file_backed_mmap_from_zeroes, graph, id_from_str, replicate, NODES};
use gperftools::profiler::PROFILER;
use storage_proofs::hasher::{Blake2sHasher, Hasher};

fn start_profile(stage: &str) {
    PROFILER
        .lock()
        .unwrap()
        .start(format!("./{}.profile", stage))
        .unwrap();
}

fn stop_profile() {
   PROFILER.lock().unwrap().stop().unwrap();
}

fn main() {
    // Load the graph from memory or generate a new one
    let gg = graph::Graph::new_cached();
    // Compute replica_id
    let replica_id = id_from_str::<<Blake2sHasher as Hasher>::Domain>("aaaa");
    // Generate a file full of zeroes to be replicated
    let mut data = file_backed_mmap_from_zeroes(NODES, true);
    // Start replication
    println!("Starting replication");
    start_profile("replicate");
    replicate::r2::<Blake2sHasher>(&replica_id, &mut data, &gg);
    stop_profile();
}
