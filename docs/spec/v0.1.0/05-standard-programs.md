# ZFS v0.1.0 — Standard programs (ZID and Z Chat)

## Purpose

This document defines the two standard programs for v0.1.0: **ZID (Zero Identity)** and **Z Chat**. Both use `ProgramDescriptor`, canonical message formats, and optional proof/signature requirements. Where these live (e.g. `zfs-programs` or dedicated modules) and encoding (e.g. canonical CBOR) are specified.

## ZID (Zero Identity)

- **Description:** Minimal identity program; versioned; canonical encoding; optional ZK conformance.
- **ProgramDescriptor:** Includes at least: program name/type (e.g. "zid"), version (e.g. 1), optional `proof_required: bool`.
- **Message types:** e.g. `ZidMessage` — identity claim or update; canonical CBOR.
- **Proof requirements:** Optional ZK conformance; if required, Valid-Sector proof per [04-proof](04-proof.md).
- **Size limits:** Implementation-defined (e.g. max message size per program).

## Z Chat

- **Description:** Structured messages; canonical encoding; size bounds; optional signature policy (inside or outside ZK).
- **ProgramDescriptor:** Includes at least: program name/type (e.g. "zchat"), version, optional proof/signature flags.
- **Message types:** e.g. `ZChatMessage` — structured chat message; canonical CBOR; fields (sender, content, timestamp, etc.) implementation-defined.
- **Signature policy:** Optional signature validation (in-ZK or out-of-ZK); spec defines message format and size bounds; signature policy is optional/TBD.
- **Size limits:** Max message size (e.g. 64 KiB) per message; document in implementation.

## Shared requirements

- **ProgramDescriptor:** Both use the base `ProgramDescriptor` from [11-core-types](11-core-types.md); canonical encoding and `program_id()` / `topic()` per [03-programs-and-topics](03-programs-and-topics.md).
- **Canonical message format:** CBOR (deterministic) for all message types; same choice as core types and protocol.
- **Proof requirement flags:** Per-program; Zode and SDK use these to decide whether to require proof (see [04-proof](04-proof.md), [06-zode](06-zode.md), [09-sdk](09-sdk.md)).

## Interfaces (summary)

```rust
// ZID
pub struct ZidDescriptor { /* extends ProgramDescriptor */ }
pub struct ZidMessage { /* identity claim/update; canonical CBOR */ }

// Z Chat
pub struct ZChatDescriptor { /* extends ProgramDescriptor */ }
pub struct ZChatMessage {
    // e.g. sender, content, timestamp; canonical CBOR
    // size limit: e.g. 64 KiB
}
```

- **Size limits:** Document in crate (e.g. `ZChatMessage::MAX_SIZE`).
- **Proof requirement:** `proof_required: bool` or enum in descriptor; Zode enforces when storing.

## Diagrams (optional)

- **Message flow:** Client builds ZidMessage/ZChatMessage → canonical encode → encrypt (if sector payload) → store with optional proof.
- **Validation:** Zode receives store → (optional) verify proof → validate message format and size → persist.

## Implementation

- **Location:** `zfs-programs` or dedicated modules (e.g. `zfs-programs::zid`, `zfs-programs::zchat`). Same crate as program identity and topic.
- **Encoding:** Canonical CBOR for descriptors and messages; consistent with [11-core-types](11-core-types.md) and [12-protocol](12-protocol.md).
- **SDK helpers:** [09-sdk](09-sdk.md) provides ZID and Z Chat helper APIs (e.g. create descriptor, build message, upload).
