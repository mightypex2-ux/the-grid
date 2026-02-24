use crate::error::CryptoError;

/// Multicodec prefix for Ed25519 public keys: 0xed01 (varint-encoded as two bytes).
const ED25519_MULTICODEC: [u8; 2] = [0xed, 0x01];

/// Encode an Ed25519 public key as a `did:key` string.
///
/// Format: `did:key:z` + base58btc(multicodec_prefix || public_key_bytes)
pub fn ed25519_to_did_key(pk: &[u8; 32]) -> String {
    let mut prefixed = Vec::with_capacity(2 + 32);
    prefixed.extend_from_slice(&ED25519_MULTICODEC);
    prefixed.extend_from_slice(pk);

    let encoded = bs58::encode(&prefixed).into_string();
    format!("did:key:z{encoded}")
}

/// Decode a `did:key` string to an Ed25519 public key (32 bytes).
///
/// Expects format: `did:key:z` + base58btc(0xed01 || 32-byte key)
pub fn did_key_to_ed25519(did: &str) -> Result<[u8; 32], CryptoError> {
    let multibase = did
        .strip_prefix("did:key:z")
        .ok_or_else(|| CryptoError::InvalidDid("must start with 'did:key:z'".into()))?;

    let decoded = bs58::decode(multibase)
        .into_vec()
        .map_err(|e| CryptoError::InvalidDid(format!("base58 decode failed: {e}")))?;

    if decoded.len() != 2 + 32 {
        return Err(CryptoError::InvalidDid(format!(
            "expected 34 bytes after base58, got {}",
            decoded.len()
        )));
    }

    if decoded[0] != ED25519_MULTICODEC[0] || decoded[1] != ED25519_MULTICODEC[1] {
        return Err(CryptoError::InvalidDid(
            "multicodec prefix is not Ed25519 (0xed01)".into(),
        ));
    }

    let mut pk = [0u8; 32];
    pk.copy_from_slice(&decoded[2..]);
    Ok(pk)
}
