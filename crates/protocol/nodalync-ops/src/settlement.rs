//! Settlement operations implementation.
//!
//! This module implements settlement batch creation and triggering
//! as specified in Protocol Specification §7.5.

use nodalync_crypto::Hash;
use nodalync_econ::{create_settlement_batch, should_settle};
use nodalync_store::SettlementQueueStore;
use nodalync_types::Payment;
use nodalync_valid::Validator;
use nodalync_wire::SettleConfirmPayload;
use tracing::{info, warn};

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
    /// 4. Broadcasts settlement confirmation (if network available)
    /// 5. Marks as settled
    /// 6. Updates last_settlement_time
    ///
    /// Returns the batch ID if settlement was triggered, None otherwise.
    pub async fn trigger_settlement_batch(&mut self) -> OpsResult<Option<Hash>> {
        let timestamp = current_timestamp();

        // Get pending total and last settlement time
        let pending_total = self.state.settlement.get_pending_total()?;
        let last_settlement = self
            .state
            .settlement
            .get_last_settlement_time()?
            .unwrap_or(0);
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
        let batch_id = batch.batch_id;

        // 4. Submit to Hedera if settlement configured
        let transaction_id = if let Some(settlement) = self.settlement().cloned() {
            match settlement.settle_batch(&batch).await {
                Ok(tx_id) => {
                    info!(batch_id = %batch_id, tx_id = %tx_id, "Batch settled on-chain");
                    tx_id.to_string()
                }
                Err(e) => {
                    warn!(batch_id = %batch_id, error = %e, "On-chain settlement failed, keeping queue intact");
                    return Err(crate::error::OpsError::SettlementFailed(e.to_string()));
                }
            }
        } else {
            format!("local-{}", batch_id) // No settlement configured
        };

        // 5. Broadcast settlement confirmation (if network available)
        if let Some(network) = self.network().cloned() {
            let confirm = SettleConfirmPayload {
                batch_id,
                transaction_id: transaction_id.clone(),
                block_number: 0, // Hedera doesn't use block numbers
                timestamp,
            };
            // Best effort broadcast
            let _ = network.broadcast_settlement_confirm(confirm).await;
        }

        // 6. Mark as settled
        let payment_ids: Vec<Hash> = pending.iter().map(|d| d.payment_id).collect();
        self.state
            .settlement
            .mark_settled(&payment_ids, &batch_id)?;

        // 7. Update last_settlement_time
        self.state.settlement.set_last_settlement_time(timestamp)?;

        Ok(Some(batch_id))
    }

    /// Check if settlement should be triggered.
    pub fn should_trigger_settlement(&self) -> OpsResult<bool> {
        let timestamp = current_timestamp();
        let pending_total = self.state.settlement.get_pending_total()?;
        let last_settlement = self
            .state
            .settlement
            .get_last_settlement_time()?
            .unwrap_or(0);
        let elapsed = timestamp.saturating_sub(last_settlement);

        Ok(should_settle(pending_total, last_settlement, elapsed))
    }

    /// Get the current pending settlement total.
    pub fn get_pending_settlement_total(&self) -> OpsResult<u64> {
        Ok(self.state.settlement.get_pending_total()?)
    }

    /// Force settlement regardless of threshold/interval.
    pub async fn force_settlement(&mut self) -> OpsResult<Option<Hash>> {
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
        let batch_id = batch.batch_id;

        // Submit to Hedera if settlement configured
        let transaction_id = if let Some(settlement) = self.settlement().cloned() {
            match settlement.settle_batch(&batch).await {
                Ok(tx_id) => {
                    info!(batch_id = %batch_id, tx_id = %tx_id, "Force batch settled on-chain");
                    tx_id.to_string()
                }
                Err(e) => {
                    warn!(batch_id = %batch_id, error = %e, "On-chain force settlement failed, keeping queue intact");
                    return Err(crate::error::OpsError::SettlementFailed(e.to_string()));
                }
            }
        } else {
            format!("local-force-{}", batch_id) // No settlement configured
        };

        // Broadcast settlement confirmation (if network available)
        if let Some(network) = self.network().cloned() {
            let confirm = SettleConfirmPayload {
                batch_id,
                transaction_id,
                block_number: 0,
                timestamp,
            };
            let _ = network.broadcast_settlement_confirm(confirm).await;
        }

        // Mark as settled
        let payment_ids: Vec<Hash> = pending.iter().map(|d| d.payment_id).collect();
        self.state
            .settlement
            .mark_settled(&payment_ids, &batch_id)?;

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

    #[tokio::test]
    async fn test_trigger_settlement_empty() {
        let (mut ops, _temp) = create_test_ops();

        // No pending distributions, should return None
        let result = ops.trigger_settlement_batch().await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_force_settlement() {
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
        let batch_id = ops.force_settlement().await.unwrap();
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
        ops.state
            .settlement
            .set_last_settlement_time(recent_time)
            .unwrap();

        // With no pending and recent settlement, should not trigger
        assert!(!ops.should_trigger_settlement().unwrap());
    }

    #[test]
    fn test_has_settlement() {
        let (ops_no_settle, _temp1) = create_test_ops();
        assert!(!ops_no_settle.has_settlement());
    }

    // Integration test for Hedera settlement.
    // Run with: cargo test -p nodalync-ops --features testnet test_settlement_with_hedera
    // Requires: HEDERA_ACCOUNT_ID, HEDERA_PRIVATE_KEY, HEDERA_CONTRACT_ID env vars (or .env file)
    #[cfg(feature = "testnet")]
    #[tokio::test]
    async fn test_settlement_with_hedera() {
        use nodalync_settle::{HederaConfig, HederaSettlement, Settlement};
        use nodalync_test_utils::{get_hedera_credentials, try_load_dotenv};
        use std::io::Write;
        use tempfile::NamedTempFile;

        try_load_dotenv();
        let (account_id, private_key, contract_id) = match get_hedera_credentials() {
            Some(creds) => creds,
            None => {
                println!("Skipping: Hedera credentials not available");
                return;
            }
        };

        // Write private key to temp file (strip 0x prefix if present)
        let key_str = private_key.strip_prefix("0x").unwrap_or(&private_key);
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(key_str.as_bytes()).unwrap();

        let config =
            HederaConfig::testnet(&account_id, temp_file.path().to_path_buf(), &contract_id);

        let settlement = HederaSettlement::new(config).await.unwrap();

        // Verify we can query the balance
        let balance_before = settlement.get_balance().await.unwrap();
        println!("Balance before deposit: {} tinybars", balance_before);

        // Deposit a small amount (1 tinybar)
        let tx_id = settlement.deposit(1).await.unwrap();
        println!("Deposit tx: {}", tx_id);

        // Verify balance increased
        let balance_after = settlement.get_balance().await.unwrap();
        println!("Balance after deposit: {} tinybars", balance_after);
        assert!(
            balance_after > balance_before,
            "Balance should increase after deposit"
        );
    }

    #[tokio::test]
    async fn test_trigger_settlement_with_mock_settlement() {
        use nodalync_test_utils::*;

        let (mut ops, _mock_net, mock_settle, _temp) = create_test_ops_with_mocks();

        // Enqueue distributions
        let dist1 = QueuedDistribution::new(
            content_hash(b"mock-payment1"),
            test_peer_id(),
            100,
            content_hash(b"mock-source1"),
            current_timestamp(),
        );
        ops.state.settlement.enqueue(dist1).unwrap();

        let dist2 = QueuedDistribution::new(
            content_hash(b"mock-payment2"),
            test_peer_id(),
            200,
            content_hash(b"mock-source2"),
            current_timestamp(),
        );
        ops.state.settlement.enqueue(dist2).unwrap();

        // Force settlement to bypass threshold check
        let batch_id = ops.force_settlement().await.unwrap();
        assert!(batch_id.is_some(), "Should have settled a batch");

        // Verify MockSettlement received the batch
        let batches = mock_settle.settled_batches();
        assert_eq!(
            batches.len(),
            1,
            "MockSettlement should have received one batch"
        );

        // Queue should be empty after settlement
        let pending = ops.state.settlement.get_pending().unwrap();
        assert!(pending.is_empty());
    }

    #[tokio::test]
    async fn test_trigger_settlement_below_threshold_no_action() {
        use nodalync_test_utils::*;

        let (mut ops, _mock_net, mock_settle, _temp) = create_test_ops_with_mocks();

        // Set a recent last_settlement_time so interval trigger doesn't fire
        let recent_time = current_timestamp();
        ops.state
            .settlement
            .set_last_settlement_time(recent_time)
            .unwrap();

        // Add a small distribution (below default threshold)
        let dist = QueuedDistribution::new(
            content_hash(b"small-payment"),
            test_peer_id(),
            1, // very small amount
            content_hash(b"small-source"),
            current_timestamp(),
        );
        ops.state.settlement.enqueue(dist).unwrap();

        // Trigger settlement - should not settle due to threshold + recent settlement
        let result = ops.trigger_settlement_batch().await.unwrap();
        assert!(
            result.is_none(),
            "Should not settle below threshold with recent settlement"
        );

        // Verify MockSettlement was not called
        let batches = mock_settle.settled_batches();
        assert!(batches.is_empty(), "No batches should have been sent");
    }

    #[tokio::test]
    async fn test_force_settlement_with_mock() {
        use nodalync_test_utils::*;

        let mock_settle = MockSettlement::new().with_balance(10000);
        let (mut ops, _temp) =
            create_test_ops_with_settlement(std::sync::Arc::new(mock_settle.clone()));

        // Enqueue a distribution
        let dist = QueuedDistribution::new(
            content_hash(b"force-payment"),
            test_peer_id(),
            500,
            content_hash(b"force-source"),
            current_timestamp(),
        );
        ops.state.settlement.enqueue(dist).unwrap();

        // Force settlement
        let batch_id = ops.force_settlement().await.unwrap();
        assert!(batch_id.is_some());

        // Verify the mock received the batch
        let batches = mock_settle.settled_batches();
        assert_eq!(batches.len(), 1);
    }

    #[tokio::test]
    async fn test_settlement_broadcasts_confirm() {
        use nodalync_test_utils::*;

        let (mut ops, mock_net, _mock_settle, _temp) = create_test_ops_with_mocks();

        // Enqueue a distribution
        let dist = QueuedDistribution::new(
            content_hash(b"broadcast-payment"),
            test_peer_id(),
            300,
            content_hash(b"broadcast-source"),
            current_timestamp(),
        );
        ops.state.settlement.enqueue(dist).unwrap();

        // Force settlement (triggers broadcast_settlement_confirm)
        let batch_id = ops.force_settlement().await.unwrap();
        assert!(batch_id.is_some());

        // MockNetwork's broadcast_settlement_confirm is a no-op that returns Ok,
        // so this test verifies the settlement path completes without errors
        // when a network is present. The settlement confirm is broadcast via
        // the network which is configured in the ops instance.
        // We can at least verify the batch was settled and no errors occurred.
        assert_eq!(mock_net.sent_message_count(), 0); // No point-to-point messages for settlement
    }
}
