# ZFS v0.1.0 — Sector Protocol Requirements

## 1. Purpose

The sector protocol is a metadata-private storage layer that reduces the Zode to an opaque, program-scoped key-value store. Clients derive deterministic sector IDs via HKDF and store encrypted blobs. The Zode sees only `(program_id, sector_id, encrypted_blob)` — it cannot determine authorship, readership, or blob relationships. This is the privacy-maximizing alternative to the transparent base protocol defined in 12-protocol.

## 2. Design Goals

- **Payload confidentiality**: Zodes never see plaintext.
- **Metadata privacy**: Zodes cannot determine sender identity, recipient set, logical grouping, ordering, or activity patterns.
- **Program-scoped routing**: `program_id` remains visible for routing and subscription. Hiding it would require private information retrieval (out of scope).
- **Simple storage primitive**: Flat key-value store — no content addressing, head tracking, or proof verification.
- **Client-managed state**: All application state is maintained client-side; clients reconstruct from the network as needed.

## 3. Threat Model

Protects against an **honest-but-curious Zode** (or network observer) that stores/serves correctly but attempts to learn metadata. The Zode can observe all writes, reads, sector IDs, blob sizes, timing, and IP addresses. It cannot break the underlying cryptography (XChaCha20-Poly1305, X25519, ML-KEM-768).

**Known limitations (v0.1.0):**

- No transport anonymity — IP addresses are visible.
- Timing correlation — batch requests reveal co-accessed sector IDs. Clients MAY include decoy IDs as a countermeasure.
- The `overwrite` flag is visible on the wire (distinguishes write-once from mutable slots).
- Active attacks (data dropping/modification by a malicious Zode) are out of scope.

## 4. Sector ID

### 4.1 Format

Sector IDs are 32 bytes (HKDF-SHA256 output), carried in the existing variable-length `SectorId` type. Zodes MAY reject non-32-byte IDs.

### 4.2 Derivation

Two-step HKDF-SHA256 to ensure domain separation between ID derivation and payload encryption:

1. **Derivation key**: `HKDF(ikm=shared_secret, salt="zfs:sector:v1", info="zfs:sector:derive-key:v1")`
2. **Sector ID**: `HKDF(ikm=derivation_key, salt="zfs:sector:v1", info=application_defined_info_string)`

The `shared_secret` is typically a `SectorKey` or derived from one. The intermediate derivation key MUST be zeroized after use. The raw `SectorKey` is never used directly as HKDF input for sector IDs — it is reserved exclusively for AEAD encryption.

### 4.3 Properties

- **Deterministic**: All parties sharing the same secret and info string compute the same sector ID.
- **Unlinkable**: Different info strings produce unrelated IDs; the Zode cannot determine two IDs share a secret.
- **Collision-resistant**: 32-byte output; negligible collision probability.
- **Domain-separated**: Fixed salt `"zfs:sector:v1"` prevents cross-use collisions.

### 4.4 Info String Conventions

Format: `"zfs:{program_short_name}:{purpose}:{...application_fields}"`

Rules:
- MUST begin with `"zfs:"`.
- Variable-length fields (IDs, hashes) MUST be hex-encoded to avoid delimiter collisions.
- MUST NOT reuse the same info string for slots with different semantics.

## 5. Wire Protocol

### 5.1 Protocol String

`/zfs/sector/1.0.0` — separate from the base protocol (`/zfs/1.0.0`). A Zode MAY serve both simultaneously. Serialization is canonical CBOR.

### 5.2 Request/Response Enums

Top-level `SectorRequest` and `SectorResponse` enums, separate from the base protocol, with four operation variants each: Store, Fetch, BatchStore, BatchFetch.

### 5.3 Error Codes

The base `ErrorCode` enum is extended with sector-specific variants:
- **SlotOccupied** — write-once slot already written.
- **BatchTooLarge** — batch exceeds entry or payload limits.
- **ConditionFailed** — `expected_hash` mismatch during compare-and-swap.

Internal `SectorStoreError` additionally covers Backend, Encode, and Decode errors (mapped to `InvalidPayload` on the wire).

### 5.4 Single Store

Fields: `program_id`, `sector_id`, `payload`, `overwrite`, `expected_hash` (optional), `ttl_seconds` (optional).

Semantics:
- `overwrite=false`: Write-once. Rejects with `SlotOccupied` if key exists.
- `overwrite=true, expected_hash=None`: Unconditional last-write-wins overwrite.
- `overwrite=true, expected_hash=Some(hash)`: Compare-and-swap — Zode computes SHA-256 of current payload, rejects with `ConditionFailed` on mismatch.
- `expected_hash` with `overwrite=false` is invalid → `InvalidPayload`.
- `ttl_seconds`: Optional eviction hint (not a guarantee). Zode MAY ignore it. Clients must tolerate early eviction or retention beyond the TTL.

Response: `ok` boolean + optional `ErrorCode`.

### 5.5 Single Fetch

Fields: `program_id`, `sector_id`.

Response: `payload` (None if sector unwritten, not an error) + optional `ErrorCode`.

### 5.6 Batch Store

Single `program_id` + up to 64 entries (each with `sector_id`, `payload`, `overwrite`, `expected_hash`, `ttl_seconds`).

Response: parallel results array (per-entry `ok` + optional error) + optional batch-level error. Each entry is independent — one failure does not affect others.

### 5.7 Batch Fetch

Single `program_id` + up to 64 `sector_id`s.

Response: parallel results array of optional payloads (None = unwritten) + optional batch-level error.

### 5.8 Batch Limits

- Maximum **64 entries** per batch.
- Maximum **4 MB total payload** per batch. Exceeding this → `BatchTooLarge`.
- Each entry within a batch is independent (partial success allowed).

## 6. Payload Encryption

All payloads MUST be encrypted client-side before storage. The Zode stores opaque bytes.

### 6.1 Encryption

XChaCha20-Poly1305 with `SectorKey`, random 192-bit nonce. **AAD is mandatory**: `program_id (32B) || sector_id (32B)`, optionally followed by additional application context. AAD binds ciphertext to its program and slot, preventing cross-program and cross-slot relocation attacks.

### 6.2 Padding

Clients MUST pad serialized content to fixed buckets before encryption to resist size analysis:

| Content size | Padded to |
|---|---|
| 0–1 KB | 1 KB |
| 1–4 KB | 4 KB |
| 4–16 KB | 16 KB |
| 16–64 KB | 64 KB |
| 64–256 KB | 256 KB |

All slot types MUST use the same buckets so they are indistinguishable by size. Minimum padded size: 1 KB.

Scheme: 4-byte little-endian length prefix + content + zero-fill to bucket boundary. On decryption, read length prefix, extract content, discard trailing zeros.

## 7. Zode Storage Model

### 7.1 Backend

Single RocksDB column family `sectors` with 64-byte composite key (`program_id || sector_id`) → encrypted blob. Added alongside the base protocol's existing column families.

### 7.2 Operations

- **put**: Write blob. Write-once rejects with `SlotOccupied` if key exists. Mutable with `expected_hash` does SHA-256 compare-and-swap (atomically via read-modify-write). Mutable without `expected_hash` overwrites unconditionally.
- **get**: Return payload or None.
- **batch_put**: Up to 64 independent puts (partial success allowed).
- **batch_get**: Up to 64 independent gets.
- **stats**: Returns DB size, slot count, program count.

Delete is **local-only** for Zode garbage collection — not exposed on the wire.

### 7.3 Policy Enforcement

**Zode CAN enforce:**
- Per-program storage quotas (total bytes per `program_id`).
- Per-slot size limits (e.g., 256 KB max).
- Program allowlist (only accept writes for subscribed programs).
- Rate limiting (recommended default: 100 req/s per connection).

**Zode CANNOT enforce (by design):**
- Per-group or per-user limits (invisible concepts).
- Payload format validation (payload is opaque).
- Write authorization (any connected client can write to any sector within an allowed program).

## 8. Replication

Zode-to-Zode via GossipSub. On accepting a client store, the Zode stores locally and publishes a `GossipSector` message to the program's GossipSub topic (`prog/{program_id_hex}`). Receiving Zodes store automatically. Clients do not subscribe to GossipSub topics — they discover new writes by polling.

### 8.1 GossipSector Message

Contains: `program_id`, `sector_id`, `payload`, `overwrite`, optional `ttl_seconds`. The `expected_hash` field is intentionally omitted — CAS is a client-to-Zode concern and does not propagate via gossip.

### 8.2 Receiving Zode Behavior

1. **Program check**: Discard if Zode does not serve the `program_id`.
2. **Write-once** (`overwrite=false`): Store if empty; silently ignore if occupied (idempotent).
3. **Mutable** (`overwrite=true`): Overwrite unconditionally (last-write-wins). No CAS check for gossip writes.
4. **Deduplication**: For mutable slots, Zode MAY compare hashes to skip identical writes (optimization, not required).
5. **Policy enforcement**: Quotas and size limits apply. Exceeding blobs are silently dropped (no gossip error path).
6. **No re-gossip**: GossipSub's mesh propagation handles fan-out. Receiving Zodes do NOT re-publish.

### 8.3 Replication Semantics

- **Write-once slots**: First-write-wins globally. Gossip to occupied slots is silently dropped.
- **Mutable slots (unconditional)**: Last-write-wins. Concurrent writes may resolve differently across Zodes until gossip converges.
- **Mutable slots (CAS)**: CAS is enforced only at the client's connected Zode. The winning write propagates via gossip with unconditional overwrite. Simultaneous CAS to different Zodes for the same slot can both succeed locally; gossip resolves via last-write-wins. Applications needing stronger consistency must implement conflict resolution above this layer.
- **TTL**: Propagated in gossip. Receiving Zodes MAY use it for eviction.

### 8.4 Optional Client Multi-Send

Clients MAY send the same store request to multiple Zodes directly for faster initial availability. At-least-one-success semantics. Gossip handles eventual replication automatically.

## 9. Notification and Polling

The sector protocol is **pull-based** in v0.1.0. Clients discover new writes by polling `batch_get` on known sector IDs.

### 9.1 Polling Guidance

- Clients SHOULD use exponential backoff when no new data is found, up to a maximum interval (e.g., 30 seconds).
- Clients SHOULD include decoy sector IDs in batch fetch requests to mask which IDs they are actually interested in.
- Clients SHOULD avoid polling from a single connection at high frequency to limit timing correlation.

### 9.2 Future: Push Notifications (Deferred)

A future version MAY add a lightweight notification mechanism where Zodes push slot-update events (sector ID + timestamp only, no payload) to subscribed clients. Deferred from v0.1.0 because push subscriptions reveal which sector IDs a client is interested in, creating a metadata correlation vector.

## 10. Protocol Coexistence

### 10.1 Multiplexing

Both `/zfs/1.0.0` and `/zfs/sector/1.0.0` are registered as separate libp2p request-response protocols on the same Swarm. Multistream-select handles routing by protocol string.

### 10.2 Shared Infrastructure

- **Swarm**: Shared.
- **GossipSub topics**: Shared (`prog/{program_id_hex}`). Base publishes `GossipBlock`; sector publishes `GossipSector`. Distinguished by CBOR tag. Only Zodes subscribe; clients use request-response polling.
- **RocksDB**: Shared instance. Sector uses its own `sectors` column family.
- **Policy engine**: Shared. Program allowlist and quotas apply uniformly.
- **Metrics**: Separate counters (`sector_puts_total`, `sector_gets_total`, etc.).

### 10.3 Discovery

Clients discover supported protocols via libp2p Identify (advertised protocol strings). Clients fall back to the base protocol if sector is not advertised.

## 11. Versioning

Protocol string follows semver (`/zfs/sector/{major}.{minor}.{patch}`).

- **Major**: Breaking wire-format changes. Incompatible with prior major versions.
- **Minor**: Additive changes (new optional fields, new request types). Backward-compatible. Unknown CBOR fields are silently ignored via `#[serde(default)]`.
- **Patch**: Spec language fixes. No wire changes.

A Zode MAY advertise multiple versions. Clients select the highest mutually supported version via multistream-select. Upgrade path: release new major alongside old, dual-advertise during transition, deprecate, then remove.

## 12. Visibility Summary

**Zode CAN see**: `program_id`, `sector_id` (opaque 32B), post-encryption payload size (mitigated by padding), `overwrite` flag, `expected_hash` presence, `ttl_seconds` value, write/read timing, co-batched sector IDs, client IP address.

**Zode CANNOT see**: Who wrote or can read a blob, which blobs are related, blob content/structure, ordering/versioning/timestamps, number of logical groups.

## 13. v0.1.0 Limitations

- No transport anonymity (IP visible).
- Timing and batch correlation leaks access patterns. Decoys are a partial countermeasure (not mandatory).
- No Zode-side payload validation — applications must validate after decryption.
- No wire-level write authorization — any connected client can write to any sector in an allowed program.
- Write-once slot squatting by attackers who can predict sector IDs. Mitigations: high-entropy info strings, prefer mutable slots with CAS where contention is possible.
- Polling latency trade-off (bandwidth vs. freshness).
- CAS is local to a single Zode, not global. Gossip resolves via last-write-wins.
- `expected_hash` reveals conditional-write intent to the Zode.
- Gossip propagation delay creates a window of single-Zode availability. Client multi-send is the mitigation.
- `ttl_seconds` is a non-binding hint.

## 14. Future Enhancements

- Decoy traffic (periodic writes to random sector IDs).
- Epoch-based ID rotation to prevent long-lived slot tracking.
- Private information retrieval (fetch without revealing sector ID).
- Write authorization tokens (blind-signature-based).
- Push notifications for slot-update events.

## 15. Implementation Crate Responsibilities

- **zfs-core**: `GossipSector`, sector wire types (`SectorRequest`/`SectorResponse` and inner structs), extended `ErrorCode` enum, `SectorStoreError`.
- **zfs-crypto**: `derive_sector_id` (two-step HKDF), `pad_to_bucket`/`unpad_from_bucket`. Existing `SectorKey` and `encrypt_sector`/`decrypt_sector` are reused unchanged.
- **zfs-storage**: `SectorStore` trait (put, get, batch_put, batch_get, stats). `RocksStorage` adds `sectors` column family. CAS put uses atomic read-modify-write.
- **zfs-net**: Registers `/zfs/sector/1.0.0` as separate request-response protocol. Extends GossipSub handling for `GossipSector` (distinguished by CBOR tag). Publishes gossip on successful client store.
- **zfs-zode**: Sector request handler (no CID/proof/head logic). Idempotent gossip handling. Policy enforcement for both client and gossip writes.
- **zfs-sdk**: `derive_sector_id`, padding helpers, batch helpers, optional multi-send, convenience `sector_encrypt` (pad + AAD + encrypt in one call). Application logic lives above this layer.
