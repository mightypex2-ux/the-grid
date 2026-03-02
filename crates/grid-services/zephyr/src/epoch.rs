use grid_programs_zephyr::{EpochId, ValidatorInfo, ZoneId};
use sha2::{Digest, Sha256};

use crate::committee::{my_assigned_zones, sample_committee};

/// Manages epoch lifecycle: randomness derivation, committee rotation,
/// and zone assignment tracking.
///
/// Invariants:
/// - `current_epoch` increases monotonically
/// - `randomness_seed` for epoch `e` is `SHA-256(R_{e-1} || e)` (deterministic)
/// - All validators derive identical committee assignments from the same seed
pub struct EpochManager {
    current_epoch: EpochId,
    epoch_duration_ms: u64,
    randomness_seed: [u8; 32],
    validators: Vec<ValidatorInfo>,
    total_zones: u32,
    committee_size: usize,
}

/// The diff produced by an epoch transition: which zones were gained and lost.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EpochTransition {
    pub new_epoch: EpochId,
    pub new_seed: [u8; 32],
    pub gained_zones: Vec<ZoneId>,
    pub lost_zones: Vec<ZoneId>,
    pub retained_zones: Vec<ZoneId>,
}

impl EpochManager {
    pub fn new(
        initial_epoch: EpochId,
        epoch_duration_ms: u64,
        initial_randomness: [u8; 32],
        validators: Vec<ValidatorInfo>,
        total_zones: u32,
        committee_size: usize,
    ) -> Self {
        Self {
            current_epoch: initial_epoch,
            epoch_duration_ms,
            randomness_seed: initial_randomness,
            validators,
            total_zones,
            committee_size,
        }
    }

    pub fn current_epoch(&self) -> EpochId {
        self.current_epoch
    }

    pub fn randomness_seed(&self) -> &[u8; 32] {
        &self.randomness_seed
    }

    pub fn epoch_duration_ms(&self) -> u64 {
        self.epoch_duration_ms
    }

    pub fn validators(&self) -> &[ValidatorInfo] {
        &self.validators
    }

    /// Derive the randomness seed for epoch `e` from the previous seed.
    ///
    /// `R_e = SHA-256(R_{e-1} || e)` — deterministic, centralized for MVP.
    pub fn derive_seed(prev_seed: &[u8; 32], epoch: EpochId) -> [u8; 32] {
        let mut input = Vec::with_capacity(40);
        input.extend_from_slice(prev_seed);
        input.extend_from_slice(&epoch.to_be_bytes());
        Sha256::digest(&input).into()
    }

    /// Get the committee for a specific zone in the current epoch.
    pub fn committee_for_zone(&self, zone_id: ZoneId) -> Vec<ValidatorInfo> {
        sample_committee(
            &self.randomness_seed,
            zone_id,
            &self.validators,
            self.committee_size,
        )
    }

    /// Get all zones assigned to a specific validator in the current epoch.
    pub fn zones_for_validator(&self, validator_id: &[u8; 32]) -> Vec<ZoneId> {
        my_assigned_zones(
            validator_id,
            &self.randomness_seed,
            &self.validators,
            self.total_zones,
            self.committee_size,
        )
    }

    /// Advance to the next epoch, returning the transition diff for a specific validator.
    ///
    /// The caller uses this diff to subscribe/unsubscribe zone topics and
    /// spawn/stop consensus tasks.
    pub fn advance_epoch(&mut self, my_validator_id: &[u8; 32]) -> EpochTransition {
        let old_zones = self.zones_for_validator(my_validator_id);

        let new_epoch = self.current_epoch + 1;
        let new_seed = Self::derive_seed(&self.randomness_seed, new_epoch);

        self.current_epoch = new_epoch;
        self.randomness_seed = new_seed;

        let new_zones = self.zones_for_validator(my_validator_id);

        let gained: Vec<ZoneId> = new_zones
            .iter()
            .filter(|z| !old_zones.contains(z))
            .copied()
            .collect();
        let lost: Vec<ZoneId> = old_zones
            .iter()
            .filter(|z| !new_zones.contains(z))
            .copied()
            .collect();
        let retained: Vec<ZoneId> = new_zones
            .iter()
            .filter(|z| old_zones.contains(z))
            .copied()
            .collect();

        EpochTransition {
            new_epoch,
            new_seed,
            gained_zones: gained,
            lost_zones: lost,
            retained_zones: retained,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_validators(n: usize) -> Vec<ValidatorInfo> {
        (0..n)
            .map(|i| {
                let mut id = [0u8; 32];
                id[0] = i as u8;
                ValidatorInfo {
                    validator_id: id,
                    pubkey: id,
                    p2p_endpoint: format!("/ip4/127.0.0.1/tcp/{}", 4000 + i),
                }
            })
            .collect()
    }

    #[test]
    fn derive_seed_is_deterministic() {
        let prev = [0xABu8; 32];
        let s1 = EpochManager::derive_seed(&prev, 42);
        let s2 = EpochManager::derive_seed(&prev, 42);
        assert_eq!(s1, s2);
    }

    #[test]
    fn derive_seed_changes_with_epoch() {
        let prev = [0xABu8; 32];
        let s1 = EpochManager::derive_seed(&prev, 1);
        let s2 = EpochManager::derive_seed(&prev, 2);
        assert_ne!(s1, s2);
    }

    #[test]
    fn advance_epoch_increments() {
        let validators = make_validators(10);
        let mut mgr = EpochManager::new(0, 120_000, [0u8; 32], validators.clone(), 16, 5);
        assert_eq!(mgr.current_epoch(), 0);

        let transition = mgr.advance_epoch(&validators[0].validator_id);
        assert_eq!(transition.new_epoch, 1);
        assert_eq!(mgr.current_epoch(), 1);
    }

    #[test]
    fn transition_zones_partition_correctly() {
        let validators = make_validators(10);
        let mut mgr = EpochManager::new(0, 120_000, [0u8; 32], validators.clone(), 16, 5);

        let old_zones = mgr.zones_for_validator(&validators[0].validator_id);
        let transition = mgr.advance_epoch(&validators[0].validator_id);
        let new_zones = mgr.zones_for_validator(&validators[0].validator_id);

        for z in &transition.gained_zones {
            assert!(!old_zones.contains(z), "gained zone {z} was in old set");
            assert!(new_zones.contains(z), "gained zone {z} not in new set");
        }
        for z in &transition.lost_zones {
            assert!(old_zones.contains(z), "lost zone {z} not in old set");
            assert!(!new_zones.contains(z), "lost zone {z} still in new set");
        }
        for z in &transition.retained_zones {
            assert!(old_zones.contains(z), "retained zone {z} not in old set");
            assert!(new_zones.contains(z), "retained zone {z} not in new set");
        }

        assert_eq!(
            transition.gained_zones.len() + transition.retained_zones.len(),
            new_zones.len()
        );
    }

    #[test]
    fn seed_chain_is_reproducible() {
        let initial = [0x42u8; 32];
        let s1 = EpochManager::derive_seed(&initial, 1);
        let s2 = EpochManager::derive_seed(&s1, 2);
        let s3 = EpochManager::derive_seed(&s2, 3);

        let s1b = EpochManager::derive_seed(&initial, 1);
        let s2b = EpochManager::derive_seed(&s1b, 2);
        let s3b = EpochManager::derive_seed(&s2b, 3);
        assert_eq!(s3, s3b);
    }

    #[test]
    fn committee_for_zone_returns_correct_size() {
        let validators = make_validators(20);
        let mgr = EpochManager::new(0, 120_000, [0u8; 32], validators, 256, 5);
        let committee = mgr.committee_for_zone(42);
        assert_eq!(committee.len(), 5);
    }
}
