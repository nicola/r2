use merkletree::merkle;
use merkletree::merkle::FromIndexedParallelIterator;
use rayon::prelude::*;
use storage_proofs::crypto::pedersen::pedersen_md_no_padding;
use storage_proofs::error::Result;
use storage_proofs::hasher::pedersen::PedersenDomain;
use storage_proofs::hasher::{Domain, Hasher};

use crate::{data_at_node, LAYERS, NODES};

type DiskStore<E> = merkletree::store::DiskStore<E>;
pub type MerkleTree<T, A> = merkle::MerkleTree<T, A, DiskStore<T>>;
pub type MerkleStore<T> = DiskStore<T>;

pub fn commit<'a, H: Hasher>(
    stack: &'a [u8],
) -> (
    PedersenDomain,
    MerkleTree<H::Domain, H::Function>,
    MerkleTree<H::Domain, H::Function>,
) {
    // Generate CommR
    let tree_c = columns::<H>(&stack).expect("t_c failed");
    let tree_rl = single::<H>(&stack, LAYERS - 1).expect("t_rl failed");
    let comm_r = comm_r(tree_c.root(), tree_rl.root());

    (comm_r, tree_rl, tree_c)
}

pub fn comm_r(a: impl AsRef<[u8]>, b: impl AsRef<[u8]>) -> PedersenDomain {
    let mut buffer = Vec::with_capacity(a.as_ref().len() + b.as_ref().len());
    buffer.extend_from_slice(a.as_ref());
    buffer.extend_from_slice(b.as_ref());

    pedersen_md_no_padding(&buffer).into()
}

pub fn single<'a, H>(data: &'a [u8], layer: usize) -> Result<MerkleTree<H::Domain, H::Function>>
where
    H: Hasher,
{
    let leafs_f = |i| {
        H::Domain::try_from_bytes(data_at_node(&data, layer, i))
            .expect("failed to convert node data to domain element")
    };

    MerkleTree::from_par_iter(
        (0..NODES).into_par_iter().map(leafs_f),
    )
}

pub fn columns<'a, H>(data: &'a [u8]) -> Result<MerkleTree<H::Domain, H::Function>>
where
    H: Hasher,
{
    let leaf_f = |i| {
        let rows: Vec<u8> = (0..LAYERS - 1)
            .flat_map(|layer| data_at_node(&data, layer, i).iter())
            .cloned()
            .collect::<Vec<u8>>();
        pedersen_md_no_padding(&rows).into()
    };

    MerkleTree::from_par_iter(
        (0..NODES).into_par_iter().map(leaf_f),
    )
}
