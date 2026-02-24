use zfs_core::{Cid, ProgramId};

use crate::ProofError;

/// Marker type returned on successful proof verification, binding the
/// proof result to the verified content and program.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedSector {
    pub cid: Cid,
    pub program_id: ProgramId,
    pub version: u64,
}

/// Pluggable Valid-Sector proof verification.
///
/// Programs may require that stored sectors are accompanied by a proof
/// the sector is valid. Verifier implementations check proofs bound to
/// `(cid, program_id, version)` using per-program verifier keys.
///
/// No concrete ZK system is mandated — only this trait and its error types.
pub trait ProofVerifier: Send + Sync {
    /// Verify that `proof` is valid for the given sector identity.
    ///
    /// * `cid` — content identifier of the stored ciphertext.
    /// * `program_id` — the program this sector belongs to.
    /// * `version` — the sector version.
    /// * `proof` — opaque proof bytes produced by the client.
    /// * `payload_context` — optional binding data (e.g. ciphertext hash
    ///   or public commitment) to tie the proof to the stored payload
    ///   without revealing plaintext.
    fn verify(
        &self,
        cid: &Cid,
        program_id: &ProgramId,
        version: u64,
        proof: &[u8],
        payload_context: Option<&[u8]>,
    ) -> Result<VerifiedSector, ProofError>;
}
