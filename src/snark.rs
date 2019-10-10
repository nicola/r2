use storage_proofs::circuit::stacked::StackedCircuit;

use crate::prove::{PublicInputs, Witness};
use crate::{BASE_PARENTS, EXP_PARENTS, NODES, OFFLINE_CHALLENGES};
use bellperson::{groth16, Circuit};
use ff::Field;
use fil_sapling_crypto::jubjub::JubjubBls12;
use fil_sapling_crypto::jubjub::JubjubEngine;
use paired::bls12_381::{Bls12, Fr};
use rand::OsRng;
use std::marker::PhantomData;

use storage_proofs::drgporep;
use storage_proofs::drgraph::new_seed;
use storage_proofs::error::Result;
use storage_proofs::hasher::{Domain, Hasher};
use storage_proofs::proof::ProofScheme;
use storage_proofs::stacked;
use storage_proofs::stacked::{LayerChallenges, StackedDrg};

pub fn snark<'a, H: Hasher>(
    pub_inputs: PublicInputs<H::Domain>,
    witness: Witness<H>,
    groth_params: &'a groth16::Parameters<Bls12>,
    engine_params: &'a <Bls12 as JubjubEngine>::Params,
) -> Result<groth16::Proof<Bls12>> {
    // let params = &JubjubBls12::new_with_window_size(4);
    let rng = &mut OsRng::new().expect("Failed to create `OsRng`");

    //
    let sp = stacked::SetupParams {
        drg: drgporep::DrgParams {
            nodes: NODES,
            degree: BASE_PARENTS,
            expansion_degree: EXP_PARENTS,
            seed: new_seed(),
        },
        layer_challenges: LayerChallenges::new(OFFLINE_CHALLENGES, OFFLINE_CHALLENGES),
    };
    let pp = StackedDrg::<H>::setup(&sp)?;

    //
    let make_circuit = || StackedCircuit {
        params: engine_params,
        public_params: pp,
        replica_id: Some(pub_inputs.replica_id),
        comm_d: Some(pub_inputs.comm_d),
        comm_r: Some(pub_inputs.comm_r),
        comm_r_last: Some(witness.comm_rl),
        comm_c: Some(witness.comm_c),
        proofs: witness.iter().cloned().map(|p| p.into()).collect(),
        _e: PhantomData,
    };

    let groth_proof = groth16::create_random_proof(make_circuit(), groth_params, rng)?;
    let mut proof_vec = vec![];
    groth_proof.write(&mut proof_vec)?;
    let gp = groth16::Proof::<Bls12>::read(&proof_vec[..])?;

    Ok(gp)
}
