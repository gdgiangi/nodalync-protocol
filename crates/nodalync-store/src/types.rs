//! Store-specific types.
//!
//! This module defines types used by the storage layer that are not
//! part of the core protocol types.

use nodalync_crypto::{Hash, PeerId, PublicKey, Timestamp};
use nodalync_types::{Amount, ContentType, Visibility};
use nodalync_wire::payload::PaymentReceipt;
use serde::{Deserialize, Serialize};

/// Filter criteria for listing manifests.
///
/// All fields are optional. When a field is `None`, no filtering
/// is applied for that criterion.
#[derive(Debug, Clone, Default)]
pub struct ManifestFilter {
    /// Filter by visibility level.
    pub visibility: Option<Visibility>,
    /// Filter by content type.
    pub content_type: Option<ContentType>,
    /// Filter by creation time (minimum).
    pub created_after: Option<Timestamp>,
    /// Filter by creation time (maximum).
    pub created_before: Option<Timestamp>,
    /// Maximum number of results to return.
    pub limit: Option<u32>,
    /// Number of results to skip (for pagination).
    pub offset: Option<u32>,
    /// Filter by owner.
    pub owner: Option<PeerId>,
}

impl ManifestFilter {
    /// Create a new empty filter (matches all manifests).
    pub fn new() -> Self {
        Self::default()
    }

    /// Filter by visibility.
    pub fn with_visibility(mut self, visibility: Visibility) -> Self {
        self.visibility = Some(visibility);
        self
    }

    /// Filter by content type.
    pub fn with_content_type(mut self, content_type: ContentType) -> Self {
        self.content_type = Some(content_type);
        self
    }

    /// Filter by minimum creation time.
    pub fn created_after(mut self, timestamp: Timestamp) -> Self {
        self.created_after = Some(timestamp);
        self
    }

    /// Filter by maximum creation time.
    pub fn created_before(mut self, timestamp: Timestamp) -> Self {
        self.created_before = Some(timestamp);
        self
    }

    /// Limit the number of results.
    pub fn limit(mut self, limit: u32) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Skip a number of results (for pagination).
    pub fn offset(mut self, offset: u32) -> Self {
        self.offset = Some(offset);
        self
    }

    /// Filter by owner.
    pub fn with_owner(mut self, owner: PeerId) -> Self {
        self.owner = Some(owner);
        self
    }
}

/// Cached content entry.
///
/// Represents content that was retrieved via a query and cached locally.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CachedContent {
    /// Content hash.
    pub hash: Hash,
    /// The actual content bytes.
    pub content: Vec<u8>,
    /// Peer from which content was retrieved.
    pub source_peer: PeerId,
    /// Timestamp when the content was queried.
    pub queried_at: Timestamp,
    /// Payment receipt proving the query was paid for.
    pub payment_proof: PaymentReceipt,
}

impl CachedContent {
    /// Create a new cached content entry.
    pub fn new(
        hash: Hash,
        content: Vec<u8>,
        source_peer: PeerId,
        queried_at: Timestamp,
        payment_proof: PaymentReceipt,
    ) -> Self {
        Self {
            hash,
            content,
            source_peer,
            queried_at,
            payment_proof,
        }
    }

    /// Get the size of the cached content in bytes.
    pub fn size(&self) -> u64 {
        self.content.len() as u64
    }
}

/// A distribution waiting to be settled on-chain.
///
/// These are created when queries are processed and payments need
/// to be distributed to content contributors.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct QueuedDistribution {
    /// Original payment ID this distribution came from.
    pub payment_id: Hash,
    /// Recipient of this distribution.
    pub recipient: PeerId,
    /// Amount owed to the recipient.
    pub amount: Amount,
    /// Source content hash (for audit trail).
    pub source_hash: Hash,
    /// When the original query happened.
    pub queued_at: Timestamp,
}

impl QueuedDistribution {
    /// Create a new queued distribution.
    pub fn new(
        payment_id: Hash,
        recipient: PeerId,
        amount: Amount,
        source_hash: Hash,
        queued_at: Timestamp,
    ) -> Self {
        Self {
            payment_id,
            recipient,
            amount,
            source_hash,
            queued_at,
        }
    }
}

/// Information about a known peer.
///
/// Spec §5.1: Stores peer metadata including network addresses,
/// last seen time, and reputation score.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PeerInfo {
    /// Unique peer identifier.
    pub peer_id: PeerId,
    /// Peer's public key (for signature verification).
    pub public_key: PublicKey,
    /// Network addresses (multiaddr format strings).
    pub addresses: Vec<String>,
    /// Last time this peer was seen active.
    pub last_seen: Timestamp,
    /// Reputation score (can be negative).
    pub reputation: i64,
}

impl PeerInfo {
    /// Create a new peer info entry.
    pub fn new(
        peer_id: PeerId,
        public_key: PublicKey,
        addresses: Vec<String>,
        last_seen: Timestamp,
    ) -> Self {
        Self {
            peer_id,
            public_key,
            addresses,
            last_seen,
            reputation: 0,
        }
    }

    /// Create peer info with initial reputation.
    pub fn with_reputation(mut self, reputation: i64) -> Self {
        self.reputation = reputation;
        self
    }

    /// Add an address to the peer.
    pub fn add_address(&mut self, address: String) {
        if !self.addresses.contains(&address) {
            self.addresses.push(address);
        }
    }

    /// Update the last seen timestamp.
    pub fn touch(&mut self, timestamp: Timestamp) {
        self.last_seen = timestamp;
    }

    /// Adjust reputation by delta.
    pub fn adjust_reputation(&mut self, delta: i64) {
        self.reputation = self.reputation.saturating_add(delta);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nodalync_crypto::{content_hash, generate_identity, peer_id_from_public_key, Signature};

    fn test_peer_id() -> PeerId {
        let (_, public_key) = generate_identity();
        peer_id_from_public_key(&public_key)
    }

    fn test_public_key() -> PublicKey {
        let (_, public_key) = generate_identity();
        public_key
    }

    fn test_hash() -> Hash {
        content_hash(b"test")
    }

    fn test_payment_receipt() -> PaymentReceipt {
        PaymentReceipt {
            payment_id: test_hash(),
            amount: 100,
            timestamp: 1234567890,
            channel_nonce: 1,
            distributor_signature: Signature::from_bytes([0u8; 64]),
        }
    }

    #[test]
    fn test_manifest_filter_builder() {
        let filter = ManifestFilter::new()
            .with_visibility(Visibility::Shared)
            .with_content_type(ContentType::L0)
            .created_after(1000)
            .created_before(2000)
            .limit(10)
            .offset(5);

        assert_eq!(filter.visibility, Some(Visibility::Shared));
        assert_eq!(filter.content_type, Some(ContentType::L0));
        assert_eq!(filter.created_after, Some(1000));
        assert_eq!(filter.created_before, Some(2000));
        assert_eq!(filter.limit, Some(10));
        assert_eq!(filter.offset, Some(5));
    }

    #[test]
    fn test_cached_content() {
        let hash = test_hash();
        let content = b"cached data".to_vec();
        let source_peer = test_peer_id();
        let receipt = test_payment_receipt();

        let cached = CachedContent::new(hash, content.clone(), source_peer, 1234567890, receipt);

        assert_eq!(cached.hash, hash);
        assert_eq!(cached.content, content);
        assert_eq!(cached.size(), content.len() as u64);
    }

    #[test]
    fn test_queued_distribution() {
        let payment_id = test_hash();
        let recipient = test_peer_id();
        let source_hash = content_hash(b"source");

        let dist = QueuedDistribution::new(payment_id, recipient, 1000, source_hash, 1234567890);

        assert_eq!(dist.payment_id, payment_id);
        assert_eq!(dist.recipient, recipient);
        assert_eq!(dist.amount, 1000);
    }

    #[test]
    fn test_peer_info() {
        let (_, public_key) = generate_identity();
        let peer_id = peer_id_from_public_key(&public_key);
        let addresses = vec!["/ip4/127.0.0.1/tcp/9000".to_string()];

        let mut info = PeerInfo::new(peer_id, public_key, addresses, 1000);

        assert_eq!(info.reputation, 0);

        info.adjust_reputation(10);
        assert_eq!(info.reputation, 10);

        info.adjust_reputation(-5);
        assert_eq!(info.reputation, 5);

        info.touch(2000);
        assert_eq!(info.last_seen, 2000);

        info.add_address("/ip4/192.168.1.1/tcp/9000".to_string());
        assert_eq!(info.addresses.len(), 2);

        // Adding duplicate address should not increase count
        info.add_address("/ip4/127.0.0.1/tcp/9000".to_string());
        assert_eq!(info.addresses.len(), 2);
    }

    #[test]
    fn test_peer_info_reputation_saturation() {
        let (_, public_key) = generate_identity();
        let peer_id = peer_id_from_public_key(&public_key);

        let mut info = PeerInfo::new(peer_id, public_key, vec![], 1000);
        info.reputation = i64::MAX;
        info.adjust_reputation(1);
        assert_eq!(info.reputation, i64::MAX);

        info.reputation = i64::MIN;
        info.adjust_reputation(-1);
        assert_eq!(info.reputation, i64::MIN);
    }
}
