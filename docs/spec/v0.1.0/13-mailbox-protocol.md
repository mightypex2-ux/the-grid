# ZFS v0.1.0 — Metadata-Private Mailbox Protocol

## Purpose

This document specifies the **mailbox protocol**, a metadata-private alternative to the transparent protocol defined in [12-protocol](12-protocol.md). The mailbox protocol ensures that Zodes learn only which **program** a write belongs to. All other metadata — who wrote it, who can read it, which logical channel it belongs to, the version history, timestamps, and recipient lists — is encrypted and invisible to the storage layer.

## Design goals

| Goal | Description |
|------|-------------|
| **Payload confidentiality** | Zodes never see plaintext. (Unchanged from base spec.) |
| **Metadata privacy** | Zodes cannot determine sender identity, recipient set, channel membership, message ordering, version history, or activity patterns per channel. |
| **Program-scoped routing** | `program_id` remains visible. Zodes subscribe to and route by program. This is an intentional, accepted tradeoff — hiding the program would require private information retrieval and is out of scope. |
| **Multi-writer correctness** | Multiple participants can write to the same logical channel concurrently without coordination or message loss. |
| **Admin-managed membership** | Channel membership is managed by a designated admin. This avoids decentralized membership coordination complexity. |
| **Client-managed state** | Clients maintain local state (counters, membership lists) and reconstruct from the network when needed. |

## Threat model

The mailbox protocol protects against a **honest-but-curious Zode** (or network observer) that:

- Stores and serves data correctly but attempts to learn metadata.
- Can observe all writes and reads (mailbox IDs, blob sizes, timing).
- Can correlate traffic by IP address and connection timing.
- Cannot break the underlying cryptography (XChaCha20-Poly1305, X25519, ML-KEM-768).

**Not in scope for v0.1.0:**

- Transport-level anonymity (IP hiding, onion routing, mixnets).
- Active attackers who modify or drop messages (integrity is assumed at the transport layer).
- Side-channel attacks on client devices.

## Architecture overview

```
┌──────────────────────────────────────────────────────┐
│                    Zode's view                        │
│                                                      │
│   program_id   │   mailbox_id    │   payload          │
│   (visible)    │   (opaque 32B)  │   (encrypted blob) │
│────────────────┼─────────────────┼────────────────────│
│   0xaa..       │   0x3f..        │   [1.2 KB]         │
│   0xaa..       │   0x71..        │   [48 B]           │
│   0xaa..       │   0xb2..        │   [1.1 KB]         │
│   ...          │   ...           │   ...              │
└──────────────────────────────────────────────────────┘
```

The Zode is a **program-scoped key-value store**: `(program_id, mailbox_id) → encrypted_blob`. It cannot determine which rows belong to the same channel, which are messages vs metadata, or who wrote or reads any of them.

## Core concepts

### Mailbox ID

A `MailboxId` is a 32-byte opaque identifier derived via HKDF from a shared secret. The Zode stores blobs keyed by `(program_id, mailbox_id)`. It cannot reverse a mailbox ID to learn the underlying sector, channel, sender, or counter.

### Channel secret

Each channel (logical grouping of messages — e.g., a chat room, an identity sector) has a `channel_secret` derived from its `SectorKey`:

```
channel_secret = HKDF-SHA256(
    ikm  = sector_key,
    salt = None,
    info = "zfs:channel-secret:v1" || program_id || sector_id
)
```

All channel participants possess the `SectorKey` (via key wrapping — see [10-crypto](10-crypto.md)) and can therefore derive the same `channel_secret`. The channel secret is the root from which all mailbox IDs for that channel are deterministically computed.

### Per-sender lanes

Each participant in a channel is assigned a **sender index** (0, 1, 2, ...) by the channel admin. Each sender has their own independent sequence of mailbox slots. Writers only write to their own lane. This eliminates multi-writer conflicts without coordination.

### Sealed envelope

All metadata that was previously visible on the wire (head, key envelope, signatures, machine DID) is packed into a `SealedEnvelope`, encrypted, and stored as an opaque blob. Only participants with the `SectorKey` can decrypt it.

## Mailbox ID derivation

All mailbox IDs are derived from the `channel_secret` using HKDF-SHA256 with distinct info strings:

| Slot type | Derivation | Overwrite policy |
|-----------|-----------|-----------------|
| **Message** | `HKDF(channel_secret, "zfs:msg:v1" \|\| sender_index_be \|\| counter_be)` | Write-once |
| **Sender head** | `HKDF(channel_secret, "zfs:sender-head:v1" \|\| sender_index_be)` | Overwrite (by owner) |
| **Members list** | `HKDF(channel_secret, "zfs:members:v1")` | Overwrite (by admin) |

- `sender_index_be`: sender index as 4-byte big-endian.
- `counter_be`: message counter as 8-byte big-endian.
- All derivations use `salt = None`.

### Properties

- **Deterministic:** All participants with the `channel_secret` compute identical mailbox IDs.
- **Unlinkable:** Different slot types and different counters produce unrelated mailbox IDs. The Zode cannot tell that two IDs belong to the same channel.
- **Collision-resistant:** HKDF-SHA256 output is 32 bytes; collision probability is negligible.

## Channel lifecycle

### 1. Channel creation

The channel admin (creator) performs:

```
1. Generate SectorKey (random 256-bit, CSPRNG)
2. Derive channel_secret from SectorKey
3. Assign self as sender_index = 0
4. Build MembersList:
     { admin_index: 0, members: [{ index: 0, did: admin_did }] }
5. Encrypt MembersList with SectorKey → members_payload
6. Compute members_mailbox = HKDF(channel_secret, "zfs:members:v1")
7. Store: StoreRequest(program_id, members_mailbox, members_payload, overwrite=true)
8. Wrap SectorKey to own public key → KeyEnvelopeEntry (for key recovery)
```

### 2. Inviting a member

The admin invites a new participant:

```
1. Assign next sender_index (e.g., 1)
2. Wrap SectorKey to the invitee's MachinePublicKey → KeyEnvelopeEntry
3. Deliver to invitee (out-of-band, or via a direct 1-to-1 channel):
     - SectorKey (wrapped)
     - sender_index
     - program_id, sector_id (needed to derive channel_secret)
4. Update MembersList: add { index: 1, did: invitee_did }
5. Encrypt and overwrite members_mailbox
```

The invitee, upon receiving the invitation:

```
1. Unwrap SectorKey using own MachineKeyPair
2. Derive channel_secret
3. Fetch members_mailbox → decrypt → learn all current members and their indices
4. Begin scanning sender lanes for message history
```

### 3. Removing a member

The admin removes a participant:

```
1. Generate a NEW SectorKey (rotation)
2. Derive new channel_secret
3. Wrap the new SectorKey to all REMAINING members
4. Deliver new SectorKey to remaining members (via their 1-to-1 channels or a rekey message in the old channel)
5. Update MembersList (remove the evicted member, keep sender indices stable for remaining members)
6. Write new MembersList to the NEW members_mailbox (derived from new channel_secret)
7. New messages use new mailbox IDs (derived from new channel_secret)
```

The evicted member still holds the old SectorKey and can read old messages. They cannot read new messages because they don't have the new SectorKey and therefore cannot derive the new channel_secret or any new mailbox IDs.

### 4. Sending a message

A sender (index `S`, local counter `C`) writes:

```
1. Build application message (e.g., ZChatMessage { content, timestamp })
2. Encrypt message content:
     ciphertext = XChaCha20-Poly1305(message, SectorKey, aad = program_id || sector_id)
3. Build SealedEnvelope:
     { ciphertext, head, machine_did, signature, counter: C }
4. Encrypt entire envelope:
     msg_payload = XChaCha20-Poly1305(canonical_cbor(envelope), SectorKey, aad)
5. Compute mailbox IDs:
     msg_mailbox  = HKDF(channel_secret, "zfs:msg:v1" || S || C)
     head_mailbox = HKDF(channel_secret, "zfs:sender-head:v1" || S)
6. Encrypt head counter:
     head_payload = XChaCha20-Poly1305(canonical_cbor(C), SectorKey, aad)
7. Batch store (message + head in one round trip):
     BatchStoreRequest(program_id, [
       { mailbox_id: msg_mailbox,  payload: msg_payload,  overwrite: false },
       { mailbox_id: head_mailbox, payload: head_payload, overwrite: true  },
     ])
8. Increment local counter: C = C + 1
```

The message and head update are sent as a single batch — one network round trip per message sent.

### 5. Reading messages (catch-up)

A reader who was last synchronized at `last_seen[sender_index]` for each sender:

```
Round 1 — Fetch all sender heads in one batch:
  Compute head_mailbox for each known sender S:
    head_ids = [HKDF(channel_secret, "zfs:sender-head:v1" || S) for S in members]
  BatchFetchRequest(program_id, head_ids)
  → decrypt each → learn latest_counter per sender

Round 2 — Fetch all missed messages in one batch:
  For each sender S where latest_counter[S] > last_seen[S]:
    For counter = last_seen[S] + 1 .. latest_counter[S]:
      Add HKDF(channel_secret, "zfs:msg:v1" || S || counter) to fetch list
  BatchFetchRequest(program_id, fetch_list)
  → decrypt each SealedEnvelope → extract messages

Sort collected messages by timestamp after decryption.
Update last_seen[S] for each sender.
```

**Total: 2 round trips** regardless of channel size or number of missed messages (up to the 256 batch limit — larger catches split across additional batches).

Example: 5-person group, reader missed 20 messages across all senders:

```
Round 1:  BatchFetch(5 head mailbox IDs)        →  5 results,  1 request
Round 2:  BatchFetch(20 message mailbox IDs)     → 20 results,  1 request
Total:    25 blobs fetched in 2 round trips
```

### 6. Loading history (pagination)

A client that wants to display the **last N messages** on screen (e.g., initial channel load or "scroll up"):

```
Round 1 — Fetch all sender heads:
  BatchFetchRequest(program_id, [head_mailbox for each sender])
  → decrypt → learn latest_counter per sender

Round 2 — Fetch recent messages from each lane:
  Per sender, compute the last K = ceil(N / num_senders × 2) message mailbox IDs
  (2x over-fetch to handle uneven distribution across senders)
  BatchFetchRequest(program_id, all_message_ids)
  → decrypt all → sort by timestamp → take last N

If the N messages aren't covered (e.g., one sender dominated):
Round 3 — Fetch deeper from under-represented lanes and repeat.
```

Example: 5-person group, loading last 100 messages:

```
Round 1:  BatchFetch(5 heads)                    →  5 results
Round 2:  BatchFetch(40 per sender × 5 = 200)   → 200 results, ~100 useful
Total:    205 blobs in 2 round trips → decrypt → sort → display last 100
```

Subsequent "load more" (user scrolls up):

```
Round 1:  BatchFetch(next 40 per sender × 5 = 200 older message IDs)
          → decrypt → sort → merge with existing messages
Total:    1 round trip per page
```

### 7. Cold start (new device, lost local state)

If a client has the `SectorKey` and `program_id + sector_id` but no local state:

```
Round 1 — Fetch members list:
  members_mailbox = HKDF(channel_secret, "zfs:members:v1")
  FetchRequest(program_id, members_mailbox)
  → decrypt → full member list with sender indices

Round 2 — Fetch all sender heads:
  BatchFetchRequest(program_id, [head_mailbox for each sender])
  → decrypt → learn latest_counter per sender

Round 3+ — Fetch message history (paginated):
  Start from the most recent messages using the pagination strategy above.
  Or fetch all messages if full history is needed (batch in chunks of 256).
```

Total: 3 round trips for initial load, plus additional batches if full history is needed.

## Wire protocol

### Single operations

For writing or fetching a single mailbox slot.

#### StoreRequest

```rust
pub struct StoreRequest {
    pub program_id: ProgramId,
    pub mailbox_id: MailboxId,
    pub payload: Vec<u8>,        // encrypted SealedEnvelope or encrypted metadata
    pub overwrite: bool,         // true for head/members slots; false for messages
}
```

#### StoreResponse

```rust
pub struct StoreResponse {
    pub ok: bool,
    pub error_code: Option<ErrorCode>,
}
```

#### FetchRequest

```rust
pub struct FetchRequest {
    pub program_id: ProgramId,
    pub mailbox_id: MailboxId,
}
```

#### FetchResponse

```rust
pub struct FetchResponse {
    pub payload: Option<Vec<u8>>,
    pub error_code: Option<ErrorCode>,
}
```

### Batch operations

Batch operations allow a client to read or write many mailbox slots in a single network round trip. This is critical for practical message loading — fetching 100 messages individually would require 100 round trips, while a batch fetch requires one.

The Zode sees the full list of mailbox IDs in a batch. Since all IDs are opaque HKDF outputs, the Zode cannot determine which belong to the same channel, which are message slots vs head slots, or any relationship between them.

#### BatchFetchRequest

```rust
pub struct BatchFetchRequest {
    pub program_id: ProgramId,
    pub mailbox_ids: Vec<MailboxId>,  // max 256 per batch
}
```

#### BatchFetchResponse

```rust
pub struct BatchFetchResponse {
    pub results: Vec<Option<Vec<u8>>>,  // parallel to mailbox_ids: payload or None
    pub error_code: Option<ErrorCode>,
}
```

`results[i]` corresponds to `mailbox_ids[i]`. A `None` entry means the mailbox does not exist (no error — the slot simply hasn't been written).

#### BatchStoreRequest

```rust
pub struct BatchStoreRequest {
    pub program_id: ProgramId,
    pub entries: Vec<BatchStoreEntry>,  // max 256 per batch
}

pub struct BatchStoreEntry {
    pub mailbox_id: MailboxId,
    pub payload: Vec<u8>,
    pub overwrite: bool,
}
```

#### BatchStoreResponse

```rust
pub struct BatchStoreResponse {
    pub results: Vec<bool>,             // parallel to entries: true if written
    pub error_code: Option<ErrorCode>,
}
```

`results[i]` is `true` if `entries[i]` was written successfully, `false` if rejected (e.g., write-once slot already exists, quota exceeded).

#### Batch limits

- Maximum **256 mailbox IDs** per batch request. Clients that need more split across multiple batches.
- The Zode MAY reject batches that exceed a per-request payload size limit (e.g., 16 MB total).
- Each entry within a batch is independent — a failure on one entry does not affect others.

### Summary

No machine_did, no signature, no head, no key_envelope, no CID on the wire. The protocol is reduced to four operations:

| Operation | Purpose |
|-----------|---------|
| `put(program, mailbox, blob)` | Write a single slot |
| `get(program, mailbox)` | Read a single slot |
| `batch_put(program, entries[])` | Write up to 256 slots in one round trip |
| `batch_get(program, mailbox_ids[])` | Read up to 256 slots in one round trip |

## Sealed envelope

The encrypted blob stored at each message mailbox contains a `SealedEnvelope`:

```rust
pub struct SealedEnvelope {
    pub ciphertext: Vec<u8>,
    pub head: Head,
    pub key_envelope: Option<KeyEnvelope>,
    pub machine_did: String,
    pub signature: HybridSignature,
    pub counter: u64,
}
```

The entire `SealedEnvelope` is serialized to canonical CBOR, then encrypted with the channel's `SectorKey` using XChaCha20-Poly1305 (random nonce, AAD = `program_id || sector_id || "envelope"`).

After decryption, the recipient verifies the `signature` against `machine_did` to confirm attribution. The `head` and `key_envelope` are processed as in the base protocol — they just aren't visible to the Zode.

## Members list

The members list is stored at the members mailbox slot, encrypted with the `SectorKey`:

```rust
pub struct MembersList {
    pub admin_index: u32,
    pub members: Vec<MemberEntry>,
}

pub struct MemberEntry {
    pub sender_index: u32,
    pub did: String,
    pub role: MemberRole,
}

pub enum MemberRole {
    Admin,
    Writer,
    Reader,
}
```

- **Admin**: Can add/remove members, update the members list. One admin per channel (the creator). Admin transfer is accomplished by updating `admin_index` and the corresponding entry's role.
- **Writer**: Can send messages (has an assigned sender_index lane).
- **Reader**: Can read messages but has no sender lane. Does not write message slots.

Only the admin writes to the members mailbox. This avoids multi-writer conflicts on the membership list.

### Sender index assignment

- Sender indices are assigned sequentially by the admin: 0, 1, 2, ...
- Indices are never reused. If member at index 2 is removed, index 2 is retired. The next member gets index 3.
- This ensures a removed member's old lane remains readable (old messages are still valid) but the index is not reassigned to a new member.

## Zode storage model

### Storage backend

The Zode stores mailbox blobs in a simple key-value scheme:

| Column family | Key | Value |
|--------------|-----|-------|
| **mailboxes** | `program_id (32B) \|\| mailbox_id (32B)` | `payload (encrypted blob)` |

No separate block store, head store, or program index is needed. The Zode is a flat key-value store scoped by program.

### Operations

| Operation | Behavior |
|-----------|----------|
| `put(program_id, mailbox_id, payload, overwrite)` | Write blob. If `overwrite=false`, reject when key exists (write-once). If `overwrite=true`, overwrite unconditionally. |
| `get(program_id, mailbox_id)` | Return payload if key exists, or None. |
| `batch_put(program_id, entries[])` | Atomic batch of up to 256 puts. Each entry is independent (partial success allowed). |
| `batch_get(program_id, mailbox_ids[])` | Batch fetch up to 256 keys. Returns parallel array of payloads (None for missing keys). |
| `delete(program_id, mailbox_id)` | Remove key. (Admin/garbage-collection use.) |

Batch operations are the primary interface for message loading and history pagination. Single put/get are convenience wrappers around batch with one entry.

### Policy enforcement

The Zode can enforce:

- **Per-program storage quotas**: Total bytes stored for a program_id.
- **Per-mailbox size limits**: Maximum blob size (e.g., 256 KB).
- **Program allowlist**: Only accept writes for subscribed program_ids.

The Zode **cannot** enforce:
- Per-channel limits (it doesn't know what a channel is).
- Message format validation (payload is opaque).
- Proof verification (proofs are inside the encrypted blob).

## What the Zode sees

| Information | Visible? |
|------------|----------|
| program_id | Yes — routing and policy |
| mailbox_id | Yes — opaque 32 bytes, cannot reverse |
| Payload size | Yes — blob length visible |
| Write timing | Yes — when a put arrives |
| Read timing | Yes — when a get arrives |
| Which mailbox IDs are accessed together | Yes — timing correlation possible |
| IP address of client | Yes — transport-level (out of scope) |

## What the Zode cannot see

| Information | Why |
|------------|-----|
| Sender identity | machine_did is inside encrypted envelope |
| Recipient set | KeyEnvelope is inside encrypted envelope |
| Channel / sector identity | sector_id is inside encrypted envelope; mailbox_id is blinded |
| Which mailbox IDs belong to the same channel | HKDF outputs are unlinkable without channel_secret |
| Message ordering / version | counter and head are inside encrypted envelope |
| Timestamps | Inside encrypted envelope |
| Signatures / attribution | Inside encrypted envelope |
| Number of channels | Cannot distinguish channels from blobs |
| Number of members per channel | Cannot see membership |
| Message content | Encrypted with SectorKey |

## Padding

To prevent message-size analysis, clients SHOULD pad payloads to fixed size buckets before encryption:

| Message size | Padded to |
|-------------|-----------|
| 0 – 1 KB | 1 KB |
| 1 – 4 KB | 4 KB |
| 4 – 16 KB | 16 KB |
| 16 – 64 KB | 64 KB |
| 64 – 256 KB | 256 KB |

Padding is applied to the serialized `SealedEnvelope` before encryption. The padding scheme is PKCS#7-style (pad byte = number of padding bytes) so it can be stripped after decryption.

Metadata blobs (sender heads, members list) SHOULD be padded to at least 256 bytes to be indistinguishable from small messages.

## Comparison with base protocol

| Aspect | Base protocol ([12-protocol](12-protocol.md)) | Mailbox protocol |
|--------|---------------------------------------------|-----------------|
| Zode role | Smart: indexes heads, verifies proofs, validates structure | Dumb: key-value put/get per program |
| Metadata visible to Zode | Sender, recipients, sector, version, timestamps, signatures | Only program_id |
| Content addressing | Zode verifies CID = SHA-256(ciphertext) | CID is inside encrypted envelope; Zode cannot verify |
| Proof verification | Zode verifies Valid-Sector proofs | Proofs are inside encrypted envelope; client-verified |
| Head management | Zode stores and serves structured heads | Client manages counters; heads are encrypted blobs |
| Multi-writer | Single head per sector (last-write-wins) | Per-sender lanes (no conflicts) |
| Client complexity | Low — Zode manages state | Higher — client manages counters, membership, scanning |
| Send cost | 1 round trip | 1 round trip (batch: message + head) |
| Catch-up (M missed msgs) | O(M) fetches | 2 batch round trips (heads + messages) |
| Load last N messages | O(1) head + O(1) block | 2 batch round trips (heads + over-fetch per lane) |
| Pagination (load more) | O(1) per page | 1 batch round trip per page |
| Cold start | Query Zode for heads and indexes | 3 batch round trips (members + heads + first page) |

## Limitations and future work

### v0.1.0 limitations

- **No transport anonymity**: IP addresses are visible to Zodes and peers. Onion routing or mixnet integration is deferred.
- **Timing correlation**: A Zode can correlate fetches that arrive close together from the same connection. Countermeasure: fetch decoy mailbox IDs alongside real ones.
- **Single admin**: Channel membership is centrally managed. Admin loss requires out-of-band recovery or a pre-designated successor.
- **No Zode-side proof verification**: Proofs move inside the encrypted envelope. Malicious clients can store invalid data. Other participants validate after decryption.
- **Scan cost for large channels**: Catching up on a 500-member channel requires 500 head fetches (2 batch requests of 256). Mitigated by batch fetch, client-side caching, and sparse scanning of inactive senders.

### Future enhancements

- **Decoy traffic**: Periodic writes to random mailbox IDs to mask real activity timing.
- **Epoch-chunked heads**: Rotate head mailbox IDs periodically so the Zode cannot track activity on a single head slot.
- **Multi-admin**: Allow multiple admins via a CRDT-based membership list with per-admin announcement lanes.
- **Federated key recovery**: Store encrypted backup of channel_secret across multiple Zodes using secret sharing.
- **Private information retrieval**: Allow clients to fetch mailbox contents without revealing which mailbox_id they're requesting.

## Implementation notes

- **Crate impact**: `zfs-core` types (MailboxId, SealedEnvelope, MembersList) are new. StoreRequest/FetchRequest/StoreResponse/FetchResponse simplify. Head, KeyEnvelope, and HybridSignature remain as types but move inside the encrypted envelope.
- **Storage**: `zfs-storage` simplifies to a single column family (`mailboxes`) with composite keys. HeadStore and ProgramIndex are no longer needed at the Zode level.
- **SDK**: `zfs-sdk` gains significant complexity: mailbox ID computation, per-sender counter management, lane scanning, membership list parsing, SealedEnvelope packing/unpacking.
- **Crypto**: `zfs-crypto` is unchanged — the same SectorKey/encrypt/decrypt/wrap/unwrap primitives are used. The channel_secret derivation is a new HKDF call.
- **Network**: `zfs-net` is simpler — request-response carries `(program_id, mailbox_id, blob)` for single ops and `(program_id, mailbox_ids[], blobs[])` for batch ops. Batch is the primary interface for message loading; single ops are convenience wrappers. GossipSub topics are unchanged (`prog/{program_id_hex}`).
