//! Error types for the settlement module.

use thiserror::Error;

/// Result type alias for settlement operations.
pub type SettleResult<T> = Result<T, SettleError>;

/// Errors that can occur during settlement operations.
#[derive(Debug, Error)]
pub enum SettleError {
    /// Insufficient balance in the contract.
    #[error("insufficient balance: have {have}, need {need}")]
    InsufficientBalance {
        /// Available balance
        have: u64,
        /// Required balance
        need: u64,
    },

    /// Account not found in the mapping.
    #[error("account not found for peer: {0}")]
    AccountNotFound(String),

    /// Transaction failed on-chain.
    #[error("transaction failed: {0}")]
    TransactionFailed(String),

    /// Channel not found.
    #[error("channel not found: {0}")]
    ChannelNotFound(String),

    /// Empty batch submitted.
    #[error("cannot settle empty batch")]
    EmptyBatch,

    /// No Hedera account configured.
    #[error("no Hedera account configured for this peer")]
    NoHederaAccount,

    /// Hedera SDK error.
    #[error("Hedera SDK error: {0}")]
    HederaSdk(String),

    /// Configuration error.
    #[error("configuration error: {0}")]
    Config(String),

    /// Network error (retryable).
    #[error("network error: {0}")]
    Network(String),

    /// Timeout error (retryable).
    #[error("operation timed out: {0}")]
    Timeout(String),

    /// Invalid account ID format.
    #[error("invalid account ID format: {0}")]
    InvalidAccountId(String),

    /// Invalid transaction ID format.
    #[error("invalid transaction ID format: {0}")]
    InvalidTransactionId(String),

    /// Channel already exists.
    #[error("channel already exists: {0}")]
    ChannelAlreadyExists(String),

    /// Channel not open.
    #[error("channel is not open: {0}")]
    ChannelNotOpen(String),

    /// Dispute period not elapsed.
    #[error("dispute period has not elapsed")]
    DisputePeriodNotElapsed,

    /// Invalid state nonce (must be higher than current).
    #[error("invalid nonce: submitted {submitted}, required > {current}")]
    InvalidNonce {
        /// Submitted nonce
        submitted: u64,
        /// Current nonce on-chain
        current: u64,
    },

    /// IO error.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

impl SettleError {
    /// Create a new InsufficientBalance error.
    pub fn insufficient_balance(have: u64, need: u64) -> Self {
        Self::InsufficientBalance { have, need }
    }

    /// Create a new AccountNotFound error.
    pub fn account_not_found(peer: impl Into<String>) -> Self {
        Self::AccountNotFound(peer.into())
    }

    /// Create a new TransactionFailed error.
    pub fn transaction_failed(reason: impl Into<String>) -> Self {
        Self::TransactionFailed(reason.into())
    }

    /// Create a new ChannelNotFound error.
    pub fn channel_not_found(channel_id: impl Into<String>) -> Self {
        Self::ChannelNotFound(channel_id.into())
    }

    /// Create a new HederaSdk error.
    pub fn hedera_sdk(msg: impl Into<String>) -> Self {
        Self::HederaSdk(msg.into())
    }

    /// Create a new Config error.
    pub fn config(msg: impl Into<String>) -> Self {
        Self::Config(msg.into())
    }

    /// Create a new Network error.
    pub fn network(msg: impl Into<String>) -> Self {
        Self::Network(msg.into())
    }

    /// Create a new Timeout error.
    pub fn timeout(msg: impl Into<String>) -> Self {
        Self::Timeout(msg.into())
    }

    /// Check if this error is retryable.
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::Network(_) | Self::Timeout(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insufficient_balance() {
        let err = SettleError::insufficient_balance(100, 200);
        assert!(err.to_string().contains("100"));
        assert!(err.to_string().contains("200"));
    }

    #[test]
    fn test_is_retryable() {
        assert!(SettleError::network("connection refused").is_retryable());
        assert!(SettleError::timeout("operation timed out").is_retryable());
        assert!(!SettleError::EmptyBatch.is_retryable());
        assert!(!SettleError::NoHederaAccount.is_retryable());
    }

    #[test]
    fn test_error_display() {
        let err = SettleError::channel_not_found("ch123");
        assert_eq!(err.to_string(), "channel not found: ch123");
    }
}
