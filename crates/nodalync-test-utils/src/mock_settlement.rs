//! Mock implementation of the `Settlement` trait for testing.
//!
//! Provides a configurable mock settlement layer that tracks deposits,
//! withdrawals, channels, and attestations in memory.

use async_trait::async_trait;
use nodalync_crypto::{Hash, PeerId, Signature};
use nodalync_settle::{
    AccountId, Attestation, ChannelId, SettleError, SettleResult, Settlement, SettlementStatus,
    TransactionId,
};
use nodalync_types::SettlementBatch;
use nodalync_wire::{ChannelBalances, ChannelUpdatePayload};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

struct MockSettlementInner {
    /// Contract balance (deposited into settlement contract).
    balance: u64,
    /// Account balance (on-chain HBAR).
    account_balance: u64,
    /// Record of all deposits made.
    deposits: Vec<u64>,
    /// Record of all withdrawals made.
    withdrawals: Vec<u64>,
    /// Open channels: channel_id_string -> (peer, deposit).
    channels: HashMap<String, (PeerId, u64)>,
    /// Stored attestations: content_hash -> Attestation.
    attestations: HashMap<Hash, Attestation>,
    /// Record of all settled batches.
    settled_batches: Vec<SettlementBatch>,
    /// Peer -> AccountId mappings.
    peer_accounts: HashMap<PeerId, AccountId>,
    /// Own account ID.
    own_account: AccountId,
    /// When true, all operations return TransactionFailed.
    should_fail: bool,
    /// Auto-incrementing transaction counter.
    tx_counter: u64,
}

/// A mock implementation of the `Settlement` trait for testing.
///
/// Tracks all operations in memory and returns configurable results.
/// Uses `Arc<RwLock<...>>` internally, so it is cheap to clone and
/// all clones share the same state.
#[derive(Clone)]
pub struct MockSettlement {
    inner: Arc<RwLock<MockSettlementInner>>,
}

impl Default for MockSettlement {
    fn default() -> Self {
        Self::new()
    }
}

impl MockSettlement {
    /// Create a new MockSettlement with default account ID `0.0.99999`.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(MockSettlementInner {
                balance: 0,
                account_balance: 1_000_000_000, // 10 HBAR in tinybars
                deposits: Vec::new(),
                withdrawals: Vec::new(),
                channels: HashMap::new(),
                attestations: HashMap::new(),
                settled_batches: Vec::new(),
                peer_accounts: HashMap::new(),
                own_account: AccountId::simple(99999),
                should_fail: false,
                tx_counter: 0,
            })),
        }
    }

    /// Create with a specific own account ID.
    pub fn with_account(account: AccountId) -> Self {
        let mock = Self::new();
        mock.inner.write().unwrap().own_account = account;
        mock
    }

    /// Set the contract balance.
    pub fn with_balance(self, balance: u64) -> Self {
        self.inner.write().unwrap().balance = balance;
        self
    }

    /// Set the account balance.
    pub fn with_account_balance(self, balance: u64) -> Self {
        self.inner.write().unwrap().account_balance = balance;
        self
    }

    /// Configure the mock to fail all operations.
    pub fn with_failure(self) -> Self {
        self.inner.write().unwrap().should_fail = true;
        self
    }

    /// Set the failure mode at runtime.
    pub fn set_should_fail(&self, should_fail: bool) {
        self.inner.write().unwrap().should_fail = should_fail;
    }

    // =========================================================================
    // Assertion Helpers
    // =========================================================================

    /// Get all deposits made.
    pub fn deposits(&self) -> Vec<u64> {
        self.inner.read().unwrap().deposits.clone()
    }

    /// Get all withdrawals made.
    pub fn withdrawals(&self) -> Vec<u64> {
        self.inner.read().unwrap().withdrawals.clone()
    }

    /// Get all settled batches.
    pub fn settled_batches(&self) -> Vec<SettlementBatch> {
        self.inner.read().unwrap().settled_batches.clone()
    }

    /// Get the current contract balance.
    pub fn current_balance(&self) -> u64 {
        self.inner.read().unwrap().balance
    }

    /// Get the number of open channels.
    pub fn channel_count(&self) -> usize {
        self.inner.read().unwrap().channels.len()
    }

    /// Get the number of attestations.
    pub fn attestation_count(&self) -> usize {
        self.inner.read().unwrap().attestations.len()
    }

    /// Generate the next transaction ID.
    fn next_tx_id(inner: &mut MockSettlementInner) -> TransactionId {
        inner.tx_counter += 1;
        TransactionId::new(format!("0.0.99999@mock.{}", inner.tx_counter))
    }
}

#[async_trait]
impl Settlement for MockSettlement {
    // =========================================================================
    // Balance Management
    // =========================================================================

    async fn deposit(&self, amount: u64) -> SettleResult<TransactionId> {
        let mut inner = self.inner.write().unwrap();
        if inner.should_fail {
            return Err(SettleError::transaction_failed("mock: configured to fail"));
        }
        inner.balance += amount;
        inner.deposits.push(amount);
        Ok(Self::next_tx_id(&mut inner))
    }

    async fn withdraw(&self, amount: u64) -> SettleResult<TransactionId> {
        let mut inner = self.inner.write().unwrap();
        if inner.should_fail {
            return Err(SettleError::transaction_failed("mock: configured to fail"));
        }
        if inner.balance < amount {
            return Err(SettleError::insufficient_balance(inner.balance, amount));
        }
        inner.balance -= amount;
        inner.withdrawals.push(amount);
        Ok(Self::next_tx_id(&mut inner))
    }

    async fn get_balance(&self) -> SettleResult<u64> {
        let inner = self.inner.read().unwrap();
        if inner.should_fail {
            return Err(SettleError::transaction_failed("mock: configured to fail"));
        }
        Ok(inner.balance)
    }

    async fn get_account_balance(&self) -> SettleResult<u64> {
        let inner = self.inner.read().unwrap();
        if inner.should_fail {
            return Err(SettleError::transaction_failed("mock: configured to fail"));
        }
        Ok(inner.account_balance)
    }

    // =========================================================================
    // Content Attestation
    // =========================================================================

    async fn attest(
        &self,
        content_hash: &Hash,
        provenance_root: &Hash,
    ) -> SettleResult<TransactionId> {
        let mut inner = self.inner.write().unwrap();
        if inner.should_fail {
            return Err(SettleError::transaction_failed("mock: configured to fail"));
        }
        let attestation = Attestation::new(
            *content_hash,
            inner.own_account,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
            *provenance_root,
        );
        inner.attestations.insert(*content_hash, attestation);
        Ok(Self::next_tx_id(&mut inner))
    }

    async fn get_attestation(&self, content_hash: &Hash) -> SettleResult<Option<Attestation>> {
        let inner = self.inner.read().unwrap();
        if inner.should_fail {
            return Err(SettleError::transaction_failed("mock: configured to fail"));
        }
        Ok(inner.attestations.get(content_hash).cloned())
    }

    // =========================================================================
    // Payment Channels
    // =========================================================================

    async fn open_channel(
        &self,
        channel_id: &ChannelId,
        peer: &PeerId,
        deposit: u64,
    ) -> SettleResult<TransactionId> {
        let mut inner = self.inner.write().unwrap();
        if inner.should_fail {
            return Err(SettleError::transaction_failed("mock: configured to fail"));
        }
        if inner.balance < deposit {
            return Err(SettleError::insufficient_balance(inner.balance, deposit));
        }
        inner.balance -= deposit;

        inner
            .channels
            .insert(channel_id.to_string(), (*peer, deposit));

        Ok(Self::next_tx_id(&mut inner))
    }

    async fn close_channel(
        &self,
        channel_id: &ChannelId,
        _final_state: &ChannelBalances,
        _signatures: &[Signature],
    ) -> SettleResult<TransactionId> {
        let mut inner = self.inner.write().unwrap();
        if inner.should_fail {
            return Err(SettleError::transaction_failed("mock: configured to fail"));
        }
        let key = channel_id.to_string();
        if inner.channels.remove(&key).is_none() {
            return Err(SettleError::channel_not_found(key));
        }
        Ok(Self::next_tx_id(&mut inner))
    }

    async fn dispute_channel(
        &self,
        channel_id: &ChannelId,
        _state: &ChannelUpdatePayload,
    ) -> SettleResult<TransactionId> {
        let mut inner = self.inner.write().unwrap();
        if inner.should_fail {
            return Err(SettleError::transaction_failed("mock: configured to fail"));
        }
        let key = channel_id.to_string();
        if !inner.channels.contains_key(&key) {
            return Err(SettleError::channel_not_found(key));
        }
        Ok(Self::next_tx_id(&mut inner))
    }

    async fn counter_dispute(
        &self,
        channel_id: &ChannelId,
        _better_state: &ChannelUpdatePayload,
    ) -> SettleResult<TransactionId> {
        let mut inner = self.inner.write().unwrap();
        if inner.should_fail {
            return Err(SettleError::transaction_failed("mock: configured to fail"));
        }
        let key = channel_id.to_string();
        if !inner.channels.contains_key(&key) {
            return Err(SettleError::channel_not_found(key));
        }
        Ok(Self::next_tx_id(&mut inner))
    }

    async fn resolve_dispute(&self, channel_id: &ChannelId) -> SettleResult<TransactionId> {
        let mut inner = self.inner.write().unwrap();
        if inner.should_fail {
            return Err(SettleError::transaction_failed("mock: configured to fail"));
        }
        let key = channel_id.to_string();
        if inner.channels.remove(&key).is_none() {
            return Err(SettleError::channel_not_found(key));
        }
        Ok(Self::next_tx_id(&mut inner))
    }

    // =========================================================================
    // Batch Settlement
    // =========================================================================

    async fn settle_batch(&self, batch: &SettlementBatch) -> SettleResult<TransactionId> {
        let mut inner = self.inner.write().unwrap();
        if inner.should_fail {
            return Err(SettleError::transaction_failed("mock: configured to fail"));
        }
        inner.settled_batches.push(batch.clone());
        Ok(Self::next_tx_id(&mut inner))
    }

    async fn verify_settlement(&self, _tx_id: &TransactionId) -> SettleResult<SettlementStatus> {
        let inner = self.inner.read().unwrap();
        if inner.should_fail {
            return Err(SettleError::transaction_failed("mock: configured to fail"));
        }
        Ok(SettlementStatus::confirmed(1, 1234567890000))
    }

    // =========================================================================
    // Account Management
    // =========================================================================

    fn get_own_account(&self) -> AccountId {
        self.inner.read().unwrap().own_account
    }

    fn get_account_for_peer(&self, peer: &PeerId) -> Option<AccountId> {
        self.inner.read().unwrap().peer_accounts.get(peer).copied()
    }

    fn register_peer_account(&self, peer: &PeerId, account: AccountId) {
        self.inner
            .write()
            .unwrap()
            .peer_accounts
            .insert(*peer, account);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nodalync_crypto::content_hash;

    #[tokio::test]
    async fn test_deposit_and_balance() {
        let settle = MockSettlement::new();
        settle.deposit(1000).await.unwrap();
        assert_eq!(settle.get_balance().await.unwrap(), 1000);
        assert_eq!(settle.deposits(), vec![1000]);
    }

    #[tokio::test]
    async fn test_withdraw() {
        let settle = MockSettlement::new().with_balance(5000);
        settle.withdraw(2000).await.unwrap();
        assert_eq!(settle.get_balance().await.unwrap(), 3000);
        assert_eq!(settle.withdrawals(), vec![2000]);
    }

    #[tokio::test]
    async fn test_withdraw_insufficient() {
        let settle = MockSettlement::new().with_balance(100);
        let result = settle.withdraw(200).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_attestation_roundtrip() {
        let settle = MockSettlement::new();
        let hash = content_hash(b"content");
        let prov = content_hash(b"provenance");

        settle.attest(&hash, &prov).await.unwrap();

        let att = settle.get_attestation(&hash).await.unwrap();
        assert!(att.is_some());
        assert_eq!(att.unwrap().content_hash, hash);
    }

    #[tokio::test]
    async fn test_channel_lifecycle() {
        let settle = MockSettlement::new().with_balance(10000);
        let peer = PeerId([1u8; 20]);
        let channel_id = ChannelId::new(Hash([42u8; 32]));

        // Open
        settle.open_channel(&channel_id, &peer, 5000).await.unwrap();
        assert_eq!(settle.get_balance().await.unwrap(), 5000);
        assert_eq!(settle.channel_count(), 1);

        // Close
        let balances = ChannelBalances::new(2500, 2500);
        settle
            .close_channel(&channel_id, &balances, &[])
            .await
            .unwrap();
        assert_eq!(settle.channel_count(), 0);
    }

    #[tokio::test]
    async fn test_failure_mode() {
        let settle = MockSettlement::new().with_failure();

        let result = settle.deposit(1000).await;
        assert!(result.is_err());

        let result = settle.get_balance().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_set_should_fail_runtime() {
        let settle = MockSettlement::new();

        settle.deposit(1000).await.unwrap();
        settle.set_should_fail(true);
        assert!(settle.deposit(1000).await.is_err());
        settle.set_should_fail(false);
        settle.deposit(1000).await.unwrap();
    }

    #[tokio::test]
    async fn test_verify_settlement() {
        let settle = MockSettlement::new();
        let tx = TransactionId::new("test-tx");
        let status = settle.verify_settlement(&tx).await.unwrap();
        assert!(status.is_confirmed());
    }

    #[test]
    fn test_peer_account_mapping() {
        let settle = MockSettlement::new();
        let peer = PeerId([3u8; 20]);
        let account = AccountId::simple(12345);

        assert!(settle.get_account_for_peer(&peer).is_none());
        settle.register_peer_account(&peer, account);
        assert_eq!(settle.get_account_for_peer(&peer), Some(account));
    }

    #[test]
    fn test_own_account() {
        let settle = MockSettlement::new();
        assert_eq!(settle.get_own_account(), AccountId::simple(99999));

        let settle2 = MockSettlement::with_account(AccountId::simple(42));
        assert_eq!(settle2.get_own_account(), AccountId::simple(42));
    }

    #[test]
    fn test_clone_shares_state() {
        let settle = MockSettlement::new();
        let settle2 = settle.clone();

        let peer = PeerId([4u8; 20]);
        let account = AccountId::simple(111);
        settle.register_peer_account(&peer, account);

        // Clone should see the same mapping
        assert_eq!(settle2.get_account_for_peer(&peer), Some(account));
    }
}
