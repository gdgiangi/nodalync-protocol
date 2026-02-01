//! Settlement trait definition.

use async_trait::async_trait;
use nodalync_crypto::{Hash, PeerId, Signature};
use nodalync_types::SettlementBatch;
use nodalync_wire::{ChannelBalances, ChannelUpdatePayload};

use crate::error::SettleResult;
use crate::types::{AccountId, Attestation, ChannelId, SettlementStatus, TransactionId};

/// Trait for on-chain settlement operations.
///
/// This trait abstracts the blockchain settlement layer, allowing for:
/// - A real Hedera implementation for production
/// - A mock implementation for testing
///
/// All methods are async and return `SettleResult`.
#[async_trait]
pub trait Settlement: Send + Sync {
    // =========================================================================
    // Balance Management
    // =========================================================================

    /// Deposit tokens into the settlement contract.
    ///
    /// Transfers `amount` from the operator's account to the contract.
    async fn deposit(&self, amount: u64) -> SettleResult<TransactionId>;

    /// Withdraw tokens from the settlement contract.
    ///
    /// Transfers `amount` from the contract back to the operator's account.
    async fn withdraw(&self, amount: u64) -> SettleResult<TransactionId>;

    /// Get the current balance in the settlement contract.
    async fn get_balance(&self) -> SettleResult<u64>;

    // =========================================================================
    // Content Attestation
    // =========================================================================

    /// Create an on-chain attestation for content.
    ///
    /// Records the content hash and provenance root on-chain for
    /// permanent proof of ownership and attribution.
    async fn attest(
        &self,
        content_hash: &Hash,
        provenance_root: &Hash,
    ) -> SettleResult<TransactionId>;

    /// Get an existing attestation by content hash.
    ///
    /// Returns `None` if no attestation exists.
    async fn get_attestation(&self, content_hash: &Hash) -> SettleResult<Option<Attestation>>;

    // =========================================================================
    // Payment Channels
    // =========================================================================

    /// Open a new payment channel with a peer.
    ///
    /// Creates an on-chain channel with the specified initial deposit.
    async fn open_channel(&self, peer: &PeerId, deposit: u64) -> SettleResult<ChannelId>;

    /// Cooperatively close a payment channel.
    ///
    /// Requires signatures from both parties agreeing to the final state.
    async fn close_channel(
        &self,
        channel_id: &ChannelId,
        final_state: &ChannelBalances,
        signatures: &[Signature],
    ) -> SettleResult<TransactionId>;

    /// Initiate a dispute on a channel.
    ///
    /// Submits the claimed state to start the dispute period.
    async fn dispute_channel(
        &self,
        channel_id: &ChannelId,
        state: &ChannelUpdatePayload,
    ) -> SettleResult<TransactionId>;

    /// Submit a counter-dispute with a higher nonce state.
    ///
    /// If you have a more recent state (higher nonce), submit it to win the dispute.
    async fn counter_dispute(
        &self,
        channel_id: &ChannelId,
        better_state: &ChannelUpdatePayload,
    ) -> SettleResult<TransactionId>;

    /// Resolve a dispute after the dispute period has elapsed.
    ///
    /// Settles the channel using the highest nonce state submitted.
    async fn resolve_dispute(&self, channel_id: &ChannelId) -> SettleResult<TransactionId>;

    // =========================================================================
    // Batch Settlement
    // =========================================================================

    /// Settle a batch of payments on-chain.
    ///
    /// This is the core settlement operation. It:
    /// 1. Validates the batch
    /// 2. Encodes the entries
    /// 3. Submits the settlement transaction
    /// 4. Distributes funds to ALL recipients in the batch
    ///
    /// Returns the transaction ID for verification.
    async fn settle_batch(&self, batch: &SettlementBatch) -> SettleResult<TransactionId>;

    /// Verify the status of a settlement transaction.
    ///
    /// Checks the on-chain status of a previously submitted transaction.
    async fn verify_settlement(&self, tx_id: &TransactionId) -> SettleResult<SettlementStatus>;

    // =========================================================================
    // Account Management
    // =========================================================================

    /// Get our own Hedera account ID.
    ///
    /// Returns the account ID used for settlement operations.
    fn get_own_account(&self) -> AccountId;

    /// Get our own Hedera account ID as a string (e.g., "0.0.12345").
    ///
    /// Convenience method for including in protocol messages.
    fn get_own_account_string(&self) -> String {
        self.get_own_account().to_string()
    }

    /// Get the Hedera account ID for a peer.
    ///
    /// Returns `None` if no account is mapped for this peer.
    fn get_account_for_peer(&self, peer: &PeerId) -> Option<AccountId>;

    /// Register a Hedera account for a peer.
    ///
    /// Associates a PeerId with a Hedera AccountId for settlement.
    /// Uses interior mutability (RwLock) for thread-safe updates.
    fn register_peer_account(&self, peer: &PeerId, account: AccountId);
}

#[cfg(test)]
mod tests {
    // Trait-level tests are in mock.rs
}
