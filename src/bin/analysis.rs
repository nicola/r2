extern crate r2;
use r2::{
    file_backed_mmap_from_zeroes, graph, id_from_str, replicate, BASE_PARENTS, EXP_PARENTS, NODES,
};
use storage_proofs::drgraph::new_seed;
use storage_proofs::hasher::{Blake2sHasher, Hasher};

fn main() {
    let gg = graph::Graph::new_cached();
    let parents_dist: Vec<usize> = gg.bas.iter().map(|x| x.len()).collect();
    gg.bas[..30].iter().for_each(|x| println!("{:?}", x));

    println!("{:?}", &parents_dist[..NODES / 2]);
}
