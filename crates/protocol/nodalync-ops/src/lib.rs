//! Protocol operations for the Nodalync protocol.
//!
//! This crate provides the orchestration layer that combines all foundation modules
//! (store, valid, econ, wire, types, crypto) to implement protocol business logic
//! as specified in Protocol Specification §7.
//!
//! # Module Organization
//!
//! - [`error`] - Operation error types
//! - [`config`] - Configuration for channels and operations
//! - [`extraction`] - L1 mention extraction
//! - [`ops`] - Main Operations trait definition
//! - [`node_ops`] - NodeOperations implementation
//! - [`content`] - Content operations (create, update, derive, reference)
//! - [`query`] - Query operations (preview, query, get_versions)
//! - [`publish`] - Publish operations (publish, unpublish, visibility, access)
//! - [`channel`] - Channel operations (open, accept, close, dispute)
//! - [`settlement`] - Settlement operations (trigger_settlement)
//! - [`handlers`] - Incoming message handlers
//! - [`helpers`] - Utility functions
//!
//! # Example
//!
//! ```ignore
//! use nodalync_ops::{DefaultNodeOperations, OpsConfig};
//! use nodalync_store::{NodeState, NodeStateConfig};
//! use nodalync_crypto::{generate_identity, peer_id_from_public_key};
//! use nodalync_types::{Metadata, Visibility};
//! use std::path::PathBuf;
//!
//! // Initialize node state
//! let config = NodeStateConfig::new(PathBuf::from("~/.nodalync"));
//! let state = NodeState::open(config).expect("Failed to open node state");
//!
//! // Generate identity
//! let (_, public_key) = generate_identity();
//! let peer_id = peer_id_from_public_key(&public_key);
//!
//! // Create operations instance
//! let mut ops = DefaultNodeOperations::with_defaults(state, peer_id);
//!
//! // Create content
//! let content = b"Hello, Nodalync!";
//! let metadata = Metadata::new("My Document", content.len() as u64);
//! let hash = ops.create_content(content, metadata).expect("Failed to create");
//!
//! // Publish content (async)
//! ops.publish_content(&hash, Visibility::Shared, 100).await.expect("Failed to publish");
//! ```
//!
//! # Operations Overview
//!
//! ## Content Operations (§7.1)
//!
//! - **create**: Create new L0 content with metadata
//! - **extract_l1**: Extract L1 mentions from L0 content
//! - **update**: Create new version of existing content
//! - **derive**: Create L3 content from multiple sources
//! - **reference_l3_as_l0**: Promote L3 synthesis to primary source
//!
//! ## Query Operations (§7.2)
//!
//! - **preview**: Get content metadata and L1 summary
//! - **query**: Retrieve full content with payment
//! - **get_versions**: List all versions of content
//!
//! ## Visibility Operations (§7.1.3)
//!
//! - **publish**: Make content discoverable with price
//! - **unpublish**: Make content private
//! - **set_visibility**: Change visibility level
//! - **set_access**: Configure access control
//!
//! ## Channel Operations (§7.3)
//!
//! - **open_channel**: Open payment channel with peer
//! - **accept_channel**: Accept incoming channel request
//! - **update_channel**: Record payment in channel
//! - **close_channel**: Close channel and settle
//! - **dispute_channel**: Submit dispute for channel
//!
//! ## Settlement Operations (§7.5)
//!
//! - **trigger_settlement**: Create and submit settlement batch
//!
//! # Design Notes
//!
//! ## Validator/Extractor Generics
//!
//! NodeOperations is generic over `Validator` and `L1Extractor` traits:
//!
//! ```ignore
//! pub struct NodeOperations<V, E>
//! where
//!     V: Validator,
//!     E: L1Extractor,
//! ```
//!
//! This allows for:
//! - Custom validation rules
//! - Different extraction implementations (rule-based, AI-powered, etc.)
//! - Easy testing with mock implementations
//!
//! ## Settlement Queue Integration
//!
//! When handling queries, ALL distributions (owner + all root contributors)
//! are enqueued to the settlement queue. The settlement contract then pays
//! everyone directly, ensuring trustless distribution.
//!
//! ## Channel Auto-Open
//!
//! The query operation checks for an existing channel and auto-opens one
//! if sufficient balance is available. This simplifies the query flow.
//!
//! ## Network Integration
//!
//! Network operations (DHT announce, queries, channel messages) are now integrated
//! via the optional `network` field in `NodeOperations`. When a network is provided,
//! operations will use P2P networking; otherwise they fall back to local-only mode.

// Module declarations
pub mod channel;
pub mod config;
pub mod content;
pub mod error;
pub mod extraction;
pub mod handlers;
pub mod helpers;
pub mod l2;
pub mod node_ops;
pub mod ops;
pub mod publish;
pub mod query;
pub mod settlement;

// Re-export main types at crate root

// Error types
pub use error::{CloseResult, OpsError, OpsResult};

// Network trait (re-exported from nodalync-net)
pub use nodalync_net::{Network, NetworkError, NetworkEvent};

// Configuration
pub use config::{ChannelConfig, OpsConfig};

// Extraction
pub use extraction::{L1Extractor, RuleBasedExtractor};

// Operations trait and implementation
pub use node_ops::{current_timestamp, DefaultNodeOperations, NodeOperations};
pub use ops::{Operations, PreviewResponse, QueryResponse};

// Query types
pub use query::{NetworkSearchResult, SearchSource};

// Helper functions
pub use helpers::{
    generate_channel_id, generate_payment_id, is_queryable_by, merge_provenance_entries,
    total_provenance_weight, truncate_string, unique_owners, verify_content_hash,
};

// Channel payment helpers
pub use channel::{create_signed_payment, create_signed_payment_for_manifest, sign_payment};

#[cfg(test)]
mod tests {
    use super::*;
    use nodalync_crypto::{content_hash, generate_identity, peer_id_from_public_key};
    use nodalync_store::NodeStateConfig;
    use nodalync_types::{Metadata, Visibility};
    use tempfile::TempDir;

    fn create_test_ops() -> (DefaultNodeOperations, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let config = NodeStateConfig::new(temp_dir.path());
        let state = nodalync_store::NodeState::open(config).unwrap();

        let (_, public_key) = generate_identity();
        let peer_id = peer_id_from_public_key(&public_key);

        let ops = DefaultNodeOperations::with_defaults(state, peer_id);
        (ops, temp_dir)
    }

    /// Integration test: Full content lifecycle
    #[tokio::test]
    async fn test_content_lifecycle() {
        let (mut ops, _temp) = create_test_ops();

        // Create content
        let content = b"Original content";
        let metadata = Metadata::new("Test Doc", content.len() as u64);
        let hash1 = ops.create_content(content, metadata).unwrap();

        // Preview (before publish - should work for owner)
        let preview = ops.preview_content(&hash1).await.unwrap();
        assert_eq!(preview.manifest.hash, hash1);

        // Publish
        ops.publish_content(&hash1, Visibility::Shared, 100)
            .await
            .unwrap();

        // Query
        let response = ops.query_content(&hash1, 100, None).await.unwrap();
        assert_eq!(response.content, content.to_vec());

        // Update
        let new_content = b"Updated content";
        let new_metadata = Metadata::new("Test Doc v2", new_content.len() as u64);
        let _hash2 = ops
            .update_content(&hash1, new_content, new_metadata)
            .unwrap();

        // Verify versions
        let versions = ops.get_content_versions(&hash1).unwrap();
        assert!(!versions.is_empty());
    }

    /// Integration test: Derive content from sources
    #[test]
    fn test_derive_content() {
        let (mut ops, _temp) = create_test_ops();

        // Create sources
        let source1 = b"First source document";
        let meta1 = Metadata::new("Source 1", source1.len() as u64);
        let hash1 = ops.create_content(source1, meta1).unwrap();

        let source2 = b"Second source document";
        let meta2 = Metadata::new("Source 2", source2.len() as u64);
        let hash2 = ops.create_content(source2, meta2).unwrap();

        // Derive new content
        let insight = b"Synthesis combining insights from both sources";
        let meta3 = Metadata::new("Synthesis", insight.len() as u64);
        let hash3 = ops.derive_content(&[hash1, hash2], insight, meta3).unwrap();

        // Verify L3 properties
        let manifest = ops.get_content_manifest(&hash3).unwrap().unwrap();
        assert_eq!(manifest.content_type, nodalync_types::ContentType::L3);
        assert!(manifest.provenance.is_derived());
        assert_eq!(manifest.provenance.depth, 1);
    }

    /// Integration test: Channel operations
    #[tokio::test]
    async fn test_channel_operations() {
        let (mut ops, _temp) = create_test_ops();

        let (_, pk) = generate_identity();
        let peer = peer_id_from_public_key(&pk);

        // Open channel
        let channel = ops
            .open_payment_channel(&peer, 100_0000_0000)
            .await
            .unwrap();
        assert!(!channel.is_open()); // Opening state

        // Accept another channel
        let (_, pk2) = generate_identity();
        let peer2 = peer_id_from_public_key(&pk2);
        let channel_id = content_hash(b"channel2");

        let channel2 = ops
            .accept_payment_channel(&channel_id, &peer2, 500, 500)
            .unwrap();
        assert!(channel2.is_open());

        // Close channel (use simple version for tests without private key)
        ops.close_payment_channel_simple(&peer2).await.unwrap();
        let closed = ops.get_payment_channel(&peer2).unwrap().unwrap();
        assert!(closed.is_closed());
    }

    /// Integration test: Settlement queue
    /// Integration test: Paid content requires on-chain settlement
    ///
    /// This test validates the CRITICAL security property that paid content
    /// is NEVER delivered without on-chain settlement confirmation.
    #[tokio::test]
    async fn test_paid_content_requires_settlement() {
        let (mut ops, _temp) = create_test_ops();

        // Create and publish content
        let content = b"Paid content";
        let meta = Metadata::new("Paid", content.len() as u64);
        let hash = ops.create_content(content, meta).unwrap();
        ops.publish_content(&hash, Visibility::Shared, 100)
            .await
            .unwrap();

        // Handle query
        let (_, pk) = generate_identity();
        let requester = peer_id_from_public_key(&pk);
        let manifest = ops.get_content_manifest(&hash).unwrap().unwrap();

        // Open a channel with the requester (required for paid content)
        let channel_id = content_hash(b"test-settlement-queue-channel");
        ops.accept_payment_channel(&channel_id, &requester, 500, 1000)
            .unwrap();

        let payment = nodalync_types::Payment::new(
            content_hash(b"payment"),
            channel_id,
            100,
            manifest.owner,
            hash,
            manifest.provenance.root_l0l1.clone(),
            current_timestamp(),
            nodalync_crypto::Signature::from_bytes([0u8; 64]),
        );
        let request = nodalync_wire::QueryRequestPayload {
            hash,
            query: None,
            payment,
            version_spec: None,
            payment_nonce: 1,
        };

        // Without settlement configured, paid queries MUST be rejected
        // This ensures trustless operation: no content without confirmed payment
        let result = ops.handle_query_request(&requester, &request).await;
        assert!(
            matches!(result, Err(crate::OpsError::SettlementRequired)),
            "Paid queries without settlement MUST fail for trustless operation: {:?}",
            result
        );
    }

    /// Integration test: L1 extraction
    #[test]
    fn test_l1_extraction() {
        let (mut ops, _temp) = create_test_ops();

        let content = b"Apple announced new products. We found significant improvements in battery life. According to researchers, the data shows a 50% increase.";
        let meta = Metadata::new("Tech News", content.len() as u64);
        let hash = ops.create_content(content, meta).unwrap();

        let summary = ops.extract_l1_summary(&hash).unwrap();

        assert_eq!(summary.l0_hash, hash);
        assert!(summary.mention_count > 0);
        assert!(!summary.preview_mentions.is_empty());
    }
}
