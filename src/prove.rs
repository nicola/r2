use crate::commit::MerkleTree;
use crate::NODES;

// use std::marker::PhantomData;
use paired::bls12_381::Fr;
use storage_proofs::error::Result;
use storage_proofs::hasher::{Domain, Hasher};

pub type MerklePath = Vec<u8>;

#[derive(Debug, Clone)]
pub struct PublicInputs<T: Domain> {
    pub comm_r: T,
    pub comm_d: T,
    pub challenge: usize,
    pub replica_id: T,
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
        openings_d: priv_inputs.tree_d.gen_proof(challenge).lemma().to_vec(),
        openings_c: priv_inputs.tree_c.gen_proof(challenge).lemma().to_vec(),
        openings_rl: priv_inputs.tree_rl.gen_proof(challenge).lemma().to_vec(),
        comm_c: priv_inputs.tree_c.root(),
        comm_rl: priv_inputs.tree_rl.root(),
    }
}

// pub fn prove<'a, H: Hasher>(
//     pub_inputs: PublicInputs<H::Domain>,
//     priv_inputs: PrivateInputs<'a, H>,
// ) {
//     let comm_d_proof = MerkleProof::new_from_proof(&priv_inputs.tree_d.gen_proof(challenge));
//     let comm_c_proof = MerkleProof::new_from_proof(&priv_inputs.tree_c.gen_proof(challenge));
//     let comm_rl = MerkleProof::new_from_proof(&priv_inputs.tree_rl.gen_proof(challenge));

//     let rpc = ReplicaColumnProof {
//         c_x: comm_c_proof,
//         drg_parents: ,
//         exp_parents,
//     };

//     // Final replica layer openings
//     trace!("final replica layer openings");
//     let comm_r_last_proof = MerkleProof::new_from_proof(&t_aux.tree_r_last.gen_proof(challenge));

//     // Encoding Proof Layer 1..l
//     let mut encoding_proofs = Vec::with_capacity(layers);

//     for layer in 1..=layers {
//         trace!(
//             "  encoding proof layer {} (include: {})",
//             layer,
//             include_challenge
//         );
//         // Due to tapering for some layers and some challenges we do not
//         // create an encoding proof.
//         if !include_challenge {
//             continue;
//         }

//         let parents_data = if layer == 1 {
//             let mut parents = vec![0; graph.base_graph().degree()];
//             graph.base_parents(challenge, &mut parents);

//             parents
//                 .into_iter()
//                 .map(|parent| t_aux.domain_node_at_layer(layer, parent))
//                 .collect::<Result<_>>()?
//         } else {
//             let mut parents = vec![0; graph.degree()];
//             graph.parents(challenge, &mut parents);
//             let base_parents_count = graph.base_graph().degree();

//             parents
//                 .into_iter()
//                 .enumerate()
//                 .map(|(i, parent)| {
//                     if i < base_parents_count {
//                         // parents data for base parents is from the current layer
//                         t_aux.domain_node_at_layer(layer, parent)
//                     } else {
//                         // parents data for exp parents is from the previous layer
//                         t_aux.domain_node_at_layer(layer - 1, parent)
//                     }
//                 })
//                 .collect::<Result<_>>()?
//         };

//         let proof = if layer == layers {
//             let encoded_node = comm_r_last_proof.verified_leaf();
//             let decoded_node = comm_d_proof.verified_leaf();

//             EncodingProof::<H>::new(
//                 challenge as u64,
//                 parents_data,
//                 encoded_node,
//                 Some(decoded_node),
//             )
//         } else {
//             let encoded_node = rpc.c_x.get_verified_node_at_layer(layer);
//             EncodingProof::<H>::new(challenge as u64, parents_data, encoded_node, None)
//         };
//         assert!(
//             proof.verify(&pub_inputs.replica_id),
//             "Invalid encoding proof generated"
//         );

//         encoding_proofs.push(proof);
//     }

//     Ok(Proof {
//         comm_d_proofs: comm_d_proof,
//         replica_column_proofs: rpc,
//         comm_r_last_proof,
//         encoding_proofs,
//     })
// }
