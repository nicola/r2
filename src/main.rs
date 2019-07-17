#![feature(async_await)]

use r2::{create_empty_file, graph, id_from_str, replicate, AsyncData, NODES, NODE_SIZE};
use storage_proofs::hasher::{Blake2sHasher, Hasher};

#[tokio::main]
pub async fn main() -> Result<(), failure::Error> {
    // Load the graph from memory or generate a new one
    // TODO: make the graph not be in memory as well
    let gg = graph::Graph::new_cached();

    // Compute replica_id
    let replica_id = id_from_str::<<Blake2sHasher as Hasher>::Domain>("aaaa");
    let file_path = "/tmp/replicate.data";

    // Create an empty file to replicate.
    create_empty_file(file_path.clone(), NODES * NODE_SIZE).await?;

    // Create the construct that allows us to do the prefetching.
    let mut data = AsyncData::new(file_path.clone()).await?;

    // Start replication
    println!("Starting replication");

    replicate::r2::<Blake2sHasher>(&replica_id, &mut data, &gg).await?;

    data.flush().await;
    Ok(())
}

// fn main() {
//     // Load the graph from memory or generate a new one
//     let gg = graph::Graph::new_cached();
//     // Compute replica_id
//     let replica_id = id_from_str::<<Blake2sHasher as Hasher>::Domain>("aaaa");
//     // Generate a file full of zeroes to be replicated
//     let mut data = file_backed_mmap_from_zeroes(NODES, true);
//     // Start replication
//     println!("Starting replication");
//     replicate::r2::<Blake2sHasher>(&replica_id, &mut data, &gg)
// }
