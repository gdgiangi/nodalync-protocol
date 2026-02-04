//! Settlement batch processing (§10.4).
//!
//! This module implements settlement batch creation and triggering logic.

use std::collections::HashMap;

use nodalync_crypto::{Hash, PeerId, Timestamp};
use nodalync_types::{
    Amount, Payment, SettlementBatch, SettlementEntry, SETTLEMENT_BATCH_INTERVAL_MS,
    SETTLEMENT_BATCH_THRESHOLD,
};

use crate::distribution::distribute_revenue;
use crate::merkle::{compute_batch_id, compute_merkle_root};

/// Check if settlement should be triggered.
///
/// Settlement is triggered when either:
/// 1. The pending total meets or exceeds the batch threshold (100 HBAR)
/// 2. The interval since last settlement has elapsed (1 hour)
///
/// # Arguments
/// * `pending_total` - Total amount pending settlement
/// * `last_settlement` - Timestamp of the last settlement
/// * `now` - Current timestamp
///
/// # Returns
/// `true` if settlement should be triggered
pub fn should_settle(pending_total: Amount, last_settlement: Timestamp, now: Timestamp) -> bool {
    // Threshold reached
    if pending_total >= SETTLEMENT_BATCH_THRESHOLD {
        return true;
    }

    // Interval elapsed
    if now >= last_settlement && (now - last_settlement) >= SETTLEMENT_BATCH_INTERVAL_MS {
        return true;
    }

    false
}

/// Create a settlement batch from pending payments.
///
/// This function:
/// 1. Distributes revenue for each payment
/// 2. Aggregates distributions by recipient
/// 3. Creates settlement entries
/// 4. Computes batch ID and merkle root
///
/// # Arguments
/// * `payments` - The payments to include in the batch
///
/// # Returns
/// A settlement batch ready for on-chain processing
pub fn create_settlement_batch(payments: &[Payment]) -> SettlementBatch {
    if payments.is_empty() {
        return SettlementBatch::default();
    }

    // Track aggregated amounts and metadata by recipient
    // (amount, provenance_hashes, payment_ids)
    let mut by_recipient: HashMap<PeerId, (Amount, Vec<Hash>, Vec<Hash>)> = HashMap::new();

    for payment in payments {
        // Distribute this payment's revenue
        let distributions =
            distribute_revenue(payment.amount, &payment.recipient, &payment.provenance);

        for dist in distributions {
            let entry = by_recipient.entry(dist.recipient).or_default();

            // Aggregate amount
            entry.0 += dist.amount;

            // Add source hash if not already present
            if dist.source_hash != Hash([0u8; 32]) && !entry.1.contains(&dist.source_hash) {
                entry.1.push(dist.source_hash);
            }

            // Add payment ID if not already present
            if !entry.2.contains(&payment.id) {
                entry.2.push(payment.id);
            }
        }
    }

    // Convert to settlement entries
    let mut entries: Vec<SettlementEntry> = by_recipient
        .into_iter()
        .map(|(recipient, (amount, provenance_hashes, payment_ids))| {
            SettlementEntry::new(recipient, amount, provenance_hashes, payment_ids)
        })
        .collect();

    // Sort entries by recipient for deterministic ordering
    entries.sort_by(|a, b| a.recipient.0.cmp(&b.recipient.0));

    // Compute batch ID and merkle root
    let batch_id = compute_batch_id(&entries);
    let merkle_root = compute_merkle_root(&entries);

    SettlementBatch::new(batch_id, entries, merkle_root)
}

/// Calculate the total pending amount from a slice of payments.
///
/// # Arguments
/// * `payments` - The payments to sum
///
/// # Returns
/// Total payment amount
pub fn calculate_pending_total(payments: &[Payment]) -> Amount {
    payments.iter().map(|p| p.amount).sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use nodalync_crypto::{content_hash, generate_identity, peer_id_from_public_key, Signature};
    use nodalync_types::{ProvenanceEntry, Visibility};

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

    fn test_payment(
        amount: Amount,
        recipient: PeerId,
        provenance: Vec<ProvenanceEntry>,
    ) -> Payment {
        Payment::new(
            test_hash(b"payment"),
            test_hash(b"channel"),
            amount,
            recipient,
            test_hash(b"query"),
            provenance,
            1234567890,
            test_signature(),
        )
    }

    #[test]
    fn test_should_settle_threshold() {
        // At threshold
        assert!(should_settle(SETTLEMENT_BATCH_THRESHOLD, 0, 1000));

        // Above threshold
        assert!(should_settle(SETTLEMENT_BATCH_THRESHOLD + 1, 0, 1000));

        // Below threshold (but interval not elapsed)
        assert!(!should_settle(SETTLEMENT_BATCH_THRESHOLD - 1, 0, 1000));
    }

    #[test]
    fn test_should_settle_interval() {
        let last_settlement = 0;
        let now = SETTLEMENT_BATCH_INTERVAL_MS;

        // Exactly at interval
        assert!(should_settle(0, last_settlement, now));

        // After interval
        assert!(should_settle(0, last_settlement, now + 1));

        // Before interval
        assert!(!should_settle(0, last_settlement, now - 1));
    }

    #[test]
    fn test_should_settle_neither() {
        // Neither threshold nor interval reached
        let pending = SETTLEMENT_BATCH_THRESHOLD - 1;
        let last = 0;
        let now = SETTLEMENT_BATCH_INTERVAL_MS - 1;

        assert!(!should_settle(pending, last, now));
    }

    #[test]
    fn test_create_settlement_batch_empty() {
        let batch = create_settlement_batch(&[]);
        assert!(batch.is_empty());
        assert_eq!(batch.merkle_root, Hash([0u8; 32]));
    }

    #[test]
    fn test_create_settlement_batch_single_payment() {
        let owner = test_peer_id();
        let root = test_peer_id();

        let entry = ProvenanceEntry::with_weight(test_hash(b"src"), root, Visibility::Shared, 1);
        let payment = test_payment(100, owner, vec![entry]);

        let batch = create_settlement_batch(&[payment]);

        // Should have 2 entries: owner (synthesis fee) and root
        assert_eq!(batch.entry_count(), 2);

        // Total should equal payment amount
        assert_eq!(batch.total_amount(), 100);

        // Batch ID should be non-zero
        assert_ne!(batch.batch_id, Hash([0u8; 32]));

        // Merkle root should be non-zero
        assert_ne!(batch.merkle_root, Hash([0u8; 32]));
    }

    #[test]
    fn test_create_settlement_batch_aggregates() {
        let owner1 = test_peer_id();
        let owner2 = test_peer_id();
        let root = test_peer_id();

        let entry = ProvenanceEntry::with_weight(test_hash(b"src"), root, Visibility::Shared, 1);

        // Two payments with same root but different owners
        let payment1 = test_payment(100, owner1, vec![entry.clone()]);
        let payment2 = test_payment(100, owner2, vec![entry]);

        let batch = create_settlement_batch(&[payment1, payment2]);

        // Total amount should be 200
        assert_eq!(batch.total_amount(), 200);

        // Root should receive aggregated payments
        assert!(batch.contains_recipient(&root));
    }

    #[test]
    fn test_create_settlement_batch_owner_is_root() {
        let owner = test_peer_id();

        // Owner is also the root contributor
        let entry = ProvenanceEntry::with_weight(test_hash(b"src"), owner, Visibility::Shared, 1);
        let payment = test_payment(100, owner, vec![entry]);

        let batch = create_settlement_batch(&[payment]);

        // Should have 1 entry (owner gets everything)
        assert_eq!(batch.entry_count(), 1);
        assert_eq!(batch.total_amount(), 100);
        assert_eq!(batch.amount_for_recipient(&owner), 100);
    }

    #[test]
    fn test_create_settlement_batch_multiple_payments_same_recipient() {
        let owner = test_peer_id();
        let root = test_peer_id();

        let entry = ProvenanceEntry::with_weight(test_hash(b"src"), root, Visibility::Shared, 1);

        // Multiple payments to same owner
        let payment1 = test_payment(100, owner, vec![entry.clone()]);
        let payment2 = test_payment(50, owner, vec![entry]);

        let batch = create_settlement_batch(&[payment1, payment2]);

        // Total: 150
        assert_eq!(batch.total_amount(), 150);
    }

    #[test]
    fn test_calculate_pending_total() {
        let owner = test_peer_id();

        let payment1 = test_payment(100, owner, vec![]);
        let payment2 = test_payment(50, owner, vec![]);
        let payment3 = test_payment(75, owner, vec![]);

        let total = calculate_pending_total(&[payment1, payment2, payment3]);
        assert_eq!(total, 225);
    }

    #[test]
    fn test_calculate_pending_total_empty() {
        let total = calculate_pending_total(&[]);
        assert_eq!(total, 0);
    }

    #[test]
    fn test_batch_deterministic() {
        let owner = test_peer_id();
        let root = test_peer_id();

        let entry = ProvenanceEntry::with_weight(test_hash(b"src"), root, Visibility::Shared, 1);
        let payment = test_payment(100, owner, vec![entry]);

        let batch1 = create_settlement_batch(std::slice::from_ref(&payment));
        let batch2 = create_settlement_batch(&[payment]);

        // Same input should produce same output
        assert_eq!(batch1.batch_id, batch2.batch_id);
        assert_eq!(batch1.merkle_root, batch2.merkle_root);
    }

    #[test]
    fn test_should_settle_just_under_threshold() {
        assert!(!should_settle(SETTLEMENT_BATCH_THRESHOLD - 1, 0, 1000));
    }

    #[test]
    fn test_should_settle_at_exact_threshold() {
        assert!(should_settle(SETTLEMENT_BATCH_THRESHOLD, 0, 1000));
    }

    #[test]
    fn test_create_batch_empty_payments() {
        let batch = create_settlement_batch(&[]);
        assert!(batch.is_empty());
        assert_eq!(batch.entry_count(), 0);
        assert_eq!(batch.total_amount(), 0);
    }

    #[test]
    fn test_create_batch_deduplicates_recipients() {
        // Same root contributor appearing in multiple payments should be aggregated
        let owner = test_peer_id();
        let root = test_peer_id();

        let entry = ProvenanceEntry::with_weight(test_hash(b"src"), root, Visibility::Shared, 1);

        let payment1 = test_payment(100, owner, vec![entry.clone()]);
        let payment2 = test_payment(200, owner, vec![entry]);

        let batch = create_settlement_batch(&[payment1, payment2]);

        // Root should appear only once (aggregated from both payments)
        let root_entries: Vec<_> = batch
            .entries
            .iter()
            .filter(|e| e.recipient == root)
            .collect();
        assert_eq!(root_entries.len(), 1);

        // Total should be 300
        assert_eq!(batch.total_amount(), 300);
    }
}
