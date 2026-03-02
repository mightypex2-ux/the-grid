use serde::{Deserialize, Serialize};

pub type ZoneId = u32;
pub type EpochId = u64;

/// A note commitment: `C = Poseidon(value || owner_pubkey || r)`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NoteCommitment(#[serde(with = "serde_bytes")] pub [u8; 32]);

/// A nullifier: `N = Poseidon(owner_secret || C)`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Nullifier(#[serde(with = "serde_bytes")] pub [u8; 32]);

impl AsRef<[u8]> for Nullifier {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl AsRef<[u8]> for NoteCommitment {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

/// An output note in a spend transaction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteOutput {
    pub commitment: NoteCommitment,
    #[serde(with = "serde_bytes")]
    pub encrypted_data: Vec<u8>,
}

/// A spend transaction with ZK proof.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpendTransaction {
    pub input_commitment: NoteCommitment,
    pub nullifier: Nullifier,
    pub outputs: Vec<NoteOutput>,
    #[serde(with = "serde_bytes")]
    pub proof: Vec<u8>,
    pub public_signals: Vec<[u8; 32]>,
}

/// A validator in the global pool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidatorInfo {
    #[serde(with = "serde_bytes")]
    pub validator_id: [u8; 32],
    #[serde(with = "serde_bytes")]
    pub pubkey: [u8; 32],
    pub p2p_endpoint: String,
}

/// A batch proposal from a committee leader.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchProposal {
    pub zone_id: ZoneId,
    pub epoch: EpochId,
    #[serde(with = "serde_bytes")]
    pub prev_zone_head: [u8; 32],
    pub nullifiers: Vec<Nullifier>,
    pub spends: Vec<SpendTransaction>,
    #[serde(with = "serde_bytes")]
    pub batch_hash: [u8; 32],
    #[serde(with = "serde_bytes")]
    pub proposer_id: [u8; 32],
    #[serde(with = "serde_bytes")]
    pub proposer_sig: Vec<u8>,
}

/// A vote on a batch proposal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchVote {
    pub zone_id: ZoneId,
    pub epoch: EpochId,
    #[serde(with = "serde_bytes")]
    pub batch_hash: [u8; 32],
    #[serde(with = "serde_bytes")]
    pub voter_id: [u8; 32],
    #[serde(with = "serde_bytes")]
    pub signature: Vec<u8>,
}

/// A finality certificate for a batch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinalityCertificate {
    pub zone_id: ZoneId,
    pub epoch: EpochId,
    #[serde(with = "serde_bytes")]
    pub prev_zone_head: [u8; 32],
    #[serde(with = "serde_bytes")]
    pub new_zone_head: [u8; 32],
    #[serde(with = "serde_bytes")]
    pub batch_hash: [u8; 32],
    pub signatures: Vec<CertSignature>,
}

/// A validator's signature in a finality certificate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CertSignature {
    #[serde(with = "serde_bytes")]
    pub validator_id: [u8; 32],
    #[serde(with = "serde_bytes")]
    pub signature: Vec<u8>,
}
