extern crate r2;
use sha2::{Digest, Sha256};
use r2::{commit, file_backed_mmap_from_zeroes, graph, replicate};
use r2::{BASE_PARENTS, EXP_PARENTS, LAYERS, NODES, REPLICA_ID_SIZE};
use storage_proofs::drgraph::new_seed;
use storage_proofs::fr32::trim_bytes_to_fr_safe;
use storage_proofs::hasher::{PedersenHasher, Sha256Hasher};
use std::thread;
use std::sync::Arc;

fn main() {
    // Load the graph from memory or generate a new one
    let gg = Arc::new(graph::Graph::new_cached(NODES, BASE_PARENTS, EXP_PARENTS, new_seed()));

    let mut children = vec![];

    for i in 0..6 {
        let gg = gg.clone();
        let handle = thread::spawn(move || {
            println!("Generating CommD");
            // Generate a file full of zeroes to be replicated
            let mut original_data = file_backed_mmap_from_zeroes(NODES, 1, false, "data");
            let tree_d = commit::single::<Sha256Hasher>(&mut original_data, 0).expect("fail to commD");
            let comm_d = tree_d.root();
            println!("CommD is: {:02x?}", &comm_d);

            // Compute replica_id
            println!("Generating ReplicaId");
            let miner_id = hex::decode("0000").expect("invalid hex for minerId");
            let ticket = hex::decode("0000").expect("invalid hex for seed");
            let sector_id = i as u64;
            let replica_id_hash = Sha256::new()
                .chain(&miner_id)
                .chain(&sector_id.to_be_bytes()[..])
                .chain(ticket)
                .chain(AsRef::<[u8]>::as_ref(&comm_d))
                .result();
            let replica_id = trim_bytes_to_fr_safe(replica_id_hash.as_ref()).unwrap();

            println!("ReplicaId is: {:02x?}", &replica_id);

            // Start replication
            println!("Starting replication");
            let mut stack = file_backed_mmap_from_zeroes(NODES, LAYERS, false, "stack");
            replicate::r2::<Sha256Hasher>(&replica_id, &original_data, &mut stack, &gg);

            println!("Generating CommR");
            let (comm_r, _tree_rl, _tree_c) = commit::commit::<PedersenHasher>(&stack);

            println!("CommR is: {:02x?}", &comm_r)
        });
        children.push(handle);
    }

    for child in children {
        // Wait for the thread to finish. Returns a result.
        let _ = child.join();
    }

    println!("Done");
}
