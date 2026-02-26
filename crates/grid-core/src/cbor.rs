use serde::{de::DeserializeOwned, Serialize};

use crate::GridError;

/// Encode a value to canonical CBOR (RFC 8949 deterministic encoding).
pub fn encode_canonical<T: Serialize>(value: &T) -> Result<Vec<u8>, GridError> {
    let mut buf = Vec::new();
    ciborium::into_writer(value, &mut buf).map_err(|e| GridError::Encode(e.to_string()))?;
    Ok(buf)
}

/// Decode a value from canonical CBOR bytes.
pub fn decode_canonical<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, GridError> {
    ciborium::from_reader(bytes).map_err(|e| GridError::Decode(e.to_string()))
}
