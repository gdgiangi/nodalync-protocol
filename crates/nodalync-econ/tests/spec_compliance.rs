//! Spec compliance tests for nodalync-econ.
//!
//! These tests verify that the economic calculations match the examples
//! given in Protocol Specification §10.

use nodalync_crypto::{content_hash, generate_identity, peer_id_from_public_key, Hash, PeerId};
use nodalync_econ::{
    calculate_root_pool, calculate_synthesis_fee, compute_merkle_root, create_merkle_proof,
    create_settlement_batch, distribute_revenue, should_settle, validate_price,
    verify_merkle_proof,
};
use nodalync_types::{
    Amount, Payment, ProvenanceEntry, Signature, Visibility, MAX_PRICE, MIN_PRICE,
    SETTLEMENT_BATCH_INTERVAL_MS, SETTLEMENT_BATCH_THRESHOLD, SYNTHESIS_FEE_DENOMINATOR,
    SYNTHESIS_FEE_NUMERATOR,
};

fn test_peer_id() -> PeerId {
    let (_, public_key) = generate_identity();
    peer_id_from_public_key(&public_key)
}

fn test_hash(data: &[u8]) -> Hash {
    content_hash(data)
}

fn test_signature() -> Signature {
    Signature([0u8; 64])
}

/// §10.1 Spec Example: Bob's L3 derives from Alice, Carol, and Bob
///
/// ```text
/// Scenario:
///     Bob's L3 derives from:
///         - Alice's L0 (weight: 2)
///         - Carol's L0 (weight: 1)
///         - Bob's L0 (weight: 2)
///     Total weight: 5
///
///     Query payment: 100 HBAR
///
/// Distribution:
///     owner_share = 100 * 5/100 = 5 HBAR (Bob's synthesis fee)
///     root_pool = 100 * 95/100 = 95 HBAR
///     per_weight = 95 / 5 = 19 HBAR
///
///     Alice: 2 * 19 = 38 HBAR
///     Carol: 1 * 19 = 19 HBAR
///     Bob (roots): 2 * 19 = 38 HBAR
///     Bob (synthesis): 5 HBAR
///     Bob total: 43 HBAR
///
/// Final:
///     Alice: 38 HBAR (38%)
///     Carol: 19 HBAR (19%)
///     Bob: 43 HBAR (43%)
/// ```
#[test]
fn test_spec_example_bob_alice_carol() {
    // Create identities
    let bob = test_peer_id();
    let alice = test_peer_id();
    let carol = test_peer_id();

    // Create provenance entries
    let alice_entry =
        ProvenanceEntry::with_weight(test_hash(b"alice_l0"), alice, Visibility::Shared, 2);
    let carol_entry =
        ProvenanceEntry::with_weight(test_hash(b"carol_l0"), carol, Visibility::Shared, 1);
    let bob_entry = ProvenanceEntry::with_weight(test_hash(b"bob_l0"), bob, Visibility::Shared, 2);

    // Total weight: 5
    let total_weight: u32 = [&alice_entry, &carol_entry, &bob_entry]
        .iter()
        .map(|e| e.weight)
        .sum();
    assert_eq!(total_weight, 5);

    // Payment: 100 HBAR
    let payment_amount: Amount = 100;

    // Distribute revenue
    let distributions =
        distribute_revenue(payment_amount, &bob, &[alice_entry, carol_entry, bob_entry]);

    // Extract amounts per recipient
    let alice_amount = distributions
        .iter()
        .find(|d| d.recipient == alice)
        .map(|d| d.amount)
        .unwrap_or(0);
    let carol_amount = distributions
        .iter()
        .find(|d| d.recipient == carol)
        .map(|d| d.amount)
        .unwrap_or(0);
    let bob_amount = distributions
        .iter()
        .find(|d| d.recipient == bob)
        .map(|d| d.amount)
        .unwrap_or(0);

    // Verify expected amounts from spec
    // owner_share = 100 * 5/100 = 5
    // root_pool = 95
    // per_weight = 95 / 5 = 19
    // Alice: 2 * 19 = 38
    assert_eq!(alice_amount, 38, "Alice should receive 38 HBAR");

    // Carol: 1 * 19 = 19
    assert_eq!(carol_amount, 19, "Carol should receive 19 HBAR");

    // Bob: 2 * 19 + 5 = 43
    assert_eq!(
        bob_amount, 43,
        "Bob should receive 43 HBAR (38 roots + 5 synthesis)"
    );

    // Verify total equals payment
    let total = alice_amount + carol_amount + bob_amount;
    assert_eq!(
        total, payment_amount,
        "Total distributions must equal payment"
    );
}

/// §10.1 Verify synthesis fee calculation
#[test]
fn test_synthesis_fee_rate() {
    // 5% fee rate
    let rate = SYNTHESIS_FEE_NUMERATOR as f64 / SYNTHESIS_FEE_DENOMINATOR as f64;
    assert!((rate - 0.05).abs() < 0.001, "Synthesis fee should be 5%");

    // Test calculation helper
    assert_eq!(calculate_synthesis_fee(100), 5);
    assert_eq!(calculate_synthesis_fee(1000), 50);
    assert_eq!(calculate_synthesis_fee(10000), 500);
}

/// §10.1 Verify root pool calculation
#[test]
fn test_root_pool_calculation() {
    // Root pool is 95%
    assert_eq!(calculate_root_pool(100), 95);
    assert_eq!(calculate_root_pool(1000), 950);

    // Synthesis + root pool = total
    let amount: Amount = 12345;
    let fee = calculate_synthesis_fee(amount);
    let pool = calculate_root_pool(amount);
    assert_eq!(fee + pool, amount);
}

/// §10.3 Price validation constraints
#[test]
fn test_price_validation_constraints() {
    // MIN_PRICE = 1
    assert_eq!(MIN_PRICE, 1);
    assert!(validate_price(MIN_PRICE).is_ok());
    assert!(validate_price(MIN_PRICE - 1).is_err()); // 0 is invalid

    // MAX_PRICE = 10^16
    assert_eq!(MAX_PRICE, 10_000_000_000_000_000);
    assert!(validate_price(MAX_PRICE).is_ok());
    assert!(validate_price(MAX_PRICE + 1).is_err());
}

/// §10.4 Settlement batch threshold
#[test]
fn test_settlement_batch_threshold() {
    // 100 HBAR = 100 * 10^8 tinybars
    assert_eq!(SETTLEMENT_BATCH_THRESHOLD, 10_000_000_000);

    // Threshold triggers settlement
    assert!(should_settle(SETTLEMENT_BATCH_THRESHOLD, 0, 0));
    assert!(should_settle(SETTLEMENT_BATCH_THRESHOLD + 1, 0, 0));
    assert!(!should_settle(SETTLEMENT_BATCH_THRESHOLD - 1, 0, 0));
}

/// §10.4 Settlement batch interval
#[test]
fn test_settlement_batch_interval() {
    // 1 hour = 3,600,000 ms
    assert_eq!(SETTLEMENT_BATCH_INTERVAL_MS, 3_600_000);

    // Interval triggers settlement
    let last = 0;
    let now = SETTLEMENT_BATCH_INTERVAL_MS;
    assert!(should_settle(0, last, now));
    assert!(should_settle(0, last, now + 1));
    assert!(!should_settle(0, last, now - 1));
}

/// Verify that settlement batch correctly aggregates distributions
#[test]
fn test_settlement_batch_aggregation() {
    let bob = test_peer_id();
    let alice = test_peer_id();
    let carol = test_peer_id();

    // Create provenance
    let alice_entry =
        ProvenanceEntry::with_weight(test_hash(b"alice"), alice, Visibility::Shared, 2);
    let carol_entry =
        ProvenanceEntry::with_weight(test_hash(b"carol"), carol, Visibility::Shared, 1);
    let bob_entry = ProvenanceEntry::with_weight(test_hash(b"bob"), bob, Visibility::Shared, 2);

    // Create payment
    let payment = Payment::new(
        test_hash(b"payment"),
        test_hash(b"channel"),
        100,
        bob,
        test_hash(b"query"),
        vec![alice_entry, carol_entry, bob_entry],
        1234567890,
        test_signature(),
    );

    // Create settlement batch
    let batch = create_settlement_batch(&[payment]);

    // Verify batch totals
    assert_eq!(batch.total_amount(), 100);
    assert_eq!(batch.entry_count(), 3); // Alice, Carol, Bob

    // Verify per-recipient amounts
    assert_eq!(batch.amount_for_recipient(&alice), 38);
    assert_eq!(batch.amount_for_recipient(&carol), 19);
    assert_eq!(batch.amount_for_recipient(&bob), 43);
}

/// Verify merkle proofs for settlement batch entries
#[test]
fn test_settlement_merkle_proofs() {
    let owner = test_peer_id();
    let root1 = test_peer_id();
    let root2 = test_peer_id();

    let entry1 = ProvenanceEntry::with_weight(test_hash(b"src1"), root1, Visibility::Shared, 1);
    let entry2 = ProvenanceEntry::with_weight(test_hash(b"src2"), root2, Visibility::Shared, 1);

    let payment = Payment::new(
        test_hash(b"payment"),
        test_hash(b"channel"),
        100,
        owner,
        test_hash(b"query"),
        vec![entry1, entry2],
        1234567890,
        test_signature(),
    );

    let batch = create_settlement_batch(&[payment]);

    // Verify merkle root is computed
    let root = compute_merkle_root(&batch.entries);
    assert_eq!(batch.merkle_root, root);

    // Verify all entries have valid proofs
    for (i, entry) in batch.entries.iter().enumerate() {
        let proof = create_merkle_proof(&batch.entries, i).unwrap();
        assert!(
            verify_merkle_proof(&batch.merkle_root, entry, &proof),
            "Merkle proof failed for entry {i}"
        );
    }
}

/// Verify that rounding remainder goes to owner
#[test]
fn test_rounding_remainder_to_owner() {
    let owner = test_peer_id();
    let root1 = test_peer_id();
    let root2 = test_peer_id();
    let root3 = test_peer_id();

    // 3 roots with equal weight
    // root_pool = 95, per_weight = 95 / 3 = 31, remainder = 2
    let entry1 = ProvenanceEntry::with_weight(test_hash(b"1"), root1, Visibility::Shared, 1);
    let entry2 = ProvenanceEntry::with_weight(test_hash(b"2"), root2, Visibility::Shared, 1);
    let entry3 = ProvenanceEntry::with_weight(test_hash(b"3"), root3, Visibility::Shared, 1);

    let distributions = distribute_revenue(100, &owner, &[entry1, entry2, entry3]);

    let owner_amount = distributions
        .iter()
        .find(|d| d.recipient == owner)
        .map(|d| d.amount)
        .unwrap_or(0);

    // Owner gets: 5 (synthesis) + 2 (remainder) = 7
    assert_eq!(
        owner_amount, 7,
        "Owner should receive synthesis fee + remainder"
    );

    // Each root gets 31
    for root in [root1, root2, root3] {
        let amount = distributions
            .iter()
            .find(|d| d.recipient == root)
            .map(|d| d.amount)
            .unwrap_or(0);
        assert_eq!(amount, 31, "Each root should receive 31");
    }

    // Verify total
    let total: Amount = distributions.iter().map(|d| d.amount).sum();
    assert_eq!(total, 100);
}

/// Verify distribution with owner as root contributor
#[test]
fn test_owner_is_root_contributor() {
    let owner = test_peer_id();
    let other = test_peer_id();

    // Owner has weight 1, other has weight 1
    let owner_entry =
        ProvenanceEntry::with_weight(test_hash(b"owner"), owner, Visibility::Shared, 1);
    let other_entry =
        ProvenanceEntry::with_weight(test_hash(b"other"), other, Visibility::Shared, 1);

    let distributions = distribute_revenue(100, &owner, &[owner_entry, other_entry]);

    let owner_amount = distributions
        .iter()
        .find(|d| d.recipient == owner)
        .map(|d| d.amount)
        .unwrap_or(0);
    let other_amount = distributions
        .iter()
        .find(|d| d.recipient == other)
        .map(|d| d.amount)
        .unwrap_or(0);

    // root_pool = 95, per_weight = 47, remainder = 1
    // Other: 47
    // Owner: 47 (roots) + 5 (synthesis) + 1 (remainder) = 53
    assert_eq!(other_amount, 47);
    assert_eq!(owner_amount, 53);
    assert_eq!(owner_amount + other_amount, 100);
}

/// Verify empty provenance case
#[test]
fn test_empty_provenance_all_to_owner() {
    let owner = test_peer_id();

    let distributions = distribute_revenue(100, &owner, &[]);

    assert_eq!(distributions.len(), 1);
    assert_eq!(distributions[0].recipient, owner);
    assert_eq!(distributions[0].amount, 100);
}

/// Verify zero payment case
#[test]
fn test_zero_payment() {
    let owner = test_peer_id();
    let root = test_peer_id();

    let entry = ProvenanceEntry::with_weight(test_hash(b"src"), root, Visibility::Shared, 1);
    let distributions = distribute_revenue(0, &owner, &[entry]);

    let total: Amount = distributions.iter().map(|d| d.amount).sum();
    assert_eq!(total, 0);
}

/// Verify large payment doesn't overflow
#[test]
fn test_large_payment_no_overflow() {
    let owner = test_peer_id();
    let root = test_peer_id();

    let entry = ProvenanceEntry::with_weight(test_hash(b"src"), root, Visibility::Shared, 1);

    // Use MAX_PRICE as a large payment
    let large_amount = MAX_PRICE;
    let distributions = distribute_revenue(large_amount, &owner, &[entry]);

    let total: Amount = distributions.iter().map(|d| d.amount).sum();
    assert_eq!(total, large_amount);
}
