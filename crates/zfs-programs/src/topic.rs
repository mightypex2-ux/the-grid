use zfs_core::ProgramId;

/// Build the GossipSub topic string for a given program ID.
///
/// Format: `prog/<program_id_hex>` (64 lowercase hex characters).
pub fn program_topic(program_id: &ProgramId) -> String {
    format!("prog/{}", program_id.to_hex())
}
