use std::cmp;
use std::fs::metadata;
use std::fs::File;
use std::io::prelude::*;
use std::io::Write;

use rand::{ChaChaRng, Rng, SeedableRng};
use serde::{Deserialize, Serialize};
use serde_json;
use storage_proofs::crypto::feistel;

use crate::{BASE_PARENTS, EXP_PARENTS, NODES, SEED};

/// A Graph holds settings and cache
#[derive(Serialize, Deserialize)]
pub struct Graph {
    pub bas: Vec<[usize; BASE_PARENTS]>,
    pub exp: Vec<[usize; EXP_PARENTS]>,
    pub exp_reversed: Vec<[usize; EXP_PARENTS]>,
}

/// Given a node and a graph, find the parents of a node DRG graph
fn bucketsample_parents(node: usize) -> [usize; BASE_PARENTS] {
    let m = BASE_PARENTS;
    let mut parents = [0; BASE_PARENTS];

    match node {
        // Special case for the first node, it self references.
        // Special case for the second node, it references only the first one.
        0 | 1 => {
            // Use the degree of the curren graph (`m`), as parents.len() might be bigger
            // than that (that's the case for ZigZag Graph)
            for parent in parents.iter_mut().take(m) {
                *parent = 0;
            }
        }
        _ => {
            // seed = self.seed | node
            let mut seed = [0u32; 8];
            seed[0..7].copy_from_slice(&SEED[..]);
            seed[7] = node as u32;
            let mut rng = ChaChaRng::from_seed(&seed);

            for (k, parent) in parents.iter_mut().take(m).enumerate() {
                // iterate over m meta nodes of the ith real node
                // simulate the edges that we would add from previous graph nodes
                // if any edge is added from a meta node of jth real node then add edge (j,i)
                let logi = ((node * m) as f32).log2().floor() as usize;
                let j = rng.gen::<usize>() % logi;
                let jj = cmp::min(node * m + k, 1 << (j + 1));
                let back_dist = rng.gen_range(cmp::max(jj >> 1, 2), jj + 1);
                let out = (node * m + k - back_dist) / m;

                // remove self references and replace with reference to previous node
                if out == node {
                    *parent = node - 1;
                } else {
                    assert!(out <= node);
                    *parent = out;
                }
            }

            // Use the degree of the curren graph (`m`), as parents.len() might be bigger
            // than that (that's the case for ZigZag Graph)
            parents[0..m].sort_unstable();
        }
    }

    parents
}

/// Given a node and a graph (and feistel settings) generate the expander
/// graph parents on a node in a layer in ZigZag.
fn expander_parents(
    node: usize,
    feistel_precomputed: feistel::FeistelPrecomputed,
) -> [usize; EXP_PARENTS] {
    // Set the Feistel permutation keys
    let feistel_keys = &[1, 2, 3, 4];

    let mut parents = [0; EXP_PARENTS];
    // The expander graph parents are calculated by computing 3 rounds of the
    // feistel permutation on the current node
    for (i, p) in (0..EXP_PARENTS).filter_map(|i| {
        let parent = feistel::permute(
            (NODES * EXP_PARENTS) as feistel::Index,
            (node * EXP_PARENTS + i) as feistel::Index,
            feistel_keys,
            feistel_precomputed,
        ) as usize
            / EXP_PARENTS;
        if parent < node {
            Some((i, parent))
        } else {
            None
        }
    }) {
        parents[i] = p;
    }

    parents
}

impl Graph {
    /// Create a graph
    pub fn new() -> Self {
        Graph {
            exp: vec![[0; EXP_PARENTS]; NODES],
            bas: vec![[0; BASE_PARENTS]; NODES],
            exp_reversed: vec![[0; EXP_PARENTS]; NODES],
        }
    }
    // Create a graph, generate its parents and cache them.
    // Parents are cached in a JSON file
    pub fn new_cached() -> Graph {
        let cache = format!("g_{}mb.json", NODES * 32 / 1024 / 1024);
        if let Err(_) = metadata(&cache) {
            println!("Parents not cached, creating {}", &cache);
            let mut gg = Graph::new();
            gg.gen_parents_cache();
            let mut f = File::create(&cache).expect("Unable to create file");
            let j = serde_json::to_string(&gg).expect("unable to create json");
            write!(f, "{}\n", j).expect("Unable to write file");

            gg
        } else {
            println!("Parents are cached, loading them");
            let cache = format!("g_{}mb.json", NODES * 32 / 1024 / 1024);
            let mut f = File::open(&cache).expect("Unable to open the file");
            let mut json = String::new();
            f.read_to_string(&mut json)
                .expect("Unable to read the file");
            let gg = serde_json::from_str::<Graph>(&json).expect("unable to parse json");
            gg
        }
    }

    pub fn gen_parents_cache(&mut self) {
        let fp = feistel::precompute((EXP_PARENTS * NODES) as feistel::Index);

        // Cache only forward DRG and Expander parents
        for node in 0..NODES {
            self.bas[node] = bucketsample_parents(node);
            self.exp[node] = expander_parents(node, fp);
        }

        // Cache reverse edges for exp
        for (n1, v) in self.exp.iter().enumerate() {
            let mut i = 0;
            for n2 in v {
                self.exp_reversed[*n2][i] = n1;
                i += 1;
            }
        }

        // TODO: sort parents
    }
}

pub struct ParentsIter<'a> {
    pub base_parents: &'a [usize],
    pub exp_parents: &'a [usize],
}

pub struct ParentsIterRev<'a> {
    pub base_parents: &'a [usize],
    pub exp_parents: &'a [usize],
}

impl<'a> ParentsIterRev<'a> {
    pub fn new(graph: &'a Graph, node: usize) -> Self {
        let base_parents = &graph.bas[NODES - node - 1];
        let exp_parents = &graph.exp_reversed[node];

        ParentsIterRev {
            base_parents,
            exp_parents,
        }
    }
}

impl<'a> ParentsIter<'a> {
    pub fn new(graph: &'a Graph, node: usize) -> Self {
        let base_parents = &graph.bas[node];
        let exp_parents = &graph.exp[node];

        ParentsIter {
            base_parents,
            exp_parents,
        }
    }
}
