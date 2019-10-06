use merkletree::merkle;
use storage_proofs::crypto::pedersen::{pedersen, pedersen_md_no_padding};
use storage_proofs::error::Result;
use storage_proofs::hasher::pedersen::PedersenDomain;
use storage_proofs::hasher::{Domain, Hasher};

use merkletree::merkle::FromIndexedParallelIterator;
use rayon::prelude::*;

use crate::data_at_node_offset;
use crate::LAYERS;
use crate::NODES;
use crate::NODE_SIZE;

type DiskStore<E> = merkletree::merkle::DiskStore<E>;
pub type MerkleTree<T, A> = merkle::MerkleTree<T, A, DiskStore<T>>;
pub type MerkleStore<T> = DiskStore<T>;

pub fn r(a: impl AsRef<[u8]>, b: impl AsRef<[u8]>) -> PedersenDomain {
    let mut buffer = Vec::with_capacity(a.as_ref().len() + b.as_ref().len());
    buffer.extend_from_slice(a.as_ref());
    buffer.extend_from_slice(b.as_ref());

    pedersen_md_no_padding(&buffer).into()
}

pub fn single<'a, H>(data: &'a mut [u8], layer: usize) -> Result<MerkleTree<H::Domain, H::Function>>
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

pub fn columns<'a, H>(data: &'a mut [u8]) -> Result<MerkleTree<H::Domain, H::Function>>
where
    H: Hasher,
{
    let leaf_f = |i| {
        let rows: Vec<H::Domain> = (0..LAYERS - 1)
            .map(|layer| {
                let start = data_at_node_offset(layer, i);
                let end = start + NODE_SIZE;
                let d = &data[start..end];
                H::Domain::try_from_bytes(d)
            })
            .collect::<Result<_>>()
            .expect("failed to commit to column");

        let buffer: Vec<u8> = rows.iter().flat_map(|row| row.as_ref()).copied().collect();
        pedersen_md_no_padding(&buffer).into()
    };

    Ok(MerkleTree::from_par_iter(
        (0..NODES).into_par_iter().map(leaf_f),
    ))
}
