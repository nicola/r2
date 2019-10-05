use crate::NODES;
use rand::{ChaChaRng, Rng, SeedableRng};
use serde::{Deserialize, Serialize};
use serde_json;
use std::cmp;
use std::fs::metadata;
use std::fs::File;
use std::io::prelude::*;
use std::io::Write;
use storage_proofs::crypto::feistel;

/// A Graph holds settings and cache
#[derive(Serialize, Deserialize)]
pub struct Graph {
    pub nodes: usize,
    base_degree: usize,
    expansion_degree: usize,
    seed: [u32; 7],
    pub bas: Vec<Vec<usize>>,
    pub exp: Vec<Vec<usize>>,
}

/// Given a node and a graph, find the parents of a node DRG graph
fn bucketsample_parents(g: &Graph, node: usize) -> Vec<usize> {
    let m = g.base_degree;
    let mut parents = [0; 5];

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
            seed[0..7].copy_from_slice(&g.seed);
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

    parents.to_vec()
}

/// Given a node and a graph (and feistel settings) generate the expander
/// graph parents on a node in a layer in ZigZag.
fn expander_parents(
    g: &Graph,
    node: usize,
    feistel_precomputed: feistel::FeistelPrecomputed,
) -> Vec<usize> {
    // Set the Feistel permutation keys
    let feistel_keys = &[1, 2, 3, 4];

    // The expander graph parents are calculated by computing 3 rounds of the
    // feistel permutation on the current node
    let parents: Vec<usize> = (0..g.expansion_degree)
        .filter_map(|i| {
            let parent = feistel::permute(
                (g.nodes * g.expansion_degree) as feistel::Index,
                (node * g.expansion_degree + i) as feistel::Index,
                feistel_keys,
                feistel_precomputed,
            ) as usize
                / g.expansion_degree;
            if parent < node {
                Some(parent)
            } else {
                None
            }
        })
        .collect();
    parents
}

impl Graph {
    /// Create a graph
    pub fn new(nodes: usize, base_degree: usize, expansion_degree: usize, seed: [u32; 7]) -> Self {
        Graph {
            nodes,
            base_degree,
            expansion_degree,
            seed,
            exp: vec![vec![]; nodes],
            bas: vec![vec![]; nodes],
        }
    }
    // Create a graph, generate its parents and cache them.
    // Parents are cached in a JSON file
    pub fn new_cached(
        nodes: usize,
        base_degree: usize,
        expander_parents: usize,
        seed: [u32; 7],
    ) -> Graph {
        let cache = format!("g_{}mb.json", NODES * 32 / 1024 / 1024);
        if let Err(_) = metadata(&cache) {
            println!("Parents not cached, creating them");
            let mut gg = Graph::new(nodes, base_degree, expander_parents, seed);
            gg.gen_parents_cache();
            let mut f = File::create(&cache).expect("Unable to create file");
            let j = serde_json::to_string(&gg).expect("unable to create json");
            write!(f, "{}\n", j).expect("Unable to write file");

            gg
        } else {
            println!("Parents are cached, loading them");
            let mut f = File::open(&cache).expect("Unable to open the file");
            let mut json = String::new();
            f.read_to_string(&mut json)
                .expect("Unable to read the file");
            let gg = serde_json::from_str::<Graph>(&json).expect("unable to parse json");
            gg
        }
    }

    /// Load the parents of a node from cache
    pub fn parents(&self, node: usize, parents: &mut [usize]) {
        // DRG Parents
        let base_parents = &self.bas[node];
        let base_parents_len = base_parents.len();
        // Copying base parents
        parents[0..base_parents.len()].copy_from_slice(base_parents);

        // Expander parents
        let exp_parents = &self.exp[node];
        let exp_parents_len = exp_parents.len();
        parents[base_parents_len..base_parents_len + exp_parents_len].copy_from_slice(exp_parents);

        // Adding needed padding only
        for i in base_parents_len + exp_parents_len..self.degree() {
            parents[i] = 0;
        }
    }

    pub fn gen_parents_cache(&mut self) {
        let fp = feistel::precompute((self.expansion_degree * self.nodes) as feistel::Index);

        // Cache only forward DRG and Expander parents
        for node in 0..self.nodes {
            self.bas[node] = bucketsample_parents(&self, node);
            self.exp[node] = expander_parents(&self, node, fp);
        }
    }

    pub fn degree(&self) -> usize {
        self.base_degree + self.expansion_degree
    }
}
