# The Grid v0.1.0 — Sector Protocol

## Purpose

This document specifies the **sector protocol**, a metadata-private storage layer for the Grid. The sector protocol reduces the Zode to an opaque key-value store scoped by program. Clients derive deterministic sector IDs via HKDF and store encrypted blobs. The Zode sees only `(program_id, sector_id, encrypted_blob)` — it cannot determine who wrote a blob, who can read it, or which blobs are related.

This is an alternative to the transparent protocol defined in [12-protocol](12-protocol.md), where heads, CIDs, sender identities, and signatures are visible on the wire.

## Design goals

| Goal | Description |
|------|-------------|
| **Payload confidentiality** | Zodes never see plaintext. (Unchanged from base spec.) |
| **Metadata privacy** | Zodes cannot determine sender identity, recipient set, logical grouping, ordering, or activity patterns. |
| **Program-scoped routing** | `program_id` remains visible. Zodes subscribe to and route by program. Hiding the program would require private information retrieval and is out of scope. |
| **Simple storage primitive** | The Zode implements a flat key-value store. No content addressing, no head tracking, no proof verification. |
| **Client-managed state** | Clients maintain all application state locally and reconstruct from the network when needed. |

## Threat model

The sector protocol protects against an **honest-but-curious Zode** (or network observer) that:

- Stores and serves data correctly but attempts to learn metadata.
- Can observe all writes and reads (sector IDs, blob sizes, timing).
- Can correlate traffic by IP address and connection timing.
- Cannot break the underlying cryptography (XChaCha20-Poly1305, X25519, ML-KEM-768).

**Known limitations (v0.1.0):**

- **No transport anonymity.** IP addresses are visible. Onion routing and mixnets are out of scope.
- **Timing correlation.** A Zode can correlate writes and reads that arrive close together from the same connection. Batch requests reveal which sector IDs are accessed together. Countermeasure: clients MAY include decoy sector IDs in batch requests.
- **Overwrite flag.** The `overwrite` flag in store requests is visible on the wire. A Zode can distinguish write-once slots from mutable slots. Applications should account for this when designing their slot layout.
- **Active attacks out of scope.** Integrity is assumed at the transport layer. A malicious Zode that drops or modifies data is not in the threat model.

## Architecture overview

```
┌──────────────────────────────────────────────────────┐
│                    Zode's view                        │
│                                                      │
│   program_id   │   sector_id    │   payload          │
│   (visible)    │   (opaque 32B)  │   (encrypted blob) │
│────────────────┼─────────────────┼────────────────────│
│   0xaa..       │   0x3f..        │   [1 KB]           │
│   0xaa..       │   0x71..        │   [1 KB]           │
│   0xaa..       │   0xb2..        │   [4 KB]           │
│   ...          │   ...           │   ...              │
└──────────────────────────────────────────────────────┘
```

The Zode is a **program-scoped key-value store**: `(program_id, sector_id) → encrypted_blob`. It cannot determine which rows are related, what type of data they contain, or who wrote or reads them.

## Sector ID

The sector protocol reuses the existing `SectorId` type from `grid-core`:

```rust
pub struct SectorId(Vec<u8>);
```

In the sector protocol, sector IDs are always **32 bytes** (HKDF-SHA256 output). The existing variable-length `SectorId` type accommodates this — the sector protocol simply produces 32-byte values. Zodes MAY reject sector protocol requests with non-32-byte sector IDs.

Sector IDs are derived client-side via HKDF-SHA256 from a derivation key and an application-defined info string. The Zode stores and retrieves blobs by `(program_id, sector_id)` without knowledge of the derivation inputs.

### Derivation

```
derivation_key = HKDF-SHA256(
    ikm  = shared_secret,
    salt = "grid:sector:v1",
    info = "grid:sector:derive-key:v1"
)

sector_id = HKDF-SHA256(
    ikm  = derivation_key,
    salt = "grid:sector:v1",
    info = application_defined_info_string
)
```

The `shared_secret` is typically a `SectorKey` or a value derived from one (see [10-crypto](10-crypto.md)). A **derivation key** is first extracted from the shared secret via HKDF to ensure the raw `SectorKey` is never used directly as HKDF input for sector IDs (it is reserved exclusively for AEAD encryption). This two-step process ensures cryptographic domain separation between sector ID derivation and payload encryption.

The `info` string is chosen by the application to produce distinct, non-colliding sector IDs for different slots. See [Info string conventions](#info-string-conventions) for recommended formats.

### Properties

- **Deterministic:** All parties sharing the same `shared_secret` and `info` string compute the same sector ID.
- **Unlinkable:** Different info strings produce unrelated sector IDs. The Zode cannot determine that two IDs were derived from the same secret.
- **Collision-resistant:** HKDF-SHA256 output is 32 bytes; collision probability is negligible.
- **Domain-separated:** The fixed salt `"grid:sector:v1"` prevents accidental collision with other HKDF uses of the same key material.

### `derive_sector_id`

```rust
pub fn derive_sector_id(shared_secret: &[u8; 32], info: &[u8]) -> SectorId;
```

Internally performs the two-step HKDF described above: derives the intermediate `derivation_key` from `shared_secret`, then derives the final `SectorId` from `derivation_key` + `info`. The intermediate key is zeroized after use. Implemented in `grid-crypto`.

### Info string conventions

Applications MUST construct info strings that are unambiguous and collision-free. The recommended format uses colon-separated structured fields:

```
"grid:{program_short_name}:{purpose}:{...application_fields}"
```

**Examples:**

| Application | Info string | Description |
|-------------|-------------|-------------|
| Interlink group inbox | `"grid:interlink:inbox:{group_id_hex}:{seq}"` | Per-message write-once slot in a group |
| Interlink group state | `"grid:interlink:state:{group_id_hex}"` | Mutable slot for group membership state |
| ZID profile | `"grid:zid:profile:{identity_id_hex}"` | Public profile blob |
| ZID device key announce | `"grid:zid:device:{identity_id_hex}:{machine_id_hex}:{epoch}"` | Per-device key publication |

Rules:
- Info strings MUST begin with `"grid:"`.
- Variable-length fields (IDs, hashes) MUST be hex-encoded to avoid delimiter collisions.
- Applications MUST NOT reuse the same info string for slots with different semantics.

## Wire protocol

### Protocol string

```
/grid/sector/1.0.0
```

Separate from the base protocol (`/grid/1.0.0`). A Zode MAY serve both protocols simultaneously (see [Protocol coexistence](#protocol-coexistence)). Serialization is canonical CBOR, matching [11-core-types](11-core-types.md).

### Request / Response enums

The sector protocol uses its own top-level enums, separate from the base protocol's `ZfsRequest` / `ZfsResponse`:

```rust
pub enum SectorRequest {
    Store(SectorStoreRequest),
    Fetch(SectorFetchRequest),
    BatchStore(SectorBatchStoreRequest),
    BatchFetch(SectorBatchFetchRequest),
}

pub enum SectorResponse {
    Store(SectorStoreResponse),
    Fetch(SectorFetchResponse),
    BatchStore(SectorBatchStoreResponse),
    BatchFetch(SectorBatchFetchResponse),
}
```

### Error codes

The sector protocol extends the base `ErrorCode` enum (defined in [11-core-types](11-core-types.md)) with sector-specific variants rather than defining a separate enum:

```rust
pub enum ErrorCode {
    // --- base protocol variants (unchanged) ---
    StorageFull,
    ProofInvalid,
    PolicyReject,
    NotFound,
    InvalidPayload,
    ProgramMismatch,
    // --- sector protocol additions ---
    SlotOccupied,      // write-once slot already written
    BatchTooLarge,     // batch exceeds entry or payload limits
    ConditionFailed,   // expected_hash did not match current content
}
```

Wire serialization distinguishes variants by integer tag in CBOR. Existing base protocol messages never produce the sector-specific variants; sector messages never produce `ProofInvalid`.

### `SectorStoreError`

The internal storage error type used by the `SectorStore` trait (not sent on the wire):

```rust
#[derive(Debug, Error)]
pub enum SectorStoreError {
    #[error("storage full")]
    StorageFull,
    #[error("slot occupied")]
    SlotOccupied,
    #[error("condition failed: expected hash mismatch")]
    ConditionFailed,
    #[error("policy reject")]
    PolicyReject,
    #[error("backend I/O error: {0}")]
    Backend(String),
    #[error("encode error: {0}")]
    Encode(String),
    #[error("decode error: {0}")]
    Decode(String),
}
```

`SectorStoreError` maps to wire `ErrorCode` variants where applicable; `Backend`, `Encode`, and `Decode` map to `InvalidPayload` on the wire.

### Single store

```rust
pub struct SectorStoreRequest {
    pub program_id: ProgramId,
    pub sector_id: SectorId,
    pub payload: Vec<u8>,
    pub overwrite: bool,
    pub expected_hash: Option<[u8; 32]>,
    pub ttl_seconds: Option<u64>,
}

pub struct SectorStoreResponse {
    pub ok: bool,
    pub error: Option<ErrorCode>,
}
```

- `overwrite: false` — write-once. Rejects with `SlotOccupied` if the key already exists.
- `overwrite: true` — mutable overwrite. If `expected_hash` is `None`, overwrite is unconditional (last-write-wins). If `expected_hash` is `Some(hash)`, the Zode computes `SHA-256(current_payload)` and rejects with `ConditionFailed` if it does not match. This provides compare-and-swap semantics for mutable slots.
- `expected_hash` with `overwrite: false` is invalid and rejected with `InvalidPayload`.
- `ttl_seconds` — optional hint to the Zode for how long the slot should be retained. The Zode MAY use this as an eviction hint but is not required to honor it. A value of `None` means no preference (Zode applies its default eviction policy). The TTL is **not** a guarantee — clients must tolerate early eviction or retention beyond the TTL.

### Single fetch

```rust
pub struct SectorFetchRequest {
    pub program_id: ProgramId,
    pub sector_id: SectorId,
}

pub struct SectorFetchResponse {
    pub payload: Option<Vec<u8>>,
    pub error: Option<ErrorCode>,
}
```

Returns `None` payload (no error) if the sector has not been written.

### Batch store

```rust
pub struct SectorBatchStoreRequest {
    pub program_id: ProgramId,
    pub entries: Vec<SectorBatchStoreEntry>,  // max 64
}

pub struct SectorBatchStoreEntry {
    pub sector_id: SectorId,
    pub payload: Vec<u8>,
    pub overwrite: bool,
    pub expected_hash: Option<[u8; 32]>,
    pub ttl_seconds: Option<u64>,
}

pub struct SectorBatchStoreResponse {
    pub results: Vec<SectorStoreResult>,      // parallel to entries
    pub error: Option<ErrorCode>,              // batch-level error only
}

pub struct SectorStoreResult {
    pub ok: bool,
    pub error: Option<ErrorCode>,              // per-entry error
}
```

`results[i]` corresponds to `entries[i]`. The top-level `error` field is set only for batch-level failures (e.g., `ProgramMismatch`, `BatchTooLarge`). Per-entry failures (e.g., `SlotOccupied`, `StorageFull`, `ConditionFailed`) appear in `results[i].error`.

### Batch fetch

```rust
pub struct SectorBatchFetchRequest {
    pub program_id: ProgramId,
    pub sector_ids: Vec<SectorId>,  // max 64
}

pub struct SectorBatchFetchResponse {
    pub results: Vec<Option<Vec<u8>>>,         // parallel to sector_ids
    pub error: Option<ErrorCode>,              // batch-level error only
}
```

`results[i]` corresponds to `sector_ids[i]`. A `None` entry means the sector has not been written (not an error).

### Batch limits

- Maximum **64 entries** per batch request. Clients split larger operations across multiple batches.
- Maximum **4 MB total payload** per batch request. The Zode MUST reject batches exceeding this limit with `BatchTooLarge`. This ensures that even with the largest padding bucket (256 KB per entry), batches stay within a reasonable size (64 × 256 KB = 16 MB worst case with decoys; 4 MB enforced on actual payload bytes).
- Each entry within a batch is independent — a failure on one entry does not affect others.

### Operation summary

| Operation | Wire type | Purpose |
|-----------|-----------|---------|
| `put` | `SectorStoreRequest` | Write a single slot |
| `get` | `SectorFetchRequest` | Read a single slot |
| `batch_put` | `SectorBatchStoreRequest` | Write up to 64 slots in one round trip |
| `batch_get` | `SectorBatchFetchRequest` | Read up to 64 slots in one round trip |

No machine_did, no signature, no head, no key_envelope, no CID on the wire.

## Payload encryption

All sector payloads MUST be encrypted before storage. The Zode stores opaque bytes; encryption and decryption are performed client-side.

### Encryption

Payloads are encrypted with a `SectorKey` using XChaCha20-Poly1305 (same primitives as [10-crypto](10-crypto.md)):

```
encrypted_payload = XChaCha20-Poly1305(
    key       = SectorKey,
    nonce     = random 192-bit,
    plaintext = padded(serialized_content),
    aad       = program_id (32 bytes) || sector_id (32 bytes)
)
```

**AAD is mandatory.** The AAD MUST include `program_id || sector_id` at minimum. This binds the ciphertext to both its program and its specific slot, preventing cross-program and cross-slot ciphertext relocation attacks. Applications MAY append additional context (e.g., a version tag) after the mandatory fields:

```
AAD = program_id (32 bytes) || sector_id (32 bytes) [|| additional_context]
```

### Padding

To resist payload-size analysis, clients MUST pad serialized content to fixed size buckets before encryption:

| Content size | Padded to |
|-------------|-----------|
| 0 – 1 KB   | 1 KB |
| 1 – 4 KB   | 4 KB |
| 4 – 16 KB  | 16 KB |
| 16 – 64 KB | 64 KB |
| 64 – 256 KB | 256 KB |

All slot types (data, metadata, control) MUST use the same padding buckets so they are indistinguishable by size. The minimum padded size is **1 KB**.

#### Padding scheme: length-prefix + zero-fill

Padding uses a **4-byte little-endian length prefix** followed by zero-fill to the bucket boundary:

```
padded = content_length (4 bytes, little-endian) || content || 0x00 * (bucket_size - 4 - content_length)
```

On decryption, the receiver reads the 4-byte length prefix, extracts `content[0..length]`, and discards the trailing zeros.

This replaces PKCS#7-style padding, which is limited to 255 bytes of padding and cannot reach the 1 KB minimum bucket size for small content. The length-prefix scheme supports arbitrary content sizes up to 2^32 - 1 bytes.

**Padding is implemented in `grid-crypto`** alongside `encrypt_sector` / `decrypt_sector`:

```rust
pub fn pad_to_bucket(content: &[u8]) -> Vec<u8>;
pub fn unpad_from_bucket(padded: &[u8]) -> Result<Vec<u8>, CryptoError>;
```

## Zode storage model

### Storage backend

| Column family | Key | Value |
|--------------|-----|-------|
| **sectors** | `program_id (32B) \|\| sector_id (32B)` | `payload (encrypted blob)` |

A single column family with 64-byte composite keys. No block store, head store, or program index is needed. This column family is added to the existing `RocksStorage` instance alongside the base protocol's `blocks`, `heads`, `program_index`, and `metadata` column families.

### Operations

| Operation | Behavior |
|-----------|----------|
| `put(program_id, sector_id, payload, overwrite, expected_hash)` | Write blob. If `overwrite=false` and key exists, reject with `SlotOccupied`. If `overwrite=true` and `expected_hash` is `Some`, compare `SHA-256(current)` and reject with `ConditionFailed` on mismatch. If `overwrite=true` and `expected_hash` is `None`, overwrite unconditionally. |
| `get(program_id, sector_id)` | Return payload if key exists, or `None`. |
| `batch_put(program_id, entries[])` | Up to 64 puts. Each entry is independent (partial success allowed). |
| `batch_get(program_id, sector_ids[])` | Up to 64 gets. Returns parallel array of payloads (`None` for missing keys). |

Delete is **local-only** for Zode garbage collection — not exposed on the wire. Zodes MAY use `ttl_seconds` hints from store requests as input to eviction decisions. Zodes MAY also implement time-based or size-based eviction policies independently.

### SectorStore trait

```rust
pub trait SectorStore {
    fn put(
        &self,
        program_id: &ProgramId,
        sector_id: &SectorId,
        payload: &[u8],
        overwrite: bool,
        expected_hash: Option<&[u8; 32]>,
    ) -> Result<(), SectorStoreError>;

    fn get(
        &self,
        program_id: &ProgramId,
        sector_id: &SectorId,
    ) -> Result<Option<Vec<u8>>, SectorStoreError>;

    fn batch_put(
        &self,
        program_id: &ProgramId,
        entries: &[SectorBatchStoreEntry],
    ) -> Result<Vec<SectorStoreResult>, SectorStoreError>;

    fn batch_get(
        &self,
        program_id: &ProgramId,
        sector_ids: &[SectorId],
    ) -> Result<Vec<Option<Vec<u8>>>, SectorStoreError>;

    fn stats(&self) -> Result<SectorStorageStats, SectorStoreError>;
}
```

### Storage statistics

```rust
pub struct SectorStorageStats {
    pub db_size_bytes: u64,
    pub slot_count: u64,
    pub program_count: u64,
}
```

### Policy enforcement

The Zode **can** enforce:

- **Per-program storage quotas**: Total bytes stored for a `program_id`.
- **Per-slot size limits**: Maximum blob size (e.g., 256 KB).
- **Program allowlist**: Only accept writes for subscribed `program_id`s.
- **Rate limiting**: Maximum requests per second per connection or per program. Recommended default: 100 req/s per connection.

The Zode **cannot** enforce (by design):

- Per-group or per-user limits (these concepts are invisible).
- Payload format validation (payload is opaque).
- Write authorization (any client that can connect can write to any sector_id within an allowed program).

## Replication

Replication is **Zode-to-Zode** via GossipSub. When a Zode accepts a `SectorStoreRequest` from a client, it stores the blob locally and publishes a `GossipSector` message to the program's GossipSub topic (`prog/{program_id_hex}`). Other Zodes subscribed to that topic receive the message and store the blob automatically.

### `GossipSector`

Analogous to the base protocol's `GossipBlock`, this is the gossip message type for sector replication:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GossipSector {
    pub program_id: ProgramId,
    pub sector_id: SectorId,
    #[serde(with = "serde_bytes")]
    pub payload: Vec<u8>,
    pub overwrite: bool,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub ttl_seconds: Option<u64>,
}
```

The `expected_hash` field is intentionally omitted — CAS semantics are a client-to-Zode concern and do not propagate via gossip (see [Replication semantics](#replication-semantics)).

### Receiving Zode behavior

When a Zode receives a `GossipSector`:

1. **Program check.** If the Zode does not serve the `program_id`, discard.
2. **Write-once slot** (`overwrite: false`). Store if empty; silently ignore if already occupied (idempotent).
3. **Mutable slot** (`overwrite: true`). Overwrite unconditionally (last-write-wins). No `expected_hash` check — CAS does not apply to gossip-propagated writes.
4. **Deduplication.** For write-once slots, occupancy is a natural dedup. For mutable slots, a Zode MAY compare `SHA-256(current_payload)` to `SHA-256(incoming_payload)` and skip the write if identical, but this is an optimization, not a requirement.
5. **Policy enforcement.** Storage quotas and per-slot size limits apply. If the incoming blob would exceed limits, the Zode silently drops it (no error path on gossip).
6. **No re-gossip.** The Zode does NOT re-publish to GossipSub after storing a gossip-received blob. GossipSub's built-in mesh propagation handles fan-out — each message is delivered to all subscribers by the GossipSub layer itself.

### Replication semantics

| Concern | Behavior |
|---------|----------|
| Write-once slots | First-write-wins globally. Gossip-received writes to occupied slots are silently dropped. |
| Mutable slots (unconditional) | Last-write-wins. Concurrent writes from different clients may resolve differently on different Zodes until gossip converges. |
| Mutable slots (CAS via `expected_hash`) | CAS is enforced only between the client and the Zode it connects to. The winning write is then propagated via gossip with unconditional overwrite semantics. If two clients CAS to different Zodes simultaneously, both may succeed locally, and gossip propagation resolves via last-write-wins. Applications requiring stronger consistency should implement conflict resolution above the sector layer. |
| `ttl_seconds` | Propagated in the gossip message. Receiving Zodes MAY use the hint for their own eviction policy. |

### Optional client multi-send

Clients MAY send the same `SectorStoreRequest` to multiple Zodes directly for faster initial availability. This is not required — gossip propagation will replicate the blob automatically — but it reduces the window during which only a single Zode holds the data. When multi-sending, at-least-one-success semantics apply: the client considers the write successful if any Zode accepts it.

## Notification and polling

The sector protocol is **pull-based** in v0.1.0. Clients discover new writes by polling `batch_get` on known sector IDs.

### Polling guidance

- Clients SHOULD use exponential backoff when no new data is found, up to a maximum interval (e.g., 30 seconds).
- Clients SHOULD include decoy sector IDs in batch fetch requests to mask which IDs they are actually interested in.
- Clients SHOULD avoid polling from a single connection at high frequency to limit timing correlation.

### Future: push notifications

A future version MAY add a lightweight notification mechanism:

```rust
pub struct SectorSubscribeRequest {
    pub program_id: ProgramId,
    pub sector_ids: Vec<SectorId>,
}

pub struct SectorNotification {
    pub sector_id: SectorId,
    pub updated_at_ms: u64,
}
```

This would allow Zodes to push `SectorNotification` events when subscribed slots are written. The notification contains only the `sector_id` and a timestamp — no payload — so the client still performs a fetch to retrieve the content. This is deferred from v0.1.0 because push subscriptions reveal which sector IDs a client is interested in, creating a metadata correlation vector.

## Protocol coexistence

### Multiplexing

Both `/grid/1.0.0` (base protocol) and `/grid/sector/1.0.0` are registered as separate libp2p request-response protocols on the same Swarm. libp2p's built-in protocol negotiation (multistream-select) handles routing — each inbound stream is matched to the correct handler by its protocol string. No application-level multiplexing is needed.

### Shared infrastructure

| Resource | Shared? | Notes |
|----------|---------|-------|
| libp2p Swarm | Yes | Single Swarm with both protocols registered |
| GossipSub topics | Yes | `prog/{program_id_hex}` used for discovery and data propagation by both protocols. Base protocol publishes `GossipBlock`; sector protocol publishes `GossipSector`. Messages are distinguished by CBOR tag. |
| RocksDB instance | Yes | Sector uses its own `sectors` column family alongside base protocol CFs |
| Policy engine | Yes | Program allowlist and quotas apply uniformly to both protocols |
| Metrics surface | Separate counters | `sector_puts_total`, `sector_gets_total`, etc. alongside base protocol metrics |

### Discovery

Clients discover which protocols a Zode supports via libp2p's Identify protocol, which advertises the list of supported protocol strings. A client connecting to a Zode that does not advertise `/grid/sector/1.0.0` falls back to the base protocol (or raises an error if sector is required).

## Versioning

### Protocol version string

The protocol string `/grid/sector/1.0.0` follows semver:

- **Major** (`1`): Breaking wire-format changes. Incompatible with prior major versions.
- **Minor** (`0`): Additive changes (new optional fields, new request types). Backward-compatible.
- **Patch** (`0`): Bug fixes in spec language. No wire changes.

### Negotiation

A Zode MAY advertise multiple sector protocol versions (e.g., `/grid/sector/1.0.0` and `/grid/sector/2.0.0`). Clients select the highest mutually supported version via multistream-select. Within a major version, new optional fields (e.g., `ttl_seconds`, `expected_hash`) are silently ignored by older implementations that do not recognize them, because CBOR deserialization with `#[serde(default)]` skips unknown fields.

### Upgrade path

When a breaking change is needed:
1. Release the new major version alongside the old one.
2. Zodes advertise both versions during a transition period.
3. Deprecate the old version after adoption threshold is reached.
4. Remove the old version in a subsequent release.

## Visibility summary

### What the Zode sees

| Information | Visible? |
|------------|----------|
| `program_id` | Yes — routing and policy |
| `sector_id` | Yes — opaque 32 bytes, cannot reverse |
| Payload size (post-encryption) | Yes — mitigated by padding |
| `overwrite` flag | Yes — distinguishes write-once from mutable slots |
| `expected_hash` presence | Yes — reveals the client is doing conditional writes |
| `ttl_seconds` value | Yes — reveals client's retention preference |
| Write timing | Yes — when a put arrives |
| Read timing | Yes — when a get arrives |
| Which sector IDs are batched together | Yes — within a single batch request |
| IP address of client | Yes — transport-level |

### What the Zode cannot see

| Information | Why hidden |
|------------|-----------|
| Who wrote a blob | Identity is inside encrypted payload (or absent) |
| Who can read a blob | Key material is never on the wire |
| Which blobs are related | HKDF outputs are unlinkable without the shared secret |
| Blob content or structure | Encrypted and padded |
| Ordering, versioning, timestamps | Inside encrypted payload |
| Number of logical groups | Cannot distinguish groups from unrelated blobs |

## Comparison with base protocol

| Aspect | Base protocol ([12-protocol](12-protocol.md)) | Sector protocol |
|--------|---------------------------------------------|-----------------|
| Zode role | Indexes heads, verifies proofs, validates structure | Flat key-value put/get per program |
| Metadata visible to Zode | Sender, recipients, sector, version, timestamps, signatures | Only `program_id` |
| Content addressing | Zode verifies CID = SHA-256(ciphertext) | No CIDs; Zode stores opaque blobs |
| Proof verification | Zode verifies Valid-Sector proofs | Not applicable; client-verified if needed |
| Head management | Zode stores and serves structured `Head`s | No heads; client manages all state |
| Client complexity | Low — Zode manages state | Higher — client manages all state |
| Replication | GossipSub (`GossipBlock`) + request-response | Zode-to-Zode via GossipSub (`GossipSector`); optional client multi-send |
| Conflict detection | Version-based via Head lineage | Optional via `expected_hash` (CAS, local to single Zode); last-write-wins across gossip |

## Sequence diagrams

### Client store (single slot)

```mermaid
sequenceDiagram
    participant C as Client
    participant Z as Zode
    C->>C: derive_sector_id(shared_secret, info)
    C->>C: pad + encrypt payload
    C->>Z: SectorStoreRequest(program_id, sector_id, payload, overwrite)
    Z->>Z: Check program allowlist
    Z->>Z: Check storage quota
    Z->>Z: Check overwrite / expected_hash
    Z-->>C: SectorStoreResponse(ok / error)
```

### Client fetch (single slot)

```mermaid
sequenceDiagram
    participant C as Client
    participant Z as Zode
    C->>C: derive_sector_id(shared_secret, info)
    C->>Z: SectorFetchRequest(program_id, sector_id)
    Z-->>C: SectorFetchResponse(payload | None)
    C->>C: decrypt + unpad payload
```

### Batch fetch with decoys

```mermaid
sequenceDiagram
    participant C as Client
    participant Z as Zode
    C->>C: Compute real sector_ids (N)
    C->>C: Generate decoy sector_ids (D)
    C->>C: Shuffle real + decoy IDs
    C->>Z: SectorBatchFetchRequest(program_id, [N + D ids])
    Z-->>C: SectorBatchFetchResponse(results[])
    C->>C: Discard decoy results, decrypt real payloads
```

### Zode-to-Zode replication via GossipSub

```mermaid
sequenceDiagram
    participant C as Client
    participant Z1 as Zode 1
    participant Z2 as Zode 2
    participant Z3 as Zode 3
    C->>C: derive_sector_id + pad + encrypt
    C->>Z1: SectorStoreRequest
    Z1->>Z1: Store blob locally
    Z1-->>C: SectorStoreResponse(ok)
    Z1->>Z2: GossipSector (via GossipSub topic)
    Z1->>Z3: GossipSector (via GossipSub topic)
    Z2->>Z2: Store blob locally
    Z3->>Z3: Store blob locally
```

### Optional client multi-send

```mermaid
sequenceDiagram
    participant C as Client
    participant Z1 as Zode 1
    participant Z2 as Zode 2
    C->>C: derive_sector_id + pad + encrypt
    par Send to Z1
        C->>Z1: SectorStoreRequest
        Z1->>Z1: Store + gossip
        Z1-->>C: SectorStoreResponse
    and Send to Z2
        C->>Z2: SectorStoreRequest
        Z2->>Z2: Store + gossip (dedup on peers)
        Z2-->>C: SectorStoreResponse
    end
    C->>C: At least one ok → success
```

### Conditional update (CAS)

```mermaid
sequenceDiagram
    participant C as Client
    participant Z as Zode
    C->>Z: SectorFetchRequest(program_id, sector_id)
    Z-->>C: SectorFetchResponse(payload=current_blob)
    C->>C: expected_hash = SHA-256(current_blob)
    C->>C: Modify content, pad + encrypt → new_blob
    C->>Z: SectorStoreRequest(overwrite=true, expected_hash, payload=new_blob)
    alt Hash matches
        Z-->>C: SectorStoreResponse(ok=true)
    else Hash mismatch (concurrent write)
        Z-->>C: SectorStoreResponse(error=ConditionFailed)
        C->>C: Re-fetch, merge, retry
    end
```

## Limitations and future work

### v0.1.0 limitations

- **No transport anonymity**: IP addresses are visible. Onion routing / mixnet integration is deferred.
- **Timing correlation**: Batch requests and write patterns can leak access structure. Decoy IDs are a partial countermeasure but are not mandatory in v0.1.0.
- **No Zode-side validation**: Malicious clients can write invalid or adversarial data. Applications must validate after decryption.
- **No wire-level write authorization**: Any client that can connect and knows a `program_id` can write to any `sector_id`. Access control is an application-layer concern.
- **Write-once slot squatting**: Because there is no write authorization, an attacker who can predict a `sector_id` (e.g., by knowing the `shared_secret`) can pre-write to a write-once slot, permanently blocking the legitimate writer. Mitigations: (1) applications SHOULD use high-entropy, unpredictable info strings for write-once slots; (2) applications MAY use mutable slots with `expected_hash` instead of write-once slots where contention is possible; (3) future versions may add write-authorization tokens (see below).
- **Polling latency**: Clients must poll to discover new writes. High-frequency polling wastes bandwidth; low-frequency polling increases latency. Applications should tune polling intervals to their use case.
- **CAS is local, not global**: `expected_hash` compare-and-swap is enforced only between the client and the single Zode it connects to. Gossip replication propagates the winning write with unconditional overwrite semantics. If two clients CAS to different Zodes simultaneously for the same mutable slot, both may succeed locally, and gossip resolves via last-write-wins. Applications requiring stronger consistency should implement conflict resolution above the sector layer.
- **`expected_hash` reveals conditional-write intent**: The Zode can observe that a client is performing compare-and-swap, which reveals that the slot is being concurrently accessed. Applications that require this to be hidden should use unconditional overwrites and handle conflicts at the application layer.
- **Gossip propagation delay**: There is a window between when a Zode stores a blob and when gossip delivers it to peers. During this window, the blob exists on only one Zode. Clients that require faster multi-Zode availability MAY use optional multi-send.
- **`ttl_seconds` is a hint only**: Zodes are not obligated to honor TTLs. Clients must tolerate both early eviction and indefinite retention.

### Future enhancements

- **Decoy traffic**: Periodic writes to random sector IDs to mask real activity timing.
- **Epoch-based ID rotation**: Rotate derivation inputs periodically so the Zode cannot track long-lived mutable slots.
- **Private information retrieval**: Fetch without revealing which `sector_id` is requested.
- **Write authorization tokens**: Zode-verified tokens to restrict who can write to specific slots or programs. Could use blind signatures so the Zode verifies a valid token without learning the writer's identity.
- **Push notifications**: `SectorSubscribeRequest` for Zode-pushed slot-update events (see [Notification and polling](#notification-and-polling)).

## Implementation notes

- **`grid-core`**: Gains `GossipSector` and sector wire types (`SectorRequest`, `SectorResponse`, and their inner structs in a new `sector_protocol.rs` module). Reuses the existing `SectorId` type from `sector_id.rs` (sector protocol IDs are always 32-byte HKDF outputs within the variable-length `SectorId`). The existing `ErrorCode` enum is extended with `SlotOccupied`, `BatchTooLarge`, and `ConditionFailed` variants. `SectorStoreError` is defined here as a shared error type.
- **`grid-crypto`**: Gains `derive_sector_id(shared_secret, info) -> SectorId` — performs the two-step HKDF derivation (extract derivation key, then derive sector ID) with salt `"grid:sector:v1"`. Also gains `pad_to_bucket(content) -> Vec<u8>` and `unpad_from_bucket(padded) -> Result<Vec<u8>, CryptoError>` for the length-prefix + zero-fill padding scheme. The existing `SectorKey`, `encrypt_sector`, `decrypt_sector`, `wrap_sector_key`, `unwrap_sector_key` are unchanged and reused for payload encryption.
- **`grid-storage`**: Gains a `SectorStore` trait with `put`, `get`, `batch_put`, `batch_get`, and `stats`. The `RocksStorage` implementation adds a `sectors` column family to its existing set (`blocks`, `heads`, `program_index`, `metadata`, `sectors`). The `put` operation with `expected_hash` uses a read-modify-write under a RocksDB single-key lock (or merge operator) to ensure atomicity. The existing base protocol traits (`BlockStore`, `HeadStore`, `ProgramIndex`) are unchanged.
- **`grid-net`**: Registers `/grid/sector/1.0.0` as a separate request-response protocol on the same libp2p Swarm. Uses multistream-select for protocol negotiation. GossipSub message handling is extended to deserialize both `GossipBlock` (base protocol) and `GossipSector` (sector protocol) from the same program topics, distinguished by CBOR tag. On successful client store, the handler publishes a `GossipSector` to the program's GossipSub topic. Incoming gossip messages are routed to the appropriate handler based on type.
- **`zode`**: Gains a `SectorStore`-backed request handler (simpler than the base handler — no CID verification, no proof verification, no head management). On accepting a `SectorStoreRequest`, the handler stores the blob and triggers gossip publication via `grid-net`. Incoming `GossipSector` messages are handled with idempotent write-once or last-write-wins semantics (no `expected_hash` check for gossip-received writes). Policy enforcement (program allowlist, quotas, rate limiting) applies to both client requests and gossip-received writes.
- **`grid-sdk`**: Gains `derive_sector_id`, `pad_to_bucket`, `unpad_from_bucket`, and batch operation helpers. Provides optional multi-send to multiple Zodes for faster initial availability (gossip handles replication automatically). Provides a convenience `sector_encrypt(sector_key, program_id, sector_id, content) -> Vec<u8>` that pads, sets the mandatory AAD, and encrypts in one call. Application-specific logic (group management, message ordering, membership) lives above this layer in application code.
