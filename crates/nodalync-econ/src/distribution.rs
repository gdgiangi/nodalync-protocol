//! Revenue distribution calculations (§10.1).
//!
//! This module implements the revenue distribution algorithm that splits
//! payments between the content owner (synthesis fee) and root contributors.

use std::collections::HashMap;

use nodalync_crypto::{Hash, PeerId};
use nodalync_types::{
    Amount, Distribution, ProvenanceEntry, SYNTHESIS_FEE_DENOMINATOR, SYNTHESIS_FEE_NUMERATOR,
};

/// Distribute payment revenue to owner and root contributors.
///
/// The distribution follows the protocol specification §10.1:
/// - Owner receives 5% synthesis fee
/// - Root pool (95%) is distributed proportionally by weight to all root contributors
/// - Any rounding remainder goes to the owner
///
/// # Arguments
/// * `payment_amount` - Total payment received (in smallest unit, 10^-8 NDL)
/// * `owner` - Content owner (receives synthesis fee)
/// * `provenance` - All root L0+L1 sources with weights
///
/// # Returns
/// Vec of distributions to each unique recipient
///
/// # Example
/// ```
/// use nodalync_econ::distribute_revenue;
/// use nodalync_crypto::{Hash, PeerId, content_hash, generate_identity, peer_id_from_public_key};
/// use nodalync_types::{ProvenanceEntry, Visibility};
///
/// let (_, pk) = generate_identity();
/// let owner = peer_id_from_public_key(&pk);
/// let hash = content_hash(b"content");
/// let entry = ProvenanceEntry::with_weight(hash, owner, Visibility::Shared, 1);
///
/// let distributions = distribute_revenue(100, &owner, &[entry]);
/// assert!(!distributions.is_empty());
/// ```
pub fn distribute_revenue(
    payment_amount: Amount,
    owner: &PeerId,
    provenance: &[ProvenanceEntry],
) -> Vec<Distribution> {
    // Calculate shares
    let owner_share = payment_amount * SYNTHESIS_FEE_NUMERATOR / SYNTHESIS_FEE_DENOMINATOR;
    let root_pool = payment_amount - owner_share; // Using subtraction to avoid rounding issues

    // Total weight across all roots
    let total_weight: u64 = provenance.iter().map(|e| e.weight as u64).sum();

    if total_weight == 0 {
        // Edge case: no roots (shouldn't happen for valid L3, but handle gracefully)
        // Owner gets everything
        return vec![Distribution::new(*owner, payment_amount, Hash([0u8; 32]))];
    }

    // Per-weight share (integer division, remainder goes to owner)
    let per_weight = root_pool / total_weight;
    let mut distributed: Amount = 0;

    // Group by owner to aggregate payments
    let mut owner_amounts: HashMap<PeerId, Amount> = HashMap::new();

    for entry in provenance {
        let amount = per_weight * (entry.weight as u64);
        distributed += amount;
        *owner_amounts.entry(entry.owner).or_default() += amount;
    }

    // Add synthesis fee and remainder to owner
    let remainder = root_pool - distributed; // Rounding dust
    *owner_amounts.entry(*owner).or_default() += owner_share + remainder;

    // Convert to distributions
    let mut distributions: Vec<Distribution> = owner_amounts
        .into_iter()
        .filter(|(_, amount)| *amount > 0)
        .map(|(recipient, amount)| {
            Distribution::new(recipient, amount, Hash([0u8; 32])) // Aggregated, no specific source
        })
        .collect();

    // Sort by recipient for deterministic output
    distributions.sort_by(|a, b| a.recipient.0.cmp(&b.recipient.0));

    distributions
}

/// Calculate the synthesis fee for a payment amount.
///
/// # Arguments
/// * `payment_amount` - Total payment amount
///
/// # Returns
/// The synthesis fee (5% of payment)
pub fn calculate_synthesis_fee(payment_amount: Amount) -> Amount {
    payment_amount * SYNTHESIS_FEE_NUMERATOR / SYNTHESIS_FEE_DENOMINATOR
}

/// Calculate the root pool for a payment amount.
///
/// # Arguments
/// * `payment_amount` - Total payment amount
///
/// # Returns
/// The root pool (95% of payment)
pub fn calculate_root_pool(payment_amount: Amount) -> Amount {
    payment_amount - calculate_synthesis_fee(payment_amount)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nodalync_crypto::{content_hash, generate_identity, peer_id_from_public_key};
    use nodalync_types::Visibility;

    fn test_peer_id() -> PeerId {
        let (_, public_key) = generate_identity();
        peer_id_from_public_key(&public_key)
    }

    fn test_hash(data: &[u8]) -> Hash {
        content_hash(data)
    }

    #[test]
    fn test_basic_distribution_single_root() {
        // 100 tokens, single root with weight 1
        // Expected: 5 to owner (synthesis fee), 95 to root
        let owner = test_peer_id();
        let root_owner = test_peer_id();
        let hash = test_hash(b"source");

        let entry = ProvenanceEntry::with_weight(hash, root_owner, Visibility::Shared, 1);
        let distributions = distribute_revenue(100, &owner, &[entry]);

        // Should have 2 distributions (owner and root)
        let owner_dist = distributions.iter().find(|d| d.recipient == owner);
        let root_dist = distributions.iter().find(|d| d.recipient == root_owner);

        assert!(owner_dist.is_some());
        assert!(root_dist.is_some());

        // Owner gets 5 (synthesis fee)
        assert_eq!(owner_dist.unwrap().amount, 5);
        // Root gets 95
        assert_eq!(root_dist.unwrap().amount, 95);
    }

    #[test]
    fn test_distribution_owner_is_root() {
        // Owner is also a root contributor
        let owner = test_peer_id();
        let hash = test_hash(b"source");

        let entry = ProvenanceEntry::with_weight(hash, owner, Visibility::Shared, 1);
        let distributions = distribute_revenue(100, &owner, &[entry]);

        // Should have 1 distribution (owner gets everything: 5 + 95)
        assert_eq!(distributions.len(), 1);
        assert_eq!(distributions[0].recipient, owner);
        assert_eq!(distributions[0].amount, 100);
    }

    #[test]
    fn test_distribution_multiple_roots_equal_weight() {
        // 100 tokens, 2 roots with equal weight
        let owner = test_peer_id();
        let root1 = test_peer_id();
        let root2 = test_peer_id();

        let entry1 = ProvenanceEntry::with_weight(test_hash(b"src1"), root1, Visibility::Shared, 1);
        let entry2 = ProvenanceEntry::with_weight(test_hash(b"src2"), root2, Visibility::Shared, 1);

        let distributions = distribute_revenue(100, &owner, &[entry1, entry2]);

        // Root pool is 95, per_weight = 95 / 2 = 47
        // Each root gets 47, remainder (1) goes to owner
        let root1_dist = distributions.iter().find(|d| d.recipient == root1);
        let root2_dist = distributions.iter().find(|d| d.recipient == root2);

        assert!(root1_dist.is_some());
        assert!(root2_dist.is_some());
        assert_eq!(root1_dist.unwrap().amount, 47);
        assert_eq!(root2_dist.unwrap().amount, 47);

        // Total: 5 (owner synthesis) + 1 (remainder) + 47 + 47 = 100
        // Owner total: 5 + 1 = 6
        let owner_dist = distributions.iter().find(|d| d.recipient == owner);
        assert!(owner_dist.is_some());
        assert_eq!(owner_dist.unwrap().amount, 6); // 5 synthesis + 1 remainder
    }

    #[test]
    fn test_spec_example_bob_alice_carol() {
        // Bob's L3 derives from:
        // - Alice's L0 (weight: 2)
        // - Carol's L0 (weight: 1)
        // - Bob's L0 (weight: 2)
        // Total weight: 5
        // Payment: 100 NDL

        let bob = test_peer_id();
        let alice = test_peer_id();
        let carol = test_peer_id();

        let entry_alice =
            ProvenanceEntry::with_weight(test_hash(b"alice"), alice, Visibility::Shared, 2);
        let entry_carol =
            ProvenanceEntry::with_weight(test_hash(b"carol"), carol, Visibility::Shared, 1);
        let entry_bob = ProvenanceEntry::with_weight(test_hash(b"bob"), bob, Visibility::Shared, 2);

        let distributions = distribute_revenue(100, &bob, &[entry_alice, entry_carol, entry_bob]);

        // owner_share = 100 * 5/100 = 5 NDL
        // root_pool = 95 NDL
        // per_weight = 95 / 5 = 19 NDL

        let alice_dist = distributions.iter().find(|d| d.recipient == alice);
        let carol_dist = distributions.iter().find(|d| d.recipient == carol);
        let bob_dist = distributions.iter().find(|d| d.recipient == bob);

        assert!(alice_dist.is_some());
        assert!(carol_dist.is_some());
        assert!(bob_dist.is_some());

        // Alice: 2 * 19 = 38 NDL
        assert_eq!(alice_dist.unwrap().amount, 38);

        // Carol: 1 * 19 = 19 NDL
        assert_eq!(carol_dist.unwrap().amount, 19);

        // Bob: 2 * 19 (roots) + 5 (synthesis) = 43 NDL
        assert_eq!(bob_dist.unwrap().amount, 43);

        // Verify total adds up
        let total: Amount = distributions.iter().map(|d| d.amount).sum();
        assert_eq!(total, 100);
    }

    #[test]
    fn test_empty_provenance() {
        // Edge case: empty provenance (all goes to owner)
        let owner = test_peer_id();
        let distributions = distribute_revenue(100, &owner, &[]);

        assert_eq!(distributions.len(), 1);
        assert_eq!(distributions[0].recipient, owner);
        assert_eq!(distributions[0].amount, 100);
    }

    #[test]
    fn test_zero_payment() {
        // Edge case: zero payment
        let owner = test_peer_id();
        let root = test_peer_id();
        let entry = ProvenanceEntry::with_weight(test_hash(b"src"), root, Visibility::Shared, 1);

        let distributions = distribute_revenue(0, &owner, &[entry]);

        // Everyone gets 0, should still produce distributions
        let total: Amount = distributions.iter().map(|d| d.amount).sum();
        assert_eq!(total, 0);
    }

    #[test]
    fn test_large_payment() {
        // Test with a large payment to ensure no overflow
        let owner = test_peer_id();
        let root = test_peer_id();
        let entry = ProvenanceEntry::with_weight(test_hash(b"src"), root, Visibility::Shared, 1);

        let large_amount: Amount = 10_000_000_000_000_000; // 10^16
        let distributions = distribute_revenue(large_amount, &owner, &[entry]);

        let total: Amount = distributions.iter().map(|d| d.amount).sum();
        assert_eq!(total, large_amount);
    }

    #[test]
    fn test_rounding_remainder_to_owner() {
        // Create a scenario where rounding produces remainder
        // 100 tokens, 3 roots with weight 1 each
        // root_pool = 95, per_weight = 95 / 3 = 31 (remainder 2)
        let owner = test_peer_id();
        let root1 = test_peer_id();
        let root2 = test_peer_id();
        let root3 = test_peer_id();

        let entry1 = ProvenanceEntry::with_weight(test_hash(b"1"), root1, Visibility::Shared, 1);
        let entry2 = ProvenanceEntry::with_weight(test_hash(b"2"), root2, Visibility::Shared, 1);
        let entry3 = ProvenanceEntry::with_weight(test_hash(b"3"), root3, Visibility::Shared, 1);

        let distributions = distribute_revenue(100, &owner, &[entry1, entry2, entry3]);

        let owner_dist = distributions.iter().find(|d| d.recipient == owner);
        let root1_dist = distributions.iter().find(|d| d.recipient == root1);
        let root2_dist = distributions.iter().find(|d| d.recipient == root2);
        let root3_dist = distributions.iter().find(|d| d.recipient == root3);

        // Each root gets 31
        assert_eq!(root1_dist.unwrap().amount, 31);
        assert_eq!(root2_dist.unwrap().amount, 31);
        assert_eq!(root3_dist.unwrap().amount, 31);

        // Owner gets 5 (synthesis) + 2 (remainder) = 7
        assert_eq!(owner_dist.unwrap().amount, 7);

        // Total: 7 + 31 + 31 + 31 = 100
        let total: Amount = distributions.iter().map(|d| d.amount).sum();
        assert_eq!(total, 100);
    }

    #[test]
    fn test_calculate_synthesis_fee() {
        assert_eq!(calculate_synthesis_fee(100), 5);
        assert_eq!(calculate_synthesis_fee(1000), 50);
        assert_eq!(calculate_synthesis_fee(0), 0);
    }

    #[test]
    fn test_calculate_root_pool() {
        assert_eq!(calculate_root_pool(100), 95);
        assert_eq!(calculate_root_pool(1000), 950);
        assert_eq!(calculate_root_pool(0), 0);
    }

    #[test]
    fn test_same_owner_multiple_entries() {
        // Same root owner appears multiple times - should aggregate
        let owner = test_peer_id();
        let root = test_peer_id();

        let entry1 = ProvenanceEntry::with_weight(test_hash(b"1"), root, Visibility::Shared, 1);
        let entry2 = ProvenanceEntry::with_weight(test_hash(b"2"), root, Visibility::Shared, 2);

        // Total weight: 3, root_pool = 95, per_weight = 31
        // root gets: 1*31 + 2*31 = 93
        // remainder: 95 - 93 = 2
        let distributions = distribute_revenue(100, &owner, &[entry1, entry2]);

        assert_eq!(distributions.len(), 2); // owner and root

        let owner_dist = distributions.iter().find(|d| d.recipient == owner);
        let root_dist = distributions.iter().find(|d| d.recipient == root);

        // Owner: 5 (synthesis) + 2 (remainder) = 7
        assert_eq!(owner_dist.unwrap().amount, 7);
        // Root: 93
        assert_eq!(root_dist.unwrap().amount, 93);
    }
}
