use grid_programs_zephyr::{EpochId, ValidatorInfo};

/// Deterministic round-robin leader election.
///
/// The leader for a given (epoch, round) is determined purely from the
/// committee order, which is itself deterministic from the epoch seed.
pub fn leader_for_round(committee: &[ValidatorInfo], epoch: EpochId, round: u64) -> &ValidatorInfo {
    let index = (epoch.wrapping_add(round) as usize) % committee.len();
    &committee[index]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_committee(n: usize) -> Vec<ValidatorInfo> {
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
    fn deterministic_leader() {
        let c = make_committee(5);
        let l1 = leader_for_round(&c, 1, 0);
        let l2 = leader_for_round(&c, 1, 0);
        assert_eq!(l1.validator_id, l2.validator_id);
    }

    #[test]
    fn rotates_with_round() {
        let c = make_committee(5);
        let l0 = leader_for_round(&c, 0, 0);
        let l1 = leader_for_round(&c, 0, 1);
        assert_ne!(l0.validator_id, l1.validator_id);
    }

    #[test]
    fn wraps_around() {
        let c = make_committee(3);
        let l0 = leader_for_round(&c, 0, 0);
        let l3 = leader_for_round(&c, 0, 3);
        assert_eq!(l0.validator_id, l3.validator_id);
    }

    #[test]
    fn epoch_shifts_leader() {
        let c = make_committee(5);
        let l_e0 = leader_for_round(&c, 0, 0);
        let l_e1 = leader_for_round(&c, 1, 0);
        assert_ne!(l_e0.validator_id, l_e1.validator_id);
    }
}
