extern crate r2;
use r2::{
    graph, NODES,
};

fn main() {
    let gg = graph::Graph::new_cached();
    let parents_dist: Vec<usize> = gg.bas.iter().map(|x| x.len()).collect();
    gg.bas[..30].iter().for_each(|x| println!("{:?}", x));

    println!("{:?}", &parents_dist[..NODES / 2]);
}
