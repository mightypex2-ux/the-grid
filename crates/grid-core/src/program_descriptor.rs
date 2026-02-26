use serde::{Deserialize, Serialize};

use crate::{ProgramId, GridError};

/// Base program descriptor used to derive a [`ProgramId`].
///
/// `program_id = SHA-256(canonical_cbor(self))`. Program-specific
/// descriptors (ZID, Interlink, etc.) are defined in individual program crates
/// and derive their identities the same way.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProgramDescriptor {
    pub name: String,
    pub version: String,
}

impl ProgramDescriptor {
    /// Derive the ProgramId: `SHA-256(canonical_cbor(self))`.
    pub fn program_id(&self) -> Result<ProgramId, GridError> {
        let canonical = crate::encode_canonical(self)?;
        Ok(ProgramId::from_descriptor_bytes(&canonical))
    }

    /// Encode to canonical CBOR bytes.
    pub fn encode_canonical(&self) -> Result<Vec<u8>, GridError> {
        crate::encode_canonical(self)
    }

    /// Decode from canonical CBOR bytes.
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, GridError> {
        crate::decode_canonical(bytes)
    }
}
