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
    pub exp_reversed: Vec<Vec<usize>>,
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
            exp_reversed: vec![vec![]; nodes],
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
        if let Err(_) = metadata("g.json") {
            println!("Parents not cached, creating them");
            let mut gg = Graph::new(nodes, base_degree, expander_parents, seed);
            gg.gen_parents_cache();
            let mut f = File::create("g.json").expect("Unable to create file");
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

    pub fn parents_odd(&self, node: usize) -> ParentsIter<'_> {
        // DRG parents
        // On an odd layer, invert the graph:
        // - given a node n, find the parents of nodes - n - 1
        // - for each parent, return nodes - parent - 1
        let n = self.nodes - node - 1;
        let base_parents = &self.bas[n];

        // Expander parents
        // On an odd layer, reverse the edges:
        // A->B is now B->A
        let exp_parents = &self.exp_reversed[node];

        ParentsIter {
            graph: self,
            base_parents,
            exp_parents,
            index: 0,
            reverse: Some(self.nodes),
        }
    }

    /// Load the parents of a node from cache
    pub fn parents_even(&self, node: usize) -> ParentsIter<'_> {
        let base_parents = &self.bas[node];
        let exp_parents = &self.exp[node];

        ParentsIter {
            graph: self,
            base_parents,
            exp_parents,
            index: 0,
            reverse: None,
        }
    }

    pub fn gen_parents_cache(&mut self) {
        let fp = feistel::precompute((self.expansion_degree * self.nodes) as feistel::Index);

        // Cache only forward DRG and Expander parents
        for node in 0..self.nodes {
            self.bas[node] = bucketsample_parents(&self, node);
            self.exp[node] = expander_parents(&self, node, fp);
        }

        // Cache reverse edges for exp
        for (n1, v) in self.exp.iter().enumerate() {
            for n2 in v {
                self.exp_reversed[*n2].push(n1);
            }
        }

        // TODO: sort parents
    }

    pub fn degree(&self) -> usize {
        self.base_degree + self.expansion_degree
    }
}

pub struct ParentsIter<'a> {
    graph: &'a Graph,
    base_parents: &'a [usize],
    exp_parents: &'a [usize],
    index: usize,
    reverse: Option<usize>,
}

impl<'a> Iterator for ParentsIter<'a> {
    type Item = usize;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index > self.graph.degree() {
            // already exhausted
            return None;
        }

        // base parents
        if self.index < self.base_parents.len() {
            let res = if let Some(nodes) = self.reverse {
                nodes - self.base_parents[self.index] - 1
            } else {
                self.base_parents[self.index]
            };
            self.index += 1;
            return Some(res);
        }

        // padding after base parents
        if self.index < self.graph.base_degree {
            self.index += 1;
            return Some(0);
        }

        // expansion parents
        if self.index < self.graph.base_degree + self.exp_parents.len() {
            let res = self.exp_parents[self.index - self.graph.base_degree];
            self.index += 1;
            return Some(res);
        }

        // Padding after expansion parents
        self.index += 1;
        return Some(0);
    }
}
