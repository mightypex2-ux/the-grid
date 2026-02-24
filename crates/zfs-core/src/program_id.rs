use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;

use crate::ZfsError;

/// 32-byte program identity: `SHA-256(canonical_cbor(ProgramDescriptor))`.
///
/// Hex-encoded in APIs and logs. Deterministic: the same descriptor always
/// produces the same ProgramId.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProgramId(#[serde(with = "serde_bytes")] [u8; 32]);

impl ProgramId {
    /// Derive a ProgramId from the canonical CBOR bytes of a descriptor.
    pub fn from_descriptor_bytes(canonical_bytes: &[u8]) -> Self {
        let hash = Sha256::digest(canonical_bytes);
        Self(hash.into())
    }

    /// Access the raw 32-byte representation.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Encode as lowercase hex string (64 characters).
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    /// Parse from a hex string (must be exactly 64 hex characters).
    pub fn from_hex(s: &str) -> Result<Self, ZfsError> {
        let bytes = hex::decode(s).map_err(|e| ZfsError::Decode(e.to_string()))?;
        let arr: [u8; 32] = bytes.try_into().map_err(|_| {
            ZfsError::Decode("ProgramId hex must decode to exactly 32 bytes".into())
        })?;
        Ok(Self(arr))
    }
}

impl From<[u8; 32]> for ProgramId {
    fn from(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

impl fmt::Debug for ProgramId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ProgramId({})", self.to_hex())
    }
}

impl fmt::Display for ProgramId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}
