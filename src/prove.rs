use crate::commit::MerkleTree;
use crate::NODES;

use merkletree::proof;
use std::marker::PhantomData;
use storage_proofs::error::Result;
use storage_proofs::hasher::{Domain, Hasher};

pub type MerklePath = Vec<u8>;

#[derive(Debug, Clone)]
pub struct PublicInputs<T: Domain> {
    /// The root hash of the underlying merkle tree.
    pub commitment: Option<T>,
    /// The challenge, which leaf to prove.
    pub challenge: usize,
}

#[derive(Debug)]
pub struct PrivateInputs<'a, H: 'a + Hasher> {
    /// The underlying merkle tree.
    pub tree_d: &'a MerkleTree<H::Domain, H::Function>,
    pub tree_c: &'a MerkleTree<H::Domain, H::Function>,
    pub tree_rl: &'a MerkleTree<H::Domain, H::Function>,
    _h: PhantomData<H>,
}

pub struct Proof<H: Hasher> {
    pub openings_d: Vec<H::Domain>,
    pub openings_c: Vec<H::Domain>,
    pub openings_rl: Vec<H::Domain>,
}

pub fn offline_witness<'a, H: Hasher>(
    pub_inputs: PublicInputs<H::Domain>,
    priv_inputs: PrivateInputs<'a, H>,
) -> Proof<H> {
    let challenge = pub_inputs.challenge % NODES;

    Proof {
        openings_d: priv_inputs.tree_d.gen_proof(challenge).lemma().to_vec(),
        openings_c: priv_inputs.tree_c.gen_proof(challenge).lemma().to_vec(),
        openings_rl: priv_inputs.tree_rl.gen_proof(challenge).lemma().to_vec(),
    }
    // Inclusion c
    // Inclusion r
    // Inclusion d
}
