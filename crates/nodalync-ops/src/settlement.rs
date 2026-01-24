//! Settlement operations implementation.
//!
//! This module implements settlement batch creation and triggering
//! as specified in Protocol Specification §7.5.

use nodalync_crypto::Hash;
use nodalync_econ::{compute_batch_id, create_settlement_batch, should_settle};
use nodalync_store::SettlementQueueStore;
use nodalync_types::Payment;
use nodalync_valid::Validator;

use crate::error::OpsResult;
use crate::extraction::L1Extractor;
use crate::node_ops::{current_timestamp, NodeOperations};

impl<V, E> NodeOperations<V, E>
where
    V: Validator,
    E: L1Extractor,
{
    /// Trigger settlement batch.
    ///
    /// Spec §7.5:
    /// 1. Checks should_settle (threshold OR interval)
    /// 2. Gets pending from queue
    /// 3. Creates batch via create_settlement_batch
    /// 4. (Submit to chain - stub for MVP)
    /// 5. Marks as settled
    /// 6. Updates last_settlement_time
    ///
    /// Returns the batch ID if settlement was triggered, None otherwise.
    pub fn trigger_settlement_batch(&mut self) -> OpsResult<Option<Hash>> {
        let timestamp = current_timestamp();

        // Get pending total and last settlement time
        let pending_total = self.state.settlement.get_pending_total()?;
        let last_settlement = self.state.settlement.get_last_settlement_time()?.unwrap_or(0);
        let elapsed = timestamp.saturating_sub(last_settlement);

        // 1. Check should_settle (threshold OR interval)
        if !should_settle(pending_total, last_settlement, elapsed) {
            return Ok(None);
        }

        // 2. Get pending distributions from queue
        let pending = self.state.settlement.get_pending()?;
        if pending.is_empty() {
            return Ok(None);
        }

        // Convert QueuedDistribution to Payment for batch creation
        // Note: In a full implementation, we'd have the actual Payment objects
        // For MVP, we create synthetic payments from the queued distributions
        let payments: Vec<Payment> = pending
            .iter()
            .map(|d| {
                Payment::new(
                    d.payment_id,
                    Hash([0u8; 32]), // No channel for batch settlement
                    d.amount,
                    d.recipient,
                    d.source_hash,
                    vec![], // Provenance already computed
                    d.queued_at,
                    nodalync_crypto::Signature::from_bytes([0u8; 64]),
                )
            })
            .collect();

        // 3. Create batch via create_settlement_batch
        let batch = create_settlement_batch(&payments);
        let batch_id = compute_batch_id(&batch.entries);

        // 4. Submit to chain (stub for MVP)
        // In full implementation: self.chain.submit_batch(&batch)?;

        // 5. Mark as settled
        let payment_ids: Vec<Hash> = pending.iter().map(|d| d.payment_id).collect();
        self.state.settlement.mark_settled(&payment_ids, &batch_id)?;

        // 6. Update last_settlement_time
        self.state.settlement.set_last_settlement_time(timestamp)?;

        Ok(Some(batch_id))
    }

    /// Check if settlement should be triggered.
    pub fn should_trigger_settlement(&self) -> OpsResult<bool> {
        let timestamp = current_timestamp();
        let pending_total = self.state.settlement.get_pending_total()?;
        let last_settlement = self.state.settlement.get_last_settlement_time()?.unwrap_or(0);
        let elapsed = timestamp.saturating_sub(last_settlement);

        Ok(should_settle(pending_total, last_settlement, elapsed))
    }

    /// Get the current pending settlement total.
    pub fn get_pending_settlement_total(&self) -> OpsResult<u64> {
        Ok(self.state.settlement.get_pending_total()?)
    }

    /// Force settlement regardless of threshold/interval.
    pub fn force_settlement(&mut self) -> OpsResult<Option<Hash>> {
        let timestamp = current_timestamp();

        // Get pending distributions
        let pending = self.state.settlement.get_pending()?;
        if pending.is_empty() {
            return Ok(None);
        }

        // Create payments from distributions
        let payments: Vec<Payment> = pending
            .iter()
            .map(|d| {
                Payment::new(
                    d.payment_id,
                    Hash([0u8; 32]),
                    d.amount,
                    d.recipient,
                    d.source_hash,
                    vec![],
                    d.queued_at,
                    nodalync_crypto::Signature::from_bytes([0u8; 64]),
                )
            })
            .collect();

        // Create batch
        let batch = create_settlement_batch(&payments);
        let batch_id = compute_batch_id(&batch.entries);

        // Mark as settled
        let payment_ids: Vec<Hash> = pending.iter().map(|d| d.payment_id).collect();
        self.state.settlement.mark_settled(&payment_ids, &batch_id)?;

        // Update last settlement time
        self.state.settlement.set_last_settlement_time(timestamp)?;

        Ok(Some(batch_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node_ops::DefaultNodeOperations;
    use nodalync_crypto::{content_hash, generate_identity, peer_id_from_public_key};
    use nodalync_store::{NodeStateConfig, QueuedDistribution, SettlementQueueStore};
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

    fn test_peer_id() -> nodalync_crypto::PeerId {
        let (_, public_key) = generate_identity();
        peer_id_from_public_key(&public_key)
    }

    #[test]
    fn test_trigger_settlement_empty() {
        let (mut ops, _temp) = create_test_ops();

        // No pending distributions, should return None
        let result = ops.trigger_settlement_batch().unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_force_settlement() {
        let (mut ops, _temp) = create_test_ops();

        // Add some distributions to the queue
        let dist1 = QueuedDistribution::new(
            content_hash(b"payment1"),
            test_peer_id(),
            100,
            content_hash(b"source1"),
            current_timestamp(),
        );
        ops.state.settlement.enqueue(dist1).unwrap();

        let dist2 = QueuedDistribution::new(
            content_hash(b"payment2"),
            test_peer_id(),
            200,
            content_hash(b"source2"),
            current_timestamp(),
        );
        ops.state.settlement.enqueue(dist2).unwrap();

        // Force settlement
        let batch_id = ops.force_settlement().unwrap();
        assert!(batch_id.is_some());

        // Queue should now be empty
        let pending = ops.state.settlement.get_pending().unwrap();
        assert!(pending.is_empty());
    }

    #[test]
    fn test_get_pending_total() {
        let (mut ops, _temp) = create_test_ops();

        // Initially zero
        assert_eq!(ops.get_pending_settlement_total().unwrap(), 0);

        // Add distribution
        let dist = QueuedDistribution::new(
            content_hash(b"payment"),
            test_peer_id(),
            500,
            content_hash(b"source"),
            current_timestamp(),
        );
        ops.state.settlement.enqueue(dist).unwrap();

        // Should be 500
        assert_eq!(ops.get_pending_settlement_total().unwrap(), 500);
    }

    #[test]
    fn test_should_trigger_settlement() {
        let (mut ops, _temp) = create_test_ops();

        // Set a recent last_settlement_time so interval trigger doesn't fire
        let recent_time = current_timestamp();
        ops.state.settlement.set_last_settlement_time(recent_time).unwrap();

        // With no pending and recent settlement, should not trigger
        assert!(!ops.should_trigger_settlement().unwrap());
    }
}
