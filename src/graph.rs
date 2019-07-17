use std::cmp;
use std::fs::metadata;
use std::fs::File;
use std::io::prelude::*;
use std::io::Write;
use std::sync::Arc;

use blake2s_simd::{Params as Blake2s, State};
use rand::{ChaChaRng, Rng, SeedableRng};
use serde::{Deserialize, Serialize};
use serde_json;
use storage_proofs::crypto::feistel;
use storage_proofs::fr32::bytes_into_fr_repr_safe;
use storage_proofs::hasher::{Domain, Hasher};
use storage_proofs::util::data_at_node_offset;

use crate::{
    next_base, next_base_rev, next_exp, BASE_PARENTS, EXP_PARENTS, NODES, NODE_SIZE, PARENT_SIZE,
    SEED,
};

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
            let mut f = File::open("g.json").expect("Unable to open the file");
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

#[derive(Clone)]
pub struct ParentsIter {
    graph: Arc<Graph>,
    node: usize,
}

#[derive(Clone)]
pub struct ParentsIterRev {
    graph: Arc<Graph>,
    node: usize,
}

impl ParentsIterRev {
    pub fn new(graph: Arc<Graph>, node: usize) -> Self {
        ParentsIterRev { graph, node }
    }

    #[inline]
    pub fn base_parents(&self) -> &[usize] {
        &self.graph.bas[self.node][..]
    }
    #[inline]
    pub fn exp_parents(&self) -> &[usize] {
        &self.graph.exp_reversed[self.node][..]
    }
}

pub trait Parents {
    fn get_all(&self, node: usize) -> [usize; 14];
}

impl Parents for ParentsIterRev {
    fn get_all(&self, node: usize) -> [usize; 14] {
        [
            node,
            next_base_rev!(self, 0),
            next_base_rev!(self, 1),
            next_base_rev!(self, 2),
            next_base_rev!(self, 3),
            next_base_rev!(self, 4),
            next_exp!(self, 5),
            next_exp!(self, 6),
            next_exp!(self, 7),
            next_exp!(self, 8),
            next_exp!(self, 9),
            next_exp!(self, 10),
            next_exp!(self, 11),
            next_exp!(self, 12),
        ]
    }
}

impl ParentsIter {
    pub fn new(graph: Arc<Graph>, node: usize) -> Self {
        ParentsIter { node, graph }
    }

    #[inline]
    pub fn base_parents(&self) -> &[usize] {
        &self.graph.bas[self.node][..]
    }
    #[inline]
    pub fn exp_parents(&self) -> &[usize] {
        &self.graph.exp[self.node][..]
    }
}

impl Parents for ParentsIter {
    fn get_all(&self, node: usize) -> [usize; 14] {
        [
            node,
            next_base!(self, 0),
            next_base!(self, 1),
            next_base!(self, 2),
            next_base!(self, 3),
            next_base!(self, 4),
            next_exp!(self, 5),
            next_exp!(self, 6),
            next_exp!(self, 7),
            next_exp!(self, 8),
            next_exp!(self, 9),
            next_exp!(self, 10),
            next_exp!(self, 11),
            next_exp!(self, 12),
        ]
    }
}
