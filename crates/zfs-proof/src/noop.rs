use zfs_core::{Cid, ProgramId};

use crate::{ProofError, ProofVerifier, VerifiedSector};

/// No-op proof verifier that always succeeds.
///
/// Default implementation for v0.1.0. Useful for development and programs
/// that do not require proof verification.
pub struct NoopVerifier;

impl ProofVerifier for NoopVerifier {
    fn verify(
        &self,
        cid: &Cid,
        program_id: &ProgramId,
        version: u64,
        _proof: &[u8],
        _payload_context: Option<&[u8]>,
    ) -> Result<VerifiedSector, ProofError> {
        Ok(VerifiedSector {
            cid: *cid,
            program_id: *program_id,
            version,
        })
    }
}
