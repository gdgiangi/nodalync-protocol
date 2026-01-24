//! End-to-End Economic Loop Tests
//!
//! These tests verify the complete Publish → Query → Pay → Settle flow
//! as specified in the Protocol Specification.
//!
//! This proves the core value proposition: knowledge creators get paid
//! when their content is queried.

use nodalync_crypto::{
    content_hash, generate_identity, peer_id_from_public_key, Hash, PeerId, Signature,
};
use nodalync_ops::{DefaultNodeOperations, OpsError};
use nodalync_store::{
    ContentStore, ManifestStore, NodeState, NodeStateConfig, SettlementQueueStore,
};
use nodalync_types::{ContentType, Manifest, Metadata, Provenance, ProvenanceEntry, Visibility};
use nodalync_wire::QueryRequestPayload;
use tempfile::TempDir;

// ============ TEST HARNESS ============

/// A test node with its own identity and storage.
struct TestNode {
    ops: DefaultNodeOperations,
    peer_id: PeerId,
    _temp_dir: TempDir,
}

impl TestNode {
    fn new() -> Self {
        let temp_dir = TempDir::new().unwrap();
        let config = NodeStateConfig::new(temp_dir.path());
        let state = NodeState::open(config).unwrap();

        let (_, public_key) = generate_identity();
        let peer_id = peer_id_from_public_key(&public_key);

        let ops = DefaultNodeOperations::with_defaults(state, peer_id);
        Self {
            ops,
            peer_id,
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

fn create_payment(amount: u64, recipient: PeerId, query_hash: Hash) -> nodalync_types::Payment {
    nodalync_types::Payment::new(
        content_hash(&[query_hash.0.as_slice(), &amount.to_be_bytes()].concat()),
        Hash([0u8; 32]), // No channel for this test
        amount,
        recipient,
        query_hash,
        vec![],
        current_timestamp(),
        Signature::from_bytes([0u8; 64]),
    )
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

    let payment = create_payment(100, manifest.owner, hash);
    let query_request = QueryRequestPayload {
        hash,
        query: None,
        payment,
        version_spec: None,
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

    // === VERIFY SETTLEMENT QUEUE ===
    let pending_total = alice.ops.get_pending_settlement_total().unwrap();
    assert_eq!(
        pending_total, 100,
        "Settlement queue should have 100 pending"
    );

    // Get pending distributions
    let pending = alice.ops.state.settlement.get_pending().unwrap();
    assert!(!pending.is_empty(), "Should have pending distributions");

    // For L0 content, Alice gets 100% (she's the only root contributor)
    // The 95/5 split only applies when there are multiple contributors
    let alice_dist: u64 = pending
        .iter()
        .filter(|d| d.recipient == alice.peer_id())
        .map(|d| d.amount)
        .sum();
    assert_eq!(alice_dist, 100, "Alice should receive full payment for L0");

    // === SETTLE ===
    let batch_id = alice.ops.force_settlement().await.unwrap();
    assert!(batch_id.is_some(), "Settlement batch should be created");

    // Verify queue is now empty
    let pending_after = alice.ops.get_pending_settlement_total().unwrap();
    assert_eq!(
        pending_after, 0,
        "Settlement queue should be empty after settle"
    );

    println!("✅ E2E Simple L0: Publish → Query → Pay → Settle completed successfully");
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
            currency: nodalync_types::Currency::NDL,
            total_queries: 0,
            total_revenue: 0,
        },
        provenance: l3_provenance.clone(),
        created_at: current_timestamp(),
        updated_at: current_timestamp(),
    };

    bob.ops.state.manifests.store(&l3_manifest).unwrap();

    // === CAROL: Query Bob's L3 ===
    let payment = create_payment(100, bob.peer_id(), l3_hash);
    let query_request = QueryRequestPayload {
        hash: l3_hash,
        query: None,
        payment,
        version_spec: None,
    };

    let response = bob
        .ops
        .handle_query_request(&carol.peer_id(), &query_request)
        .await
        .unwrap();

    assert_eq!(response.content, l3_content.to_vec());

    // === VERIFY 95/5 DISTRIBUTION ===
    let pending = bob.ops.state.settlement.get_pending().unwrap();

    // Calculate distributions per recipient
    let mut alice_total: u64 = 0;
    let mut bob_total: u64 = 0;

    for dist in &pending {
        if dist.recipient == alice.peer_id() {
            alice_total += dist.amount;
        } else if dist.recipient == bob.peer_id() {
            bob_total += dist.amount;
        }
    }

    // 95% to roots (Alice), 5% to synthesizer (Bob)
    assert_eq!(alice_total, 95, "Alice (root) should receive 95%");
    assert_eq!(bob_total, 5, "Bob (synthesizer) should receive 5%");

    // === SETTLE ===
    let batch_id = bob.ops.force_settlement().await.unwrap();
    assert!(batch_id.is_some());

    println!("✅ E2E Multi-hop: L0→L3→Query with 95/5 split completed successfully");
    println!("   Alice (root L0): {} NDL", alice_total);
    println!("   Bob (L3 synth):  {} NDL", bob_total);
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

    // Bob queries 3 times
    for _ in 0..3 {
        let payment = create_payment(10, manifest.owner, hash);
        let request = QueryRequestPayload {
            hash,
            query: None,
            payment,
            version_spec: None,
        };
        alice
            .ops
            .handle_query_request(&bob.peer_id(), &request)
            .await
            .unwrap();
    }

    // Carol queries 2 times
    for _ in 0..2 {
        let payment = create_payment(10, manifest.owner, hash);
        let request = QueryRequestPayload {
            hash,
            query: None,
            payment,
            version_spec: None,
        };
        alice
            .ops
            .handle_query_request(&carol.peer_id(), &request)
            .await
            .unwrap();
    }

    // Verify accumulated payments
    let pending_total = alice.ops.get_pending_settlement_total().unwrap();
    assert_eq!(pending_total, 50, "5 queries × 10 = 50 pending");

    // Batch settle all at once
    let batch_id = alice.ops.force_settlement().await.unwrap();
    assert!(batch_id.is_some());

    // All settled
    assert_eq!(alice.ops.get_pending_settlement_total().unwrap(), 0);

    println!("✅ E2E Batch Settlement: 5 queries settled in single batch");
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

    // Query content
    let payment = create_payment(100, manifest_before.owner, hash);
    let request = QueryRequestPayload {
        hash,
        query: None,
        payment,
        version_spec: None,
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

    // Bob tries to query
    let payment = create_payment(100, alice.peer_id(), hash);
    let request = QueryRequestPayload {
        hash,
        query: None,
        payment,
        version_spec: None,
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

    // Bob tries to pay less
    let manifest = alice.ops.get_content_manifest(&hash).unwrap().unwrap();
    let payment = create_payment(100, manifest.owner, hash); // Only 100, needs 1000

    let request = QueryRequestPayload {
        hash,
        query: None,
        payment,
        version_spec: None,
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
