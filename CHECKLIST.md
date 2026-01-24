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
- [ ] Enum: L0 = 0x00, L1 = 0x01, L3 = 0x03
- [ ] Serialization matches spec

#### §4.2 Visibility
- [ ] Enum: Private = 0x00, Unlisted = 0x01, Shared = 0x02
- [ ] Serialization matches spec

#### §4.3 Version
- [ ] Struct: number, previous, root, timestamp
- [ ] Constraints: v1 has null previous, root == self
- [ ] Constraints: v2+ has non-null previous, root == previous.root

#### §4.4 Mention (L1)
- [ ] Mention struct with all fields
- [ ] SourceLocation struct
- [ ] LocationType enum
- [ ] Classification enum
- [ ] Confidence enum

#### §4.5 Provenance
- [ ] Provenance struct: root_L0L1, derived_from, depth
- [ ] ProvenanceEntry struct: hash, owner, visibility, weight
- [ ] Constraints documented in types

#### §4.6 AccessControl
- [ ] Struct: allowlist, denylist, require_bond, bond_amount, max_queries_per_peer
- [ ] Access logic documented

#### §4.7 Economics
- [ ] Struct: price, currency, total_queries, total_revenue
- [ ] Currency enum (NDL = 0x00)

#### §4.8 Manifest
- [ ] Complete Manifest struct with all fields
- [ ] **owner: PeerId field** (content owner)
- [ ] Metadata struct

#### §4.9 L1Summary
- [ ] Struct: l0_hash, mention_count, preview_mentions, primary_topics, summary

---

## Phase 2: Core Logic

### `nodalync-wire` (Spec §6, Appendix A)

#### §6.1 Message Envelope
- [ ] Message struct: version, type, id, timestamp, sender, payload, signature
- [ ] MessageType enum with all 17 types
- [ ] Protocol magic byte (0x00)
- [ ] Protocol version (0x01)

#### §6.2 Discovery Messages
- [ ] AnnouncePayload
- [ ] AnnounceUpdatePayload
- [ ] SearchPayload + SearchFilters
- [ ] SearchResponsePayload + SearchResult

#### §6.3 Preview Messages
- [ ] PreviewRequestPayload
- [ ] PreviewResponsePayload

#### §6.4 Query Messages
- [ ] QueryRequestPayload + VersionSpec
- [ ] QueryResponsePayload + PaymentReceipt
- [ ] QueryErrorPayload + QueryError enum

#### §6.5 Version Messages
- [ ] VersionRequestPayload
- [ ] VersionResponsePayload + VersionInfo

#### §6.6 Channel Messages
- [ ] ChannelOpenPayload
- [ ] ChannelAcceptPayload
- [ ] ChannelUpdatePayload + ChannelBalances
- [ ] ChannelClosePayload
- [ ] ChannelDisputePayload

#### §6.7 Settlement Messages
- [ ] SettleBatchPayload + SettlementEntry
- [ ] SettleConfirmPayload

#### §6.8 Peer Messages
- [ ] PingPayload
- [ ] PongPayload
- [ ] PeerInfoPayload + Capability enum

#### Appendix A: Wire Formats
- [ ] Deterministic CBOR encoding
- [ ] Map keys sorted lexicographically
- [ ] No indefinite-length arrays
- [ ] Minimal integer encoding
- [ ] ContentHash domain separator
- [ ] MessageHash domain separator
- [ ] ChannelStateHash domain separator

---

### `nodalync-store` (Spec §5)

#### §5.1 State Components
- [ ] NodeState struct
- [ ] Identity storage (encrypted private key)
- [ ] ContentRecord struct
- [ ] PeerInfo struct
- [ ] CachedContent struct

#### §5.2 Provenance Graph
- [ ] Forward edges: derived_from
- [ ] Backward edges: derivations
- [ ] Flattened roots cache
- [ ] `add_content()` operation
- [ ] `get_roots()` operation
- [ ] `get_derivations()` operation

#### §5.3 Payment Channels
- [ ] Channel struct
- [ ] ChannelState enum
- [ ] Payment struct

#### Storage Implementation
- [ ] Content storage (filesystem)
- [ ] Manifest storage (SQLite)
- [ ] Provenance graph storage
- [ ] Channel state storage
- [ ] Cache management
- [ ] **Settlement queue storage**
- [ ] **QueuedDistribution struct**
- [ ] **enqueue() method**
- [ ] **get_pending() method**
- [ ] **mark_settled() method**
- [ ] **get_pending_total() method**
- [ ] Test: store and retrieve content
- [ ] Test: store and retrieve manifest
- [ ] Test: provenance graph traversal
- [ ] Test: settlement queue operations

---

### `nodalync-valid` (Spec §9)

#### §9.1 Content Validation
- [ ] `ContentHash(content) == manifest.hash`
- [ ] `len(content) == manifest.metadata.content_size`
- [ ] Title length <= 200
- [ ] Description length <= 2000
- [ ] Tags count <= 20, each <= 50 chars
- [ ] content_type valid
- [ ] visibility valid
- [ ] Test: all validation rules

#### §9.2 Version Validation
- [ ] v1: previous null, root == hash
- [ ] v2+: previous not null, root == previous.root
- [ ] number == previous.number + 1
- [ ] timestamp > previous.timestamp
- [ ] Test: valid version chain
- [ ] Test: invalid version rejected

#### §9.3 Provenance Validation
- [ ] L0: root_L0L1 == [self], derived_from == [], depth == 0
- [ ] L3: root_L0L1 >= 1, derived_from >= 1
- [ ] All derived_from exist in sources
- [ ] root_L0L1 computation correct
- [ ] depth == max(sources.depth) + 1
- [ ] No self-reference
- [ ] Test: valid L0 provenance
- [ ] Test: valid L3 provenance
- [ ] Test: invalid provenance rejected

#### §9.4 Payment Validation
- [ ] amount >= price
- [ ] recipient == owner
- [ ] query_hash == manifest.hash
- [ ] channel.state == Open
- [ ] channel.their_balance >= amount
- [ ] nonce > channel.nonce
- [ ] signature valid
- [ ] provenance matches manifest
- [ ] Test: all payment validation rules

#### §9.5 Message Validation
- [ ] version == PROTOCOL_VERSION
- [ ] type is valid MessageType
- [ ] timestamp within ±5 minutes
- [ ] sender is valid PeerId
- [ ] signature valid
- [ ] payload decodes correctly
- [ ] Test: valid message accepted
- [ ] Test: invalid message rejected

#### §9.6 Access Validation
- [ ] Private: always deny external
- [ ] Unlisted: check allowlist/denylist
- [ ] Shared: check denylist only
- [ ] Bond requirement check
- [ ] Test: all access scenarios

---

### `nodalync-econ` (Spec §10)

#### §10.1 Revenue Distribution
- [ ] SYNTHESIS_FEE = 0.05 (5%)
- [ ] ROOT_POOL = 0.95 (95%)
- [ ] Distribution calculation
- [ ] Weight handling for duplicates
- [ ] Owner gets synthesis fee + own roots
- [ ] Test: distribution example from spec (Bob/Alice/Carol)

#### §10.3 Price Constraints
- [ ] MIN_PRICE = 1
- [ ] MAX_PRICE = 10^16
- [ ] price is uint64
- [ ] Test: price validation

#### §10.4 Settlement Batching
- [ ] BATCH_THRESHOLD = 100 NDL
- [ ] BATCH_INTERVAL = 3600 seconds
- [ ] Batch trigger logic
- [ ] Aggregate by recipient
- [ ] Merkle root computation
- [ ] Test: batching logic

---

## Phase 3: Operations

### `nodalync-ops` (Spec §7)

#### §7.1.1 CREATE
- [ ] Hash computation
- [ ] Version initialization (v1)
- [ ] Provenance initialization (L0: self-referential)
- [ ] **Owner field set to creator**
- [ ] Manifest creation
- [ ] Local storage
- [ ] Test: create L0 content

#### §7.1.2 EXTRACT_L1
- [ ] Load content
- [ ] **Rule-based extraction (MVP)**
- [ ] classify_sentence() helper
- [ ] extract_entities() helper
- [ ] Generate L1Summary
- [ ] Store L1 data
- [ ] **L1Extractor trait for plugin architecture**
- [ ] Test: L1 extraction

#### §7.1.3 PUBLISH
- [ ] Update visibility
- [ ] Update price
- [ ] Update access control
- [ ] DHT announce (if Shared)
- [ ] Test: publish with each visibility

#### §7.1.4 UPDATE
- [ ] New hash computation
- [ ] Version linking (previous, root)
- [ ] Inherit visibility
- [ ] DHT update announcement
- [ ] Test: version chain creation

#### §7.1.5 DERIVE
- [ ] Verify all sources queried
- [ ] Compute provenance (merge root_L0L1)
- [ ] Handle weight for duplicates
- [ ] Calculate depth
- [ ] **Owner field set to creator**
- [ ] Store locally
- [ ] Test: derive from multiple sources
- [ ] Test: weight accumulation

#### §7.1.6 REFERENCE_L3_AS_L0
- [ ] Verify L3 was queried
- [ ] Create reference (not copy)
- [ ] Provenance inheritance documented
- [ ] Test: reference creation

#### §7.2.1 DISCOVER (Note: Hash-only for MVP)
- [ ] DHT lookup by hash
- [ ] **No keyword search (application layer)**
- [ ] Test: lookup returns AnnouncePayload

#### §7.2.2 PREVIEW
- [ ] Send PREVIEW_REQUEST
- [ ] Receive PREVIEW_RESPONSE
- [ ] Handler: check visibility
- [ ] Handler: check access
- [ ] Handler: return L1Summary
- [ ] Test: preview for each visibility

#### §7.2.3 QUERY
- [ ] **Auto-open channel if needed**
- [ ] **Check available balance for auto-open**
- [ ] **Return PAYMENT_REQUIRED if insufficient**
- [ ] Ensure channel exists
- [ ] Get price from preview
- [ ] Send QUERY_REQUEST
- [ ] Verify response
- [ ] Update channel state
- [ ] Cache content
- [ ] Handler: validate access
- [ ] Handler: validate payment
- [ ] Handler: update economics
- [ ] **Handler: write ALL distributions to settlement queue**
- [ ] **Handler: check settlement trigger**
- [ ] Test: full query flow

#### §7.3.1 CHANNEL_OPEN
- [ ] Channel ID generation
- [ ] CHANNEL_OPEN message
- [ ] CHANNEL_ACCEPT handling
- [ ] Channel state initialization
- [ ] Test: channel opening

#### §7.3.2 CHANNEL_ACCEPT (Handler)
- [ ] Validate no existing channel
- [ ] Create channel state
- [ ] Return ChannelAcceptPayload
- [ ] Test: accept incoming channel

#### §7.3.3 CHANNEL_CLOSE
- [ ] Aggregate pending payments
- [ ] Sign final state
- [ ] CHANNEL_CLOSE message
- [ ] Settlement submission
- [ ] Test: cooperative close

#### §7.3.4 CHANNEL_DISPUTE
- [ ] Submit dispute with latest state
- [ ] Update local state to Disputed
- [ ] Test: dispute initiation

#### §7.4 VERSION_REQUEST (Handler)
- [ ] Get all versions for root
- [ ] Return VersionResponsePayload
- [ ] Test: version listing

#### §7.5 SETTLE_BATCH
- [ ] Check trigger conditions (threshold/interval)
- [ ] Get pending distributions from queue
- [ ] Create batch (aggregate by recipient)
- [ ] Batch ID generation
- [ ] Merkle root computation
- [ ] On-chain submission
- [ ] Mark settled in queue
- [ ] Confirmation broadcast
- [ ] Test: batch settlement

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

- [ ] PROTOCOL_VERSION = 0x01
- [ ] PROTOCOL_MAGIC = 0x00
- [ ] MESSAGE_TIMEOUT_MS = 30000
- [ ] CHANNEL_DISPUTE_PERIOD_MS = 86400000
- [ ] MAX_CLOCK_SKEW_MS = 300000
- [ ] MAX_CONTENT_SIZE = 104857600
- [ ] MAX_MESSAGE_SIZE = 10485760
- [ ] MAX_MENTIONS_PER_L0 = 1000
- [ ] MAX_SOURCES_PER_L3 = 100
- [ ] MAX_PROVENANCE_DEPTH = 100
- [ ] MAX_TAGS = 20
- [ ] MAX_TAG_LENGTH = 50
- [ ] MAX_TITLE_LENGTH = 200
- [ ] MAX_DESCRIPTION_LENGTH = 2000
- [ ] MIN_PRICE = 1
- [ ] SYNTHESIS_FEE_NUMERATOR = 5
- [ ] SYNTHESIS_FEE_DENOMINATOR = 100
- [ ] SETTLEMENT_BATCH_THRESHOLD = 10000000000
- [ ] SETTLEMENT_BATCH_INTERVAL_MS = 3600000
- [ ] DHT_BUCKET_SIZE = 20
- [ ] DHT_ALPHA = 3
- [ ] DHT_REPLICATION = 20

---

## Error Codes Verification (Appendix C)

- [ ] NOT_FOUND = 0x0001
- [ ] ACCESS_DENIED = 0x0002
- [ ] PAYMENT_REQUIRED = 0x0003
- [ ] PAYMENT_INVALID = 0x0004
- [ ] RATE_LIMITED = 0x0005
- [ ] VERSION_NOT_FOUND = 0x0006
- [ ] CHANNEL_NOT_FOUND = 0x0100
- [ ] CHANNEL_CLOSED = 0x0101
- [ ] INSUFFICIENT_BALANCE = 0x0102
- [ ] INVALID_NONCE = 0x0103
- [ ] INVALID_SIGNATURE = 0x0104
- [ ] INVALID_HASH = 0x0200
- [ ] INVALID_PROVENANCE = 0x0201
- [ ] INVALID_VERSION = 0x0202
- [ ] INVALID_MANIFEST = 0x0203
- [ ] CONTENT_TOO_LARGE = 0x0204
- [ ] PEER_NOT_FOUND = 0x0300
- [ ] CONNECTION_FAILED = 0x0301
- [ ] TIMEOUT = 0x0302
- [ ] INTERNAL_ERROR = 0xFFFF

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
