# Module: nodalync-wire

**Source:** Protocol Specification §6, Appendix A

## Overview

Message serialization and deserialization. Defines the wire format for all protocol messages.

## Dependencies

- `nodalync-types` — All data structures
- `ciborium` — CBOR encoding

---

## Message Envelope (§6.1)

```rust
pub struct Message {
    /// Protocol version (0x01)
    pub version: u8,
    /// Message type
    pub message_type: MessageType,
    /// Unique message ID
    pub id: Hash,
    /// Creation timestamp
    pub timestamp: Timestamp,
    /// Sender's peer ID
    pub sender: PeerId,
    /// Type-specific payload (CBOR encoded)
    pub payload: Vec<u8>,
    /// Signs H(version || type || id || timestamp || sender || payload_hash)
    pub signature: Signature,
}

#[repr(u16)]
pub enum MessageType {
    // Discovery (0x01xx)
    Announce = 0x0100,
    AnnounceUpdate = 0x0101,
    Search = 0x0110,
    SearchResponse = 0x0111,
    
    // Preview (0x02xx)
    PreviewRequest = 0x0200,
    PreviewResponse = 0x0201,
    
    // Query (0x03xx)
    QueryRequest = 0x0300,
    QueryResponse = 0x0301,
    QueryError = 0x0302,
    
    // Version (0x04xx)
    VersionRequest = 0x0400,
    VersionResponse = 0x0401,
    
    // Channel (0x05xx)
    ChannelOpen = 0x0500,
    ChannelAccept = 0x0501,
    ChannelUpdate = 0x0502,
    ChannelClose = 0x0503,
    ChannelDispute = 0x0504,
    
    // Settlement (0x06xx)
    SettleBatch = 0x0600,
    SettleConfirm = 0x0601,
    
    // Peer (0x07xx)
    Ping = 0x0700,
    Pong = 0x0701,
    PeerInfo = 0x0710,
}
```

---

## Payload Types (§6.2 - §6.8)

### Discovery Payloads

```rust
pub struct AnnouncePayload {
    pub hash: Hash,
    pub content_type: ContentType,
    pub title: String,
    pub l1_summary: L1Summary,
    pub price: Amount,
    pub addresses: Vec<String>,  // Multiaddrs
}

pub struct SearchPayload {
    pub query: String,
    pub filters: Option<SearchFilters>,
    pub limit: u32,
    pub offset: u32,
}

pub struct SearchFilters {
    pub content_types: Option<Vec<ContentType>>,
    pub max_price: Option<Amount>,
    pub min_reputation: Option<i64>,
    pub created_after: Option<Timestamp>,
    pub created_before: Option<Timestamp>,
    pub tags: Option<Vec<String>>,
}

pub struct SearchResult {
    pub hash: Hash,
    pub content_type: ContentType,
    pub title: String,
    pub owner: PeerId,
    pub l1_summary: L1Summary,
    pub price: Amount,
    pub total_queries: u64,
    pub relevance_score: f64,
    /// Publisher's reachable multiaddresses for reconnection
    pub publisher_addresses: Vec<String>,
}
```

### Query Payloads

```rust
pub struct QueryRequestPayload {
    pub hash: Hash,
    pub query: Option<String>,
    pub payment: Payment,
    pub version_spec: Option<VersionSpec>,
}

pub enum VersionSpec {
    Latest,
    Number(u32),
    Hash(Hash),
}

pub struct QueryResponsePayload {
    pub hash: Hash,
    pub content: Vec<u8>,
    pub manifest: Manifest,
    pub payment_receipt: PaymentReceipt,
}

pub struct PaymentReceipt {
    pub payment_id: Hash,
    pub amount: Amount,
    pub timestamp: Timestamp,
    pub channel_nonce: u64,
    pub distributor_signature: Signature,
}

pub struct QueryErrorPayload {
    pub hash: Hash,
    pub error_code: ErrorCode,
    pub message: Option<String>,
}
```

### Channel Payloads

```rust
pub struct ChannelOpenPayload {
    pub channel_id: Hash,
    pub initial_balance: Amount,
    pub funding_tx: Option<Vec<u8>>,
}

pub struct ChannelAcceptPayload {
    pub channel_id: Hash,
    pub initial_balance: Amount,
    pub funding_tx: Option<Vec<u8>>,
}

pub struct ChannelUpdatePayload {
    pub channel_id: Hash,
    pub nonce: u64,
    pub balances: ChannelBalances,
    pub payments: Vec<Payment>,
    pub signature: Signature,
}

pub struct ChannelBalances {
    pub initiator: Amount,
    pub responder: Amount,
}

pub struct ChannelClosePayload {
    pub channel_id: Hash,
    pub final_balances: ChannelBalances,
    /// Proposed on-chain settlement transaction
    pub settlement_tx: Vec<u8>,
}

pub struct ChannelDisputePayload {
    pub channel_id: Hash,
    /// Highest known state
    pub claimed_state: ChannelUpdatePayload,
    /// Supporting evidence
    pub evidence: Vec<Vec<u8>>,
}
```

### Version Payloads

```rust
pub struct VersionRequestPayload {
    /// Stable version root identifier
    pub version_root: Hash,
}

pub struct VersionResponsePayload {
    pub version_root: Hash,
    pub versions: Vec<VersionInfo>,
    pub latest: Hash,
}

pub struct VersionInfo {
    pub hash: Hash,
    pub number: u32,
    pub timestamp: Timestamp,
    pub visibility: Visibility,
    pub price: Amount,
}
```

### Settlement Payloads

```rust
pub struct SettleBatchPayload {
    pub batch_id: Hash,
    pub entries: Vec<SettlementEntry>,
    /// Root of entries merkle tree
    pub merkle_root: Hash,
    /// Signature from batch creator
    pub signature: Signature,
}

pub struct SettlementEntry {
    pub recipient: PeerId,
    pub amount: Amount,
    /// Content hashes for audit trail
    pub provenance_hashes: Vec<Hash>,
    /// Payment IDs included in this entry
    pub payment_ids: Vec<Hash>,
}

pub struct SettleConfirmPayload {
    pub batch_id: Hash,
    /// On-chain transaction ID
    pub transaction_id: String,
    pub block_number: u64,
    pub timestamp: Timestamp,
}
```

### Peer Payloads

```rust
pub struct PingPayload {
    pub nonce: u64,
}

pub struct PongPayload {
    pub nonce: u64,
}

pub struct PeerInfoPayload {
    pub peer_id: PeerId,
    pub public_key: PublicKey,
    pub addresses: Vec<String>,  // Multiaddrs
    pub capabilities: Vec<Capability>,
    pub content_count: u64,
    pub uptime: u64,  // Seconds
}

#[repr(u8)]
pub enum Capability {
    /// Can serve queries
    Query = 0x01,
    /// Supports payment channels
    Channel = 0x02,
    /// Can initiate settlement
    Settle = 0x04,
    /// Participates in DHT indexing
    Index = 0x08,
}
```

### Announce Update Payload

```rust
pub struct AnnounceUpdatePayload {
    /// Stable version root identifier
    pub version_root: Hash,
    /// New version hash
    pub new_hash: Hash,
    pub version_number: u32,
    pub title: String,
    pub l1_summary: L1Summary,
    pub price: Amount,
}
```

---

## Wire Format (Appendix A)

### Encoding Rules

1. **CBOR encoding** (RFC 8949) with deterministic rules:
   - Map keys sorted lexicographically
   - No indefinite-length arrays or maps
   - Minimal integer encoding
   - No floating-point for amounts (use u64)

2. **Message wire format:**
```
[0x00]                  # Protocol magic byte
[version: u8]           # Protocol version
[type: u16 BE]          # Message type
[length: u32 BE]        # Payload length
[payload: bytes]        # CBOR-encoded payload
[signature: 64 bytes]   # Ed25519 signature
```

### Hash Computation

```rust
// Content hash (domain separator 0x00)
fn content_hash(content: &[u8]) -> Hash {
    let mut hasher = Sha256::new();
    hasher.update(&[0x00]);  // Domain separator
    hasher.update(&(content.len() as u64).to_be_bytes());
    hasher.update(content);
    Hash(hasher.finalize().into())
}

// Message hash for signing (domain separator 0x01)
fn message_hash(msg: &Message) -> Hash {
    let mut hasher = Sha256::new();
    hasher.update(&[0x01]);  // Domain separator
    hasher.update(&[msg.version]);
    hasher.update(&(msg.message_type as u16).to_be_bytes());
    hasher.update(&msg.id.0);
    hasher.update(&msg.timestamp.to_be_bytes());
    hasher.update(&msg.sender.0);
    hasher.update(&content_hash(&msg.payload).0);
    Hash(hasher.finalize().into())
}

// Channel state hash (domain separator 0x02)
fn channel_state_hash(channel_id: &Hash, nonce: u64, balances: &ChannelBalances) -> Hash {
    let mut hasher = Sha256::new();
    hasher.update(&[0x02]);  // Domain separator
    hasher.update(&channel_id.0);
    hasher.update(&nonce.to_be_bytes());
    hasher.update(&balances.initiator.to_be_bytes());
    hasher.update(&balances.responder.to_be_bytes());
    Hash(hasher.finalize().into())
}
```

---

## Public API

```rust
// Encoding
pub fn encode_message(msg: &Message) -> Result<Vec<u8>, EncodeError>;
pub fn encode_payload<T: Serialize>(payload: &T) -> Result<Vec<u8>, EncodeError>;

// Decoding
pub fn decode_message(bytes: &[u8]) -> Result<Message, DecodeError>;
pub fn decode_payload<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, DecodeError>;

// Message construction helpers
pub fn create_message(
    message_type: MessageType,
    payload: Vec<u8>,
    identity: &Identity,
) -> Message;

// Validation (checks format, not semantic validity)
pub fn validate_message_format(msg: &Message) -> Result<(), FormatError>;
```

---

## Test Cases

1. **Roundtrip**: Encode → Decode → identical message
2. **Determinism**: Same message → same bytes (important for signatures)
3. **Invalid magic byte**: Reject
4. **Invalid version**: Reject
5. **Truncated message**: Reject
6. **Invalid CBOR**: Reject
7. **Signature mismatch**: Reject
