use ark_bn254::{Bn254, Fr};
use ark_groth16::{Groth16, ProvingKey};
use ark_serialize::CanonicalSerialize;
use ark_snark::SNARK;
use thiserror::Error;

use super::circuit::{compute_commitment, compute_nullifier, SpendCircuit, SpendWitness};

#[derive(Debug, Error)]
pub enum ProverError {
    #[error("proof generation failed: {0}")]
    ProofGeneration(String),
    #[error("serialization failed: {0}")]
    Serialization(String),
}

/// Client-side spend prover. Generates Groth16 proofs for spend transactions.
pub struct SpendProver {
    proving_key: ProvingKey<Bn254>,
}

impl SpendProver {
    pub fn new(proving_key: ProvingKey<Bn254>) -> Self {
        Self { proving_key }
    }

    /// Generate a spend proof.
    ///
    /// Returns `(proof_bytes, public_signals)` where public signals are
    /// `[input_commitment, nullifier, output_commitment_0, ...]`.
    pub fn prove(
        &self,
        owner_secret: Fr,
        input_value: Fr,
        input_randomness: Fr,
        output_values: Vec<Fr>,
        output_randomnesses: Vec<Fr>,
        output_owner_pubkeys: Vec<Fr>,
    ) -> Result<(Vec<u8>, Vec<Fr>), ProverError> {
        let commitment = compute_commitment(input_value, owner_secret, input_randomness);
        let nullifier = compute_nullifier(owner_secret, commitment);

        let output_commitments: Vec<Fr> = output_values
            .iter()
            .zip(output_owner_pubkeys.iter())
            .zip(output_randomnesses.iter())
            .map(|((v, p), r)| compute_commitment(*v, *p, *r))
            .collect();

        let circuit = SpendCircuit::new(SpendWitness {
            owner_secret,
            input_value,
            input_randomness,
            output_values,
            output_randomnesses,
            output_owner_pubkeys,
            input_commitment: commitment,
            nullifier,
            output_commitments: output_commitments.clone(),
        });

        let mut rng = rand::rngs::OsRng;
        let proof = Groth16::<Bn254>::prove(&self.proving_key, circuit, &mut rng)
            .map_err(|e| ProverError::ProofGeneration(e.to_string()))?;

        let mut proof_bytes = Vec::new();
        proof
            .serialize_compressed(&mut proof_bytes)
            .map_err(|e| ProverError::Serialization(e.to_string()))?;

        let mut public_signals = vec![commitment, nullifier];
        public_signals.extend(output_commitments);

        Ok((proof_bytes, public_signals))
    }
}
