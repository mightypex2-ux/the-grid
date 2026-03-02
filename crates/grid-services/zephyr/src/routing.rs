use grid_programs_zephyr::{Nullifier, ZoneId};
use sha2::{Digest, Sha256};

/// Deterministic zone assignment from nullifier.
///
/// All nodes MUST compute identical results — this is the foundation of
/// Zephyr's double-spend prevention (same nullifier always routes to the
/// same zone). Uses SHA-256 for collision resistance; ZK-friendliness is
/// not needed here.
pub fn zone_for_nullifier(nullifier: &Nullifier, total_zones: u32) -> ZoneId {
    let hash = Sha256::digest(nullifier.as_ref());
    let n = u32::from_be_bytes([hash[0], hash[1], hash[2], hash[3]]);
    n % total_zones
}
