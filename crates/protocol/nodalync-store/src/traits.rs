//! Trait definitions for storage components.
//!
//! This module defines the trait contracts for all storage components.
//! Implementations may vary (e.g., in-memory vs SQLite) but must satisfy
//! these interfaces.

use nodalync_crypto::{Hash, PeerId, Timestamp};
use nodalync_types::{Amount, Channel, Manifest, Payment, ProvenanceEntry};

use crate::error::Result;
use crate::types::{CachedContent, ManifestFilter, PeerInfo, QueuedDistribution};

// =============================================================================
// Content Storage
// =============================================================================

/// Trait for storing raw content by hash.
///
/// Content is stored on the filesystem, keyed by content hash.
pub trait ContentStore {
    /// Store content and return its hash.
    ///
    /// Computes the content hash and stores the content.
    /// If content with the same hash already exists, this is a no-op.
    fn store(&mut self, content: &[u8]) -> Result<Hash>;

    /// Store content with a known hash.
    ///
    /// Verifies that the content matches the provided hash before storing.
    /// Returns an error if the hash doesn't match.
    fn store_verified(&mut self, hash: &Hash, content: &[u8]) -> Result<()>;

    /// Load content by hash.
    ///
    /// Returns `None` if the content doesn't exist.
    fn load(&self, hash: &Hash) -> Result<Option<Vec<u8>>>;

    /// Check if content exists.
    fn exists(&self, hash: &Hash) -> bool;

    /// Delete content by hash.
    ///
    /// Returns Ok(()) even if the content doesn't exist.
    fn delete(&mut self, hash: &Hash) -> Result<()>;

    /// Get content size without loading the full content.
    ///
    /// Returns `None` if the content doesn't exist.
    fn size(&self, hash: &Hash) -> Result<Option<u64>>;
}

// =============================================================================
// Manifest Storage
// =============================================================================

/// Trait for storing content manifests.
///
/// Manifests are stored in SQLite for efficient querying and filtering.
pub trait ManifestStore {
    /// Store a manifest.
    ///
    /// If a manifest with the same hash exists, this is a no-op.
    fn store(&mut self, manifest: &Manifest) -> Result<()>;

    /// Load a manifest by content hash.
    ///
    /// Returns `None` if the manifest doesn't exist.
    fn load(&self, hash: &Hash) -> Result<Option<Manifest>>;

    /// Update an existing manifest.
    ///
    /// Returns an error if the manifest doesn't exist.
    fn update(&mut self, manifest: &Manifest) -> Result<()>;

    /// Delete a manifest by hash.
    ///
    /// Returns Ok(()) even if the manifest doesn't exist.
    fn delete(&mut self, hash: &Hash) -> Result<()>;

    /// List manifests matching filter criteria.
    fn list(&self, filter: ManifestFilter) -> Result<Vec<Manifest>>;

    /// Get all versions of content by version root.
    ///
    /// Returns all manifests that share the same version root,
    /// ordered by version number.
    fn get_versions(&self, version_root: &Hash) -> Result<Vec<Manifest>>;
}

// =============================================================================
// Provenance Graph
// =============================================================================

/// Trait for tracking content derivation relationships.
///
/// The provenance graph tracks which content was derived from which sources,
/// enabling revenue distribution to all contributors.
pub trait ProvenanceGraph {
    /// Add content with its derivation sources.
    ///
    /// For L0 content, `derived_from` should be empty.
    /// For L3 content, `derived_from` contains the hashes of source content.
    fn add(&mut self, hash: &Hash, derived_from: &[Hash]) -> Result<()>;

    /// Get all root L0+L1 sources for content (flattened).
    ///
    /// Returns the provenance entries with accumulated weights.
    fn get_roots(&self, hash: &Hash) -> Result<Vec<ProvenanceEntry>>;

    /// Get all content derived from this hash.
    ///
    /// Returns hashes of content that directly derives from this source.
    fn get_derivations(&self, hash: &Hash) -> Result<Vec<Hash>>;

    /// Check if `ancestor` is an ancestor of `descendant`.
    ///
    /// Traverses the provenance graph to determine ancestry.
    fn is_ancestor(&self, ancestor: &Hash, descendant: &Hash) -> Result<bool>;

    /// Store a provenance entry in the root cache.
    ///
    /// This is used to cache flattened root entries for performance.
    fn cache_root(&mut self, content_hash: &Hash, entry: &ProvenanceEntry) -> Result<()>;
}

// =============================================================================
// Channel Storage
// =============================================================================

/// Trait for storing payment channel state.
///
/// Channels enable off-chain payments between peers.
pub trait ChannelStore {
    /// Create a new channel with a peer.
    ///
    /// Returns an error if a channel already exists with this peer.
    fn create(&mut self, peer: &PeerId, channel: Channel) -> Result<()>;

    /// Get channel state for a peer.
    ///
    /// Returns `None` if no channel exists with this peer.
    fn get(&self, peer: &PeerId) -> Result<Option<Channel>>;

    /// Update channel state.
    ///
    /// Returns an error if no channel exists with this peer.
    fn update(&mut self, peer: &PeerId, channel: &Channel) -> Result<()>;

    /// List all open channels.
    ///
    /// Returns tuples of (peer_id, channel) for all open channels.
    fn list_open(&self) -> Result<Vec<(PeerId, Channel)>>;

    /// Clear all channels.
    ///
    /// Used for recovery when channel state becomes inconsistent.
    fn clear_all(&mut self) -> Result<()>;

    /// Add a payment to a channel.
    ///
    /// The payment is stored as pending until settlement.
    fn add_payment(&mut self, peer: &PeerId, payment: Payment) -> Result<()>;

    /// Get pending payments for a channel.
    fn get_pending_payments(&self, peer: &PeerId) -> Result<Vec<Payment>>;

    /// Clear pending payments by ID.
    ///
    /// Called after payments have been settled.
    fn clear_payments(&mut self, peer: &PeerId, payment_ids: &[Hash]) -> Result<()>;
}

// =============================================================================
// Peer Storage
// =============================================================================

/// Trait for storing peer information.
///
/// Tracks known peers with their addresses and reputation.
pub trait PeerStore {
    /// Insert or update peer information.
    ///
    /// If the peer exists, updates the record. Otherwise, inserts a new one.
    fn upsert(&mut self, peer: &PeerInfo) -> Result<()>;

    /// Get peer information by ID.
    ///
    /// Returns `None` if the peer is not known.
    fn get(&self, peer_id: &PeerId) -> Result<Option<PeerInfo>>;

    /// List all known peers.
    fn list(&self) -> Result<Vec<PeerInfo>>;

    /// Update last seen timestamp for a peer.
    ///
    /// Returns an error if the peer is not known.
    fn update_last_seen(&mut self, peer_id: &PeerId, timestamp: Timestamp) -> Result<()>;

    /// Adjust reputation for a peer.
    ///
    /// Delta can be positive or negative.
    /// Returns an error if the peer is not known.
    fn update_reputation(&mut self, peer_id: &PeerId, delta: i64) -> Result<()>;

    /// Delete a peer from the store.
    ///
    /// Returns Ok(()) even if the peer doesn't exist.
    fn delete(&mut self, peer_id: &PeerId) -> Result<()>;
}

// =============================================================================
// Cache Storage
// =============================================================================

/// Trait for caching queried content.
///
/// Caches content retrieved from other peers to avoid re-querying.
pub trait CacheStore {
    /// Cache content from a query.
    ///
    /// Stores both the content and metadata about the query.
    fn cache(&mut self, entry: CachedContent) -> Result<()>;

    /// Get cached content.
    ///
    /// Returns `None` if the content is not cached.
    fn get(&self, hash: &Hash) -> Result<Option<CachedContent>>;

    /// Check if content is cached.
    fn is_cached(&self, hash: &Hash) -> bool;

    /// Evict cached entries to free space (LRU eviction).
    ///
    /// Removes entries until total cache size is below `max_size_bytes`.
    /// Returns the number of bytes freed.
    fn evict(&mut self, max_size_bytes: u64) -> Result<u64>;

    /// Clear all cached content.
    fn clear(&mut self) -> Result<()>;

    /// Get total cache size in bytes.
    fn total_size(&self) -> Result<u64>;
}

// =============================================================================
// Settlement Queue Storage
// =============================================================================

/// Trait for the settlement queue.
///
/// Stores pending distributions until they are batch-settled on-chain.
pub trait SettlementQueueStore {
    /// Add a distribution to the queue.
    fn enqueue(&mut self, distribution: QueuedDistribution) -> Result<()>;

    /// Get all pending distributions.
    fn get_pending(&self) -> Result<Vec<QueuedDistribution>>;

    /// Get pending distributions for a specific recipient.
    fn get_pending_for(&self, recipient: &PeerId) -> Result<Vec<QueuedDistribution>>;

    /// Get total pending amount across all recipients.
    fn get_pending_total(&self) -> Result<Amount>;

    /// Mark distributions as settled.
    ///
    /// Associates the distributions with a batch ID.
    fn mark_settled(&mut self, payment_ids: &[Hash], batch_id: &Hash) -> Result<()>;

    /// Get the last settlement timestamp.
    ///
    /// Returns `None` if no settlement has occurred yet.
    fn get_last_settlement_time(&self) -> Result<Option<Timestamp>>;

    /// Set the last settlement timestamp.
    fn set_last_settlement_time(&mut self, timestamp: Timestamp) -> Result<()>;
}
