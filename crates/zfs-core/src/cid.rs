use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;

use crate::ZfsError;

/// Content identifier: `SHA-256(ciphertext)`.
///
/// 32-byte content-addressed identifier derived from the stored ciphertext.
/// Same ciphertext always produces the same Cid. Hex-encoded in logs and APIs.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Cid(#[serde(with = "serde_bytes")] [u8; 32]);

impl Cid {
    /// Derive a Cid from ciphertext bytes: `SHA-256(ciphertext)`.
    pub fn from_ciphertext(ciphertext: &[u8]) -> Self {
        let hash = Sha256::digest(ciphertext);
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
        let arr: [u8; 32] = bytes
            .try_into()
            .map_err(|_| ZfsError::Decode("Cid hex must decode to exactly 32 bytes".into()))?;
        Ok(Self(arr))
    }
}

impl From<[u8; 32]> for Cid {
    fn from(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

impl fmt::Debug for Cid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Cid({})", self.to_hex())
    }
}

impl fmt::Display for Cid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}
