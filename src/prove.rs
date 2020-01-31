use crate::commit::MerkleTree;
use crate::NODES;

// use std::marker::PhantomData;
// use storage_proofs::circuit::stacked;
// use storage_proofs::error::Result;
use storage_proofs::hasher::{Domain, Hasher};

pub type MerklePath = Vec<u8>;

#[derive(Debug, Clone)]
pub struct PublicInputs<T: Domain> {
    pub comm_r: Option<T>,
    pub challenge: usize,
}

#[derive(Debug)]
pub struct PrivateInputs<'a, H: 'a + Hasher> {
    pub tree_d: &'a MerkleTree<H::Domain, H::Function>,
    pub tree_c: &'a MerkleTree<H::Domain, H::Function>,
    pub tree_rl: &'a MerkleTree<H::Domain, H::Function>,
    // _h: PhantomData<H>,
}

pub struct Witness<H: Hasher> {
    pub openings_d: Vec<H::Domain>,
    pub openings_c: Vec<H::Domain>,
    pub openings_rl: Vec<H::Domain>,
    pub comm_rl: H::Domain,
    pub comm_c: H::Domain,
}

pub fn witness<'a, H: Hasher>(
    pub_inputs: PublicInputs<H::Domain>,
    priv_inputs: PrivateInputs<'a, H>,
) -> Witness<H> {
    let challenge = pub_inputs.challenge % NODES;

    Witness {
        openings_d: priv_inputs.tree_d.gen_proof(challenge).unwrap().lemma().to_vec(),
        openings_c: priv_inputs.tree_c.gen_proof(challenge).unwrap().lemma().to_vec(),
        openings_rl: priv_inputs.tree_rl.gen_proof(challenge).unwrap().lemma().to_vec(),
        comm_c: priv_inputs.tree_c.root(),
        comm_rl: priv_inputs.tree_rl.root(),
    }
}

pub fn snark<H: Hasher>(pub_inputs: PublicInputs<H::Domain>, witness: Witness<H>) {}
