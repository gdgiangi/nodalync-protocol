# Module: nodalync-types

**Source:** Protocol Specification §4

## Overview

This module defines all data structures used across the protocol. No logic, just definitions with validation constraints documented.

## Dependencies

- `nodalync-crypto` — Hash, PeerId, Signature types
- `serde` — Serialization derives

---

## §4.1 ContentType

```rust
#[repr(u8)]
pub enum ContentType {
    /// Raw input (documents, notes, transcripts)
    L0 = 0x00,
    /// Mentions (extracted atomic facts)
    L1 = 0x01,
    /// Insights (emergent synthesis)
    L3 = 0x03,
}
```

**Note:** L2 (Entity Graph) is internal only, not part of protocol messages.

---

## §4.2 Visibility

```rust
#[repr(u8)]
pub enum Visibility {
    /// Local only, not served to others
    Private = 0x00,
    /// Served if hash known, not announced to DHT
    Unlisted = 0x01,
    /// Announced to DHT, publicly queryable
    Shared = 0x02,
}
```

---

## §4.3 Version

```rust
pub struct Version {
    /// Sequential version number (1-indexed)
    pub number: u32,
    /// Hash of previous version (None if first version)
    pub previous: Option<Hash>,
    /// Hash of first version (stable identifier across versions)
    pub root: Hash,
    /// Creation timestamp
    pub timestamp: Timestamp,
}
```

**Constraints:**
- If `number == 1`: `previous` MUST be `None`, `root` MUST equal content hash
- If `number > 1`: `previous` MUST be `Some`, `root` MUST equal `previous.root`

---

## §4.4 Mention (L1)

```rust
pub struct Mention {
    /// H(content || source_location)
    pub id: Hash,
    /// The atomic fact (max 1000 chars)
    pub content: String,
    /// Where in L0 this fact came from
    pub source_location: SourceLocation,
    /// Type of fact
    pub classification: Classification,
    /// How certain we are this is in the source
    pub confidence: Confidence,
    /// Extracted entity names
    pub entities: Vec<String>,
}

pub struct SourceLocation {
    pub location_type: LocationType,
    /// Location identifier (paragraph number, page, timestamp, etc.)
    pub reference: String,
    /// Exact quote from source (max 500 chars)
    pub quote: Option<String>,
}

#[repr(u8)]
pub enum LocationType {
    Paragraph = 0x00,
    Page = 0x01,
    Timestamp = 0x02,
    Line = 0x03,
    Section = 0x04,
}

#[repr(u8)]
pub enum Classification {
    Claim = 0x00,
    Statistic = 0x01,
    Definition = 0x02,
    Observation = 0x03,
    Method = 0x04,
    Result = 0x05,
}

#[repr(u8)]
pub enum Confidence {
    /// Directly stated in source
    Explicit = 0x00,
    /// Reasonably inferred
    Inferred = 0x01,
}
```

---

## §4.5 Provenance

```rust
pub struct Provenance {
    /// All foundational L0+L1 sources
    pub root_L0L1: Vec<ProvenanceEntry>,
    /// Direct parent hashes (immediate sources)
    pub derived_from: Vec<Hash>,
    /// Max derivation depth from any L0
    pub depth: u32,
}

pub struct ProvenanceEntry {
    /// Content hash
    pub hash: Hash,
    /// Owner's node ID
    pub owner: PeerId,
    /// Visibility at time of derivation
    pub visibility: Visibility,
    /// Weight for duplicate handling
    /// (same source appearing multiple times gets higher weight)
    pub weight: u32,
}
```

**Constraints:**
- For L0: `root_L0L1 = [self]`, `derived_from = []`, `depth = 0`
- For L3: `root_L0L1.len() >= 1`, `derived_from.len() >= 1`
- All hashes in `derived_from` must have been queried by creator
- No self-reference allowed

---

## §4.6 AccessControl

```rust
pub struct AccessControl {
    /// If set, only these peers can query (None = all allowed)
    pub allowlist: Option<Vec<PeerId>>,
    /// These peers are blocked (None = none blocked)
    pub denylist: Option<Vec<PeerId>>,
    /// Require payment bond to query
    pub require_bond: bool,
    /// Bond amount if required
    pub bond_amount: Option<Amount>,
    /// Rate limit per peer (None = unlimited)
    pub max_queries_per_peer: Option<u32>,
}
```

**Access Logic:**
```
Access granted if:
    (allowlist is None OR peer in allowlist) AND
    (denylist is None OR peer NOT in denylist) AND
    (require_bond is false OR peer has posted bond)
```

---

## §4.7 Economics

```rust
pub struct Economics {
    /// Price per query (in smallest unit, 10^-8 NDL)
    pub price: Amount,
    /// Currency identifier
    pub currency: Currency,
    /// Total queries served
    pub total_queries: u64,
    /// Total revenue generated
    pub total_revenue: Amount,
}

#[repr(u8)]
pub enum Currency {
    /// Native Nodalync token
    NDL = 0x00,
}

/// Amount in smallest unit (10^-8 NDL)
pub type Amount = u64;
```

---

## §4.8 Manifest

The complete metadata for a content item:

```rust
pub struct Manifest {
    // === Identity ===
    /// Content hash (unique identifier)
    pub hash: Hash,
    /// Type of content
    pub content_type: ContentType,
    /// Owner's peer ID (receives synthesis fee, serves content)
    pub owner: PeerId,
    
    // === Versioning ===
    pub version: Version,
    
    // === Visibility & Access ===
    pub visibility: Visibility,
    pub access: AccessControl,
    
    // === Metadata ===
    pub metadata: Metadata,
    
    // === Economics ===
    pub economics: Economics,
    
    // === Provenance ===
    pub provenance: Provenance,
    
    // === Timestamps ===
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

pub struct Metadata {
    /// Max 200 chars
    pub title: String,
    /// Max 2000 chars
    pub description: Option<String>,
    /// Max 20 tags, each max 50 chars
    pub tags: Vec<String>,
    /// Size in bytes
    pub content_size: u64,
    /// MIME type if applicable
    pub mime_type: Option<String>,
}
```

---

## §4.9 L1Summary (Preview)

```rust
pub struct L1Summary {
    /// Source L0 hash
    pub l0_hash: Hash,
    /// Total mentions extracted
    pub mention_count: u32,
    /// First N mentions (max 5)
    pub preview_mentions: Vec<Mention>,
    /// Main topics (max 5)
    pub primary_topics: Vec<String>,
    /// 2-3 sentence summary (max 500 chars)
    pub summary: String,
}
```

---

## Additional Types

### Payment Channel

```rust
pub struct Channel {
    /// Unique channel identifier: H(initiator || responder || nonce)
    pub channel_id: Hash,
    pub peer_id: PeerId,
    pub state: ChannelState,
    pub my_balance: Amount,
    pub their_balance: Amount,
    pub nonce: u64,
    pub last_update: Timestamp,
    pub pending_payments: Vec<Payment>,
}

#[repr(u8)]
pub enum ChannelState {
    Opening = 0x00,
    Open = 0x01,
    Closing = 0x02,
    Closed = 0x03,
    Disputed = 0x04,
}

pub struct Payment {
    /// H(channel_id || nonce || amount || recipient)
    pub id: Hash,
    /// Channel this payment belongs to
    /// NOTE: Not in spec §5.3 but added for implementation convenience
    /// (needed to compute id, lookup payments by channel)
    pub channel_id: Hash,
    pub amount: Amount,
    pub recipient: PeerId,
    /// Content that was queried
    pub query_hash: Hash,
    /// For distribution to all root contributors
    pub provenance: Vec<ProvenanceEntry>,
    pub timestamp: Timestamp,
    /// Signed by payer
    pub signature: Signature,
}
```

### Distribution

```rust
pub struct Distribution {
    pub recipient: PeerId,
    pub amount: Amount,
    /// Which source this is for
    pub source_hash: Hash,
}
```

### Settlement

```rust
pub struct SettlementEntry {
    pub recipient: PeerId,
    pub amount: Amount,
    /// Content hashes for audit
    pub provenance_hashes: Vec<Hash>,
    /// Payment IDs included
    pub payment_ids: Vec<Hash>,
}

pub struct SettlementBatch {
    pub batch_id: Hash,
    pub entries: Vec<SettlementEntry>,
    /// Root of entries merkle tree
    pub merkle_root: Hash,
}
```

---

## Constants (from Appendix B)

```rust
pub mod constants {
    use super::Amount;
    
    // Limits
    pub const MAX_CONTENT_SIZE: u64 = 104_857_600;  // 100 MB
    pub const MAX_MESSAGE_SIZE: u64 = 10_485_760;   // 10 MB
    pub const MAX_MENTIONS_PER_L0: u32 = 1000;
    pub const MAX_SOURCES_PER_L3: u32 = 100;
    pub const MAX_PROVENANCE_DEPTH: u32 = 100;
    pub const MAX_TAGS: usize = 20;
    pub const MAX_TAG_LENGTH: usize = 50;
    pub const MAX_TITLE_LENGTH: usize = 200;
    pub const MAX_DESCRIPTION_LENGTH: usize = 2000;
    pub const MAX_SUMMARY_LENGTH: usize = 500;
    pub const MAX_MENTION_CONTENT_LENGTH: usize = 1000;
    pub const MAX_QUOTE_LENGTH: usize = 500;
    
    // Economics
    pub const MIN_PRICE: Amount = 1;
    pub const MAX_PRICE: Amount = 10_000_000_000_000_000;  // 10^16
    pub const SYNTHESIS_FEE_NUMERATOR: u64 = 5;
    pub const SYNTHESIS_FEE_DENOMINATOR: u64 = 100;  // 5%
    pub const SETTLEMENT_BATCH_THRESHOLD: Amount = 10_000_000_000;  // 100 NDL
    pub const SETTLEMENT_BATCH_INTERVAL_MS: u64 = 3_600_000;  // 1 hour
    
    // Timing
    pub const MESSAGE_TIMEOUT_MS: u64 = 30_000;
    pub const CHANNEL_DISPUTE_PERIOD_MS: u64 = 86_400_000;  // 24 hours
    pub const MAX_CLOCK_SKEW_MS: u64 = 300_000;  // 5 minutes
    
    // DHT
    pub const DHT_BUCKET_SIZE: usize = 20;
    pub const DHT_ALPHA: usize = 3;
    pub const DHT_REPLICATION: usize = 20;
}
```

---

## Error Types

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum ErrorCode {
    // Query Errors (0x0001 - 0x00FF)
    NotFound = 0x0001,
    AccessDenied = 0x0002,
    PaymentRequired = 0x0003,
    PaymentInvalid = 0x0004,
    RateLimited = 0x0005,
    VersionNotFound = 0x0006,
    
    // Channel Errors (0x0100 - 0x01FF)
    ChannelNotFound = 0x0100,
    ChannelClosed = 0x0101,
    InsufficientBalance = 0x0102,
    InvalidNonce = 0x0103,
    InvalidSignature = 0x0104,
    
    // Validation Errors (0x0200 - 0x02FF)
    InvalidHash = 0x0200,
    InvalidProvenance = 0x0201,
    InvalidVersion = 0x0202,
    InvalidManifest = 0x0203,
    ContentTooLarge = 0x0204,
    
    // Network Errors (0x0300 - 0x03FF)
    PeerNotFound = 0x0300,
    ConnectionFailed = 0x0301,
    Timeout = 0x0302,
    
    // Internal Errors
    InternalError = 0xFFFF,
}
```

---

## Implementation Notes

1. All types should derive `Debug`, `Clone`, `PartialEq`, `Eq` where sensible
2. All types should derive `Serialize`, `Deserialize` for wire format
3. Use `#[serde(rename_all = "snake_case")]` for consistent JSON representation
4. Consider `#[non_exhaustive]` for enums to allow future extension
5. Implement `Default` for types where a sensible default exists
