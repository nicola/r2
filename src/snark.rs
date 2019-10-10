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
// use storage_proofs::error::Result;
use storage_proofs::hasher::{Domain, Hasher};
use storage_proofs::proof::ProofScheme;
use storage_proofs::stacked;
use storage_proofs::stacked::{LayerChallenges, StackedDrg};

use bellperson::{ConstraintSystem, SynthesisError};
use fil_sapling_crypto::circuit::{boolean::Boolean, num};
use paired::Engine;
use storage_proofs::circuit::pedersen::{
    pedersen_compression_num as pedersen, pedersen_md_no_padding,
};

use storage_proofs::crypto::pedersen::PEDERSEN_BLOCK_SIZE;

pub struct StackedCircuit<'a, E: JubjubEngine, H: 'static + Hasher> {
    params: &'a E::Params,
    replica_id: Option<H::Domain>,
    comm_d: Option<H::Domain>,
    comm_r: Option<H::Domain>,
    comm_r_last: Option<H::Domain>,
    comm_c: Option<H::Domain>,
    proofs: Vec<Witness<H>>,
    _e: PhantomData<E>,
}

impl<'a, H: 'static + Hasher> StackedCircuit<'a, Bls12, H> {
    #[allow(clippy::too_many_arguments)]
    pub fn synthesize<CS>(
        mut cs: CS,
        params: &'a <Bls12 as JubjubEngine>::Params,
        replica_id: Option<H::Domain>,
        comm_d: Option<H::Domain>,
        comm_r: Option<H::Domain>,
        comm_r_last: Option<H::Domain>,
        comm_c: Option<H::Domain>,
        proofs: Vec<Witness<H>>,
    ) -> Result<(), SynthesisError>
    where
        CS: ConstraintSystem<Bls12>,
    {
        let circuit = StackedCircuit::<'a, Bls12, H> {
            params,
            replica_id,
            comm_d,
            comm_r,
            comm_r_last,
            comm_c,
            proofs,
            _e: PhantomData,
        };

        circuit.synthesize(&mut cs)
    }
}

pub fn hash2<E, CS>(
    mut cs: CS,
    params: &E::Params,
    first: &[Boolean],
    second: &[Boolean],
) -> Result<num::AllocatedNum<E>, SynthesisError>
where
    E: JubjubEngine,
    CS: ConstraintSystem<E>,
{
    let mut values = Vec::new();
    values.extend_from_slice(first);

    // pad to full bytes
    while values.len() % 8 > 0 {
        values.push(Boolean::Constant(false));
    }

    values.extend_from_slice(second);
    // pad to full bytes
    while values.len() % 8 > 0 {
        values.push(Boolean::Constant(false));
    }

    hash1(cs.namespace(|| "hash2"), params, &values)
}

/// Hash a list of bits.
pub fn hash1<E, CS>(
    mut cs: CS,
    params: &E::Params,
    values: &[Boolean],
) -> Result<num::AllocatedNum<E>, SynthesisError>
where
    E: JubjubEngine,
    CS: ConstraintSystem<E>,
{
    assert!(values.len() % 32 == 0, "input must be a multiple of 32bits");

    if values.is_empty() {
        // can happen with small layers
        num::AllocatedNum::alloc(cs.namespace(|| "hash1"), || Ok(E::Fr::zero()))
    } else if values.len() > PEDERSEN_BLOCK_SIZE {
        pedersen_md_no_padding(cs.namespace(|| "hash1"), params, values)
    } else {
        pedersen(cs.namespace(|| "hash1"), params, values)
    }
}

// pub fn challenge_proof<CS: ConstraintSystem<Bls12>>(
//     mut cs: CS,
//     params: &<Bls12 as JubjubEngine>::Params,
//     replica_id: Option<H::Domain>,
//     comm_d: Option<H::Domain>,
//     comm_r: Option<H::Domain>,
//     comm_r_last: Option<H::Domain>,
//     comm_c: Option<H::Domain>,
// ) -> Result<(), SynthesisError>
// where
//     CS: ConstraintSystem<Bls12>,
// {
//     // verify initial data layer
//     let comm_d_leaf = comm_d_proof.alloc_value(cs.namespace(|| "comm_d_leaf"))?;
//     comm_d_proof.synthesize(
//         cs.namespace(|| "comm_d_inclusion"),
//         params,
//         comm_d.clone(),
//         comm_d_leaf.clone(),
//     )?;

//     // verify encodings
//     for (layer, proof) in encoding_proofs.into_iter().enumerate() {
//         proof.synthesize(
//             cs.namespace(|| format!("encoding_proof_{}", layer)),
//             params,
//             replica_id,
//         )?;
//     }

//     // verify replica column openings
//     replica_column_proof.synthesize(cs.namespace(|| "replica_column_proof"), params, comm_c)?;

//     // verify final replica layer
//     let comm_r_last_data_leaf =
//         comm_r_last_proof.alloc_value(cs.namespace(|| "comm_r_last_data_leaf"))?;
//     comm_r_last_proof.synthesize(
//         cs.namespace(|| "comm_r_last_data_inclusion"),
//         params,
//         comm_r_last.clone(),
//         comm_r_last_data_leaf,
//     )?;

//     Ok(())
// }

impl<'a, H: Hasher> Circuit<Bls12> for StackedCircuit<'a, Bls12, H> {
    fn synthesize<CS: ConstraintSystem<Bls12>>(self, cs: &mut CS) -> Result<(), SynthesisError> {
        let StackedCircuit {
            params,
            proofs,
            replica_id,
            comm_r,
            comm_d,
            comm_r_last,
            comm_c,
            ..
        } = self;

        let params = &self.params;

        // Allocate replica_id
        let replica_id_num = num::AllocatedNum::alloc(cs.namespace(|| "replica_id_num"), || {
            replica_id
                .map(Into::into)
                .ok_or_else(|| SynthesisError::AssignmentMissing)
        })?;

        let mut replica_id_bits =
            replica_id_num.into_bits_le(cs.namespace(|| "replica_id_bits"))?;
        // pad
        while replica_id_bits.len() % 8 > 0 {
            replica_id_bits.push(Boolean::Constant(false));
        }

        // Allocate comm_d as Fr
        let comm_d_num = num::AllocatedNum::alloc(cs.namespace(|| "comm_d"), || {
            comm_d
                .map(Into::into)
                .ok_or_else(|| SynthesisError::AssignmentMissing)
        })?;

        // make comm_d a public input
        comm_d_num.inputize(cs.namespace(|| "comm_d_input"))?;

        // Allocate comm_r as Fr
        let comm_r_num = num::AllocatedNum::alloc(cs.namespace(|| "comm_r"), || {
            comm_r
                .map(Into::into)
                .ok_or_else(|| SynthesisError::AssignmentMissing)
        })?;

        // make comm_r a public input
        comm_r_num.inputize(cs.namespace(|| "comm_r_input"))?;

        // Allocate comm_r_last as Fr
        let comm_r_last_num = num::AllocatedNum::alloc(cs.namespace(|| "comm_r_last"), || {
            comm_r_last
                .map(Into::into)
                .ok_or_else(|| SynthesisError::AssignmentMissing)
        })?;

        // Allocate comm_r_last as booleans
        let comm_r_last_bits = comm_r_last_num.into_bits_le(cs.namespace(|| "comm_r_last_bits"))?;

        // Allocate comm_c as Fr
        let comm_c_num = num::AllocatedNum::alloc(cs.namespace(|| "comm_c"), || {
            comm_c
                .map(Into::into)
                .ok_or_else(|| SynthesisError::AssignmentMissing)
        })?;

        // Allocate comm_c as booleans
        let comm_c_bits = comm_c_num.into_bits_le(cs.namespace(|| "comm_c_bits"))?;

        // Verify comm_r = H(comm_c || comm_r_last)
        {
            let hash_num = hash2(
                cs.namespace(|| "H_comm_c_comm_r_last"),
                params,
                &comm_c_bits,
                &comm_r_last_bits,
            )?;

            // Check actual equality
            equal(
                cs,
                || "enforce comm_r = H(comm_c || comm_r_last)",
                &comm_r_num,
                &hash_num,
            );
        }

        for (i, proof) in proofs.into_iter().enumerate() {
            // challenge_proof(
            //     &mut cs.namespace(|| format!("challenge_{}", i)),
            //     &params,
            //     &replica_id,
            //     &comm_d_num,
            //     &comm_c_num,
            //     &comm_r_last_num,
            //     &replica_id_bits,
            // )?;
        }
        Ok(())
    }
}

pub fn equal<E: Engine, A, AR, CS: ConstraintSystem<E>>(
    cs: &mut CS,
    annotation: A,
    a: &num::AllocatedNum<E>,
    b: &num::AllocatedNum<E>,
) where
    A: FnOnce() -> AR,
    AR: Into<String>,
{
    // a * 1 = b
    cs.enforce(
        annotation,
        |lc| lc + a.get_variable(),
        |lc| lc + CS::one(),
        |lc| lc + b.get_variable(),
    );
}

pub fn snark<'a, H: Hasher>(
    pub_inputs: PublicInputs<H::Domain>,
    witness: Witness<H>,
    groth_params: &'a groth16::Parameters<Bls12>,
    engine_params: &'a <Bls12 as JubjubEngine>::Params,
) -> storage_proofs::error::Result<groth16::Proof<Bls12>> {
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
