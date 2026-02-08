//! Error types for the operations layer.
//!
//! This module defines the `OpsError` enum used by all operation
//! functions in this crate.

use nodalync_crypto::Hash;
use nodalync_types::ErrorCode;
use thiserror::Error;

/// Result type for operations.
pub type OpsResult<T> = std::result::Result<T, OpsError>;

/// Result of a channel close operation.
///
/// Represents the different outcomes of attempting to close a payment channel.
#[derive(Debug, Clone)]
pub enum CloseResult {
    /// Channel was successfully closed on-chain.
    Success {
        /// The on-chain transaction ID.
        transaction_id: String,
        /// Final balances: (our balance, their balance).
        final_balances: (u64, u64),
    },
    /// Channel was closed off-chain only (no settlement layer configured).
    SuccessOffChain {
        /// Final balances: (our balance, their balance).
        final_balances: (u64, u64),
    },
    /// Peer did not respond to cooperative close request.
    ///
    /// The channel is now in pending close state. The user can either:
    /// 1. Wait for the peer to come online and retry
    /// 2. Use dispute-based close (24-hour wait)
    PeerUnresponsive {
        /// Suggestion for the user.
        suggestion: String,
    },
    /// On-chain transaction failed.
    OnChainFailed {
        /// Error message from the settlement layer.
        error: String,
    },
}

impl CloseResult {
    /// Check if the close was successful (on-chain or off-chain).
    pub fn is_success(&self) -> bool {
        matches!(
            self,
            CloseResult::Success { .. } | CloseResult::SuccessOffChain { .. }
        )
    }
}

/// Errors that can occur during protocol operations.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum OpsError {
    // =========================================================================
    // Content Errors
    // =========================================================================
    /// Content not found in storage.
    #[error("content not found: {0}")]
    NotFound(Hash),

    /// Source content was not queried before derivation.
    #[error("source not queried: {0}")]
    SourceNotQueried(Hash),

    /// Content hash mismatch.
    #[error("content hash mismatch")]
    ContentHashMismatch,

    /// Content is not an L3 (for reference_l3_as_l0).
    #[error("content is not an L3")]
    NotAnL3,

    // =========================================================================
    // Access Errors
    // =========================================================================
    /// Access denied to content.
    #[error("access denied")]
    AccessDenied,

    // =========================================================================
    // Payment Errors
    // =========================================================================
    /// Payment required to query content.
    #[error("payment required: {0}")]
    PaymentRequired(String),

    /// Payment amount is insufficient.
    #[error("payment insufficient")]
    PaymentInsufficient,

    /// Payment validation failed.
    #[error("payment validation failed: {0}")]
    PaymentValidationFailed(String),

    /// Payment channel required for paid content.
    #[error("payment channel required")]
    ChannelRequired,

    /// Payment channel required with server peer info for opening channel.
    /// This variant is returned when the server provides its peer IDs so the
    /// client can open a channel and retry.
    #[error("payment channel required (server peer info available)")]
    ChannelRequiredWithPeerInfo {
        /// Server's Nodalync peer ID (20 bytes).
        nodalync_peer_id: Option<nodalync_crypto::PeerId>,
        /// Server's libp2p peer ID (base58 string).
        libp2p_peer_id: Option<String>,
    },

    /// Insufficient balance in channel.
    #[error("insufficient channel balance")]
    InsufficientChannelBalance,

    /// Private key required for paid queries.
    #[error("private key required for paid queries")]
    PrivateKeyRequired,

    // =========================================================================
    // Channel Errors
    // =========================================================================
    /// Channel not found.
    #[error("channel not found")]
    ChannelNotFound,

    /// Channel already exists.
    #[error("channel already exists")]
    ChannelAlreadyExists,

    /// Channel is not open.
    #[error("channel not open")]
    ChannelNotOpen,

    /// Channel deposit is below the minimum required.
    #[error("channel deposit {provided} tinybars below minimum {minimum} tinybars")]
    ChannelDepositTooLow {
        /// The deposit amount provided.
        provided: u64,
        /// The minimum required deposit.
        minimum: u64,
    },

    // =========================================================================
    // Settlement Errors
    // =========================================================================
    /// Settlement failed - content will not be delivered without confirmed payment.
    #[error("settlement failed: {0}")]
    SettlementFailed(String),

    /// Settlement required for paid queries.
    #[error("settlement required: no on-chain settlement configured for paid queries")]
    SettlementRequired,

    // =========================================================================
    // Operation Errors
    // =========================================================================
    /// Invalid operation.
    #[error("invalid operation: {0}")]
    InvalidOperation(String),

    /// Manifest not found.
    #[error("manifest not found: {0}")]
    ManifestNotFound(Hash),

    // =========================================================================
    // Network Errors
    // =========================================================================
    /// Network error.
    #[error("network error: {0}")]
    Network(#[from] nodalync_net::NetworkError),

    /// Peer ID mapping not found.
    #[error("peer ID not found for libp2p peer")]
    PeerIdNotFound,

    // =========================================================================
    // Wrapped Errors
    // =========================================================================
    /// Storage error.
    #[error("store error: {0}")]
    Store(#[from] nodalync_store::StoreError),

    /// Validation error.
    #[error("validation error: {0}")]
    Validation(#[from] nodalync_valid::ValidationError),

    /// Economics error.
    #[error("econ error: {0}")]
    Econ(#[from] nodalync_econ::EconError),
}

impl OpsError {
    /// Create an invalid operation error.
    pub fn invalid_operation(msg: impl Into<String>) -> Self {
        OpsError::InvalidOperation(msg.into())
    }

    /// Create a payment required error.
    pub fn payment_required(msg: impl Into<String>) -> Self {
        OpsError::PaymentRequired(msg.into())
    }

    /// Get the protocol error code for this error.
    ///
    /// Maps operational errors to the appropriate `ErrorCode` from spec Appendix C.
    pub fn error_code(&self) -> ErrorCode {
        match self {
            // Content errors
            Self::NotFound(_) | Self::ManifestNotFound(_) => ErrorCode::NotFound,
            Self::SourceNotQueried(_) => ErrorCode::NotFound,
            Self::ContentHashMismatch => ErrorCode::InvalidHash,
            Self::NotAnL3 => ErrorCode::InvalidManifest,

            // Access errors
            Self::AccessDenied => ErrorCode::AccessDenied,

            // Payment errors
            Self::PaymentRequired(_) => ErrorCode::PaymentRequired,
            Self::PaymentInsufficient => ErrorCode::PaymentInvalid,
            Self::PaymentValidationFailed(_) => ErrorCode::PaymentInvalid,
            Self::ChannelRequired => ErrorCode::ChannelNotFound,
            Self::ChannelRequiredWithPeerInfo { .. } => ErrorCode::ChannelNotFound,
            Self::InsufficientChannelBalance => ErrorCode::InsufficientBalance,
            Self::PrivateKeyRequired => ErrorCode::PaymentInvalid,

            // Channel errors
            Self::ChannelNotFound => ErrorCode::ChannelNotFound,
            Self::ChannelAlreadyExists => ErrorCode::ChannelNotFound, // Closest match
            Self::ChannelNotOpen => ErrorCode::ChannelClosed,
            Self::ChannelDepositTooLow { .. } => ErrorCode::PaymentInvalid,

            // Settlement errors
            Self::SettlementFailed(_) => ErrorCode::InternalError,
            Self::SettlementRequired => ErrorCode::PaymentRequired,

            // Operation errors
            Self::InvalidOperation(_) => ErrorCode::InvalidManifest,

            // Network errors
            Self::Network(_) => ErrorCode::ConnectionFailed,
            Self::PeerIdNotFound => ErrorCode::PeerNotFound,

            // Wrapped errors - delegate to inner type
            Self::Validation(e) => e.error_code(),
            Self::Store(_) => ErrorCode::InternalError,
            Self::Econ(_) => ErrorCode::InternalError,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nodalync_crypto::content_hash;

    #[test]
    fn test_error_display() {
        let hash = content_hash(b"test");
        let err = OpsError::NotFound(hash);
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn test_error_constructors() {
        let err = OpsError::invalid_operation("bad op");
        assert!(matches!(err, OpsError::InvalidOperation(_)));

        let err = OpsError::payment_required("need funds");
        assert!(matches!(err, OpsError::PaymentRequired(_)));
    }

    #[test]
    fn test_error_code_mapping() {
        let hash = content_hash(b"test");

        // Content errors
        assert_eq!(OpsError::NotFound(hash).error_code(), ErrorCode::NotFound);
        assert_eq!(
            OpsError::ContentHashMismatch.error_code(),
            ErrorCode::InvalidHash
        );

        // Access errors
        assert_eq!(OpsError::AccessDenied.error_code(), ErrorCode::AccessDenied);

        // Payment errors
        assert_eq!(
            OpsError::PaymentRequired("test".into()).error_code(),
            ErrorCode::PaymentRequired
        );
        assert_eq!(
            OpsError::PaymentInsufficient.error_code(),
            ErrorCode::PaymentInvalid
        );
        assert_eq!(
            OpsError::InsufficientChannelBalance.error_code(),
            ErrorCode::InsufficientBalance
        );

        // Channel errors
        assert_eq!(
            OpsError::ChannelNotFound.error_code(),
            ErrorCode::ChannelNotFound
        );
        assert_eq!(
            OpsError::ChannelNotOpen.error_code(),
            ErrorCode::ChannelClosed
        );

        // Network errors
        assert_eq!(
            OpsError::PeerIdNotFound.error_code(),
            ErrorCode::PeerNotFound
        );
    }
}
