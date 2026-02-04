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

    /// Internal error (lock poisoning, unexpected state).
    #[error("internal error: {0}")]
    Internal(String),

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

    /// Create a new Internal error.
    pub fn internal(msg: impl Into<String>) -> Self {
        Self::Internal(msg.into())
    }

    /// Check if this error is retryable.
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::Network(_) | Self::Timeout(_))
    }
}

/// Classify an SDK error into the appropriate `SettleError` variant.
///
/// Transient errors (timeout, gRPC failures, BUSY/PLATFORM_TRANSACTION_NOT_CREATED/UNKNOWN
/// status codes) are mapped to retryable variants (`Timeout`, `Network`).
/// All other SDK errors are mapped to `HederaSdk` (non-retryable).
#[cfg(feature = "hedera-sdk")]
pub fn classify_sdk_error(error: hiero_sdk::Error) -> SettleError {
    match &error {
        hiero_sdk::Error::TimedOut(_) => SettleError::Timeout(error.to_string()),
        hiero_sdk::Error::GrpcStatus(_) => {
            SettleError::Network(format!("gRPC error (will retry): {}", error))
        }
        hiero_sdk::Error::TransactionPreCheckStatus { status, .. }
        | hiero_sdk::Error::QueryPreCheckStatus { status, .. }
        | hiero_sdk::Error::QueryPaymentPreCheckStatus { status, .. }
        | hiero_sdk::Error::QueryNoPaymentPreCheckStatus { status }
        | hiero_sdk::Error::ReceiptStatus { status, .. }
            if matches!(
                status,
                hiero_sdk::Status::Busy
                    | hiero_sdk::Status::PlatformTransactionNotCreated
                    | hiero_sdk::Status::Unknown
            ) =>
        {
            SettleError::Network(format!(
                "transient Hedera status {:?} (will retry): {}",
                status, error
            ))
        }
        _ => SettleError::HederaSdk(error.to_string()),
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

    #[test]
    fn test_hedera_sdk_not_retryable() {
        let err = SettleError::hedera_sdk("INVALID_SIGNATURE");
        assert!(!err.is_retryable());
    }

    #[test]
    fn test_transient_network_retryable() {
        let err = SettleError::network("transient Hedera status Busy");
        assert!(err.is_retryable());
    }

    #[test]
    fn test_timeout_retryable() {
        let err = SettleError::timeout("request timed out");
        assert!(err.is_retryable());
    }

    #[test]
    fn test_internal_error() {
        let err = SettleError::internal("lock poisoned");
        assert_eq!(err.to_string(), "internal error: lock poisoned");
        assert!(!err.is_retryable());
    }
}
