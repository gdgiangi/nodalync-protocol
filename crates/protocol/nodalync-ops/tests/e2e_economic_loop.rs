//! End-to-End Economic Loop Tests
//!
//! These tests verify the complete Publish → Query → Pay → Settle flow
//! as specified in the Protocol Specification.
//!
//! This proves the core value proposition: knowledge creators get paid
//! when their content is queried.
//!
//! # Test Categories
//!
//! 1. **Trustless Validation Tests** (run by default):
//!    - Verify that paid queries REQUIRE on-chain settlement
//!    - Verify access control and payment validation
//!
//! 2. **Full Settlement Tests** (run by default, use mock settlement):
//!    - Test complete Publish → Query → Pay → Settle flow
//!    - Use `MockSettlement` to satisfy the settlement requirement without Hedera
//!    - `force_settlement()` routes through the mock and returns success

use std::sync::Arc;

use nodalync_crypto::{
    content_hash, generate_identity, peer_id_from_public_key, Hash, PeerId, Signature,
};
use nodalync_ops::{DefaultNodeOperations, OpsError};
use nodalync_store::{
    ContentStore, ManifestStore, NodeState, NodeStateConfig, SettlementQueueStore,
};
use nodalync_test_utils::MockSettlement;
use nodalync_types::{ContentType, Manifest, Metadata, Provenance, ProvenanceEntry, Visibility};
use nodalync_wire::QueryRequestPayload;
use tempfile::TempDir;

// ============ TEST HARNESS ============

/// A test node with its own identity, storage, and mock settlement.
struct TestNode {
    ops: DefaultNodeOperations,
    peer_id: PeerId,
    mock_settle: MockSettlement,
    _temp_dir: TempDir,
}

impl TestNode {
    fn new() -> Self {
        let temp_dir = TempDir::new().unwrap();
        let config = NodeStateConfig::new(temp_dir.path());
        let state = NodeState::open(config).unwrap();

        let (_, public_key) = generate_identity();
        let peer_id = peer_id_from_public_key(&public_key);

        let mock_settle = MockSettlement::new();
        let ops = DefaultNodeOperations::with_defaults_and_settlement(
            state,
            peer_id,
            Arc::new(mock_settle.clone()),
        );
        Self {
            ops,
            peer_id,
            mock_settle,
            _temp_dir: temp_dir,
        }
    }

    fn peer_id(&self) -> PeerId {
        self.peer_id
    }
}

fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

fn create_payment(
    amount: u64,
    recipient: PeerId,
    query_hash: Hash,
    channel_id: Hash,
    provenance: Vec<ProvenanceEntry>,
) -> nodalync_types::Payment {
    // Use a random nonce to ensure unique payment IDs for repeated queries
    let nonce: u64 = rand::random();
    nodalync_types::Payment::new(
        content_hash(
            &[
                query_hash.0.as_slice(),
                &amount.to_be_bytes(),
                &nonce.to_be_bytes(),
            ]
            .concat(),
        ),
        channel_id,
        amount,
        recipient,
        query_hash,
        provenance,
        current_timestamp(),
        Signature::from_bytes([0u8; 64]),
    )
}

/// Open a payment channel between two nodes.
/// Returns the channel_id.
fn open_channel_between(
    owner: &mut TestNode,
    requester_peer_id: &PeerId,
    channel_name: &str,
) -> Hash {
    let channel_id = content_hash(channel_name.as_bytes());
    // Accept the channel from the requester's perspective (they deposited funds)
    owner
        .ops
        .accept_payment_channel(&channel_id, requester_peer_id, 10_000, 20_000)
        .unwrap();
    channel_id
}

// ============ E2E TESTS ============

/// Test 1: Simple L0 content - Publish → Query → Pay → Settle
///
/// Scenario:
/// - Alice publishes L0 content with price = 100
/// - Bob queries Alice's content
/// - Payment flows: 100 → Alice (100% since she's the only contributor)
#[tokio::test]
async fn test_e2e_simple_l0_publish_query_settle() {
    // === SETUP ===
    let mut alice = TestNode::new();
    let bob = TestNode::new();

    // Prevent automatic settlement triggers
    alice
        .ops
        .state
        .settlement
        .set_last_settlement_time(current_timestamp())
        .unwrap();

    // === PUBLISH (Alice) ===
    let content = b"Alice's knowledge about Rust async programming";
    let metadata = Metadata::new("Rust Async Guide", content.len() as u64);
    let hash = alice.ops.create_content(content, metadata).unwrap();

    // Publish with price = 100
    alice
        .ops
        .publish_content(&hash, Visibility::Shared, 100)
        .await
        .unwrap();

    // Verify content is published
    let manifest = alice.ops.get_content_manifest(&hash).unwrap().unwrap();
    assert_eq!(manifest.visibility, Visibility::Shared);
    assert_eq!(manifest.economics.price, 100);
    assert_eq!(manifest.content_type, ContentType::L0);

    // === QUERY (Bob → Alice) ===
    // In a real network, Bob would discover Alice via DHT and send request
    // Here we simulate by calling Alice's handler directly

    // Open a payment channel between Bob and Alice (required for paid content)
    let channel_id = open_channel_between(&mut alice, &bob.peer_id(), "bob-alice-channel");

    let payment = create_payment(
        100,
        manifest.owner,
        hash,
        channel_id,
        manifest.provenance.root_l0l1.clone(),
    );
    let query_request = QueryRequestPayload {
        hash,
        query: None,
        payment,
        version_spec: None,
        payment_nonce: 1,
    };

    // Simulate Bob sending query to Alice
    let response = alice
        .ops
        .handle_query_request(&bob.peer_id(), &query_request)
        .await
        .unwrap();

    // Verify response
    assert_eq!(response.content, content.to_vec());
    assert_eq!(response.payment_receipt.amount, 100);

    // === VERIFY IMMEDIATE SETTLEMENT VIA MOCK ===
    // With immediate settlement, the handler settles on-chain before delivering content.
    // The mock records all settled batches for inspection.
    let batches = alice.mock_settle.settled_batches();
    assert_eq!(
        batches.len(),
        1,
        "One batch should have been settled immediately"
    );

    // For L0 content, Alice gets 100% (she's the only root contributor)
    let total_settled: u64 = batches[0].entries.iter().map(|e| e.amount).sum();
    assert_eq!(
        total_settled, 100,
        "Total settled should equal payment amount"
    );

    // Verify economics updated
    let manifest_after = alice.ops.get_content_manifest(&hash).unwrap().unwrap();
    assert_eq!(manifest_after.economics.total_queries, 1);
    assert_eq!(manifest_after.economics.total_revenue, 100);

    println!("E2E Simple L0: Publish -> Query -> Pay -> Settle completed successfully");
}

/// Test 2: Multi-hop provenance - L0 → L1 → L3 → Query → All get paid
///
/// Scenario:
/// - Alice creates L0 content
/// - Alice extracts L1 from her L0
/// - Bob creates L3 synthesis from Alice's L1
/// - Carol queries Bob's L3
/// - Payment distribution: 95% to Alice (root), 5% to Bob (synthesis fee)
#[tokio::test]
async fn test_e2e_multihop_provenance_distribution() {
    // === SETUP ===
    let mut alice = TestNode::new();
    let mut bob = TestNode::new();
    let carol = TestNode::new();

    // Prevent automatic settlement triggers
    bob.ops
        .state
        .settlement
        .set_last_settlement_time(current_timestamp())
        .unwrap();

    // === ALICE: Create L0 content ===
    let l0_content = b"Original research about distributed systems consensus algorithms";
    let l0_metadata = Metadata::new("Consensus Algorithms", l0_content.len() as u64);
    let l0_hash = alice.ops.create_content(l0_content, l0_metadata).unwrap();

    alice
        .ops
        .publish_content(&l0_hash, Visibility::Shared, 50)
        .await
        .unwrap();

    let l0_manifest = alice.ops.get_content_manifest(&l0_hash).unwrap().unwrap();

    // === BOB: Create L3 synthesis from Alice's L0 ===
    // Bob has Alice's content (simulated by storing it locally)
    // In a real scenario, Bob would have queried and paid for this content
    bob.ops
        .state
        .content
        .store_verified(&l0_hash, l0_content)
        .unwrap();
    bob.ops.state.manifests.store(&l0_manifest).unwrap();

    // Bob creates L3 insight synthesizing Alice's knowledge
    let l3_content = b"Synthesis: Modern consensus algorithms combine Paxos and Raft insights";
    let l3_metadata = Metadata::new("Consensus Synthesis", l3_content.len() as u64);

    // Create L3 with provenance pointing to Alice's L0
    let l3_hash = bob.ops.state.content.store(l3_content).unwrap();

    let l3_provenance = Provenance {
        root_l0l1: vec![ProvenanceEntry::new(
            l0_hash,
            alice.peer_id(),
            Visibility::Shared,
        )],
        derived_from: vec![l0_hash],
        depth: 1,
    };

    let l3_manifest = Manifest {
        hash: l3_hash,
        content_type: ContentType::L3,
        owner: bob.peer_id(),
        version: nodalync_types::Version {
            number: 1,
            previous: None,
            root: l3_hash,
            timestamp: current_timestamp(),
        },
        visibility: Visibility::Shared,
        access: nodalync_types::AccessControl::default(),
        metadata: l3_metadata,
        economics: nodalync_types::Economics {
            price: 100,
            currency: nodalync_types::Currency::HBAR,
            total_queries: 0,
            total_revenue: 0,
        },
        provenance: l3_provenance.clone(),
        created_at: current_timestamp(),
        updated_at: current_timestamp(),
    };

    bob.ops.state.manifests.store(&l3_manifest).unwrap();

    // === CAROL: Query Bob's L3 ===
    // Open a payment channel between Carol and Bob (required for paid content)
    let channel_id = open_channel_between(&mut bob, &carol.peer_id(), "carol-bob-channel");

    let payment = create_payment(
        100,
        bob.peer_id(),
        l3_hash,
        channel_id,
        l3_provenance.root_l0l1.clone(),
    );
    let query_request = QueryRequestPayload {
        hash: l3_hash,
        query: None,
        payment,
        version_spec: None,
        payment_nonce: 1,
    };

    let response = bob
        .ops
        .handle_query_request(&carol.peer_id(), &query_request)
        .await
        .unwrap();

    assert_eq!(response.content, l3_content.to_vec());

    // === VERIFY 95/5 DISTRIBUTION VIA MOCK SETTLEMENT ===
    // With immediate settlement, the batch is settled on-chain before content delivery.
    let batches = bob.mock_settle.settled_batches();
    assert_eq!(
        batches.len(),
        1,
        "One batch should have been settled immediately"
    );

    // Verify 95/5 split in the settled batch entries
    let mut alice_total: u64 = 0;
    let mut bob_total: u64 = 0;
    for entry in &batches[0].entries {
        if entry.recipient == alice.peer_id() {
            alice_total += entry.amount;
        } else if entry.recipient == bob.peer_id() {
            bob_total += entry.amount;
        }
    }

    // 95% to roots (Alice), 5% to synthesizer (Bob)
    assert_eq!(alice_total, 95, "Alice (root) should receive 95%");
    assert_eq!(bob_total, 5, "Bob (synthesizer) should receive 5%");

    println!("E2E Multi-hop: L0->L3->Query with 95/5 split completed successfully");
    println!("   Alice (root L0): {} HBAR", alice_total);
    println!("   Bob (L3 synth):  {} HBAR", bob_total);
}

/// Test 3: Multiple queries accumulate and batch settle
///
/// Scenario:
/// - Alice publishes content
/// - Bob queries 3 times
/// - Carol queries 2 times
/// - All 5 payments batch settle together
#[tokio::test]
async fn test_e2e_batch_settlement() {
    let mut alice = TestNode::new();
    let bob = TestNode::new();
    let carol = TestNode::new();

    // Prevent automatic settlement
    alice
        .ops
        .state
        .settlement
        .set_last_settlement_time(current_timestamp())
        .unwrap();

    // Alice publishes content
    let content = b"Premium knowledge content";
    let metadata = Metadata::new("Premium Content", content.len() as u64);
    let hash = alice.ops.create_content(content, metadata).unwrap();
    alice
        .ops
        .publish_content(&hash, Visibility::Shared, 10)
        .await
        .unwrap();

    let manifest = alice.ops.get_content_manifest(&hash).unwrap().unwrap();

    // Open payment channels for Bob and Carol
    let bob_channel = open_channel_between(&mut alice, &bob.peer_id(), "bob-batch-channel");
    let carol_channel = open_channel_between(&mut alice, &carol.peer_id(), "carol-batch-channel");

    // Bob queries 3 times (with incrementing nonces)
    for nonce in 1..=3 {
        let payment = create_payment(
            10,
            manifest.owner,
            hash,
            bob_channel,
            manifest.provenance.root_l0l1.clone(),
        );
        let request = QueryRequestPayload {
            hash,
            query: None,
            payment,
            version_spec: None,
            payment_nonce: nonce,
        };
        alice
            .ops
            .handle_query_request(&bob.peer_id(), &request)
            .await
            .unwrap();
    }

    // Carol queries 2 times (with incrementing nonces)
    for nonce in 1..=2 {
        let payment = create_payment(
            10,
            manifest.owner,
            hash,
            carol_channel,
            manifest.provenance.root_l0l1.clone(),
        );
        let request = QueryRequestPayload {
            hash,
            query: None,
            payment,
            version_spec: None,
            payment_nonce: nonce,
        };
        alice
            .ops
            .handle_query_request(&carol.peer_id(), &request)
            .await
            .unwrap();
    }

    // === VERIFY IMMEDIATE SETTLEMENT VIA MOCK ===
    // With immediate settlement, each query settles on-chain individually.
    let batches = alice.mock_settle.settled_batches();
    assert_eq!(
        batches.len(),
        5,
        "5 queries should produce 5 immediate settlements"
    );

    // Verify total amount settled across all batches
    let total_settled: u64 = batches
        .iter()
        .flat_map(|b| b.entries.iter())
        .map(|e| e.amount)
        .sum();
    assert_eq!(total_settled, 50, "5 queries x 10 = 50 total settled");

    // Verify economics updated on the manifest
    let manifest_after = alice.ops.get_content_manifest(&hash).unwrap().unwrap();
    assert_eq!(manifest_after.economics.total_queries, 5);
    assert_eq!(manifest_after.economics.total_revenue, 50);

    println!("E2E Batch Settlement: 5 queries settled individually via immediate settlement");
}

/// Test 4: Verify economics are tracked correctly
#[tokio::test]
async fn test_e2e_economics_tracking() {
    let mut alice = TestNode::new();
    let bob = TestNode::new();

    alice
        .ops
        .state
        .settlement
        .set_last_settlement_time(current_timestamp())
        .unwrap();

    // Publish content
    let content = b"Tracked content";
    let metadata = Metadata::new("Tracked", content.len() as u64);
    let hash = alice.ops.create_content(content, metadata).unwrap();
    alice
        .ops
        .publish_content(&hash, Visibility::Shared, 100)
        .await
        .unwrap();

    // Check initial economics
    let manifest_before = alice.ops.get_content_manifest(&hash).unwrap().unwrap();
    assert_eq!(manifest_before.economics.total_queries, 0);
    assert_eq!(manifest_before.economics.total_revenue, 0);

    // Open a payment channel for Bob
    let channel_id = open_channel_between(&mut alice, &bob.peer_id(), "bob-economics-channel");

    // Query content
    let payment = create_payment(
        100,
        manifest_before.owner,
        hash,
        channel_id,
        manifest_before.provenance.root_l0l1.clone(),
    );
    let request = QueryRequestPayload {
        hash,
        query: None,
        payment,
        version_spec: None,
        payment_nonce: 1,
    };
    alice
        .ops
        .handle_query_request(&bob.peer_id(), &request)
        .await
        .unwrap();

    // Check updated economics
    let manifest_after = alice.ops.get_content_manifest(&hash).unwrap().unwrap();
    assert_eq!(manifest_after.economics.total_queries, 1);
    assert_eq!(manifest_after.economics.total_revenue, 100);

    println!("✅ E2E Economics Tracking: Query count and revenue tracked correctly");
}

/// Test 5: Access control - private content cannot be queried
#[tokio::test]
async fn test_e2e_access_control() {
    let mut alice = TestNode::new();
    let bob = TestNode::new();

    // Create private content (not published)
    let content = b"Private secrets";
    let metadata = Metadata::new("Private", content.len() as u64);
    let hash = alice.ops.create_content(content, metadata).unwrap();
    // Don't publish - stays private

    // Bob tries to query (no channel needed - will fail at access check first)
    let dummy_channel = content_hash(b"dummy-channel");
    let payment = create_payment(100, alice.peer_id(), hash, dummy_channel, vec![]);
    let request = QueryRequestPayload {
        hash,
        query: None,
        payment,
        version_spec: None,
        payment_nonce: 1,
    };

    let result = alice
        .ops
        .handle_query_request(&bob.peer_id(), &request)
        .await;

    assert!(
        matches!(result, Err(OpsError::AccessDenied)),
        "Private content should not be accessible"
    );

    println!("✅ E2E Access Control: Private content properly protected");
}

/// Test 6: Insufficient payment is rejected
#[tokio::test]
async fn test_e2e_payment_validation() {
    let mut alice = TestNode::new();
    let bob = TestNode::new();

    // Publish expensive content
    let content = b"Premium content";
    let metadata = Metadata::new("Premium", content.len() as u64);
    let hash = alice.ops.create_content(content, metadata).unwrap();
    alice
        .ops
        .publish_content(&hash, Visibility::Shared, 1000)
        .await
        .unwrap();

    // Bob tries to pay less (will fail at payment amount check before channel check)
    let manifest = alice.ops.get_content_manifest(&hash).unwrap().unwrap();
    let dummy_channel = content_hash(b"dummy-payment-channel");
    let payment = create_payment(
        100, // Only 100, needs 1000
        manifest.owner,
        hash,
        dummy_channel,
        manifest.provenance.root_l0l1.clone(),
    );

    let request = QueryRequestPayload {
        hash,
        query: None,
        payment,
        version_spec: None,
        payment_nonce: 1,
    };

    let result = alice
        .ops
        .handle_query_request(&bob.peer_id(), &request)
        .await;

    assert!(
        matches!(result, Err(OpsError::PaymentInsufficient)),
        "Insufficient payment should be rejected"
    );

    println!("✅ E2E Payment Validation: Insufficient payment rejected");
}
