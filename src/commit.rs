use merkletree::merkle;
use storage_proofs::error::Result;
use storage_proofs::hasher::{Domain, Hasher};

use merkletree::merkle::FromIndexedParallelIterator;
use rayon::prelude::*;

use crate::data_at_node_offset;
use crate::NODES;
use crate::NODE_SIZE;

type DiskStore<E> = merkletree::merkle::DiskStore<E>;
pub type MerkleTree<T, A> = merkle::MerkleTree<T, A, DiskStore<T>>;
pub type MerkleStore<T> = DiskStore<T>;

pub fn commit<'a, H>(
    data: &'a mut [u8],
    columns: usize,
) -> Result<MerkleTree<H::Domain, H::Function>>
where
    H: Hasher,
{
    let leafs_f = |i| {
        let start = data_at_node_offset(0, i);
        let end = start + NODE_SIZE;
        let d = &data[start..end];
        H::Domain::try_from_bytes(d).expect("failed to convert node data to domain element")
    };

    Ok(MerkleTree::from_par_iter(
        (0..NODES).into_par_iter().map(leafs_f),
    ))
}
