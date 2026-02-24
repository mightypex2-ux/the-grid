use serde::{Deserialize, Serialize};

/// Encrypted sector key(s) for one or more recipients.
///
/// Used in [`StoreRequest`](crate::StoreRequest) to deliver the symmetric
/// sector key to authorized readers via hybrid key wrapping.
/// The wrapping logic itself lives in `zfs-crypto`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KeyEnvelope {
    pub entries: Vec<KeyEnvelopeEntry>,
}

/// A single recipient entry within a [`KeyEnvelope`].
///
/// Contains the hybrid-wrapped sector key for one recipient,
/// identified by their `did:key`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KeyEnvelopeEntry {
    pub recipient_did: String,
    #[serde(with = "serde_bytes")]
    pub sender_x25519_public: Vec<u8>,
    #[serde(with = "serde_bytes")]
    pub mlkem_ciphertext: Vec<u8>,
    #[serde(with = "serde_bytes")]
    pub wrapped_key: Vec<u8>,
}
