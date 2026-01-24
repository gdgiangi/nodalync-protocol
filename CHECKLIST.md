# Nodalync Implementation Checklist

Track implementation progress by checking off items as they're completed. Each item references its spec section.

## Legend
- [ ] Not started
- [~] In progress
- [x] Complete
- [!] Blocked

---

## Phase 1: Foundation

### `nodalync-crypto` (Spec §3)

#### §3.1 Hash Function
- [x] `ContentHash(content)` — SHA-256 with length prefix
- [x] Domain separator `0x00` for content hashing
- [x] Test: identical content produces identical hash
- [x] Test: different content produces different hash

#### §3.2 Identity
- [x] Ed25519 keypair generation
- [x] PeerId derivation: `H(0x00 || public_key)[0:20]`
- [x] Human-readable format: `ndl1` + base58(PeerId)
- [x] Test: PeerId is deterministic from public key
- [x] Test: human-readable encoding roundtrips

#### §3.3 Signatures
- [x] `Sign(private_key, message)` — Ed25519
- [x] `Verify(public_key, message, signature)`
- [x] SignedMessage struct
- [x] Test: valid signature verifies
- [x] Test: tampered message fails verification
- [x] Test: wrong key fails verification

#### §3.4 Content Addressing
- [x] Content referenced by hash
- [x] Verification: `ContentHash(received) == claimed_hash`
- [x] Test: content verification succeeds for valid content
- [x] Test: content verification fails for tampered content

---

### `nodalync-types` (Spec §4)

#### §4.1 ContentType
- [x] Enum: L0 = 0x00, L1 = 0x01, L3 = 0x03
- [x] Serialization matches spec

#### §4.2 Visibility
- [x] Enum: Private = 0x00, Unlisted = 0x01, Shared = 0x02
- [x] Serialization matches spec

#### §4.3 Version
- [x] Struct: number, previous, root, timestamp
- [x] Constraints: v1 has null previous, root == self
- [x] Constraints: v2+ has non-null previous, root == previous.root

#### §4.4 Mention (L1)
- [x] Mention struct with all fields
- [x] SourceLocation struct
- [x] LocationType enum
- [x] Classification enum
- [x] Confidence enum

#### §4.5 Provenance
- [x] Provenance struct: root_L0L1, derived_from, depth
- [x] ProvenanceEntry struct: hash, owner, visibility, weight
- [x] Constraints documented in types

#### §4.6 AccessControl
- [x] Struct: allowlist, denylist, require_bond, bond_amount, max_queries_per_peer
- [x] Access logic documented

#### §4.7 Economics
- [x] Struct: price, currency, total_queries, total_revenue
- [x] Currency enum (NDL = 0x00)

#### §4.8 Manifest
- [x] Complete Manifest struct with all fields
- [x] **owner: PeerId field** (content owner)
- [x] Metadata struct

#### §4.9 L1Summary
- [x] Struct: l0_hash, mention_count, preview_mentions, primary_topics, summary

---

## Phase 2: Core Logic

### `nodalync-wire` (Spec §6, Appendix A)

#### §6.1 Message Envelope
- [x] Message struct: version, type, id, timestamp, sender, payload, signature
- [x] MessageType enum with all 17 types
- [x] Protocol magic byte (0x00)
- [x] Protocol version (0x01)

#### §6.2 Discovery Messages
- [x] AnnouncePayload
- [x] AnnounceUpdatePayload
- [x] SearchPayload + SearchFilters
- [x] SearchResponsePayload + SearchResult

#### §6.3 Preview Messages
- [x] PreviewRequestPayload
- [x] PreviewResponsePayload

#### §6.4 Query Messages
- [x] QueryRequestPayload + VersionSpec
- [x] QueryResponsePayload + PaymentReceipt
- [x] QueryErrorPayload + QueryError enum

#### §6.5 Version Messages
- [x] VersionRequestPayload
- [x] VersionResponsePayload + VersionInfo

#### §6.6 Channel Messages
- [x] ChannelOpenPayload
- [x] ChannelAcceptPayload
- [x] ChannelUpdatePayload + ChannelBalances
- [x] ChannelClosePayload
- [x] ChannelDisputePayload

#### §6.7 Settlement Messages
- [x] SettleBatchPayload + SettlementEntry
- [x] SettleConfirmPayload

#### §6.8 Peer Messages
- [x] PingPayload
- [x] PongPayload
- [x] PeerInfoPayload + Capability enum

#### Appendix A: Wire Formats
- [x] Deterministic CBOR encoding
- [x] Map keys sorted lexicographically
- [x] No indefinite-length arrays
- [x] Minimal integer encoding
- [x] ContentHash domain separator
- [x] MessageHash domain separator
- [x] ChannelStateHash domain separator

---

### `nodalync-store` (Spec §5)

#### §5.1 State Components
- [x] NodeState struct
- [x] Identity storage (encrypted private key)
- [x] ContentRecord struct
- [x] PeerInfo struct
- [x] CachedContent struct

#### §5.2 Provenance Graph
- [x] Forward edges: derived_from
- [x] Backward edges: derivations
- [x] Flattened roots cache
- [x] `add_content()` operation
- [x] `get_roots()` operation
- [x] `get_derivations()` operation

#### §5.3 Payment Channels
- [x] Channel struct
- [x] ChannelState enum
- [x] Payment struct

#### Storage Implementation
- [x] Content storage (filesystem)
- [x] Manifest storage (SQLite)
- [x] Provenance graph storage
- [x] Channel state storage
- [x] Cache management
- [x] **Settlement queue storage**
- [x] **QueuedDistribution struct**
- [x] **enqueue() method**
- [x] **get_pending() method**
- [x] **mark_settled() method**
- [x] **get_pending_total() method**
- [x] Test: store and retrieve content
- [x] Test: store and retrieve manifest
- [x] Test: provenance graph traversal
- [x] Test: settlement queue operations

---

### `nodalync-valid` (Spec §9)

#### §9.1 Content Validation
- [x] `ContentHash(content) == manifest.hash`
- [x] `len(content) == manifest.metadata.content_size`
- [x] Title length <= 200
- [x] Description length <= 2000
- [x] Tags count <= 20, each <= 50 chars
- [x] content_type valid
- [x] visibility valid
- [x] Test: all validation rules

#### §9.2 Version Validation
- [x] v1: previous null, root == hash
- [x] v2+: previous not null, root == previous.root
- [x] number == previous.number + 1
- [x] timestamp > previous.timestamp
- [x] Test: valid version chain
- [x] Test: invalid version rejected

#### §9.3 Provenance Validation
- [x] L0: root_L0L1 == [self], derived_from == [], depth == 0
- [x] L3: root_L0L1 >= 1, derived_from >= 1
- [x] All derived_from exist in sources
- [x] root_L0L1 computation correct
- [x] depth == max(sources.depth) + 1
- [x] No self-reference
- [x] Test: valid L0 provenance
- [x] Test: valid L3 provenance
- [x] Test: invalid provenance rejected

#### §9.4 Payment Validation
- [x] amount >= price
- [x] recipient == owner
- [x] query_hash == manifest.hash
- [x] channel.state == Open
- [x] channel.their_balance >= amount
- [x] nonce > channel.nonce
- [x] signature valid
- [x] provenance matches manifest
- [x] Test: all payment validation rules

#### §9.5 Message Validation
- [x] version == PROTOCOL_VERSION
- [x] type is valid MessageType
- [x] timestamp within ±5 minutes
- [x] sender is valid PeerId
- [x] signature valid
- [x] payload decodes correctly
- [x] Test: valid message accepted
- [x] Test: invalid message rejected

#### §9.6 Access Validation
- [x] Private: always deny external
- [x] Unlisted: check allowlist/denylist
- [x] Shared: check denylist only
- [x] Bond requirement check
- [x] Test: all access scenarios

---

### `nodalync-econ` (Spec §10)

#### §10.1 Revenue Distribution
- [x] SYNTHESIS_FEE = 0.05 (5%)
- [x] ROOT_POOL = 0.95 (95%)
- [x] Distribution calculation
- [x] Weight handling for duplicates
- [x] Owner gets synthesis fee + own roots
- [x] Test: distribution example from spec (Bob/Alice/Carol)

#### §10.3 Price Constraints
- [x] MIN_PRICE = 1
- [x] MAX_PRICE = 10^16
- [x] price is uint64
- [x] Test: price validation

#### §10.4 Settlement Batching
- [x] BATCH_THRESHOLD = 100 NDL
- [x] BATCH_INTERVAL = 3600 seconds
- [x] Batch trigger logic
- [x] Aggregate by recipient
- [x] Merkle root computation
- [x] Test: batching logic

---

## Phase 3: Operations

### `nodalync-ops` (Spec §7)

#### §7.1.1 CREATE
- [x] Hash computation
- [x] Version initialization (v1)
- [x] Provenance initialization (L0: self-referential)
- [x] **Owner field set to creator**
- [x] Manifest creation
- [x] Local storage
- [x] Test: create L0 content

#### §7.1.2 EXTRACT_L1
- [x] Load content
- [x] **Rule-based extraction (MVP)**
- [x] classify_sentence() helper
- [x] extract_entities() helper
- [x] Generate L1Summary
- [x] Store L1 data
- [x] **L1Extractor trait for plugin architecture**
- [x] Test: L1 extraction

#### §7.1.3 PUBLISH
- [x] Update visibility
- [x] Update price
- [x] Update access control
- [ ] DHT announce (if Shared)
- [x] Test: publish with each visibility

#### §7.1.4 UPDATE
- [x] New hash computation
- [x] Version linking (previous, root)
- [x] Inherit visibility
- [ ] DHT update announcement
- [x] Test: version chain creation

#### §7.1.5 DERIVE
- [x] Verify all sources queried
- [x] Compute provenance (merge root_L0L1)
- [x] Handle weight for duplicates
- [x] Calculate depth
- [x] **Owner field set to creator**
- [x] Store locally
- [x] Test: derive from multiple sources
- [x] Test: weight accumulation

#### §7.1.6 REFERENCE_L3_AS_L0
- [x] Verify L3 was queried
- [x] Create reference (not copy)
- [x] Provenance inheritance documented
- [x] Test: reference creation

#### §7.2.1 DISCOVER (Note: Hash-only for MVP)
- [ ] DHT lookup by hash
- [x] **No keyword search (application layer)**
- [ ] Test: lookup returns AnnouncePayload — requires nodalync-net

#### §7.2.2 PREVIEW
- [ ] Send PREVIEW_REQUEST
- [ ] Receive PREVIEW_RESPONSE
- [x] Handler: check visibility
- [x] Handler: check access
- [x] Handler: return L1Summary
- [x] Test: preview for each visibility

#### §7.2.3 QUERY
- [ ] **Auto-open channel if needed** — config ready, network stub
- [x] **Check available balance for auto-open**
- [x] **Return PAYMENT_REQUIRED if insufficient**
- [x] Ensure channel exists
- [x] Get price from preview
- [ ] Send QUERY_REQUEST
- [x] Verify response
- [x] Update channel state
- [x] Cache content
- [x] Handler: validate access
- [x] Handler: validate payment
- [x] Handler: update economics
- [x] **Handler: write ALL distributions to settlement queue**
- [x] **Handler: check settlement trigger**
- [ ] Test: full query flow

#### §7.3.1 CHANNEL_OPEN
- [x] Channel ID generation
- [ ] CHANNEL_OPEN message
- [x] CHANNEL_ACCEPT handling
- [x] Channel state initialization
- [x] Test: channel opening

#### §7.3.2 CHANNEL_ACCEPT (Handler)
- [x] Validate no existing channel
- [x] Create channel state
- [x] Return ChannelAcceptPayload
- [x] Test: accept incoming channel

#### §7.3.3 CHANNEL_CLOSE
- [x] Aggregate pending payments
- [ ] Sign final state
- [ ] CHANNEL_CLOSE message
- [ ] Settlement submission
- [x] Test: cooperative close

#### §7.3.4 CHANNEL_DISPUTE
- [ ] Submit dispute with latest state
- [x] Update local state to Disputed
- [x] Test: dispute initiation

#### §7.4 VERSION_REQUEST (Handler)
- [x] Get all versions for root
- [x] Return VersionResponsePayload
- [x] Test: version listing

#### §7.5 SETTLE_BATCH
- [x] Check trigger conditions (threshold/interval)
- [x] Get pending distributions from queue
- [x] Create batch (aggregate by recipient)
- [x] Batch ID generation
- [x] Merkle root computation
- [ ] On-chain submission
- [x] Mark settled in queue
- [ ] Confirmation broadcast
- [x] Test: batch settlement

---

## Phase 4: External Integration

### `nodalync-net` (Spec §11)

#### §11.1 Transport
- [ ] TCP transport
- [ ] QUIC transport (optional)
- [ ] WebSocket transport (optional)
- [ ] yamux multiplexing
- [ ] Noise protocol security

#### §11.2 Discovery (Hash-Only for MVP)
- [ ] Kademlia DHT
- [ ] Bucket size: 20
- [ ] Alpha: 3
- [ ] Replication: 20
- [ ] **dht_announce(): hash -> AnnouncePayload**
- [ ] **dht_get(): hash -> AnnouncePayload**
- [ ] **dht_remove(): remove announcement**
- [ ] **No keyword search (application layer)**
- [ ] Test: DHT announce and lookup by hash

#### §11.3 Peer Discovery
- [ ] Bootstrap nodes
- [ ] DNS discovery (optional)
- [ ] Peer exchange
- [ ] NAT traversal (STUN)
- [ ] Test: peer discovery

#### §11.4 Message Routing
- [ ] Point-to-point messages
- [ ] DHT lookup for peer addresses
- [ ] GossipSub for announcements
- [ ] Request-response protocol
- [ ] Timeout handling (30s)
- [ ] Retry logic (3 attempts)
- [ ] Test: message delivery

---

### `nodalync-settle` (Spec §12)

#### §12.2 On-Chain Data
- [ ] Balance tracking
- [ ] Channel state on-chain
- [ ] Attestation storage

#### §12.3 Contract Operations
- [ ] deposit()
- [ ] withdraw()
- [ ] attest()
- [ ] openChannel()
- [ ] updateChannel()
- [ ] closeChannel()
- [ ] disputeChannel()
- [ ] **counterDispute()** (submit higher-nonce state)
- [ ] resolveDispute()
- [ ] **settleBatch() - distributes to ALL recipients**
- [ ] Test: all contract operations (testnet)

#### Settlement Queue Integration
- [ ] Read from nodalync-store settlement queue
- [ ] Aggregate distributions by recipient
- [ ] Submit batch to chain
- [ ] Mark distributions as settled
- [ ] Test: end-to-end settlement flow

---

## Phase 5: User Interface

### `nodalync-cli`

#### Commands
- [ ] `nodalync init` — Create identity
- [ ] `nodalync publish <file>` — Publish content
- [ ] `nodalync search <query>` — Search network
- [ ] `nodalync preview <hash>` — Get L1 preview
- [ ] `nodalync query <hash>` — Query content
- [ ] `nodalync synthesize` — Create L3
- [ ] `nodalync list` — List local content
- [ ] `nodalync balance` — Show balance
- [ ] `nodalync settle` — Trigger settlement
- [ ] `nodalync visibility <hash> <tier>` — Change visibility
- [ ] `nodalync versions <hash>` — List versions
- [ ] `nodalync update <hash> <file>` — Create new version

#### Configuration
- [ ] Config file loading
- [ ] Default config generation
- [ ] Config validation

#### Output Formatting
- [ ] JSON output option
- [ ] Human-readable tables
- [ ] Progress indicators

---

## Constants Verification (Appendix B)

- [x] PROTOCOL_VERSION = 0x01
- [x] PROTOCOL_MAGIC = 0x00
- [x] MESSAGE_TIMEOUT_MS = 30000
- [x] CHANNEL_DISPUTE_PERIOD_MS = 86400000
- [x] MAX_CLOCK_SKEW_MS = 300000
- [x] MAX_CONTENT_SIZE = 104857600
- [x] MAX_MESSAGE_SIZE = 10485760
- [x] MAX_MENTIONS_PER_L0 = 1000
- [x] MAX_SOURCES_PER_L3 = 100
- [x] MAX_PROVENANCE_DEPTH = 100
- [x] MAX_TAGS = 20
- [x] MAX_TAG_LENGTH = 50
- [x] MAX_TITLE_LENGTH = 200
- [x] MAX_DESCRIPTION_LENGTH = 2000
- [x] MIN_PRICE = 1
- [x] SYNTHESIS_FEE_NUMERATOR = 5
- [x] SYNTHESIS_FEE_DENOMINATOR = 100
- [x] SETTLEMENT_BATCH_THRESHOLD = 10000000000
- [x] SETTLEMENT_BATCH_INTERVAL_MS = 3600000
- [x] DHT_BUCKET_SIZE = 20
- [x] DHT_ALPHA = 3
- [x] DHT_REPLICATION = 20

---

## Error Codes Verification (Appendix C)

- [x] NOT_FOUND = 0x0001
- [x] ACCESS_DENIED = 0x0002
- [x] PAYMENT_REQUIRED = 0x0003
- [x] PAYMENT_INVALID = 0x0004
- [x] RATE_LIMITED = 0x0005
- [x] VERSION_NOT_FOUND = 0x0006
- [x] CHANNEL_NOT_FOUND = 0x0100
- [x] CHANNEL_CLOSED = 0x0101
- [x] INSUFFICIENT_BALANCE = 0x0102
- [x] INVALID_NONCE = 0x0103
- [x] INVALID_SIGNATURE = 0x0104
- [x] INVALID_HASH = 0x0200
- [x] INVALID_PROVENANCE = 0x0201
- [x] INVALID_VERSION = 0x0202
- [x] INVALID_MANIFEST = 0x0203
- [x] CONTENT_TOO_LARGE = 0x0204
- [x] PEER_NOT_FOUND = 0x0300
- [x] CONNECTION_FAILED = 0x0301
- [x] TIMEOUT = 0x0302
- [x] INTERNAL_ERROR = 0xFFFF

---

## Integration Tests

- [ ] Full flow: create → publish → search → query
- [ ] Full flow: derive from multiple sources → query → verify distribution
- [ ] Full flow: version update → query old vs new
- [ ] Full flow: channel open → payments → close → settle
- [ ] Multi-node: two nodes, one publishes, one queries
- [ ] Multi-node: provenance chain across 3+ nodes

---

## Documentation

- [ ] README.md updated with build instructions
- [ ] Each crate has module-level docs
- [ ] Public API fully documented
- [ ] Examples in doc comments
- [ ] Architecture decision records (if any deviations from spec)
