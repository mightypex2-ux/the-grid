use grid_programs_zephyr::ValidatorInfo;
use serde::{Deserialize, Serialize};

/// Configuration for the Zephyr service.
///
/// Invariants:
/// - `total_zones` > 0
/// - `committee_size` > 0
/// - `quorum_threshold` <= `committee_size`
/// - `epoch_duration_ms` > 0
/// - `round_interval_ms` > 0
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZephyrConfig {
    /// Number of zones (A). Nullifiers are routed via `H(N) mod A`.
    pub total_zones: u32,
    /// Committee size per zone (k).
    pub committee_size: usize,
    /// Epoch duration in milliseconds (E).
    pub epoch_duration_ms: u64,
    /// Proposal round interval in milliseconds (T_round).
    pub round_interval_ms: u64,
    /// Quorum threshold for certificate assembly: `ceil(2k/3)`.
    pub quorum_threshold: usize,
    /// Maximum spends per batch.
    pub max_batch_size: usize,
    /// Genesis randomness seed (R_0).
    #[serde(with = "hex_bytes")]
    pub initial_randomness: [u8; 32],
    /// Static validator list (MVP).
    pub validators: Vec<ValidatorInfo>,
}

impl Default for ZephyrConfig {
    fn default() -> Self {
        Self {
            total_zones: 256,
            committee_size: 5,
            epoch_duration_ms: 120_000,
            round_interval_ms: 500,
            quorum_threshold: 4,
            max_batch_size: 64,
            initial_randomness: [0u8; 32],
            validators: vec![],
        }
    }
}

mod hex_bytes {
    use serde::{self, Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(bytes: &[u8; 32], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&hex::encode(bytes))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<[u8; 32], D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let bytes = hex::decode(&s).map_err(serde::de::Error::custom)?;
        bytes
            .try_into()
            .map_err(|_| serde::de::Error::custom("expected 32 bytes"))
    }
}
