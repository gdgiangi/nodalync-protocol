//! Distributor trait and implementations.
//!
//! This module provides the `Distributor` trait for abstraction over
//! revenue distribution logic, allowing for different distribution strategies.

use nodalync_types::{Distribution, Payment, ProvenanceEntry, SettlementBatch};

use crate::distribution::distribute_revenue;
use crate::settlement::create_settlement_batch;

/// Trait for revenue distribution and settlement batch creation.
///
/// This trait abstracts the distribution logic, allowing for different
/// implementations (e.g., testing, alternative fee structures).
pub trait Distributor {
    /// Distribute revenue for a single payment.
    ///
    /// # Arguments
    /// * `payment` - The payment to distribute
    /// * `provenance` - Optional override provenance (uses payment.provenance if None)
    ///
    /// # Returns
    /// Vec of distributions to recipients
    fn distribute(
        &self,
        payment: &Payment,
        provenance: Option<&[ProvenanceEntry]>,
    ) -> Vec<Distribution>;

    /// Create a settlement batch from multiple payments.
    ///
    /// # Arguments
    /// * `payments` - The payments to batch
    ///
    /// # Returns
    /// A settlement batch ready for on-chain processing
    fn calculate_batch(&self, payments: &[Payment]) -> SettlementBatch;
}

/// Default distributor using protocol-specified distribution rules.
///
/// Uses 5% synthesis fee and 95% root pool distribution as specified in §10.1.
#[derive(Debug, Clone, Default)]
pub struct DefaultDistributor;

impl DefaultDistributor {
    /// Create a new default distributor.
    pub fn new() -> Self {
        Self
    }
}

impl Distributor for DefaultDistributor {
    fn distribute(
        &self,
        payment: &Payment,
        provenance: Option<&[ProvenanceEntry]>,
    ) -> Vec<Distribution> {
        let prov = provenance.unwrap_or(&payment.provenance);
        distribute_revenue(payment.amount, &payment.recipient, prov)
    }

    fn calculate_batch(&self, payments: &[Payment]) -> SettlementBatch {
        create_settlement_batch(payments)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nodalync_crypto::{
        content_hash, generate_identity, peer_id_from_public_key, Hash, PeerId, Signature,
    };
    use nodalync_types::Visibility;

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

    fn test_payment(owner: PeerId, provenance: Vec<ProvenanceEntry>) -> Payment {
        Payment::new(
            test_hash(b"payment"),
            test_hash(b"channel"),
            100,
            owner,
            test_hash(b"query"),
            provenance,
            1234567890,
            test_signature(),
        )
    }

    #[test]
    fn test_default_distributor_distribute() {
        let distributor = DefaultDistributor::new();
        let owner = test_peer_id();
        let root = test_peer_id();

        let entry = ProvenanceEntry::with_weight(test_hash(b"src"), root, Visibility::Shared, 1);
        let payment = test_payment(owner, vec![entry]);

        let distributions = distributor.distribute(&payment, None);

        // Should have distributions
        assert!(!distributions.is_empty());

        // Total should equal payment amount
        let total: u64 = distributions.iter().map(|d| d.amount).sum();
        assert_eq!(total, 100);
    }

    #[test]
    fn test_default_distributor_distribute_override_provenance() {
        let distributor = DefaultDistributor::new();
        let owner = test_peer_id();
        let root1 = test_peer_id();
        let root2 = test_peer_id();

        let entry1 = ProvenanceEntry::with_weight(test_hash(b"src1"), root1, Visibility::Shared, 1);
        let entry2 = ProvenanceEntry::with_weight(test_hash(b"src2"), root2, Visibility::Shared, 1);

        // Payment has entry1
        let payment = test_payment(owner, vec![entry1]);

        // Override with entry2
        let distributions = distributor.distribute(&payment, Some(&[entry2]));

        // Should use overridden provenance (root2, not root1)
        let has_root2 = distributions.iter().any(|d| d.recipient == root2);
        assert!(has_root2);
    }

    #[test]
    fn test_default_distributor_calculate_batch() {
        let distributor = DefaultDistributor::new();
        let owner = test_peer_id();
        let root = test_peer_id();

        let entry = ProvenanceEntry::with_weight(test_hash(b"src"), root, Visibility::Shared, 1);
        let payment = test_payment(owner, vec![entry]);

        let batch = distributor.calculate_batch(&[payment]);

        assert!(!batch.is_empty());
        assert_eq!(batch.total_amount(), 100);
    }

    #[test]
    fn test_default_distributor_calculate_batch_empty() {
        let distributor = DefaultDistributor::new();
        let batch = distributor.calculate_batch(&[]);

        assert!(batch.is_empty());
    }

    #[test]
    fn test_default_distributor_default() {
        let distributor = DefaultDistributor;
        let owner = test_peer_id();
        let payment = test_payment(owner, vec![]);

        let distributions = distributor.distribute(&payment, None);
        assert!(!distributions.is_empty());
    }
}
