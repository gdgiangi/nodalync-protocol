//! Manifest and related types for content metadata.
//!
//! This module defines the `Manifest` struct and its component types
//! as specified in Protocol Specification §4.3, §4.6, §4.7, §4.8.

use nodalync_crypto::{Hash, PeerId, Timestamp};
use serde::{Deserialize, Serialize};

use crate::enums::{ContentType, Currency, Visibility};
use crate::provenance::Provenance;
use crate::Amount;

/// Version information for content.
///
/// Spec §4.3: Tracks the version history of content items.
///
/// # Constraints
/// - If `number == 1`: `previous` MUST be `None`, `root` MUST equal content hash
/// - If `number > 1`: `previous` MUST be `Some`, `root` MUST equal `previous.root`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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

impl Version {
    /// Create a new first version (v1).
    ///
    /// For v1, previous is None and root equals the content hash.
    pub fn new_v1(content_hash: Hash, timestamp: Timestamp) -> Self {
        Self {
            number: 1,
            previous: None,
            root: content_hash,
            timestamp,
        }
    }

    /// Create a new version from a previous version.
    ///
    /// Inherits the root from the previous version.
    pub fn new_from_previous(
        previous_version: &Version,
        previous_hash: Hash,
        timestamp: Timestamp,
    ) -> Self {
        Self {
            number: previous_version.number + 1,
            previous: Some(previous_hash),
            root: previous_version.root,
            timestamp,
        }
    }

    /// Check if this is the first version.
    pub fn is_first_version(&self) -> bool {
        self.number == 1
    }

    /// Validate version constraints.
    ///
    /// Returns true if the version satisfies all spec constraints.
    pub fn is_valid(&self, content_hash: &Hash) -> bool {
        if self.number == 1 {
            // v1: previous must be None, root must equal content hash
            self.previous.is_none() && self.root == *content_hash
        } else {
            // v2+: previous must be Some
            self.previous.is_some()
        }
    }
}

/// Content metadata.
///
/// Spec §4.8: Descriptive information about content.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Metadata {
    /// Content title (max 200 chars)
    pub title: String,
    /// Content description (max 2000 chars)
    pub description: Option<String>,
    /// Content tags (max 20 tags, each max 50 chars)
    pub tags: Vec<String>,
    /// Content size in bytes
    pub content_size: u64,
    /// MIME type if applicable
    pub mime_type: Option<String>,
}

impl Metadata {
    /// Create new metadata with required fields.
    pub fn new(title: impl Into<String>, content_size: u64) -> Self {
        Self {
            title: title.into(),
            description: None,
            tags: Vec::new(),
            content_size,
            mime_type: None,
        }
    }

    /// Set the description.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Set the tags.
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    /// Set the MIME type.
    pub fn with_mime_type(mut self, mime_type: impl Into<String>) -> Self {
        self.mime_type = Some(mime_type.into());
        self
    }
}

/// Access control settings for content.
///
/// Spec §4.6: Controls who can access content and under what conditions.
///
/// # Access Logic
/// Access granted if:
/// - (allowlist is None OR peer in allowlist) AND
/// - (denylist is None OR peer NOT in denylist) AND
/// - (require_bond is false OR peer has posted bond)
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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

impl AccessControl {
    /// Create open access control (no restrictions).
    pub fn open() -> Self {
        Self::default()
    }

    /// Create access control with an allowlist.
    pub fn with_allowlist(peers: Vec<PeerId>) -> Self {
        Self {
            allowlist: Some(peers),
            ..Self::default()
        }
    }

    /// Create access control with a denylist.
    pub fn with_denylist(peers: Vec<PeerId>) -> Self {
        Self {
            denylist: Some(peers),
            ..Self::default()
        }
    }

    /// Check if a peer is allowed access based on these rules.
    ///
    /// Note: This does not check bond requirements, only list membership.
    pub fn is_peer_allowed(&self, peer: &PeerId) -> bool {
        // Check allowlist
        if let Some(ref allowlist) = self.allowlist {
            if !allowlist.contains(peer) {
                return false;
            }
        }

        // Check denylist
        if let Some(ref denylist) = self.denylist {
            if denylist.contains(peer) {
                return false;
            }
        }

        true
    }
}

/// Economic parameters for content.
///
/// Spec §4.7: Pricing and revenue tracking.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Economics {
    /// Price per query (in tinybars, 10^-8 HBAR)
    pub price: Amount,
    /// Currency identifier
    pub currency: Currency,
    /// Total queries served
    pub total_queries: u64,
    /// Total revenue generated
    pub total_revenue: Amount,
}

impl Default for Economics {
    fn default() -> Self {
        Self {
            price: 0,
            currency: Currency::HBAR,
            total_queries: 0,
            total_revenue: 0,
        }
    }
}

impl Economics {
    /// Create economics with a specific price.
    pub fn with_price(price: Amount) -> Self {
        Self {
            price,
            currency: Currency::HBAR,
            total_queries: 0,
            total_revenue: 0,
        }
    }

    /// Record a query and update statistics.
    pub fn record_query(&mut self, payment: Amount) {
        self.total_queries += 1;
        self.total_revenue += payment;
    }
}

/// Complete manifest for a content item.
///
/// Spec §4.8: Contains all metadata for a content item including
/// identity, versioning, visibility, access control, economics,
/// and provenance information.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Manifest {
    // === Identity ===
    /// Content hash (unique identifier)
    pub hash: Hash,
    /// Type of content
    pub content_type: ContentType,
    /// Owner's peer ID (receives synthesis fee, serves content)
    pub owner: PeerId,

    // === Versioning ===
    /// Version information
    pub version: Version,

    // === Visibility & Access ===
    /// Visibility level
    pub visibility: Visibility,
    /// Access control rules
    pub access: AccessControl,

    // === Metadata ===
    /// Content metadata
    pub metadata: Metadata,

    // === Economics ===
    /// Economic parameters
    pub economics: Economics,

    // === Provenance ===
    /// Provenance chain
    pub provenance: Provenance,

    // === Timestamps ===
    /// Creation timestamp
    pub created_at: Timestamp,
    /// Last update timestamp
    pub updated_at: Timestamp,
}

impl Manifest {
    /// Create a new manifest for L0 content.
    ///
    /// This is the most common way to create a manifest for newly added content.
    pub fn new_l0(hash: Hash, owner: PeerId, metadata: Metadata, timestamp: Timestamp) -> Self {
        Self {
            hash,
            content_type: ContentType::L0,
            owner,
            version: Version::new_v1(hash, timestamp),
            visibility: Visibility::Private,
            access: AccessControl::default(),
            metadata,
            economics: Economics::default(),
            provenance: Provenance::new_l0(hash, owner),
            created_at: timestamp,
            updated_at: timestamp,
        }
    }

    /// Check if this content is queryable by a peer.
    ///
    /// Checks visibility and access control rules.
    pub fn is_queryable_by(&self, peer: &PeerId) -> bool {
        match self.visibility {
            Visibility::Private => false,
            Visibility::Unlisted | Visibility::Shared => self.access.is_peer_allowed(peer),
        }
    }

    /// Get the root hash (stable identifier across versions).
    pub fn root_hash(&self) -> Hash {
        self.version.root
    }

    /// Check if this is the first version.
    pub fn is_first_version(&self) -> bool {
        self.version.is_first_version()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nodalync_crypto::{content_hash, generate_identity, peer_id_from_public_key};

    fn test_hash() -> Hash {
        content_hash(b"test content")
    }

    fn test_peer_id() -> PeerId {
        let (_, public_key) = generate_identity();
        peer_id_from_public_key(&public_key)
    }

    #[test]
    fn test_version_v1() {
        let hash = test_hash();
        let timestamp = 1234567890u64;
        let version = Version::new_v1(hash, timestamp);

        assert_eq!(version.number, 1);
        assert!(version.previous.is_none());
        assert_eq!(version.root, hash);
        assert_eq!(version.timestamp, timestamp);
        assert!(version.is_first_version());
        assert!(version.is_valid(&hash));
    }

    #[test]
    fn test_version_v2() {
        let hash1 = test_hash();
        let hash2 = content_hash(b"updated content");
        let v1 = Version::new_v1(hash1, 1000);
        let v2 = Version::new_from_previous(&v1, hash1, 2000);

        assert_eq!(v2.number, 2);
        assert_eq!(v2.previous, Some(hash1));
        assert_eq!(v2.root, hash1); // Root stays the same
        assert_eq!(v2.timestamp, 2000);
        assert!(!v2.is_first_version());
        assert!(v2.is_valid(&hash2)); // v2+ just needs previous to be Some
    }

    #[test]
    fn test_version_invalid_v1() {
        let hash = test_hash();
        let different_hash = content_hash(b"different");
        let mut version = Version::new_v1(hash, 1000);

        // v1 with different root is invalid
        version.root = different_hash;
        assert!(!version.is_valid(&hash));
    }

    #[test]
    fn test_metadata_builder() {
        let metadata = Metadata::new("Test Title", 1024)
            .with_description("A test description")
            .with_tags(vec!["tag1".to_string(), "tag2".to_string()])
            .with_mime_type("text/plain");

        assert_eq!(metadata.title, "Test Title");
        assert_eq!(metadata.description, Some("A test description".to_string()));
        assert_eq!(metadata.tags.len(), 2);
        assert_eq!(metadata.content_size, 1024);
        assert_eq!(metadata.mime_type, Some("text/plain".to_string()));
    }

    #[test]
    fn test_access_control_open() {
        let access = AccessControl::open();
        let peer = test_peer_id();

        assert!(access.is_peer_allowed(&peer));
        assert!(access.allowlist.is_none());
        assert!(access.denylist.is_none());
    }

    #[test]
    fn test_access_control_allowlist() {
        let allowed_peer = test_peer_id();
        let other_peer = test_peer_id();
        let access = AccessControl::with_allowlist(vec![allowed_peer]);

        assert!(access.is_peer_allowed(&allowed_peer));
        assert!(!access.is_peer_allowed(&other_peer));
    }

    #[test]
    fn test_access_control_denylist() {
        let blocked_peer = test_peer_id();
        let other_peer = test_peer_id();
        let access = AccessControl::with_denylist(vec![blocked_peer]);

        assert!(!access.is_peer_allowed(&blocked_peer));
        assert!(access.is_peer_allowed(&other_peer));
    }

    #[test]
    fn test_economics() {
        let mut economics = Economics::with_price(100);

        assert_eq!(economics.price, 100);
        assert_eq!(economics.total_queries, 0);
        assert_eq!(economics.total_revenue, 0);

        economics.record_query(100);
        assert_eq!(economics.total_queries, 1);
        assert_eq!(economics.total_revenue, 100);

        economics.record_query(100);
        assert_eq!(economics.total_queries, 2);
        assert_eq!(economics.total_revenue, 200);
    }

    #[test]
    fn test_manifest_new_l0() {
        let hash = test_hash();
        let owner = test_peer_id();
        let metadata = Metadata::new("Test Content", 1024);
        let timestamp = 1234567890u64;

        let manifest = Manifest::new_l0(hash, owner, metadata, timestamp);

        assert_eq!(manifest.hash, hash);
        assert_eq!(manifest.content_type, ContentType::L0);
        assert_eq!(manifest.owner, owner);
        assert!(manifest.is_first_version());
        assert_eq!(manifest.visibility, Visibility::Private);
        assert_eq!(manifest.created_at, timestamp);
        assert_eq!(manifest.updated_at, timestamp);
    }

    #[test]
    fn test_manifest_queryable() {
        let hash = test_hash();
        let owner = test_peer_id();
        let other = test_peer_id();
        let metadata = Metadata::new("Test", 100);

        let mut manifest = Manifest::new_l0(hash, owner, metadata, 1000);

        // Private is not queryable
        assert!(!manifest.is_queryable_by(&other));

        // Shared is queryable
        manifest.visibility = Visibility::Shared;
        assert!(manifest.is_queryable_by(&other));

        // Unlisted is queryable
        manifest.visibility = Visibility::Unlisted;
        assert!(manifest.is_queryable_by(&other));

        // Denylist blocks
        manifest.access = AccessControl::with_denylist(vec![other]);
        assert!(!manifest.is_queryable_by(&other));
    }

    #[test]
    fn test_manifest_serialization() {
        let hash = test_hash();
        let owner = test_peer_id();
        let metadata = Metadata::new("Test", 100);
        let manifest = Manifest::new_l0(hash, owner, metadata, 1000);

        let json = serde_json::to_string(&manifest).unwrap();
        let deserialized: Manifest = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.hash, manifest.hash);
        assert_eq!(deserialized.owner, manifest.owner);
        assert_eq!(deserialized.content_type, manifest.content_type);
    }
}
