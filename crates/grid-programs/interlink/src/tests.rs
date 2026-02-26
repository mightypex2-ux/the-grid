use crate::interlink::{
    sector_id_for_channel, sector_id_for_message, ChannelId, InterlinkDescriptor, ZMessage,
};
use grid_core::ProofSystem;

#[test]
fn interlink_program_id_deterministic() {
    let d1 = InterlinkDescriptor::v1();
    let d2 = InterlinkDescriptor::v1();
    assert_eq!(d1.program_id().expect("id1"), d2.program_id().expect("id2"));
}

#[test]
fn interlink_descriptor_round_trip() {
    let original = InterlinkDescriptor::v1();
    let bytes = original.encode_canonical().expect("encode");
    let decoded = InterlinkDescriptor::decode_canonical(&bytes).expect("decode");
    assert_eq!(original, decoded);
}

#[test]
fn zid_and_interlink_have_different_program_ids() {
    let zid_id = grid_programs_zid::ZidDescriptor::v1().program_id().expect("zid");
    let interlink_id = InterlinkDescriptor::v1().program_id().expect("interlink");
    assert_ne!(zid_id, interlink_id);
}

#[test]
fn program_topic_matches_descriptor_topic() {
    let desc = InterlinkDescriptor::v1();
    let pid = desc.program_id().expect("pid");
    assert_eq!(grid_core::program_topic(&pid), desc.topic().expect("topic"));
}

#[test]
fn channel_id_sector_deterministic() {
    let ch1 = ChannelId::from_str_id("general");
    let ch2 = ChannelId::from_str_id("general");
    assert_eq!(ch1.sector_id(), ch2.sector_id());
}

#[test]
fn different_channels_different_sectors() {
    let ch1 = ChannelId::from_str_id("general");
    let ch2 = ChannelId::from_str_id("random");
    assert_ne!(ch1.sector_id(), ch2.sector_id());
}

#[test]
fn sector_id_for_channel_consistency() {
    let ch = ChannelId::from_str_id("test-channel");
    let sid1 = sector_id_for_channel(&ch);
    let sid2 = ch.sector_id();
    assert_eq!(sid1.as_bytes(), sid2.as_bytes());
}

#[test]
fn zmessage_round_trip() {
    let msg = ZMessage {
        sender_did: "did:key:z6Mk...".into(),
        channel_id: ChannelId::from_str_id("general"),
        content: "Hello, world!".into(),
        timestamp_ms: 1700000000000,
        signature: vec![],
    };
    let bytes = msg.encode_canonical().expect("encode");
    let decoded = ZMessage::decode_canonical(&bytes).expect("decode");
    assert_eq!(msg, decoded);
}

#[test]
fn zmessage_size_limit() {
    let large_content = "x".repeat(100_000);
    let msg = ZMessage {
        sender_did: "did:key:z6Mk...".into(),
        channel_id: ChannelId::from_str_id("big"),
        content: large_content,
        timestamp_ms: 0,
        signature: vec![],
    };
    let result = msg.encode_canonical();
    assert!(result.is_err());
}

#[test]
fn test_channel_id_reserved() {
    let ch = ChannelId::from_str_id(crate::interlink::TEST_CHANNEL_ID);
    assert_eq!(ch.as_bytes(), b"INTERLINK-MAIN");
    let _ = ch.sector_id();
}

#[test]
fn sector_id_for_message_deterministic() {
    let ch = ChannelId::from_str_id("general");
    let s1 = sector_id_for_message(&ch, 1700000000000, "did:key:z6Mk...");
    let s2 = sector_id_for_message(&ch, 1700000000000, "did:key:z6Mk...");
    assert_eq!(s1, s2);
}

#[test]
fn sector_id_for_message_differs_by_timestamp() {
    let ch = ChannelId::from_str_id("general");
    let s1 = sector_id_for_message(&ch, 1700000000000, "did:key:z6Mk...");
    let s2 = sector_id_for_message(&ch, 1700000000001, "did:key:z6Mk...");
    assert_ne!(s1, s2);
}

#[test]
fn sector_id_for_message_differs_by_sender() {
    let ch = ChannelId::from_str_id("general");
    let s1 = sector_id_for_message(&ch, 1700000000000, "did:key:alice");
    let s2 = sector_id_for_message(&ch, 1700000000000, "did:key:bob");
    assert_ne!(s1, s2);
}

#[test]
fn sector_id_for_message_differs_by_channel() {
    let ch1 = ChannelId::from_str_id("general");
    let ch2 = ChannelId::from_str_id("random");
    let s1 = sector_id_for_message(&ch1, 1700000000000, "did:key:z6Mk...");
    let s2 = sector_id_for_message(&ch2, 1700000000000, "did:key:z6Mk...");
    assert_ne!(s1, s2);
}

#[test]
fn sector_id_for_message_differs_from_channel_sector() {
    let ch = ChannelId::from_str_id("general");
    let channel_sid = sector_id_for_channel(&ch);
    let msg_sid = sector_id_for_message(&ch, 0, "");
    assert_ne!(channel_sid, msg_sid);
}

#[test]
fn zmessage_empty_content() {
    let msg = ZMessage {
        sender_did: "did:key:z6Mk...".into(),
        channel_id: ChannelId::new(vec![1, 2, 3]),
        content: String::new(),
        timestamp_ms: 42,
        signature: vec![],
    };
    let bytes = msg.encode_canonical().expect("encode");
    let decoded = ZMessage::decode_canonical(&bytes).expect("decode");
    assert_eq!(msg, decoded);
}

#[test]
fn zmessage_with_signature_round_trip() {
    let msg = ZMessage {
        sender_did: "did:key:z6Mk...".into(),
        channel_id: ChannelId::from_str_id("general"),
        content: "signed message".into(),
        timestamp_ms: 1700000000000,
        signature: vec![0xDE, 0xAD, 0xBE, 0xEF],
    };
    let bytes = msg.encode_canonical().expect("encode");
    let decoded = ZMessage::decode_canonical(&bytes).expect("decode");
    assert_eq!(msg, decoded);
}

#[test]
fn zmessage_signable_bytes_excludes_signature() {
    let msg1 = ZMessage {
        sender_did: "did:key:z6Mk...".into(),
        channel_id: ChannelId::from_str_id("general"),
        content: "same content".into(),
        timestamp_ms: 1700000000000,
        signature: vec![1, 2, 3],
    };
    let msg2 = ZMessage {
        sender_did: "did:key:z6Mk...".into(),
        channel_id: ChannelId::from_str_id("general"),
        content: "same content".into(),
        timestamp_ms: 1700000000000,
        signature: vec![4, 5, 6],
    };
    let sb1 = msg1.signable_bytes().expect("signable_bytes 1");
    let sb2 = msg2.signable_bytes().expect("signable_bytes 2");
    assert_eq!(sb1, sb2);
}

#[test]
fn interlink_field_schema_has_signature_field() {
    let schema = InterlinkDescriptor::field_schema();
    let has_sig = schema.fields.iter().any(|f| f.key == "signature");
    assert!(has_sig, "Interlink field schema must contain a 'signature' field");
}

#[test]
fn interlink_v2_descriptor_has_proof_system() {
    let desc = InterlinkDescriptor::v2();
    assert_eq!(desc.proof_system, Some(ProofSystem::Groth16));
}
