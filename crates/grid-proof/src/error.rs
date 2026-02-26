use thiserror::Error;

/// Errors produced by proof verification.
#[derive(Debug, Error)]
pub enum ProofError {
    #[error("verifier key not found for program {program_id}")]
    VerifierKeyNotFound { program_id: String },

    #[error("proof verification failed: {reason}")]
    VerificationFailed { reason: String },

    #[error("invalid proof format: {reason}")]
    InvalidProofFormat { reason: String },
}
