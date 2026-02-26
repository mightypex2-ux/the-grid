use programs_interlink::InterlinkDescriptor;
use programs_zid::ZidDescriptor;

/// Create the canonical ZID v1 program descriptor.
pub fn zid_descriptor() -> ZidDescriptor {
    ZidDescriptor::v1()
}

/// Create the canonical Interlink v1 program descriptor.
pub fn interlink_descriptor() -> InterlinkDescriptor {
    InterlinkDescriptor::v1()
}

/// Create the ZID v2 descriptor (with Groth16 shape proofs).
pub fn zid_descriptor_v2() -> ZidDescriptor {
    ZidDescriptor::v2()
}

/// Create the Interlink v2 descriptor (with Groth16 shape proofs).
pub fn interlink_descriptor_v2() -> InterlinkDescriptor {
    InterlinkDescriptor::v2()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zid_descriptor_has_deterministic_program_id() {
        let d1 = zid_descriptor();
        let d2 = zid_descriptor();
        assert_eq!(d1.program_id().unwrap(), d2.program_id().unwrap(),);
    }

    #[test]
    fn interlink_descriptor_has_deterministic_program_id() {
        let d1 = interlink_descriptor();
        let d2 = interlink_descriptor();
        assert_eq!(d1.program_id().unwrap(), d2.program_id().unwrap(),);
    }

    #[test]
    fn zid_and_interlink_have_different_program_ids() {
        let zid = zid_descriptor().program_id().unwrap();
        let interlink = interlink_descriptor().program_id().unwrap();
        assert_ne!(zid, interlink);
    }
}
