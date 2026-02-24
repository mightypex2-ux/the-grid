use zero_neural::HybridSignature;
use zfs_core::*;

fn test_signature() -> HybridSignature {
    HybridSignature {
        ed25519: [0x42; 64],
        ml_dsa: vec![0x43; HybridSignature::ML_DSA_65_LEN],
    }
}

// --- Cid ---

#[test]
fn cid_from_ciphertext_is_deterministic() {
    let data = b"hello world ciphertext";
    let cid1 = Cid::from_ciphertext(data);
    let cid2 = Cid::from_ciphertext(data);
    assert_eq!(cid1, cid2);
}

#[test]
fn cid_different_data_different_ids() {
    let a = Cid::from_ciphertext(b"alpha");
    let b = Cid::from_ciphertext(b"beta");
    assert_ne!(a, b);
}

#[test]
fn cid_hex_roundtrip() {
    let cid = Cid::from_ciphertext(b"test data for hex");
    let hex = cid.to_hex();
    assert_eq!(hex.len(), 64);
    let parsed = Cid::from_hex(&hex).unwrap();
    assert_eq!(cid, parsed);
}

#[test]
fn cid_from_bytes() {
    let bytes = [0xAB; 32];
    let cid = Cid::from(bytes);
    assert_eq!(*cid.as_bytes(), bytes);
}

#[test]
fn cid_cbor_roundtrip() {
    let cid = Cid::from_ciphertext(b"cbor test");
    let encoded = encode_canonical(&cid).unwrap();
    let decoded: Cid = decode_canonical(&encoded).unwrap();
    assert_eq!(cid, decoded);
}

#[test]
fn cid_display_is_hex() {
    let cid = Cid::from([0x00; 32]);
    assert_eq!(format!("{cid}"), "0".repeat(64));
}

// --- SectorId ---

#[test]
fn sector_id_cbor_roundtrip() {
    let sid = SectorId::from_bytes(vec![1, 2, 3, 4, 5]);
    let encoded = encode_canonical(&sid).unwrap();
    let decoded: SectorId = decode_canonical(&encoded).unwrap();
    assert_eq!(sid, decoded);
}

#[test]
fn sector_id_empty_roundtrip() {
    let sid = SectorId::from_bytes(vec![]);
    let encoded = encode_canonical(&sid).unwrap();
    let decoded: SectorId = decode_canonical(&encoded).unwrap();
    assert_eq!(sid, decoded);
}

#[test]
fn sector_id_hex() {
    let sid = SectorId::from_bytes(vec![0xDE, 0xAD]);
    assert_eq!(sid.to_hex(), "dead");
}

// --- ProgramId ---

#[test]
fn program_id_from_descriptor_deterministic() {
    let desc = ProgramDescriptor {
        name: "test-program".into(),
        version: "1.0.0".into(),
    };
    let id1 = desc.program_id().unwrap();
    let id2 = desc.program_id().unwrap();
    assert_eq!(id1, id2);
}

#[test]
fn program_id_different_descriptors_different_ids() {
    let d1 = ProgramDescriptor {
        name: "alpha".into(),
        version: "1.0".into(),
    };
    let d2 = ProgramDescriptor {
        name: "beta".into(),
        version: "1.0".into(),
    };
    assert_ne!(d1.program_id().unwrap(), d2.program_id().unwrap());
}

#[test]
fn program_id_hex_roundtrip() {
    let desc = ProgramDescriptor {
        name: "hex-test".into(),
        version: "0.1".into(),
    };
    let id = desc.program_id().unwrap();
    let hex = id.to_hex();
    assert_eq!(hex.len(), 64);
    let parsed = ProgramId::from_hex(&hex).unwrap();
    assert_eq!(id, parsed);
}

#[test]
fn program_id_cbor_roundtrip() {
    let id = ProgramId::from([0xFF; 32]);
    let encoded = encode_canonical(&id).unwrap();
    let decoded: ProgramId = decode_canonical(&encoded).unwrap();
    assert_eq!(id, decoded);
}

// --- ProgramDescriptor ---

#[test]
fn program_descriptor_cbor_roundtrip() {
    let desc = ProgramDescriptor {
        name: "my-program".into(),
        version: "0.1.0".into(),
    };
    let encoded = desc.encode_canonical().unwrap();
    let decoded = ProgramDescriptor::decode_canonical(&encoded).unwrap();
    assert_eq!(desc, decoded);
}

#[test]
fn program_descriptor_canonical_encoding_is_deterministic() {
    let desc = ProgramDescriptor {
        name: "stable".into(),
        version: "2.0".into(),
    };
    let enc1 = desc.encode_canonical().unwrap();
    let enc2 = desc.encode_canonical().unwrap();
    assert_eq!(enc1, enc2);
}

// --- Head ---

#[test]
fn head_cbor_roundtrip_minimal() {
    let head = Head {
        sector_id: SectorId::from_bytes(vec![10, 20, 30]),
        cid: Cid::from_ciphertext(b"payload"),
        version: 1,
        program_id: ProgramId::from([0xAA; 32]),
        prev_head_cid: None,
        timestamp_ms: 1_700_000_000_000,
        signature: None,
    };
    let encoded = head.encode_canonical().unwrap();
    let decoded = Head::decode_canonical(&encoded).unwrap();
    assert_eq!(head, decoded);
}

#[test]
fn head_cbor_roundtrip_with_prev_cid() {
    let head = Head {
        sector_id: SectorId::from_bytes(vec![1]),
        cid: Cid::from_ciphertext(b"v2"),
        version: 2,
        program_id: ProgramId::from([0xBB; 32]),
        prev_head_cid: Some(Cid::from_ciphertext(b"v1")),
        timestamp_ms: 1_700_000_001_000,
        signature: None,
    };
    let encoded = head.encode_canonical().unwrap();
    let decoded = Head::decode_canonical(&encoded).unwrap();
    assert_eq!(head, decoded);
}

#[test]
fn head_cbor_roundtrip_with_signature() {
    let head = Head {
        sector_id: SectorId::from_bytes(vec![5, 6]),
        cid: Cid::from_ciphertext(b"signed payload"),
        version: 3,
        program_id: ProgramId::from([0xCC; 32]),
        prev_head_cid: Some(Cid::from_ciphertext(b"prev")),
        timestamp_ms: 1_700_000_002_000,
        signature: Some(test_signature()),
    };
    let encoded = head.encode_canonical().unwrap();
    let decoded = Head::decode_canonical(&encoded).unwrap();
    assert_eq!(head, decoded);
}

// --- ErrorCode / ZfsError ---

#[test]
fn error_code_to_zfs_error_roundtrip() {
    let codes = [
        ErrorCode::StorageFull,
        ErrorCode::ProofInvalid,
        ErrorCode::PolicyReject,
        ErrorCode::NotFound,
        ErrorCode::InvalidPayload,
        ErrorCode::ProgramMismatch,
    ];
    for code in codes {
        let err: ZfsError = code.into();
        assert_eq!(err.error_code(), Some(code));
    }
}

#[test]
fn error_code_cbor_roundtrip() {
    let code = ErrorCode::ProofInvalid;
    let encoded = encode_canonical(&code).unwrap();
    let decoded: ErrorCode = decode_canonical(&encoded).unwrap();
    assert_eq!(code, decoded);
}

#[test]
fn zfs_error_io_has_no_code() {
    let err = ZfsError::Io(std::io::Error::new(std::io::ErrorKind::NotFound, "gone"));
    assert_eq!(err.error_code(), None);
}

// --- KeyEnvelope ---

#[test]
fn key_envelope_cbor_roundtrip() {
    let envelope = KeyEnvelope {
        entries: vec![KeyEnvelopeEntry {
            recipient_did: "did:key:z6Mktest123".into(),
            sender_x25519_public: vec![0xAA; 32],
            mlkem_ciphertext: vec![0xBB; 1088],
            wrapped_key: vec![0xCC; 60],
        }],
    };
    let encoded = encode_canonical(&envelope).unwrap();
    let decoded: KeyEnvelope = decode_canonical(&encoded).unwrap();
    assert_eq!(envelope, decoded);
}

#[test]
fn key_envelope_multiple_entries() {
    let envelope = KeyEnvelope {
        entries: vec![
            KeyEnvelopeEntry {
                recipient_did: "did:key:recipient1".into(),
                sender_x25519_public: vec![0x01; 32],
                mlkem_ciphertext: vec![0x02; 1088],
                wrapped_key: vec![0x03; 60],
            },
            KeyEnvelopeEntry {
                recipient_did: "did:key:recipient2".into(),
                sender_x25519_public: vec![0x04; 32],
                mlkem_ciphertext: vec![0x05; 1088],
                wrapped_key: vec![0x06; 60],
            },
        ],
    };
    let encoded = encode_canonical(&envelope).unwrap();
    let decoded: KeyEnvelope = decode_canonical(&encoded).unwrap();
    assert_eq!(envelope, decoded);
}

// --- Protocol messages ---

#[test]
fn store_response_success_roundtrip() {
    let resp = StoreResponse {
        ok: true,
        error_code: None,
    };
    let encoded = encode_canonical(&resp).unwrap();
    let decoded: StoreResponse = decode_canonical(&encoded).unwrap();
    assert_eq!(resp, decoded);
}

#[test]
fn store_response_error_roundtrip() {
    let resp = StoreResponse {
        ok: false,
        error_code: Some(ErrorCode::StorageFull),
    };
    let encoded = encode_canonical(&resp).unwrap();
    let decoded: StoreResponse = decode_canonical(&encoded).unwrap();
    assert_eq!(resp, decoded);
}

#[test]
fn store_request_cbor_roundtrip() {
    let req = StoreRequest {
        program_id: ProgramId::from([0x11; 32]),
        cid: Cid::from_ciphertext(b"test block"),
        ciphertext: b"test block".to_vec(),
        head: None,
        proof: None,
        key_envelope: None,
        machine_did: "did:key:zTestMachine".into(),
        signature: test_signature(),
    };
    let encoded = encode_canonical(&req).unwrap();
    let decoded: StoreRequest = decode_canonical(&encoded).unwrap();
    assert_eq!(req, decoded);
}

#[test]
fn store_request_with_all_fields() {
    let req = StoreRequest {
        program_id: ProgramId::from([0x22; 32]),
        cid: Cid::from_ciphertext(b"full block"),
        ciphertext: b"full block".to_vec(),
        head: Some(Head {
            sector_id: SectorId::from_bytes(vec![1, 2]),
            cid: Cid::from_ciphertext(b"full block"),
            version: 1,
            program_id: ProgramId::from([0x22; 32]),
            prev_head_cid: None,
            timestamp_ms: 1_700_000_000_000,
            signature: None,
        }),
        proof: Some(vec![0xDE, 0xAD, 0xBE, 0xEF]),
        key_envelope: Some(KeyEnvelope {
            entries: vec![KeyEnvelopeEntry {
                recipient_did: "did:key:zRecip".into(),
                sender_x25519_public: vec![0xAA; 32],
                mlkem_ciphertext: vec![0xBB; 1088],
                wrapped_key: vec![0xCC; 60],
            }],
        }),
        machine_did: "did:key:zSender".into(),
        signature: test_signature(),
    };
    let encoded = encode_canonical(&req).unwrap();
    let decoded: StoreRequest = decode_canonical(&encoded).unwrap();
    assert_eq!(req, decoded);
}

#[test]
fn fetch_request_by_cid_roundtrip() {
    let req = FetchRequest {
        program_id: ProgramId::from([0x33; 32]),
        by_cid: Some(Cid::from_ciphertext(b"wanted")),
        by_sector_id: None,
        machine_did: None,
        signature: None,
    };
    let encoded = encode_canonical(&req).unwrap();
    let decoded: FetchRequest = decode_canonical(&encoded).unwrap();
    assert_eq!(req, decoded);
}

#[test]
fn fetch_request_by_sector_id_roundtrip() {
    let req = FetchRequest {
        program_id: ProgramId::from([0x44; 32]),
        by_cid: None,
        by_sector_id: Some(SectorId::from_bytes(vec![9, 8, 7])),
        machine_did: Some("did:key:zFetcher".into()),
        signature: Some(test_signature()),
    };
    let encoded = encode_canonical(&req).unwrap();
    let decoded: FetchRequest = decode_canonical(&encoded).unwrap();
    assert_eq!(req, decoded);
}

#[test]
fn fetch_response_with_data_roundtrip() {
    let resp = FetchResponse {
        ciphertext: Some(vec![1, 2, 3, 4, 5]),
        head: Some(Head {
            sector_id: SectorId::from_bytes(vec![10]),
            cid: Cid::from_ciphertext(&[1, 2, 3, 4, 5]),
            version: 7,
            program_id: ProgramId::from([0x55; 32]),
            prev_head_cid: None,
            timestamp_ms: 1_700_000_003_000,
            signature: None,
        }),
        error_code: None,
    };
    let encoded = encode_canonical(&resp).unwrap();
    let decoded: FetchResponse = decode_canonical(&encoded).unwrap();
    assert_eq!(resp, decoded);
}

#[test]
fn fetch_response_not_found_roundtrip() {
    let resp = FetchResponse {
        ciphertext: None,
        head: None,
        error_code: Some(ErrorCode::NotFound),
    };
    let encoded = encode_canonical(&resp).unwrap();
    let decoded: FetchResponse = decode_canonical(&encoded).unwrap();
    assert_eq!(resp, decoded);
}
