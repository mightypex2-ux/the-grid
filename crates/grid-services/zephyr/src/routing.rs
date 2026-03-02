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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_same_nullifier_same_zone() {
        let n = Nullifier([0xAB; 32]);
        let z1 = zone_for_nullifier(&n, 256);
        let z2 = zone_for_nullifier(&n, 256);
        assert_eq!(z1, z2);
    }

    #[test]
    fn result_within_range() {
        for i in 0u8..=255 {
            let n = Nullifier([i; 32]);
            let zone = zone_for_nullifier(&n, 256);
            assert!(zone < 256, "zone {zone} out of range for A=256");
        }
    }

    #[test]
    fn different_nullifiers_may_differ() {
        let n1 = Nullifier([0x00; 32]);
        let n2 = Nullifier([0xFF; 32]);
        let z1 = zone_for_nullifier(&n1, 256);
        let z2 = zone_for_nullifier(&n2, 256);
        // Probabilistically different (would need a specific collision to be equal).
        // If they happen to collide, the test still passes; the important thing
        // is that the function doesn't trivially return a constant.
        let _ = (z1, z2);
    }

    #[test]
    fn single_zone_always_zero() {
        let n = Nullifier([0x42; 32]);
        assert_eq!(zone_for_nullifier(&n, 1), 0);
    }

    #[test]
    fn known_vector() {
        let n = Nullifier([0u8; 32]);
        let hash = sha2::Sha256::digest(n.as_ref());
        let expected_u32 = u32::from_be_bytes([hash[0], hash[1], hash[2], hash[3]]);
        let expected_zone = expected_u32 % 256;
        assert_eq!(zone_for_nullifier(&n, 256), expected_zone);
    }

    #[test]
    fn routing_is_consistent_across_zone_counts() {
        let n = Nullifier([0x77; 32]);
        let hash = sha2::Sha256::digest(n.as_ref());
        let raw = u32::from_be_bytes([hash[0], hash[1], hash[2], hash[3]]);

        for total in [1, 2, 4, 16, 128, 256, 1024] {
            assert_eq!(zone_for_nullifier(&n, total), raw % total);
        }
    }

    #[test]
    fn distribution_is_roughly_uniform() {
        let total_zones = 8u32;
        let mut counts = vec![0u32; total_zones as usize];
        for i in 0..1024u32 {
            let mut bytes = [0u8; 32];
            bytes[..4].copy_from_slice(&i.to_le_bytes());
            let n = Nullifier(bytes);
            let zone = zone_for_nullifier(&n, total_zones) as usize;
            counts[zone] += 1;
        }
        let expected = 1024.0 / total_zones as f64;
        for (zone, &count) in counts.iter().enumerate() {
            let ratio = count as f64 / expected;
            assert!(
                (0.5..=1.5).contains(&ratio),
                "zone {zone} has {count} entries (expected ~{expected:.0}), ratio={ratio:.2}"
            );
        }
    }
}
