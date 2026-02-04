//! End-to-end integration tests using mock network and settlement.
//!
//! These tests exercise complete protocol flows using `MockNetwork`
//! and `MockSettlement` from `nodalync_test_utils`.

use nodalync_crypto::{content_hash, generate_identity, peer_id_from_public_key};
use nodalync_ops::DefaultNodeOperations;
use nodalync_store::{NodeStateConfig, QueuedDistribution, SettlementQueueStore};
use nodalync_test_utils::*;
use nodalync_types::{Metadata, Visibility};
use std::sync::Arc;

/// Helper to get the current timestamp.
fn now() -> u64 {
    nodalync_ops::current_timestamp()
}

// =========================================================================
// Publish and Query
// =========================================================================

#[tokio::test]
async fn test_publish_and_query_local() {
    let (mut ops, _mock_net, _mock_settle, _temp) = create_test_ops_with_mocks();

    // Create and publish free content
    let content = b"Knowledge about distributed systems and consensus algorithms";
    let meta = Metadata::new("Distributed Systems", content.len() as u64);
    let hash = ops.create_content(content, meta).unwrap();
    ops.publish_content(&hash, Visibility::Shared, 0)
        .await
        .unwrap();

    // Query it locally (free content, no settlement needed)
    let response = ops.query_content(&hash, 0, None).await.unwrap();
    assert_eq!(response.content, content.to_vec());
    assert_eq!(response.manifest.hash, hash);
    assert_eq!(response.manifest.economics.price, 0);
}

#[tokio::test]
async fn test_publish_and_preview_local() {
    let (mut ops, _mock_net, _mock_settle, _temp) = create_test_ops_with_mocks();

    let content = b"Content about cryptographic hash functions and their applications";
    let meta = Metadata::new("Crypto Hashes", content.len() as u64);
    let hash = ops.create_content(content, meta).unwrap();
    ops.publish_content(&hash, Visibility::Shared, 50)
        .await
        .unwrap();

    // Preview should work without payment
    let preview = ops.preview_content(&hash).await.unwrap();
    assert_eq!(preview.manifest.hash, hash);
    assert_eq!(preview.manifest.economics.price, 50);
}

#[tokio::test]
async fn test_publish_multiple_and_search() {
    let (mut ops, _mock_net, _mock_settle, _temp) = create_test_ops_with_mocks();

    // Create and publish multiple pieces of content
    let content1 = b"Machine learning algorithms for natural language processing";
    let meta1 = Metadata::new("ML and NLP", content1.len() as u64);
    let hash1 = ops.create_content(content1, meta1).unwrap();
    ops.publish_content(&hash1, Visibility::Shared, 100)
        .await
        .unwrap();

    let content2 = b"Deep learning neural networks for computer vision tasks";
    let meta2 = Metadata::new("Deep Learning Vision", content2.len() as u64);
    let hash2 = ops.create_content(content2, meta2).unwrap();
    ops.publish_content(&hash2, Visibility::Shared, 200)
        .await
        .unwrap();

    let content3 = b"Quantum computing and its impact on cryptography";
    let meta3 = Metadata::new("Quantum Crypto", content3.len() as u64);
    let hash3 = ops.create_content(content3, meta3).unwrap();
    ops.publish_content(&hash3, Visibility::Shared, 50)
        .await
        .unwrap();

    // Search for "learning" should find the ML-related content
    let results = ops.search_network("learning", None, 10).await.unwrap();
    assert!(
        !results.is_empty(),
        "Search should find content matching 'learning'"
    );
}

// =========================================================================
// Settlement Loop
// =========================================================================

#[tokio::test]
async fn test_full_settlement_loop_with_mocks() {
    let (mut ops, _mock_net, mock_settle, _temp) = create_test_ops_with_mocks();

    let (_, _, peer1) = test_keypair();
    let (_, _, peer2) = test_keypair();

    // Enqueue multiple distributions
    let dist1 = QueuedDistribution::new(
        content_hash(b"e2e-payment1"),
        peer1,
        1000,
        content_hash(b"e2e-source1"),
        now(),
    );
    ops.state.settlement.enqueue(dist1).unwrap();

    let dist2 = QueuedDistribution::new(
        content_hash(b"e2e-payment2"),
        peer2,
        2000,
        content_hash(b"e2e-source2"),
        now(),
    );
    ops.state.settlement.enqueue(dist2).unwrap();

    // Verify pending total
    assert_eq!(ops.get_pending_settlement_total().unwrap(), 3000);

    // Force settlement
    let batch_id = ops.force_settlement().await.unwrap();
    assert!(batch_id.is_some(), "Should have created a batch");

    // Verify MockSettlement received exactly one batch
    let batches = mock_settle.settled_batches();
    assert_eq!(batches.len(), 1);

    // Verify queue is now empty
    let pending_total = ops.get_pending_settlement_total().unwrap();
    assert_eq!(pending_total, 0);
}

#[tokio::test]
async fn test_settlement_failure_does_not_clear_queue() {
    let mock_settle = MockSettlement::new().with_failure();
    let (mut ops, _temp) = create_test_ops_with_settlement(Arc::new(mock_settle.clone()));

    let (_, _, peer) = test_keypair();
    let dist = QueuedDistribution::new(
        content_hash(b"fail-payment"),
        peer,
        500,
        content_hash(b"fail-source"),
        now(),
    );
    ops.state.settlement.enqueue(dist).unwrap();

    // force_settlement must return Err when on-chain settlement fails
    let result = ops.force_settlement().await;
    assert!(
        result.is_err(),
        "force_settlement must return Err on on-chain failure"
    );

    // Queue must NOT be cleared — payments preserved for retry
    let pending = ops.state.settlement.get_pending().unwrap();
    assert_eq!(
        pending.len(),
        1,
        "Queue must retain pending items after failed settlement"
    );
    assert_eq!(pending[0].amount, 500);

    // MockSettlement should have been called but returned failure
    let batches = mock_settle.settled_batches();
    assert!(
        batches.is_empty(),
        "Failed settlement should not record batch"
    );
}

#[tokio::test]
async fn test_multiple_settlement_rounds() {
    let (mut ops, _mock_net, mock_settle, _temp) = create_test_ops_with_mocks();

    // Round 1
    let (_, _, peer1) = test_keypair();
    ops.state
        .settlement
        .enqueue(QueuedDistribution::new(
            content_hash(b"round1-pay"),
            peer1,
            100,
            content_hash(b"round1-src"),
            now(),
        ))
        .unwrap();
    ops.force_settlement().await.unwrap();

    // Round 2
    let (_, _, peer2) = test_keypair();
    ops.state
        .settlement
        .enqueue(QueuedDistribution::new(
            content_hash(b"round2-pay"),
            peer2,
            200,
            content_hash(b"round2-src"),
            now(),
        ))
        .unwrap();
    ops.force_settlement().await.unwrap();

    // Verify two batches were settled
    assert_eq!(mock_settle.settled_batches().len(), 2);
}

// =========================================================================
// Channel Operations with Mocks
// =========================================================================

#[tokio::test]
async fn test_channel_open_without_network() {
    let mock_settle = MockSettlement::new().with_balance(100_0000_0000);
    let (mut ops, _temp) = create_test_ops_with_settlement(Arc::new(mock_settle.clone()));

    let (_, _, peer) = test_keypair();

    // Open channel with settlement but no network
    let channel = ops
        .open_payment_channel(&peer, 100_0000_0000)
        .await
        .unwrap();

    // Channel should be in Opening state (no network to send ChannelOpen)
    assert!(!channel.is_open());
    assert_eq!(channel.my_balance, 100_0000_0000);
}

#[tokio::test]
async fn test_channel_lifecycle_with_mocks() {
    let (mut ops, _mock_net, _mock_settle, _temp) = create_test_ops_with_mocks();

    let (_, _, peer) = test_keypair();
    let channel_id = content_hash(b"lifecycle-channel");

    // Accept a channel (simulates receiving ChannelOpen)
    let channel = ops
        .accept_payment_channel(&channel_id, &peer, 500, 1000)
        .unwrap();
    assert!(channel.is_open());

    // Verify channel exists
    assert!(ops.has_open_channel(&peer).unwrap());

    // Verify balance
    let balance = ops.get_channel_balance(&peer).unwrap().unwrap();
    assert_eq!(balance, 1000);

    // Get next nonce
    let nonce = ops.get_next_payment_nonce(&peer).unwrap();
    assert_eq!(nonce, 1);
}

#[tokio::test]
async fn test_channel_accept_duplicate_fails() {
    let (mut ops, _mock_net, _mock_settle, _temp) = create_test_ops_with_mocks();

    let (_, _, peer) = test_keypair();
    let channel_id = content_hash(b"dup-channel");

    // Accept first channel
    ops.accept_payment_channel(&channel_id, &peer, 500, 500)
        .unwrap();

    // Accepting a second channel with the same peer should fail
    let result = ops.accept_payment_channel(&content_hash(b"dup-channel-2"), &peer, 500, 500);
    assert!(
        matches!(result, Err(nodalync_ops::OpsError::ChannelAlreadyExists)),
        "Should fail with ChannelAlreadyExists: {:?}",
        result
    );
}

#[tokio::test]
async fn test_channel_update_receive_payment() {
    let (mut ops, _mock_net, _mock_settle, _temp) = create_test_ops_with_mocks();

    let (_, _, peer) = test_keypair();
    let channel_id = content_hash(b"receive-channel");

    // Accept a channel
    ops.accept_payment_channel(&channel_id, &peer, 500, 1000)
        .unwrap();

    // Receive a payment (peer pays us)
    let payment = nodalync_types::Payment::new(
        content_hash(b"recv-pay"),
        channel_id,
        100,
        ops.peer_id(), // we are the recipient
        content_hash(b"recv-query"),
        vec![],
        now(),
        nodalync_crypto::Signature::from_bytes([0u8; 64]),
    );

    ops.update_payment_channel(&peer, payment).unwrap();

    // Our balance should increase (receiving payment)
    let channel = ops.get_payment_channel(&peer).unwrap().unwrap();
    assert_eq!(channel.my_balance, 1100); // 1000 + 100
}

// =========================================================================
// Content and Version Operations
// =========================================================================

#[tokio::test]
async fn test_create_update_and_versions() {
    let (mut ops, _mock_net, _mock_settle, _temp) = create_test_ops_with_mocks();

    // Create content
    let content1 = b"Version 1 of the document";
    let meta1 = Metadata::new("Doc v1", content1.len() as u64);
    let hash1 = ops.create_content(content1, meta1).unwrap();

    // Update it
    let content2 = b"Version 2 of the document with improvements";
    let meta2 = Metadata::new("Doc v2", content2.len() as u64);
    let hash2 = ops.update_content(&hash1, content2, meta2).unwrap();

    // Verify versions
    let versions = ops.get_content_versions(&hash1).unwrap();
    assert!(!versions.is_empty());

    // Both manifests should exist
    assert!(ops.get_content_manifest(&hash1).unwrap().is_some());
    assert!(ops.get_content_manifest(&hash2).unwrap().is_some());
}

#[tokio::test]
async fn test_derive_content_with_mocks() {
    let (mut ops, _mock_net, _mock_settle, _temp) = create_test_ops_with_mocks();

    // Create two source documents
    let src1 = b"First source about consensus protocols";
    let meta1 = Metadata::new("Source 1", src1.len() as u64);
    let hash1 = ops.create_content(src1, meta1).unwrap();

    let src2 = b"Second source about Byzantine fault tolerance";
    let meta2 = Metadata::new("Source 2", src2.len() as u64);
    let hash2 = ops.create_content(src2, meta2).unwrap();

    // Derive L3 content
    let synthesis = b"Synthesis: BFT consensus protocols combine ideas from both sources";
    let meta3 = Metadata::new("BFT Synthesis", synthesis.len() as u64);
    let hash3 = ops
        .derive_content(&[hash1, hash2], synthesis, meta3)
        .unwrap();

    // Verify it is L3
    let manifest = ops.get_content_manifest(&hash3).unwrap().unwrap();
    assert_eq!(manifest.content_type, nodalync_types::ContentType::L3);
    assert!(manifest.provenance.is_derived());
}

// =========================================================================
// Paid Content with Settlement
// =========================================================================

#[tokio::test]
async fn test_paid_query_with_mock_settlement() {
    let (mut ops, _mock_net, mock_settle, _temp) = create_test_ops_with_mocks();

    // Create and publish paid content
    let content = b"Premium content requiring payment";
    let meta = Metadata::new("Premium Content", content.len() as u64);
    let hash = ops.create_content(content, meta).unwrap();
    ops.publish_content(&hash, Visibility::Shared, 100)
        .await
        .unwrap();

    // Open a channel with a requester
    let (_, _, requester) = test_keypair();
    let channel_id = content_hash(b"paid-query-channel");
    ops.accept_payment_channel(&channel_id, &requester, 500, 1000)
        .unwrap();

    let manifest = ops.get_content_manifest(&hash).unwrap().unwrap();
    let payment = nodalync_types::Payment::new(
        content_hash(b"paid-query-payment"),
        channel_id,
        100,
        manifest.owner,
        hash,
        manifest.provenance.root_l0l1.clone(),
        now(),
        nodalync_crypto::Signature::from_bytes([0u8; 64]),
    );

    let request = nodalync_wire::QueryRequestPayload {
        hash,
        query: None,
        payment,
        version_spec: None,
        payment_nonce: 1,
    };

    // With settlement configured, paid query should succeed
    let result = ops.handle_query_request(&requester, &request).await;
    assert!(
        result.is_ok(),
        "Paid query with settlement should succeed: {:?}",
        result
    );

    // Verify content was delivered
    let response = result.unwrap();
    assert_eq!(response.content, content.to_vec());

    // Verify settlement was called
    let batches = mock_settle.settled_batches();
    assert_eq!(
        batches.len(),
        1,
        "Settlement should have processed one batch"
    );
}

// =========================================================================
// L1 Extraction
// =========================================================================

#[test]
fn test_l1_extraction_with_mocks() {
    let (mut ops, _mock_net, _mock_settle, _temp) = create_test_ops_with_mocks();

    let content = b"Apple and Microsoft announced partnerships. We found significant breakthroughs in quantum computing.";
    let meta = Metadata::new("Tech Announcements", content.len() as u64);
    let hash = ops.create_content(content, meta).unwrap();

    let summary = ops.extract_l1_summary(&hash).unwrap();
    assert_eq!(summary.l0_hash, hash);
    assert!(summary.mention_count > 0);
}

// =========================================================================
// Node Configuration
// =========================================================================

#[test]
fn test_ops_with_defaults_has_no_network_or_settlement() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let config = NodeStateConfig::new(temp_dir.path());
    let state = nodalync_store::NodeState::open(config).unwrap();
    let (_, pk) = generate_identity();
    let peer_id = peer_id_from_public_key(&pk);

    let ops = DefaultNodeOperations::with_defaults(state, peer_id);
    assert!(!ops.has_network());
    assert!(!ops.has_settlement());
    assert!(!ops.has_private_key());
}

#[test]
fn test_ops_with_mocks_has_both() {
    let (ops, _mock_net, _mock_settle, _temp) = create_test_ops_with_mocks();
    assert!(ops.has_network());
    assert!(ops.has_settlement());
}

#[test]
fn test_ops_with_settlement_only() {
    let settle = Arc::new(MockSettlement::new());
    let (ops, _temp) = create_test_ops_with_settlement(settle);
    assert!(!ops.has_network());
    assert!(ops.has_settlement());
}

#[test]
fn test_ops_with_network_only() {
    let net = Arc::new(MockNetwork::new());
    let (ops, _temp) = create_test_ops_with_network(net);
    assert!(ops.has_network());
    assert!(!ops.has_settlement());
}

// =========================================================================
// Additional Edge Cases
// =========================================================================

#[tokio::test]
async fn test_preview_nonexistent_content() {
    let (mut ops, _mock_net, _mock_settle, _temp) = create_test_ops_with_mocks();

    let unknown_hash = content_hash(b"this content does not exist");
    let result = ops.preview_content(&unknown_hash).await;
    assert!(
        matches!(result, Err(nodalync_ops::OpsError::ManifestNotFound(_))),
        "Preview of nonexistent content should return ManifestNotFound: {:?}",
        result
    );
}

#[tokio::test]
async fn test_query_nonexistent_content() {
    let (mut ops, _mock_net, _mock_settle, _temp) = create_test_ops_with_mocks();

    let unknown_hash = content_hash(b"missing content");
    let result = ops.query_content(&unknown_hash, 0, None).await;
    assert!(result.is_err(), "Query of nonexistent content should fail");
}

#[tokio::test]
async fn test_unpublish_makes_content_private() {
    let (mut ops, _mock_net, _mock_settle, _temp) = create_test_ops_with_mocks();

    let content = b"Content that will be unpublished";
    let meta = Metadata::new("Temp Content", content.len() as u64);
    let hash = ops.create_content(content, meta).unwrap();
    ops.publish_content(&hash, Visibility::Shared, 0)
        .await
        .unwrap();

    // Unpublish
    ops.unpublish_content(&hash).await.unwrap();

    // Manifest should now be private
    let manifest = ops.get_content_manifest(&hash).unwrap().unwrap();
    assert_eq!(manifest.visibility, Visibility::Private);
}

#[tokio::test]
async fn test_close_channel_no_channel() {
    let (mut ops, _mock_net, _mock_settle, _temp) = create_test_ops_with_mocks();

    let (private_key, _, peer) = test_keypair();

    // Close a non-existent channel should fail
    let result = ops.close_payment_channel(&peer, &private_key).await;
    assert!(
        matches!(result, Err(nodalync_ops::OpsError::ChannelNotFound)),
        "Closing non-existent channel should fail: {:?}",
        result
    );
}

#[tokio::test]
async fn test_force_settlement_empty_queue() {
    let (mut ops, _mock_net, mock_settle, _temp) = create_test_ops_with_mocks();

    // Force settlement with empty queue
    let result = ops.force_settlement().await.unwrap();
    assert!(result.is_none(), "Empty queue should return None");

    // No batches should have been sent
    assert!(mock_settle.settled_batches().is_empty());
}

#[tokio::test]
async fn test_free_content_query_handler_no_settlement_needed() {
    let (mut ops, _mock_net, mock_settle, _temp) = create_test_ops_with_mocks();

    // Create and publish free content
    let content = b"Free content for everyone";
    let meta = Metadata::new("Free Content", content.len() as u64);
    let hash = ops.create_content(content, meta).unwrap();
    ops.publish_content(&hash, Visibility::Shared, 0)
        .await
        .unwrap();

    // Handle query with zero payment (free content)
    let (_, _, requester) = test_keypair();
    let payment = nodalync_types::Payment::new(
        content_hash(b"free-payment"),
        nodalync_crypto::Hash([0u8; 32]),
        0,
        ops.peer_id(),
        hash,
        vec![],
        now(),
        nodalync_crypto::Signature::from_bytes([0u8; 64]),
    );

    let request = nodalync_wire::QueryRequestPayload {
        hash,
        query: None,
        payment,
        version_spec: None,
        payment_nonce: 0,
    };

    let result = ops.handle_query_request(&requester, &request).await;
    assert!(
        result.is_ok(),
        "Free content should not require settlement: {:?}",
        result
    );

    // Settlement should not have been called for free content
    assert!(mock_settle.settled_batches().is_empty());
}

#[test]
fn test_helper_test_hash_deterministic() {
    let h1 = test_hash("consistent");
    let h2 = test_hash("consistent");
    assert_eq!(h1, h2, "Same label should produce same hash");

    let h3 = test_hash("different");
    assert_ne!(h1, h3, "Different labels should produce different hashes");
}

#[test]
fn test_helper_test_manifest_fields() {
    let (_, _, owner) = test_keypair();
    let hash = test_hash("manifest-test");
    let manifest = test_manifest(hash, owner, 250);

    assert_eq!(manifest.hash, hash);
    assert_eq!(manifest.owner, owner);
    assert_eq!(manifest.economics.price, 250);
    assert_eq!(manifest.visibility, Visibility::Shared);
    assert_eq!(manifest.content_type, nodalync_types::ContentType::L0);
}
