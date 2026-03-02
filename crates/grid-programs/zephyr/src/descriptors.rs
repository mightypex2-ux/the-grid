use grid_core::{GridError, ProgramId, ProofSystem};
use serde::{Deserialize, Serialize};

const MAX_INPUT_SIZE: usize = 64 * 1024;

/// Per-zone gossip and nullifier/batch storage.
///
/// Each `zone_id` produces a distinct `ProgramId`, giving each zone its own
/// isolated GossipSub topic and sector namespace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZephyrZoneDescriptor {
    pub name: String,
    pub version: u32,
    pub zone_id: u32,
}

impl ZephyrZoneDescriptor {
    pub fn new(zone_id: u32) -> Self {
        Self {
            name: "zephyr/zone".to_owned(),
            version: 1,
            zone_id,
        }
    }

    pub fn program_id(&self) -> Result<ProgramId, GridError> {
        let canonical = self.encode_canonical()?;
        Ok(ProgramId::from_descriptor_bytes(&canonical))
    }

    pub fn topic(&self) -> Result<String, GridError> {
        Ok(grid_core::program_topic(&self.program_id()?))
    }

    pub fn encode_canonical(&self) -> Result<Vec<u8>, GridError> {
        grid_core::encode_canonical(self)
    }

    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, GridError> {
        if bytes.len() > MAX_INPUT_SIZE {
            return Err(GridError::InvalidPayload(format!(
                "ZephyrZoneDescriptor input too large: {} > {MAX_INPUT_SIZE}",
                bytes.len(),
            )));
        }
        grid_core::decode_canonical(bytes)
    }
}

/// Global coordination: certificates, epoch announcements.
///
/// All Zephyr validators subscribe to the single global topic derived
/// from this descriptor's `ProgramId`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZephyrGlobalDescriptor {
    pub name: String,
    pub version: u32,
}

impl ZephyrGlobalDescriptor {
    pub fn new() -> Self {
        Self {
            name: "zephyr/global".to_owned(),
            version: 1,
        }
    }

    pub fn program_id(&self) -> Result<ProgramId, GridError> {
        let canonical = self.encode_canonical()?;
        Ok(ProgramId::from_descriptor_bytes(&canonical))
    }

    pub fn topic(&self) -> Result<String, GridError> {
        Ok(grid_core::program_topic(&self.program_id()?))
    }

    pub fn encode_canonical(&self) -> Result<Vec<u8>, GridError> {
        grid_core::encode_canonical(self)
    }

    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, GridError> {
        if bytes.len() > MAX_INPUT_SIZE {
            return Err(GridError::InvalidPayload(format!(
                "ZephyrGlobalDescriptor input too large: {} > {MAX_INPUT_SIZE}",
                bytes.len(),
            )));
        }
        grid_core::decode_canonical(bytes)
    }
}

impl Default for ZephyrGlobalDescriptor {
    fn default() -> Self {
        Self::new()
    }
}

/// Spend proof verification parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZephyrSpendDescriptor {
    pub name: String,
    pub version: u32,
    pub proof_system: ProofSystem,
}

impl ZephyrSpendDescriptor {
    pub fn new() -> Self {
        Self {
            name: "zephyr/spend".to_owned(),
            version: 1,
            proof_system: ProofSystem::Groth16,
        }
    }

    pub fn program_id(&self) -> Result<ProgramId, GridError> {
        let canonical = self.encode_canonical()?;
        Ok(ProgramId::from_descriptor_bytes(&canonical))
    }

    pub fn encode_canonical(&self) -> Result<Vec<u8>, GridError> {
        grid_core::encode_canonical(self)
    }

    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, GridError> {
        if bytes.len() > MAX_INPUT_SIZE {
            return Err(GridError::InvalidPayload(format!(
                "ZephyrSpendDescriptor input too large: {} > {MAX_INPUT_SIZE}",
                bytes.len(),
            )));
        }
        grid_core::decode_canonical(bytes)
    }
}

impl Default for ZephyrSpendDescriptor {
    fn default() -> Self {
        Self::new()
    }
}

/// Validator registry (static list for MVP).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZephyrValidatorDescriptor {
    pub name: String,
    pub version: u32,
}

impl ZephyrValidatorDescriptor {
    pub fn new() -> Self {
        Self {
            name: "zephyr/validators".to_owned(),
            version: 1,
        }
    }

    pub fn program_id(&self) -> Result<ProgramId, GridError> {
        let canonical = self.encode_canonical()?;
        Ok(ProgramId::from_descriptor_bytes(&canonical))
    }

    pub fn encode_canonical(&self) -> Result<Vec<u8>, GridError> {
        grid_core::encode_canonical(self)
    }

    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, GridError> {
        if bytes.len() > MAX_INPUT_SIZE {
            return Err(GridError::InvalidPayload(format!(
                "ZephyrValidatorDescriptor input too large: {} > {MAX_INPUT_SIZE}",
                bytes.len(),
            )));
        }
        grid_core::decode_canonical(bytes)
    }
}

impl Default for ZephyrValidatorDescriptor {
    fn default() -> Self {
        Self::new()
    }
}
