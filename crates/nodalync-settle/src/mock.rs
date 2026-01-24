//! Mock settlement implementation for testing.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;

use async_trait::async_trait;
use nodalync_crypto::{Hash, PeerId, Signature, Timestamp};
use nodalync_types::SettlementBatch;
use nodalync_wire::{ChannelBalances, ChannelUpdatePayload};

use crate::account_mapping::AccountMapper;
use crate::error::{SettleError, SettleResult};
use crate::traits::Settlement;
use crate::types::{
    AccountId, Attestation, ChannelId, OnChainChannelState, OnChainChannelStatus,
    SettlementStatus, TransactionId,
};

/// Mock settlement implementation for testing.
///
/// This implementation maintains in-memory state without any
/// network calls, making it suitable for unit and integration tests.
pub struct MockSettlement {
    /// Operator's balance in the contract
    balance: AtomicU64,
    /// Content attestations
    attestations: RwLock<HashMap<Hash, Attestation>>,
    /// Payment channels
    channels: RwLock<HashMap<ChannelId, OnChainChannelState>>,
    /// Settled batches
    settled_batches: RwLock<HashMap<Hash, SettlementStatus>>,
    /// Transaction counter for generating IDs
    tx_counter: AtomicU64,
    /// Account mapping
    account_mapper: RwLock<AccountMapper>,
    /// Operator's account ID
    operator_account: AccountId,
    /// Simulated current timestamp
    current_time: AtomicU64,
}

impl MockSettlement {
    /// Create a new mock settlement.
    pub fn new(operator_account: AccountId) -> Self {
        Self {
            balance: AtomicU64::new(0),
            attestations: RwLock::new(HashMap::new()),
            channels: RwLock::new(HashMap::new()),
            settled_batches: RwLock::new(HashMap::new()),
            tx_counter: AtomicU64::new(1),
            account_mapper: RwLock::new(AccountMapper::new()),
            operator_account,
            current_time: AtomicU64::new(1_700_000_000_000), // Some reasonable timestamp
        }
    }

    /// Create with an initial balance.
    pub fn with_balance(operator_account: AccountId, balance: u64) -> Self {
        let mock = Self::new(operator_account);
        mock.balance.store(balance, Ordering::SeqCst);
        mock
    }

    /// Generate a unique transaction ID.
    fn next_tx_id(&self) -> TransactionId {
        let counter = self.tx_counter.fetch_add(1, Ordering::SeqCst);
        TransactionId::new(format!(
            "{}@{}.{}",
            self.operator_account,
            self.current_time.load(Ordering::SeqCst) / 1000,
            counter
        ))
    }

    /// Get the current simulated time.
    pub fn current_time(&self) -> Timestamp {
        self.current_time.load(Ordering::SeqCst)
    }

    /// Advance the simulated time.
    pub fn advance_time(&self, millis: u64) {
        self.current_time.fetch_add(millis, Ordering::SeqCst);
    }

    /// Set the simulated time.
    pub fn set_time(&self, time: Timestamp) {
        self.current_time.store(time, Ordering::SeqCst);
    }

    /// Get the number of settled batches.
    pub fn settled_batch_count(&self) -> usize {
        self.settled_batches.read().unwrap().len()
    }

    /// Get the number of attestations.
    pub fn attestation_count(&self) -> usize {
        self.attestations.read().unwrap().len()
    }

    /// Get the number of open channels.
    pub fn open_channel_count(&self) -> usize {
        self.channels
            .read()
            .unwrap()
            .values()
            .filter(|c| c.status.is_open())
            .count()
    }

    /// Check if a batch was settled.
    pub fn is_batch_settled(&self, batch_id: &Hash) -> bool {
        self.settled_batches
            .read()
            .unwrap()
            .get(batch_id)
            .map(|s| s.is_confirmed())
            .unwrap_or(false)
    }
}

impl Default for MockSettlement {
    fn default() -> Self {
        Self::new(AccountId::simple(12345))
    }
}

#[async_trait]
impl Settlement for MockSettlement {
    async fn deposit(&self, amount: u64) -> SettleResult<TransactionId> {
        self.balance.fetch_add(amount, Ordering::SeqCst);
        Ok(self.next_tx_id())
    }

    async fn withdraw(&self, amount: u64) -> SettleResult<TransactionId> {
        let current = self.balance.load(Ordering::SeqCst);
        if current < amount {
            return Err(SettleError::insufficient_balance(current, amount));
        }
        self.balance.fetch_sub(amount, Ordering::SeqCst);
        Ok(self.next_tx_id())
    }

    async fn get_balance(&self) -> SettleResult<u64> {
        Ok(self.balance.load(Ordering::SeqCst))
    }

    async fn attest(
        &self,
        content_hash: &Hash,
        provenance_root: &Hash,
    ) -> SettleResult<TransactionId> {
        let attestation = Attestation::new(
            *content_hash,
            self.operator_account,
            self.current_time(),
            *provenance_root,
        );

        self.attestations
            .write()
            .unwrap()
            .insert(*content_hash, attestation);

        Ok(self.next_tx_id())
    }

    async fn get_attestation(&self, content_hash: &Hash) -> SettleResult<Option<Attestation>> {
        Ok(self.attestations.read().unwrap().get(content_hash).cloned())
    }

    async fn open_channel(&self, peer: &PeerId, deposit: u64) -> SettleResult<ChannelId> {
        // Check balance
        let current = self.balance.load(Ordering::SeqCst);
        if current < deposit {
            return Err(SettleError::insufficient_balance(current, deposit));
        }

        // Get peer's account
        let peer_account = self
            .account_mapper
            .read()
            .unwrap()
            .require_account(peer)?;

        // Generate channel ID
        let channel_id = ChannelId::new(nodalync_crypto::content_hash(&[
            &self.operator_account.num.to_be_bytes()[..],
            &peer_account.num.to_be_bytes()[..],
            &self.tx_counter.load(Ordering::SeqCst).to_be_bytes()[..],
        ].concat()));

        // Check channel doesn't exist
        if self.channels.read().unwrap().contains_key(&channel_id) {
            return Err(SettleError::ChannelAlreadyExists(channel_id.to_string()));
        }

        // Deduct deposit
        self.balance.fetch_sub(deposit, Ordering::SeqCst);

        // Create channel state
        let channel = OnChainChannelState {
            channel_id: channel_id.clone(),
            participant1: self.operator_account,
            participant2: peer_account,
            balance1: deposit,
            balance2: 0,
            nonce: 0,
            status: OnChainChannelStatus::Open,
        };

        self.channels
            .write()
            .unwrap()
            .insert(channel_id.clone(), channel);

        Ok(channel_id)
    }

    async fn close_channel(
        &self,
        channel_id: &ChannelId,
        final_state: &ChannelBalances,
        _signatures: &[Signature],
    ) -> SettleResult<TransactionId> {
        let mut channels = self.channels.write().unwrap();

        let channel = channels
            .get_mut(channel_id)
            .ok_or_else(|| SettleError::channel_not_found(channel_id.to_string()))?;

        if !channel.status.is_open() {
            return Err(SettleError::ChannelNotOpen(channel_id.to_string()));
        }

        // Update balances and close
        channel.balance1 = final_state.initiator;
        channel.balance2 = final_state.responder;
        channel.status = OnChainChannelStatus::Closed;

        // Return balances to participants' contract balances
        self.balance
            .fetch_add(final_state.initiator, Ordering::SeqCst);

        Ok(self.next_tx_id())
    }

    async fn dispute_channel(
        &self,
        channel_id: &ChannelId,
        state: &ChannelUpdatePayload,
    ) -> SettleResult<TransactionId> {
        let mut channels = self.channels.write().unwrap();

        let channel = channels
            .get_mut(channel_id)
            .ok_or_else(|| SettleError::channel_not_found(channel_id.to_string()))?;

        if channel.status.is_closed() {
            return Err(SettleError::ChannelNotOpen(channel_id.to_string()));
        }

        // Update to disputed state
        channel.nonce = state.nonce;
        channel.balance1 = state.balances.initiator;
        channel.balance2 = state.balances.responder;
        channel.status = OnChainChannelStatus::Disputed {
            dispute_start: self.current_time(),
        };

        Ok(self.next_tx_id())
    }

    async fn counter_dispute(
        &self,
        channel_id: &ChannelId,
        better_state: &ChannelUpdatePayload,
    ) -> SettleResult<TransactionId> {
        let mut channels = self.channels.write().unwrap();

        let channel = channels
            .get_mut(channel_id)
            .ok_or_else(|| SettleError::channel_not_found(channel_id.to_string()))?;

        if !channel.status.is_disputed() {
            return Err(SettleError::ChannelNotOpen(channel_id.to_string()));
        }

        // Check nonce is higher
        if better_state.nonce <= channel.nonce {
            return Err(SettleError::InvalidNonce {
                submitted: better_state.nonce,
                current: channel.nonce,
            });
        }

        // Update state with better nonce
        channel.nonce = better_state.nonce;
        channel.balance1 = better_state.balances.initiator;
        channel.balance2 = better_state.balances.responder;

        Ok(self.next_tx_id())
    }

    async fn resolve_dispute(&self, channel_id: &ChannelId) -> SettleResult<TransactionId> {
        let mut channels = self.channels.write().unwrap();

        let channel = channels
            .get_mut(channel_id)
            .ok_or_else(|| SettleError::channel_not_found(channel_id.to_string()))?;

        match channel.status {
            OnChainChannelStatus::Disputed { dispute_start } => {
                // Check dispute period elapsed (24 hours)
                const DISPUTE_PERIOD: u64 = 24 * 60 * 60 * 1000; // 24 hours in ms
                if self.current_time() < dispute_start + DISPUTE_PERIOD {
                    return Err(SettleError::DisputePeriodNotElapsed);
                }

                // Close the channel with current state
                channel.status = OnChainChannelStatus::Closed;

                // Return balance to operator
                self.balance.fetch_add(channel.balance1, Ordering::SeqCst);

                Ok(self.next_tx_id())
            }
            _ => Err(SettleError::ChannelNotOpen(channel_id.to_string())),
        }
    }

    async fn settle_batch(&self, batch: &SettlementBatch) -> SettleResult<TransactionId> {
        if batch.is_empty() {
            return Err(SettleError::EmptyBatch);
        }

        // Check we have enough balance
        let total = batch.total_amount();
        let current = self.balance.load(Ordering::SeqCst);
        if current < total {
            return Err(SettleError::insufficient_balance(current, total));
        }

        // Verify all recipients have accounts
        let mapper = self.account_mapper.read().unwrap();
        for entry in &batch.entries {
            if mapper.get_account(&entry.recipient).is_none() {
                return Err(SettleError::account_not_found(format!(
                    "{}",
                    entry.recipient
                )));
            }
        }
        drop(mapper);

        // Deduct total amount
        self.balance.fetch_sub(total, Ordering::SeqCst);

        // Record settlement
        let tx_id = self.next_tx_id();
        let status = SettlementStatus::confirmed(
            self.tx_counter.load(Ordering::SeqCst),
            self.current_time(),
        );

        self.settled_batches
            .write()
            .unwrap()
            .insert(batch.batch_id, status);

        Ok(tx_id)
    }

    async fn verify_settlement(&self, tx_id: &TransactionId) -> SettleResult<SettlementStatus> {
        // In mock, we just check if a batch was settled
        // A more sophisticated mock could track tx_id -> status mapping
        let batches = self.settled_batches.read().unwrap();

        // Find any batch that matches the transaction timestamp
        for status in batches.values() {
            if let SettlementStatus::Confirmed { block, .. } = status {
                // Simple check: if the tx counter in the ID matches
                if tx_id.as_str().contains(&format!(".{}", block)) {
                    return Ok(status.clone());
                }
            }
        }

        Ok(SettlementStatus::Pending)
    }

    fn get_account_for_peer(&self, peer: &PeerId) -> Option<AccountId> {
        self.account_mapper.read().unwrap().get_account(peer)
    }

    fn register_peer_account(&mut self, peer: &PeerId, account: AccountId) {
        self.account_mapper.write().unwrap().register(peer, account);
    }
}

/// Builder for creating mock settlements with specific state.
pub struct MockSettlementBuilder {
    mock: MockSettlement,
}

impl MockSettlementBuilder {
    /// Create a new builder with default operator account.
    pub fn new() -> Self {
        Self {
            mock: MockSettlement::default(),
        }
    }

    /// Set the operator account.
    pub fn operator(mut self, account: AccountId) -> Self {
        self.mock.operator_account = account;
        self
    }

    /// Set the initial balance.
    pub fn balance(self, amount: u64) -> Self {
        self.mock.balance.store(amount, Ordering::SeqCst);
        self
    }

    /// Register a peer account mapping.
    pub fn peer_account(mut self, peer: PeerId, account: AccountId) -> Self {
        self.mock.register_peer_account(&peer, account);
        self
    }

    /// Set the initial time.
    pub fn time(self, timestamp: Timestamp) -> Self {
        self.mock.set_time(timestamp);
        self
    }

    /// Build the mock settlement.
    pub fn build(self) -> MockSettlement {
        self.mock
    }
}

impl Default for MockSettlementBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nodalync_crypto::{content_hash, generate_identity, peer_id_from_public_key};
    use nodalync_types::SettlementEntry;

    fn test_peer_id() -> PeerId {
        let (_, public_key) = generate_identity();
        peer_id_from_public_key(&public_key)
    }

    fn test_batch(recipients: &[(PeerId, u64)]) -> SettlementBatch {
        let entries: Vec<SettlementEntry> = recipients
            .iter()
            .map(|(peer, amount)| SettlementEntry::new(*peer, *amount, vec![], vec![]))
            .collect();

        SettlementBatch::new(
            content_hash(b"batch"),
            entries,
            content_hash(b"merkle"),
        )
    }

    #[tokio::test]
    async fn test_deposit_withdraw() {
        let mock = MockSettlement::default();

        // Deposit
        let tx = mock.deposit(1000).await.unwrap();
        assert!(!tx.as_str().is_empty());
        assert_eq!(mock.get_balance().await.unwrap(), 1000);

        // Withdraw
        mock.withdraw(300).await.unwrap();
        assert_eq!(mock.get_balance().await.unwrap(), 700);
    }

    #[tokio::test]
    async fn test_withdraw_insufficient() {
        let mock = MockSettlement::with_balance(AccountId::simple(1), 100);

        let result = mock.withdraw(200).await;
        assert!(matches!(
            result,
            Err(SettleError::InsufficientBalance { have: 100, need: 200 })
        ));
    }

    #[tokio::test]
    async fn test_attest_and_get() {
        let mock = MockSettlement::default();
        let hash = content_hash(b"my content");
        let prov_root = content_hash(b"provenance");

        mock.attest(&hash, &prov_root).await.unwrap();

        let attestation = mock.get_attestation(&hash).await.unwrap();
        assert!(attestation.is_some());

        let att = attestation.unwrap();
        assert_eq!(att.content_hash, hash);
        assert_eq!(att.provenance_root, prov_root);
    }

    #[tokio::test]
    async fn test_settle_batch() {
        let peer1 = test_peer_id();
        let peer2 = test_peer_id();

        let mock = MockSettlementBuilder::new()
            .balance(1000)
            .peer_account(peer1, AccountId::simple(111))
            .peer_account(peer2, AccountId::simple(222))
            .build();

        let batch = test_batch(&[(peer1, 300), (peer2, 200)]);
        let tx = mock.settle_batch(&batch).await.unwrap();

        assert!(!tx.as_str().is_empty());
        assert!(mock.is_batch_settled(&batch.batch_id));
        assert_eq!(mock.get_balance().await.unwrap(), 500);
    }

    #[tokio::test]
    async fn test_settle_batch_empty() {
        let mock = MockSettlement::with_balance(AccountId::simple(1), 1000);
        let batch = SettlementBatch::default();

        let result = mock.settle_batch(&batch).await;
        assert!(matches!(result, Err(SettleError::EmptyBatch)));
    }

    #[tokio::test]
    async fn test_settle_batch_insufficient_balance() {
        let peer = test_peer_id();
        let mock = MockSettlementBuilder::new()
            .balance(100)
            .peer_account(peer, AccountId::simple(111))
            .build();

        let batch = test_batch(&[(peer, 500)]);
        let result = mock.settle_batch(&batch).await;

        assert!(matches!(
            result,
            Err(SettleError::InsufficientBalance { .. })
        ));
    }

    #[tokio::test]
    async fn test_settle_batch_missing_account() {
        let peer = test_peer_id();
        let mock = MockSettlement::with_balance(AccountId::simple(1), 1000);

        let batch = test_batch(&[(peer, 100)]);
        let result = mock.settle_batch(&batch).await;

        assert!(matches!(result, Err(SettleError::AccountNotFound(_))));
    }

    #[tokio::test]
    async fn test_channel_lifecycle() {
        let peer = test_peer_id();
        let mock = MockSettlementBuilder::new()
            .balance(1000)
            .peer_account(peer, AccountId::simple(222))
            .build();

        // Open channel
        let channel_id = mock.open_channel(&peer, 500).await.unwrap();
        assert_eq!(mock.get_balance().await.unwrap(), 500);
        assert_eq!(mock.open_channel_count(), 1);

        // Close channel
        let final_balances = ChannelBalances::new(400, 100);
        mock.close_channel(&channel_id, &final_balances, &[])
            .await
            .unwrap();

        assert_eq!(mock.get_balance().await.unwrap(), 900); // 500 + 400 returned
        assert_eq!(mock.open_channel_count(), 0);
    }

    #[tokio::test]
    async fn test_dispute_and_resolve() {
        let peer = test_peer_id();
        let mock = MockSettlementBuilder::new()
            .balance(1000)
            .peer_account(peer, AccountId::simple(222))
            .build();

        // Open channel
        let channel_id = mock.open_channel(&peer, 500).await.unwrap();

        // Create a channel update payload for dispute
        let state = ChannelUpdatePayload {
            channel_id: *channel_id.as_hash(),
            nonce: 5,
            balances: ChannelBalances::new(300, 200),
            payments: vec![],
            signature: Signature::from_bytes([0u8; 64]),
        };

        // Initiate dispute
        mock.dispute_channel(&channel_id, &state).await.unwrap();

        // Try to resolve too early
        let result = mock.resolve_dispute(&channel_id).await;
        assert!(matches!(result, Err(SettleError::DisputePeriodNotElapsed)));

        // Advance time past dispute period (24 hours)
        mock.advance_time(24 * 60 * 60 * 1000 + 1);

        // Now resolve should work
        mock.resolve_dispute(&channel_id).await.unwrap();
        assert_eq!(mock.get_balance().await.unwrap(), 800); // 500 + 300 returned
    }

    #[tokio::test]
    async fn test_counter_dispute() {
        let peer = test_peer_id();
        let mock = MockSettlementBuilder::new()
            .balance(1000)
            .peer_account(peer, AccountId::simple(222))
            .build();

        let channel_id = mock.open_channel(&peer, 500).await.unwrap();

        // Initial dispute with nonce 5
        let state = ChannelUpdatePayload {
            channel_id: *channel_id.as_hash(),
            nonce: 5,
            balances: ChannelBalances::new(300, 200),
            payments: vec![],
            signature: Signature::from_bytes([0u8; 64]),
        };
        mock.dispute_channel(&channel_id, &state).await.unwrap();

        // Counter with higher nonce
        let better_state = ChannelUpdatePayload {
            channel_id: *channel_id.as_hash(),
            nonce: 10,
            balances: ChannelBalances::new(400, 100),
            payments: vec![],
            signature: Signature::from_bytes([0u8; 64]),
        };
        mock.counter_dispute(&channel_id, &better_state).await.unwrap();

        // Counter with lower nonce should fail
        let worse_state = ChannelUpdatePayload {
            channel_id: *channel_id.as_hash(),
            nonce: 8,
            balances: ChannelBalances::new(350, 150),
            payments: vec![],
            signature: Signature::from_bytes([0u8; 64]),
        };
        let result = mock.counter_dispute(&channel_id, &worse_state).await;
        assert!(matches!(result, Err(SettleError::InvalidNonce { .. })));
    }
}
