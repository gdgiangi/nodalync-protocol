# Module: nodalync-store

**Source:** Protocol Specification §5

## Overview

Local storage for content, manifests, provenance graph, and payment channels.

## Dependencies

- `nodalync-types` — All data structures
- `rusqlite` — SQLite for structured data
- `directories` — Platform-specific paths

---

## Storage Layout

```
~/.nodalync/
├── config.toml              # Node configuration
├── identity/
│   ├── keypair.key          # Ed25519 private key (encrypted)
│   └── peer_id              # Public identity
├── content/
│   └── {hash_prefix}/
│       └── {hash}           # Raw content files
├── nodalync.db              # SQLite: manifests, provenance, channels
└── cache/
    └── {hash_prefix}/
        └── {hash}           # Cached content from queries
```

---

## §5.1 State Components

### NodeState

```rust
pub struct NodeState {
    pub identity: Identity,
    pub content: ContentStore,
    pub manifests: ManifestStore,
    pub provenance: ProvenanceGraph,
    pub channels: ChannelStore,
    pub cache: CacheStore,
}
```

### Identity Storage

Private key encrypted at rest:
- Encryption: AES-256-GCM
- Key derivation: Argon2id from user password
- Nonce: Random 12 bytes, stored with ciphertext

---

## §5.2 Provenance Graph

Bidirectional graph for efficient traversal:

```rust
pub trait ProvenanceGraph {
    /// Add content with its derivation sources
    fn add(&mut self, hash: &Hash, derived_from: &[Hash]) -> Result<()>;
    
    /// Get all root L0+L1 sources (flattened)
    fn get_roots(&self, hash: &Hash) -> Result<Vec<ProvenanceEntry>>;
    
    /// Get all content derived from this hash
    fn get_derivations(&self, hash: &Hash) -> Result<Vec<Hash>>;
    
    /// Check if A is an ancestor of B
    fn is_ancestor(&self, ancestor: &Hash, descendant: &Hash) -> Result<bool>;
}
```

**SQL Schema:**
```sql
-- Forward edges
CREATE TABLE derived_from (
    content_hash BLOB NOT NULL,
    source_hash BLOB NOT NULL,
    PRIMARY KEY (content_hash, source_hash)
);

-- Cached flattened roots (for performance)
CREATE TABLE root_cache (
    content_hash BLOB NOT NULL,
    root_hash BLOB NOT NULL,
    owner BLOB NOT NULL,
    visibility INTEGER NOT NULL,
    weight INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (content_hash, root_hash)
);

CREATE INDEX idx_derivations ON derived_from(source_hash);
```

---

## §5.3 Payment Channels

```rust
pub trait ChannelStore {
    fn create(&mut self, peer: &PeerId, channel: Channel) -> Result<()>;
    fn get(&self, peer: &PeerId) -> Result<Option<Channel>>;
    fn update(&mut self, peer: &PeerId, channel: &Channel) -> Result<()>;
    fn list_open(&self) -> Result<Vec<(PeerId, Channel)>>;
    fn add_payment(&mut self, peer: &PeerId, payment: Payment) -> Result<()>;
    fn get_pending_payments(&self, peer: &PeerId) -> Result<Vec<Payment>>;
    fn clear_payments(&mut self, peer: &PeerId, payment_ids: &[Hash]) -> Result<()>;
}
```

---

## Trait Definitions

### ContentStore

```rust
pub trait ContentStore {
    /// Store content, returns hash
    fn store(&mut self, content: &[u8]) -> Result<Hash>;
    
    /// Store content with known hash (for verification)
    fn store_verified(&mut self, hash: &Hash, content: &[u8]) -> Result<()>;
    
    /// Load content by hash
    fn load(&self, hash: &Hash) -> Result<Option<Vec<u8>>>;
    
    /// Check if content exists
    fn exists(&self, hash: &Hash) -> bool;
    
    /// Delete content
    fn delete(&mut self, hash: &Hash) -> Result<()>;
    
    /// Get content size without loading
    fn size(&self, hash: &Hash) -> Result<Option<u64>>;
}
```

### ManifestStore

```rust
pub trait ManifestStore {
    fn store(&mut self, manifest: &Manifest) -> Result<()>;
    fn load(&self, hash: &Hash) -> Result<Option<Manifest>>;
    fn update(&mut self, manifest: &Manifest) -> Result<()>;
    fn delete(&mut self, hash: &Hash) -> Result<()>;
    
    /// List manifests with optional filtering
    fn list(&self, filter: ManifestFilter) -> Result<Vec<Manifest>>;
    
    /// Get all versions of content by version_root
    fn get_versions(&self, version_root: &Hash) -> Result<Vec<Manifest>>;
}

pub struct ManifestFilter {
    pub visibility: Option<Visibility>,
    pub content_type: Option<ContentType>,
    pub created_after: Option<Timestamp>,
    pub created_before: Option<Timestamp>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}
```

### CacheStore

```rust
pub trait CacheStore {
    /// Cache content from a query
    fn cache(&mut self, entry: CachedContent) -> Result<()>;
    
    /// Get cached content
    fn get(&self, hash: &Hash) -> Result<Option<CachedContent>>;
    
    /// Check if cached
    fn is_cached(&self, hash: &Hash) -> bool;
    
    /// Evict old entries (LRU)
    fn evict(&mut self, max_size_bytes: u64) -> Result<u64>;
    
    /// Clear all cache
    fn clear(&mut self) -> Result<()>;
}

pub struct CachedContent {
    pub hash: Hash,
    pub content: Vec<u8>,
    pub source_peer: PeerId,
    pub queried_at: Timestamp,
    /// NOTE: Spec §5.1 says "PaymentProof" but that type is undefined.
    /// Using PaymentReceipt from §6.4 instead.
    pub payment_proof: PaymentReceipt,
}
```

### SettlementQueueStore

The settlement queue stores pending distributions until batch settlement.
`nodalync-ops` writes to this queue after processing queries.
`nodalync-settle` reads from this queue to create settlement batches.

```rust
pub trait SettlementQueueStore {
    /// Add a distribution to the queue
    fn enqueue(&mut self, distribution: QueuedDistribution) -> Result<()>;
    
    /// Get all pending distributions
    fn get_pending(&self) -> Result<Vec<QueuedDistribution>>;
    
    /// Get pending distributions for a specific recipient
    fn get_pending_for(&self, recipient: &PeerId) -> Result<Vec<QueuedDistribution>>;
    
    /// Get total pending amount across all recipients
    fn get_pending_total(&self) -> Result<Amount>;
    
    /// Mark distributions as settled (by payment IDs)
    fn mark_settled(&mut self, payment_ids: &[Hash], batch_id: &Hash) -> Result<()>;
    
    /// Get last settlement timestamp
    fn get_last_settlement_time(&self) -> Result<Option<Timestamp>>;
    
    /// Set last settlement timestamp
    fn set_last_settlement_time(&mut self, timestamp: Timestamp) -> Result<()>;
}

pub struct QueuedDistribution {
    /// Original payment ID this distribution came from
    pub payment_id: Hash,
    /// Recipient of this distribution
    pub recipient: PeerId,
    /// Amount owed
    pub amount: Amount,
    /// Source content hash (for audit)
    pub source_hash: Hash,
    /// When the original query happened
    pub queued_at: Timestamp,
}
```
```

---

## SQL Schema (Full)

```sql
-- Manifests
CREATE TABLE manifests (
    hash BLOB PRIMARY KEY,
    content_type INTEGER NOT NULL,
    version_number INTEGER NOT NULL,
    version_previous BLOB,
    version_root BLOB NOT NULL,
    version_timestamp INTEGER NOT NULL,
    visibility INTEGER NOT NULL,
    title TEXT NOT NULL,
    description TEXT,
    tags TEXT,  -- JSON array
    content_size INTEGER NOT NULL,
    mime_type TEXT,
    price INTEGER NOT NULL,
    total_queries INTEGER NOT NULL DEFAULT 0,
    total_revenue INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    -- Access control stored as JSON
    access_control TEXT NOT NULL
);

CREATE INDEX idx_manifests_visibility ON manifests(visibility);
CREATE INDEX idx_manifests_version_root ON manifests(version_root);
CREATE INDEX idx_manifests_created ON manifests(created_at);

-- L1 Summaries
CREATE TABLE l1_summaries (
    l0_hash BLOB PRIMARY KEY,
    mention_count INTEGER NOT NULL,
    preview_mentions TEXT NOT NULL,  -- JSON
    primary_topics TEXT NOT NULL,     -- JSON
    summary TEXT NOT NULL
);

-- Payment Channels
CREATE TABLE channels (
    peer_id BLOB PRIMARY KEY,
    state INTEGER NOT NULL,
    my_balance INTEGER NOT NULL,
    their_balance INTEGER NOT NULL,
    nonce INTEGER NOT NULL,
    last_update INTEGER NOT NULL
);

-- Pending Payments
CREATE TABLE payments (
    id BLOB PRIMARY KEY,
    channel_peer BLOB NOT NULL,
    amount INTEGER NOT NULL,
    recipient BLOB NOT NULL,
    query_hash BLOB NOT NULL,
    provenance TEXT NOT NULL,  -- JSON
    timestamp INTEGER NOT NULL,
    signature BLOB NOT NULL,
    settled INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY (channel_peer) REFERENCES channels(peer_id)
);

CREATE INDEX idx_payments_channel ON payments(channel_peer);
CREATE INDEX idx_payments_settled ON payments(settled);

-- Cache metadata (content stored on filesystem)
CREATE TABLE cache (
    hash BLOB PRIMARY KEY,
    source_peer BLOB NOT NULL,
    queried_at INTEGER NOT NULL,
    size_bytes INTEGER NOT NULL,
    payment_receipt TEXT NOT NULL  -- JSON
);

CREATE INDEX idx_cache_queried ON cache(queried_at);

-- Settlement Queue
CREATE TABLE settlement_queue (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    payment_id BLOB NOT NULL,
    recipient BLOB NOT NULL,
    amount INTEGER NOT NULL,
    source_hash BLOB NOT NULL,
    queued_at INTEGER NOT NULL,
    settled INTEGER NOT NULL DEFAULT 0,
    batch_id BLOB  -- Set when settled
);

CREATE INDEX idx_settlement_queue_recipient ON settlement_queue(recipient);
CREATE INDEX idx_settlement_queue_settled ON settlement_queue(settled);

-- Settlement metadata
CREATE TABLE settlement_meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
-- Stores: last_settlement_time
```

---

## Test Cases

1. **Content roundtrip**: Store → Load → identical
2. **Manifest CRUD**: Create, read, update, delete
3. **Provenance graph**: Add edges → get_roots returns correct set
4. **Weight accumulation**: Same source via multiple paths → weight increases
5. **Channel state**: Open → payments → state updates correctly
6. **Cache eviction**: LRU eviction frees correct entries
7. **Concurrent access**: Multiple readers, single writer
8. **Settlement queue enqueue**: Add distribution → retrievable
9. **Settlement queue totals**: Multiple distributions → correct sum
10. **Settlement queue mark settled**: Mark as settled → no longer in pending
11. **Settlement queue by recipient**: Filter by recipient works
