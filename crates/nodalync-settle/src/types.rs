//! Core types for the settlement module.

use nodalync_crypto::{Hash, PeerId, Timestamp};
use serde::{Deserialize, Serialize};

use crate::error::{SettleError, SettleResult};

/// Hedera transaction identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TransactionId(pub String);

impl TransactionId {
    /// Create a new transaction ID.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Get the transaction ID as a string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for TransactionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for TransactionId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for TransactionId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// Hedera account identifier (format: shard.realm.num, e.g., 0.0.12345).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AccountId {
    /// Shard number (typically 0)
    pub shard: u64,
    /// Realm number (typically 0)
    pub realm: u64,
    /// Account number
    pub num: u64,
}

impl AccountId {
    /// Create a new account ID.
    pub fn new(shard: u64, realm: u64, num: u64) -> Self {
        Self { shard, realm, num }
    }

    /// Create an account ID in the default shard/realm (0.0.xxx).
    pub fn simple(num: u64) -> Self {
        Self::new(0, 0, num)
    }

    /// Parse an account ID from string (format: shard.realm.num).
    pub fn from_string(s: &str) -> SettleResult<Self> {
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() != 3 {
            return Err(SettleError::InvalidAccountId(s.to_string()));
        }

        let shard = parts[0]
            .parse()
            .map_err(|_| SettleError::InvalidAccountId(s.to_string()))?;
        let realm = parts[1]
            .parse()
            .map_err(|_| SettleError::InvalidAccountId(s.to_string()))?;
        let num = parts[2]
            .parse()
            .map_err(|_| SettleError::InvalidAccountId(s.to_string()))?;

        Ok(Self { shard, realm, num })
    }
}

impl std::fmt::Display for AccountId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.shard, self.realm, self.num)
    }
}

/// On-chain content attestation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attestation {
    /// Hash of the attested content
    pub content_hash: Hash,
    /// Owner's account ID
    pub owner: AccountId,
    /// Attestation timestamp
    pub timestamp: Timestamp,
    /// Root of the provenance tree
    pub provenance_root: Hash,
}

impl Attestation {
    /// Create a new attestation.
    pub fn new(
        content_hash: Hash,
        owner: AccountId,
        timestamp: Timestamp,
        provenance_root: Hash,
    ) -> Self {
        Self {
            content_hash,
            owner,
            timestamp,
            provenance_root,
        }
    }
}

/// Status of a settlement transaction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SettlementStatus {
    /// Transaction is pending (not yet confirmed).
    Pending,
    /// Transaction confirmed on-chain.
    Confirmed {
        /// Block/consensus timestamp
        block: u64,
        /// Confirmation timestamp
        timestamp: Timestamp,
    },
    /// Transaction failed.
    Failed {
        /// Failure reason
        reason: String,
    },
}

impl SettlementStatus {
    /// Create a confirmed status.
    pub fn confirmed(block: u64, timestamp: Timestamp) -> Self {
        Self::Confirmed { block, timestamp }
    }

    /// Create a failed status.
    pub fn failed(reason: impl Into<String>) -> Self {
        Self::Failed {
            reason: reason.into(),
        }
    }

    /// Check if the status is confirmed.
    pub fn is_confirmed(&self) -> bool {
        matches!(self, Self::Confirmed { .. })
    }

    /// Check if the status is pending.
    pub fn is_pending(&self) -> bool {
        matches!(self, Self::Pending)
    }

    /// Check if the status is failed.
    pub fn is_failed(&self) -> bool {
        matches!(self, Self::Failed { .. })
    }
}

/// On-chain channel identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChannelId(pub Hash);

impl ChannelId {
    /// Create a new channel ID.
    pub fn new(hash: Hash) -> Self {
        Self(hash)
    }

    /// Get the underlying hash.
    pub fn as_hash(&self) -> &Hash {
        &self.0
    }
}

impl From<Hash> for ChannelId {
    fn from(hash: Hash) -> Self {
        Self(hash)
    }
}

impl std::fmt::Display for ChannelId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// On-chain channel state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OnChainChannelState {
    /// Channel identifier
    pub channel_id: ChannelId,
    /// Participant 1 (initiator)
    pub participant1: AccountId,
    /// Participant 2 (responder)
    pub participant2: AccountId,
    /// Balance of participant 1
    pub balance1: u64,
    /// Balance of participant 2
    pub balance2: u64,
    /// Current nonce
    pub nonce: u64,
    /// Channel status
    pub status: OnChainChannelStatus,
}

/// On-chain channel status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OnChainChannelStatus {
    /// Channel is open and active.
    Open,
    /// Channel is in dispute period.
    Disputed {
        /// When the dispute was initiated
        dispute_start: Timestamp,
    },
    /// Channel is closed.
    Closed,
}

impl OnChainChannelStatus {
    /// Check if the channel is open.
    pub fn is_open(&self) -> bool {
        matches!(self, Self::Open)
    }

    /// Check if the channel is disputed.
    pub fn is_disputed(&self) -> bool {
        matches!(self, Self::Disputed { .. })
    }

    /// Check if the channel is closed.
    pub fn is_closed(&self) -> bool {
        matches!(self, Self::Closed)
    }
}

/// Mapping entry for PeerId to AccountId.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountMapping {
    /// The peer's off-chain identifier
    pub peer_id: PeerId,
    /// The peer's Hedera account ID
    pub account_id: AccountId,
}

impl AccountMapping {
    /// Create a new account mapping.
    pub fn new(peer_id: PeerId, account_id: AccountId) -> Self {
        Self {
            peer_id,
            account_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transaction_id() {
        let tx_id = TransactionId::new("0.0.12345@1234567890.123456789");
        assert_eq!(tx_id.as_str(), "0.0.12345@1234567890.123456789");
        assert_eq!(
            tx_id.to_string(),
            "0.0.12345@1234567890.123456789"
        );
    }

    #[test]
    fn test_account_id_simple() {
        let account = AccountId::simple(12345);
        assert_eq!(account.shard, 0);
        assert_eq!(account.realm, 0);
        assert_eq!(account.num, 12345);
        assert_eq!(account.to_string(), "0.0.12345");
    }

    #[test]
    fn test_account_id_from_string() {
        let account = AccountId::from_string("0.0.12345").unwrap();
        assert_eq!(account.num, 12345);

        let account2 = AccountId::from_string("1.2.34567").unwrap();
        assert_eq!(account2.shard, 1);
        assert_eq!(account2.realm, 2);
        assert_eq!(account2.num, 34567);
    }

    #[test]
    fn test_account_id_from_string_invalid() {
        assert!(AccountId::from_string("invalid").is_err());
        assert!(AccountId::from_string("0.0").is_err());
        assert!(AccountId::from_string("0.0.abc").is_err());
    }

    #[test]
    fn test_settlement_status() {
        let pending = SettlementStatus::Pending;
        assert!(pending.is_pending());
        assert!(!pending.is_confirmed());
        assert!(!pending.is_failed());

        let confirmed = SettlementStatus::confirmed(100, 1234567890);
        assert!(confirmed.is_confirmed());
        assert!(!confirmed.is_pending());

        let failed = SettlementStatus::failed("out of gas");
        assert!(failed.is_failed());
        assert!(!failed.is_confirmed());
    }

    #[test]
    fn test_on_chain_channel_status() {
        assert!(OnChainChannelStatus::Open.is_open());
        assert!(OnChainChannelStatus::Disputed { dispute_start: 0 }.is_disputed());
        assert!(OnChainChannelStatus::Closed.is_closed());
    }

    #[test]
    fn test_channel_id() {
        let hash = Hash([1u8; 32]);
        let channel_id = ChannelId::new(hash);
        assert_eq!(*channel_id.as_hash(), hash);
    }
}
