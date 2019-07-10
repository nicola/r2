extern crate r2;
use storage_proofs::drgraph::new_seed;
use r2::{NODES, BASE_PARENTS, EXP_PARENTS, file_backed_mmap_from_zeroes, replicate, id_from_str, graph,};
use storage_proofs::hasher::{Blake2sHasher, Hasher};

fn main() {
    let gg = graph::Graph::new_cached(NODES, BASE_PARENTS, EXP_PARENTS, new_seed());
    let parents_dist : Vec<usize> = gg.bas.iter().map(|x| x.len()).collect();
    gg.bas[..30].iter().for_each(|x| println!("{:?}", x));

    println!("{:?}", &parents_dist[..NODES/2]);
    
}
