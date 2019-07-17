use std::time::{Duration, Instant};

use blake2s_simd::{Params as Blake2s, State};
use ff::Field;
use paired::bls12_381::Fr;
use storage_proofs::fr32::bytes_into_fr_repr_safe;
use storage_proofs::hasher::{Domain, Hasher};

#[cfg(feature = "profile")]
use gperftools::profiler::PROFILER;

#[cfg(feature = "profile")]
#[inline(always)]
fn start_profile(stage: &str) {
    PROFILER
        .lock()
        .unwrap()
        .start(format!("./{}.profile", stage))
        .unwrap();
}

#[cfg(not(feature = "profile"))]
#[inline(always)]
fn start_profile(_stage: &str) {}

#[cfg(feature = "profile")]
#[inline(always)]
fn stop_profile() {
    PROFILER.lock().unwrap().stop().unwrap();
}

#[cfg(not(feature = "profile"))]
#[inline(always)]
fn stop_profile() {}

use crate::graph::{Graph, ParentsIter, ParentsIterRev};
use crate::{next_base, next_base_rev, next_exp, AsyncData, BASE_PARENTS, NODES, NODE_SIZE};

macro_rules! replicate_layer {
    ($graph:expr, $replica_id:expr, $layer:expr, $data:expr) => {
        print!("Replicating layer {}", $layer);
        let start = Instant::now();

        let mut hasher = Blake2s::new().hash_length(NODE_SIZE).to_state();
        hasher.update($replica_id.as_ref());

        let mut key_dur = Duration::new(0, 0);
        let mut write_time = Duration::new(0, 0);

        start_profile("layer");

        for node in 0..NODES {
            let parents = ParentsIter::new($graph.clone(), node);
            $data.prefetch(node, parents.clone());

            let start = Instant::now();
            // Compute `key` from `parents`
            let key = create_key::<H>(&parents, node, $data, hasher.clone()).await;
            key_dur += start.elapsed();

            // Get the `unencoded` node
            let raw_node_data = $data.get_node(node).await;
            let node_data = H::Domain::try_from_bytes(raw_node_data).unwrap();
            let mut node_fr: Fr = node_data.into();

            // Compute the `encoded` node by adding the `key` to it
            node_fr.add_assign(&key.into());
            let encoded: H::Domain = node_fr.into();

            let start = Instant::now();
            // Store the `encoded` data
            let node_mut = $data.get_node_mut(node).await;
            encoded.write_bytes(node_mut).unwrap();
            $data.write_node(node).await;
            write_time += start.elapsed();
        }
        stop_profile();
        println!(" ... took {:0.4}ms", start.elapsed().as_millis());
        println!("  key: {:0.4}ms", key_dur.as_millis());
        println!("  write: {:0.4}ms", write_time.as_millis());
    };
}

macro_rules! replicate_layer_rev {
    ($graph:expr, $replica_id:expr, $layer:expr, $data:expr) => {
        print!("Replicating layer {}", $layer);
        let start = Instant::now();

        let mut hasher = Blake2s::new().hash_length(NODE_SIZE).to_state();
        hasher.update($replica_id.as_ref());

        for node in 0..NODES {
            // TODO: use rev iter again
            let parents = ParentsIterRev::new($graph.clone(), node);
            $data.prefetch_rev(node, parents.clone());

            // Compute `key` from `parents`
            // TODO: use rev again
            let key = create_key_rev::<H>(&parents, node, $data, hasher.clone()).await;

            // Get the `unencoded` node
            let raw_node_data = $data.get_node(node).await;
            let node_data = H::Domain::try_from_bytes(raw_node_data).unwrap();
            let mut node_fr: Fr = node_data.into();

            // Compute the `encoded` node by adding the `key` to it
            node_fr.add_assign(&key.into());
            let encoded: H::Domain = node_fr.into();

            // Store the `encoded` data
            let node_mut = $data.get_node_mut(node).await;
            encoded.write_bytes(node_mut).unwrap();
            $data.write_node(node).await;
        }

        println!(" ... took {:0.4}ms", start.elapsed().as_millis());
    };
}

/// Generates a ZigZag replicated sector.
#[inline(never)]
pub async fn r2<H>(
    replica_id: H::Domain,
    data: &mut AsyncData,
    g: Graph,
) -> Result<(), failure::Error>
where
    H: Hasher,
{
    use std::sync::Arc;
    let g = Arc::new(g);

    // Generate a replica at each layer of the 10 layers
    replicate_layer!(g, replica_id, 0, data);
    // replicate_layer_rev!(g, replica_id, 1, data);

    // replicate_layer!(g, replica_id, 2, data);
    // replicate_layer_rev!(g, replica_id, 3, data);

    // replicate_layer!(g, replica_id, 4, data);
    // replicate_layer_rev!(g, replica_id, 5, data);

    // replicate_layer!(g, replica_id, 6, data);
    // replicate_layer_rev!(g, replica_id, 7, data);

    // replicate_layer!(g, replica_id, 8, data);
    // replicate_layer_rev!(g, replica_id, 9, data);

    Ok(())
}

macro_rules! hash {
    ($parent:expr, $hasher:expr, $data:expr) => {
        $hasher.update($data.get_node($parent).await);
    };
}

async fn create_key<'a, H: Hasher>(
    parents: &'a ParentsIter,
    node: usize,
    data: &'a mut AsyncData,
    mut hasher: State,
) -> H::Domain {
    // compile time fixed at 5 + 8 = 13 parents

    // The hash is about the parents, hence skip if a node doesn't have any parents
    let p0 = next_base!(parents, 0);
    if node != p0 {
        // hash first parent
        hasher.update(data.get_node(p0).await);

        // base parents
        hash!(next_base!(parents, 1), hasher, data);
        hash!(next_base!(parents, 2), hasher, data);
        hash!(next_base!(parents, 3), hasher, data);
        hash!(next_base!(parents, 4), hasher, data);

        // exp parents
        hash!(next_exp!(parents, 5), hasher, data);
        hash!(next_exp!(parents, 6), hasher, data);
        hash!(next_exp!(parents, 7), hasher, data);
        hash!(next_exp!(parents, 8), hasher, data);
        hash!(next_exp!(parents, 9), hasher, data);
        hash!(next_exp!(parents, 10), hasher, data);
        hash!(next_exp!(parents, 11), hasher, data);
        hash!(next_exp!(parents, 12), hasher, data);
    }

    let hash = hasher.finalize();
    bytes_into_fr_repr_safe(hash.as_ref()).into()
}

async fn create_key_rev<'a, H: Hasher>(
    parents: &'a ParentsIterRev,
    node: usize,
    data: &'a mut AsyncData,
    mut hasher: State,
) -> H::Domain {
    // compile time fixed at 5 + 8 = 13 parents

    // The hash is about the parents, hence skip if a node doesn't have any parents
    let p0 = next_base_rev!(parents, 0);
    if node != p0 {
        // hash first parent
        hasher.update(data.get_node(p0).await);

        // base parents
        hash!(next_base_rev!(parents, 1), hasher, data);
        hash!(next_base_rev!(parents, 2), hasher, data);
        hash!(next_base_rev!(parents, 3), hasher, data);
        hash!(next_base_rev!(parents, 4), hasher, data);

        // exp parents
        hash!(next_exp!(parents, 5), hasher, data);
        hash!(next_exp!(parents, 6), hasher, data);
        hash!(next_exp!(parents, 7), hasher, data);
        hash!(next_exp!(parents, 8), hasher, data);
        hash!(next_exp!(parents, 9), hasher, data);
        hash!(next_exp!(parents, 10), hasher, data);
        hash!(next_exp!(parents, 11), hasher, data);
        hash!(next_exp!(parents, 12), hasher, data);
    }

    let hash = hasher.finalize();
    bytes_into_fr_repr_safe(hash.as_ref()).into()
}
