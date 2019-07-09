use rand::{ChaChaRng, OsRng, Rng, SeedableRng};
use std::cmp;
use storage_proofs::crypto::feistel;

pub struct Graph {
    nodes: usize,
    base_degree: usize,
    expansion_degree: usize,
    seed: [u32; 7],
}

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

fn expander_parents(
    g: &Graph,
    node: usize,
    feistel_precomputed: feistel::FeistelPrecomputed,
) -> Vec<usize> {
    let feistel_keys = &[1, 2, 3, 4];
    let parents: Vec<usize> = (0..g.expansion_degree)
        .filter_map(|i| {
            let parent = feistel::invert_permute(
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
    pub fn new(nodes: usize, base_degree: usize, expansion_degree: usize, seed: [u32; 7]) -> Self {
        Graph {
            nodes,
            base_degree,
            expansion_degree,
            seed,
        }
    }

    pub fn gen_parents_cache(&self) -> (Vec<Vec<usize>>, Vec<Vec<usize>>) {
        let mut all_base_parents: Vec<Vec<usize>> = Vec::with_capacity(self.nodes);
        let mut all_expansion_parents: Vec<Vec<usize>> = Vec::with_capacity(self.nodes);

        let feistel_precomputed =
            feistel::precompute((self.expansion_degree * self.nodes) as feistel::Index);

        // Forward parents
        for node in 0..self.nodes {
            // The parents of a node are the parents generated from BucketSample and the parents derived from the Bipartite Expander Graph
            let base_parents = bucketsample_parents(&self, node);
            let expanded_parents = expander_parents(&self, node, feistel_precomputed);
            all_base_parents.push(base_parents);
            all_expansion_parents.push(expanded_parents);
        }

        (all_base_parents, all_expansion_parents)
    }

    fn degree(&self) -> usize {
        self.base_degree + self.expansion_degree
    }
}
