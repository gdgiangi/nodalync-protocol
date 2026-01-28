//! L2 Integration Tests
//!
//! These tests verify the L2 Entity Graph flows as specified in CHECKLIST.md:
//! 1. L0 -> L1 -> L2 -> L3 full flow test
//! 2. Multiple L1s -> build L2 -> merge L2s flow
//! 3. L3 from L2 -> query L3 -> verify provenance traces to L0/L1
//! 4. Verify L2 creator economics (value via synthesis fee only)
//!
//! Key L2 Design Understanding:
//! - L2 is a personal knowledge graph
//! - L2 visibility is ALWAYS Private
//! - L2 price is ALWAYS 0
//! - L2 cannot be published
//! - root_l0l1 contains ONLY L0/L1 entries (never L2/L3)
//! - L2 creators earn through L3 synthesis fees, not direct L2 queries

use nodalync_crypto::{
    content_hash, generate_identity, peer_id_from_public_key, Hash, PeerId, Signature,
};
use nodalync_econ::distribute_revenue;
use nodalync_ops::{DefaultNodeOperations, OpsError};
use nodalync_store::{
    CacheStore, CachedContent, ContentStore, ManifestStore, NodeState, NodeStateConfig,
    ProvenanceGraph,
};
use nodalync_types::{
    ContentType, L2EntityGraph, Manifest, Metadata, Provenance, ProvenanceEntry, Version,
    Visibility,
};
use nodalync_wire::PaymentReceipt;
use tempfile::TempDir;

// ============ HELPERS ============

/// Create test operations with a new identity and temp storage.
fn create_test_ops() -> (DefaultNodeOperations, TempDir, PeerId) {
    let temp_dir = TempDir::new().unwrap();
    let config = NodeStateConfig::new(temp_dir.path());
    let state = NodeState::open(config).unwrap();

    let (_, public_key) = generate_identity();
    let peer_id = peer_id_from_public_key(&public_key);

    let ops = DefaultNodeOperations::with_defaults(state, peer_id);
    (ops, temp_dir, peer_id)
}

/// Create L0 content and return its hash.
fn create_l0_content(ops: &mut DefaultNodeOperations, content: &[u8], title: &str) -> Hash {
    let meta = Metadata::new(title, content.len() as u64);
    ops.create_content(content, meta).unwrap()
}

/// Create an L1 manifest from L0 content.
///
/// This is needed because `extract_l1_summary` doesn't persist L1 manifests.
/// This helper creates a proper L1 manifest with `ContentType::L1`.
fn create_l1_from_l0(ops: &mut DefaultNodeOperations, l0_hash: &Hash) -> Result<Hash, OpsError> {
    // 1. Load L0 manifest
    let l0_manifest = ops
        .state
        .manifests
        .load(l0_hash)?
        .ok_or(OpsError::ManifestNotFound(*l0_hash))?;

    // Verify it's L0
    if l0_manifest.content_type != ContentType::L0 {
        return Err(OpsError::invalid_operation("source is not L0 content"));
    }

    // 2. Extract L1 summary
    let l1_summary = ops.extract_l1_summary(l0_hash)?;

    // 3. Serialize L1 summary as content
    let l1_content = serde_json::to_vec(&l1_summary).map_err(|e| {
        OpsError::invalid_operation(format!("failed to serialize L1 summary: {}", e))
    })?;

    // 4. Store content and get hash from store (let store compute the hash)
    let l1_hash = ops.state.content.store(&l1_content)?;

    // 5. Create L1 provenance
    // For L1, root_l0l1 contains the L0 source
    let provenance = Provenance {
        root_l0l1: vec![ProvenanceEntry::new(
            *l0_hash,
            l0_manifest.owner,
            l0_manifest.visibility,
        )],
        derived_from: vec![*l0_hash],
        depth: 1,
    };

    // 6. Create L1 manifest with same owner as L0
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;

    let l1_manifest = Manifest {
        hash: l1_hash,
        content_type: ContentType::L1,
        owner: l0_manifest.owner,
        version: Version::new_v1(l1_hash, timestamp),
        visibility: l0_manifest.visibility,
        access: Default::default(),
        metadata: Metadata::new("L1 Summary", l1_content.len() as u64),
        economics: Default::default(),
        provenance,
        created_at: timestamp,
        updated_at: timestamp,
    };

    // 7. Store manifest and update provenance
    ops.state.manifests.store(&l1_manifest)?;
    ops.state.provenance.add(&l1_hash, &[*l0_hash])?;

    Ok(l1_hash)
}

// ============ TEST 1: L0 -> L1 -> L2 -> L3 Full Flow ============

#[test]
fn test_l0_l1_l2_l3_full_flow() {
    let (mut ops, _temp, bob) = create_test_ops();

    // 1. Create L0 content (owned by Bob) - keep content short to avoid label length issues
    let l0_content = b"ML uses neural nets.";
    let l0_hash = create_l0_content(&mut ops, l0_content, "Bob ML");

    let l0_manifest = ops.state.manifests.load(&l0_hash).unwrap().unwrap();
    assert_eq!(l0_manifest.content_type, ContentType::L0);
    assert_eq!(l0_manifest.owner, bob);

    // 2. Create L1 from L0 (owned by Bob)
    let l1_hash = create_l1_from_l0(&mut ops, &l0_hash).unwrap();

    let l1_manifest = ops.state.manifests.load(&l1_hash).unwrap().unwrap();
    assert_eq!(l1_manifest.content_type, ContentType::L1);
    assert_eq!(l1_manifest.owner, bob, "L1 should have same owner as L0");

    // 3. Build L2 from L1 (owned by Bob)
    let l2_hash = ops.build_l2(vec![l1_hash], None).unwrap();

    let l2_manifest = ops.state.manifests.load(&l2_hash).unwrap().unwrap();
    assert_eq!(l2_manifest.content_type, ContentType::L2);
    assert_eq!(l2_manifest.owner, bob, "L2 should be owned by Bob");
    assert_eq!(
        l2_manifest.visibility,
        Visibility::Private,
        "L2 MUST be Private"
    );
    assert_eq!(l2_manifest.economics.price, 0, "L2 MUST have price 0");

    // Verify L2 provenance: root_l0l1 should contain L0/L1, not L2
    for entry in &l2_manifest.provenance.root_l0l1 {
        let entry_manifest = ops.state.manifests.load(&entry.hash).unwrap().unwrap();
        assert!(
            entry_manifest.content_type == ContentType::L0
                || entry_manifest.content_type == ContentType::L1,
            "root_l0l1 should only contain L0/L1, found {:?}",
            entry_manifest.content_type
        );
    }

    // 4. Derive L3 from L2 (owned by Bob)
    let insight = b"More data helps ML.";
    let l3_meta = Metadata::new("Insight", insight.len() as u64);
    let l3_hash = ops.derive_content(&[l2_hash], insight, l3_meta).unwrap();

    let l3_manifest = ops.state.manifests.load(&l3_hash).unwrap().unwrap();
    assert_eq!(l3_manifest.content_type, ContentType::L3);
    assert_eq!(l3_manifest.owner, bob, "L3 should be owned by Bob");

    // Verify L3 provenance
    assert!(
        l3_manifest.provenance.derived_from.contains(&l2_hash),
        "L3 derived_from should contain L2"
    );

    // root_l0l1 should NOT contain L2
    for entry in &l3_manifest.provenance.root_l0l1 {
        let entry_manifest = ops.state.manifests.load(&entry.hash).unwrap().unwrap();
        assert!(
            entry_manifest.content_type == ContentType::L0
                || entry_manifest.content_type == ContentType::L1,
            "L3 root_l0l1 should only contain L0/L1, found {:?}",
            entry_manifest.content_type
        );
    }

    // Verify depth increased
    assert!(
        l3_manifest.provenance.depth > l2_manifest.provenance.depth,
        "L3 depth should be greater than L2 depth"
    );
}

// ============ TEST 2: Multiple L1s -> Build L2 -> Merge L2s ============

#[test]
fn test_multiple_l1s_build_l2_merge_l2s() {
    let (mut ops, _temp, bob) = create_test_ops();

    // 1. Create 3 L0 documents (all owned by Bob) - keep short
    let l0_1_hash = create_l0_content(&mut ops, b"AI uses deep nets.", "AI Doc");
    let l0_2_hash = create_l0_content(&mut ops, b"NLP uses transformers.", "NLP Doc");
    let l0_3_hash = create_l0_content(&mut ops, b"CV uses image nets.", "CV Doc");

    // 2. Create L1 from each L0
    let l1_1_hash = create_l1_from_l0(&mut ops, &l0_1_hash).unwrap();
    let l1_2_hash = create_l1_from_l0(&mut ops, &l0_2_hash).unwrap();
    let l1_3_hash = create_l1_from_l0(&mut ops, &l0_3_hash).unwrap();

    // 3. Build L2-A from [L1-1, L1-2]
    let l2_a_hash = ops.build_l2(vec![l1_1_hash, l1_2_hash], None).unwrap();

    let l2_a_manifest = ops.state.manifests.load(&l2_a_hash).unwrap().unwrap();
    assert_eq!(l2_a_manifest.content_type, ContentType::L2);
    assert_eq!(l2_a_manifest.owner, bob);
    assert_eq!(l2_a_manifest.visibility, Visibility::Private);
    assert_eq!(l2_a_manifest.economics.price, 0);

    // 4. Build L2-B from [L1-2, L1-3] (overlapping L1-2)
    let l2_b_hash = ops.build_l2(vec![l1_2_hash, l1_3_hash], None).unwrap();

    let l2_b_manifest = ops.state.manifests.load(&l2_b_hash).unwrap().unwrap();
    assert_eq!(l2_b_manifest.content_type, ContentType::L2);
    assert_eq!(l2_b_manifest.owner, bob);
    assert_eq!(l2_b_manifest.visibility, Visibility::Private);
    assert_eq!(l2_b_manifest.economics.price, 0);

    // 5. Merge L2-A and L2-B
    let merged_l2_hash = ops.merge_l2(vec![l2_a_hash, l2_b_hash], None).unwrap();

    let merged_manifest = ops.state.manifests.load(&merged_l2_hash).unwrap().unwrap();
    assert_eq!(merged_manifest.content_type, ContentType::L2);
    assert_eq!(merged_manifest.owner, bob);
    assert_eq!(
        merged_manifest.visibility,
        Visibility::Private,
        "Merged L2 MUST be Private"
    );
    assert_eq!(
        merged_manifest.economics.price, 0,
        "Merged L2 MUST have price 0"
    );

    // Load the merged L2 graph to verify structure
    let merged_content = ops.state.content.load(&merged_l2_hash).unwrap().unwrap();
    let merged_graph: L2EntityGraph = serde_json::from_slice(&merged_content).unwrap();

    // Verify source_l2s contains both L2-A and L2-B
    assert!(
        merged_graph.source_l2s.contains(&l2_a_hash),
        "Merged L2 should reference L2-A"
    );
    assert!(
        merged_graph.source_l2s.contains(&l2_b_hash),
        "Merged L2 should reference L2-B"
    );

    // Verify L1 references are deduplicated (L1-2 appears once, not twice)
    let l1_hashes: Vec<Hash> = merged_graph.source_l1s.iter().map(|r| r.l1_hash).collect();
    let unique_l1_count = {
        let mut unique = l1_hashes.clone();
        unique.sort_by(|a, b| a.0.cmp(&b.0));
        unique.dedup();
        unique.len()
    };
    assert_eq!(
        l1_hashes.len(),
        unique_l1_count,
        "L1 references should be deduplicated"
    );

    // Verify provenance weights are accumulated for overlapping sources
    // (L1-2's corresponding L0 should have higher weight)
    let l0_2_entries: Vec<&ProvenanceEntry> = merged_manifest
        .provenance
        .root_l0l1
        .iter()
        .filter(|e| e.hash == l0_2_hash)
        .collect();

    // The overlapping L0-2 should have accumulated weight
    if !l0_2_entries.is_empty() {
        let total_weight: u32 = l0_2_entries.iter().map(|e| e.weight).sum();
        assert!(
            total_weight >= 2,
            "Overlapping source should have accumulated weight"
        );
    }
}

// ============ TEST 3: L3 Provenance Traces to L0/L1 (Not L2) ============

#[test]
fn test_l3_provenance_traces_to_l0_l1_not_l2() {
    let (mut ops, _temp, _bob) = create_test_ops();

    // 1. Build L0 -> L1 -> L2 chain with 2 sources - keep short
    let l0_1_hash = create_l0_content(&mut ops, b"First source data.", "Src1");
    let l0_2_hash = create_l0_content(&mut ops, b"Second source data.", "Src2");

    let l1_1_hash = create_l1_from_l0(&mut ops, &l0_1_hash).unwrap();
    let l1_2_hash = create_l1_from_l0(&mut ops, &l0_2_hash).unwrap();

    let l2_hash = ops.build_l2(vec![l1_1_hash, l1_2_hash], None).unwrap();

    let l2_manifest = ops.state.manifests.load(&l2_hash).unwrap().unwrap();

    // 2. Derive L3 from L2
    let insight = b"Combined insight.";
    let l3_meta = Metadata::new("Prov Test", insight.len() as u64);
    let l3_hash = ops.derive_content(&[l2_hash], insight, l3_meta).unwrap();

    // 3. Inspect L3 provenance
    let l3_manifest = ops.state.manifests.load(&l3_hash).unwrap().unwrap();

    // All root_l0l1 entries must resolve to L0 or L1
    for entry in &l3_manifest.provenance.root_l0l1 {
        let entry_manifest = ops.state.manifests.load(&entry.hash).unwrap().unwrap();
        assert!(
            entry_manifest.content_type == ContentType::L0
                || entry_manifest.content_type == ContentType::L1,
            "All root_l0l1 entries must be L0 or L1, found {:?} for hash {:?}",
            entry_manifest.content_type,
            entry.hash
        );
    }

    // L2 hash should NOT be in root_l0l1
    let l2_in_root: bool = l3_manifest
        .provenance
        .root_l0l1
        .iter()
        .any(|e| e.hash == l2_hash);
    assert!(
        !l2_in_root,
        "L2 hash should NOT be in root_l0l1 (L2 is intermediate, not foundational)"
    );

    // L2 hash SHOULD be in derived_from
    assert!(
        l3_manifest.provenance.derived_from.contains(&l2_hash),
        "L2 hash should be in derived_from"
    );

    // Depth should be L2 depth + 1
    assert_eq!(
        l3_manifest.provenance.depth,
        l2_manifest.provenance.depth + 1,
        "L3 depth should be L2 depth + 1"
    );
}

// ============ TEST 4: L2 Creator Economics ============

/// Test economics when Bob creates L0 -> L1 -> L2 -> L3 (all own content).
#[test]
fn test_l2_creator_economics_own_content() {
    let (mut ops, _temp, bob) = create_test_ops();

    // Bob creates L0
    let l0_hash = create_l0_content(&mut ops, b"Bob data.", "Bob");

    // Bob creates L1 from L0
    let l1_hash = create_l1_from_l0(&mut ops, &l0_hash).unwrap();

    // Bob builds L2 from L1
    let l2_hash = ops.build_l2(vec![l1_hash], None).unwrap();

    // Bob derives L3 from L2
    let insight = b"Bob insight.";
    let l3_meta = Metadata::new("BobL3", insight.len() as u64);
    let l3_hash = ops.derive_content(&[l2_hash], insight, l3_meta).unwrap();

    let l3_manifest = ops.state.manifests.load(&l3_hash).unwrap().unwrap();

    // Simulate: Someone queries Bob's L3 for 100 HBAR
    let payment_amount = 100u64;

    // Distribute revenue using L3's provenance
    let distributions = distribute_revenue(
        payment_amount,
        &l3_manifest.owner,
        &l3_manifest.provenance.root_l0l1,
    );

    // Bob should get everything:
    // - 5 HBAR synthesis fee (5%)
    // - 95 HBAR root pool (he's the only root creator)
    // Total: 100 HBAR
    let bob_amount: u64 = distributions
        .iter()
        .filter(|d| d.recipient == bob)
        .map(|d| d.amount)
        .sum();

    assert_eq!(
        bob_amount, 100,
        "Bob should get 100% when he owns all L0/L1 sources"
    );

    // Verify total distribution equals payment
    let total: u64 = distributions.iter().map(|d| d.amount).sum();
    assert_eq!(
        total, payment_amount,
        "Total distribution should equal payment"
    );
}

/// Test economics with mixed sources (queried + owned).
#[test]
fn test_l2_creator_economics_mixed_sources() {
    // We need two separate operations contexts for Alice and Bob
    let (mut alice_ops, _temp_alice, alice) = create_test_ops();
    let (mut bob_ops, _temp_bob, bob) = create_test_ops();

    // Alice creates and publishes L0-alice
    let alice_content = b"Alice research.";
    let l0_alice_hash = create_l0_content(&mut alice_ops, alice_content, "Alice");

    // Create L1 from Alice's L0
    let l1_alice_hash = create_l1_from_l0(&mut alice_ops, &l0_alice_hash).unwrap();

    // Get Alice's manifests
    let l0_alice_manifest = alice_ops
        .state
        .manifests
        .load(&l0_alice_hash)
        .unwrap()
        .unwrap();
    let l1_alice_manifest = alice_ops
        .state
        .manifests
        .load(&l1_alice_hash)
        .unwrap()
        .unwrap();

    // Simulate: Bob "queries" Alice's content (in real impl, this would be via network)
    // For testing, we manually store Alice's content and manifest in Bob's store
    let alice_content_for_bob = alice_ops
        .state
        .content
        .load(&l0_alice_hash)
        .unwrap()
        .unwrap();
    bob_ops
        .state
        .content
        .store_verified(&l0_alice_hash, &alice_content_for_bob)
        .unwrap();
    bob_ops.state.manifests.store(&l0_alice_manifest).unwrap();

    let l1_alice_content = alice_ops
        .state
        .content
        .load(&l1_alice_hash)
        .unwrap()
        .unwrap();
    bob_ops
        .state
        .content
        .store_verified(&l1_alice_hash, &l1_alice_content)
        .unwrap();
    bob_ops.state.manifests.store(&l1_alice_manifest).unwrap();

    // Cache Alice's L1 (simulating a query) - this is needed for build_l2 to accept non-owned content
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    let dummy_receipt = PaymentReceipt {
        payment_id: content_hash(b"dummy"),
        amount: 0,
        timestamp,
        channel_nonce: 0,
        distributor_signature: Signature::from_bytes([0u8; 64]),
    };
    let cached = CachedContent::new(
        l1_alice_hash,
        l1_alice_content.clone(),
        alice,
        timestamp,
        dummy_receipt,
    );
    bob_ops.state.cache.cache(cached).unwrap();

    // Bob creates his own L0-bob
    let bob_content = b"Bob research.";
    let l0_bob_hash = create_l0_content(&mut bob_ops, bob_content, "BobDoc");

    // Bob creates L1 from his L0
    let l1_bob_hash = create_l1_from_l0(&mut bob_ops, &l0_bob_hash).unwrap();

    // Bob builds L2 from both L1s (his personal knowledge graph mixing sources)
    // First, we need to ensure Alice's L1 is accessible
    // In test, we store it directly; in prod, it would be queried
    let l2_hash = bob_ops
        .build_l2(vec![l1_bob_hash, l1_alice_hash], None)
        .unwrap();

    // Bob derives L3 from L2
    let insight = b"Combined insight.";
    let l3_meta = Metadata::new("BobL3", insight.len() as u64);
    let l3_hash = bob_ops
        .derive_content(&[l2_hash], insight, l3_meta)
        .unwrap();

    let l3_manifest = bob_ops.state.manifests.load(&l3_hash).unwrap().unwrap();

    // Verify L3 provenance contains roots from both Alice and Bob
    let root_owners: Vec<PeerId> = l3_manifest
        .provenance
        .root_l0l1
        .iter()
        .map(|e| e.owner)
        .collect();

    assert!(
        root_owners.contains(&alice),
        "L3 provenance should include Alice's L0/L1"
    );
    assert!(
        root_owners.contains(&bob),
        "L3 provenance should include Bob's L0/L1"
    );

    // Simulate: Someone queries Bob's L3 for 100 HBAR
    let payment_amount = 100u64;

    // Distribute revenue using L3's provenance
    let distributions = distribute_revenue(
        payment_amount,
        &l3_manifest.owner, // Bob owns the L3
        &l3_manifest.provenance.root_l0l1,
    );

    // Calculate expected distribution:
    // - Bob gets 5 HBAR synthesis fee (5%)
    // - Root pool (95 HBAR) split between Alice and Bob based on weights
    // - With 2 sources of equal weight (1 each), each gets 95/2 = 47 HBAR
    // - Remainder (1) goes to owner (Bob)
    // Expected: Bob = 5 + 47 + 1 = 53, Alice = 47

    let bob_amount: u64 = distributions
        .iter()
        .filter(|d| d.recipient == bob)
        .map(|d| d.amount)
        .sum();

    let alice_amount: u64 = distributions
        .iter()
        .filter(|d| d.recipient == alice)
        .map(|d| d.amount)
        .sum();

    // Bob gets synthesis fee + his root share + remainder
    assert!(
        bob_amount >= 52,
        "Bob should get at least synthesis fee (5) + his root share (47) = 52, got {}",
        bob_amount
    );

    // Alice gets her root share
    assert!(
        alice_amount >= 47,
        "Alice should get her root share (47), got {}",
        alice_amount
    );

    // Verify total distribution equals payment
    let total: u64 = distributions.iter().map(|d| d.amount).sum();
    assert_eq!(
        total, payment_amount,
        "Total distribution should equal payment"
    );

    // Key invariant: L2 is invisible to economics
    // Bob doesn't get extra for creating L2 - his value is in the L3 synthesis fee
    // and any L0/L1 content he owns

    // The distribution should only have Bob and Alice
    let unique_recipients: std::collections::HashSet<PeerId> =
        distributions.iter().map(|d| d.recipient).collect();
    assert_eq!(
        unique_recipients.len(),
        2,
        "Distribution should only have Alice and Bob"
    );
}

// ============ Additional Invariant Tests ============

#[test]
fn test_l2_always_private() {
    let (mut ops, _temp, _bob) = create_test_ops();

    let l0_hash = create_l0_content(&mut ops, b"Data.", "T");
    let l1_hash = create_l1_from_l0(&mut ops, &l0_hash).unwrap();
    let l2_hash = ops.build_l2(vec![l1_hash], None).unwrap();

    let l2_manifest = ops.state.manifests.load(&l2_hash).unwrap().unwrap();

    assert_eq!(
        l2_manifest.visibility,
        Visibility::Private,
        "L2 MUST always be Private"
    );
}

#[test]
fn test_l2_always_price_zero() {
    let (mut ops, _temp, _bob) = create_test_ops();

    let l0_hash = create_l0_content(&mut ops, b"Data.", "T");
    let l1_hash = create_l1_from_l0(&mut ops, &l0_hash).unwrap();
    let l2_hash = ops.build_l2(vec![l1_hash], None).unwrap();

    let l2_manifest = ops.state.manifests.load(&l2_hash).unwrap().unwrap();

    assert_eq!(
        l2_manifest.economics.price, 0,
        "L2 MUST always have price 0"
    );
}

#[tokio::test]
async fn test_l2_cannot_publish() {
    let (mut ops, _temp, _bob) = create_test_ops();

    let l0_hash = create_l0_content(&mut ops, b"Data.", "T");
    let l1_hash = create_l1_from_l0(&mut ops, &l0_hash).unwrap();
    let l2_hash = ops.build_l2(vec![l1_hash], None).unwrap();

    // Attempt to publish L2 (should fail)
    let result = ops.publish_content(&l2_hash, Visibility::Shared, 100).await;

    assert!(result.is_err(), "L2 content cannot be published");
}

#[test]
fn test_only_l2_owner_can_derive_from_l2() {
    let (mut bob_ops, _temp_bob, _bob) = create_test_ops();
    let (mut alice_ops, _temp_alice, _alice) = create_test_ops();

    // Bob creates L0 -> L1 -> L2
    let l0_hash = create_l0_content(&mut bob_ops, b"Bob data.", "Bob");
    let l1_hash = create_l1_from_l0(&mut bob_ops, &l0_hash).unwrap();
    let l2_hash = bob_ops.build_l2(vec![l1_hash], None).unwrap();

    // Get Bob's L2 manifest and content
    let l2_manifest = bob_ops.state.manifests.load(&l2_hash).unwrap().unwrap();
    let l2_content = bob_ops.state.content.load(&l2_hash).unwrap().unwrap();

    // Store Bob's L2 in Alice's store (simulating Alice somehow getting the hash)
    alice_ops
        .state
        .content
        .store_verified(&l2_hash, &l2_content)
        .unwrap();
    alice_ops.state.manifests.store(&l2_manifest).unwrap();

    // Alice tries to derive from Bob's L2 (should fail - she doesn't own it)
    let insight = b"Alice insight.";
    let meta = Metadata::new("Alice", insight.len() as u64);
    let result = alice_ops.derive_content(&[l2_hash], insight, meta);

    assert!(
        matches!(result, Err(OpsError::AccessDenied)),
        "Only L2 owner can derive from L2"
    );
}
