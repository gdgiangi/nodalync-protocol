//! Error types for nodalync-crypto

use thiserror::Error;

/// Errors that can occur in cryptographic operations
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum CryptoError {
    /// Invalid PeerId string format
    #[error("Invalid PeerId format: {0}")]
    InvalidPeerIdFormat(String),

    /// Invalid prefix in human-readable PeerId
    #[error("Invalid PeerId prefix: expected 'ndl1', got '{0}'")]
    InvalidPeerIdPrefix(String),

    /// Invalid base58 encoding
    #[error("Invalid base58 encoding: {0}")]
    InvalidBase58(String),

    /// Invalid key length
    #[error("Invalid key length: expected {expected}, got {actual}")]
    InvalidKeyLength { expected: usize, actual: usize },

    /// Signature verification failed
    #[error("Signature verification failed")]
    SignatureVerificationFailed,
}
