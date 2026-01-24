//! Error types for the operations layer.
//!
//! This module defines the `OpsError` enum used by all operation
//! functions in this crate.

use nodalync_crypto::Hash;
use thiserror::Error;

/// Result type for operations.
pub type OpsResult<T> = std::result::Result<T, OpsError>;

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

    /// Insufficient balance in channel.
    #[error("insufficient channel balance")]
    InsufficientChannelBalance,

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
}
