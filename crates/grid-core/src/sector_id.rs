use serde::{Deserialize, Serialize};
use std::fmt;

/// Opaque sector identifier (variable-length canonical bytes).
///
/// Represents a logical sector within a program. The byte format is
/// program-specific (e.g., a channel ID, a user ID hash, etc.).
#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SectorId(#[serde(with = "serde_bytes")] Vec<u8>);

impl SectorId {
    /// Create a SectorId from raw bytes.
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    /// Access the raw byte representation.
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Encode as lowercase hex string.
    pub fn to_hex(&self) -> String {
        hex::encode(&self.0)
    }
}

impl From<Vec<u8>> for SectorId {
    fn from(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }
}

impl fmt::Debug for SectorId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SectorId({})", self.to_hex())
    }
}

impl fmt::Display for SectorId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}
