//! Error types for the storage layer.
//!
//! This module defines the error types used throughout the nodalync-store crate.

use nodalync_crypto::Hash;
use thiserror::Error;

/// Result type alias for store operations.
pub type Result<T> = std::result::Result<T, StoreError>;

/// Errors that can occur during storage operations.
#[derive(Debug, Error)]
pub enum StoreError {
    /// I/O error during file operations.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Database error from SQLite.
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    /// JSON serialization/deserialization error.
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Content not found in store.
    #[error("Content not found: {0}")]
    ContentNotFound(Hash),

    /// Manifest not found in store.
    #[error("Manifest not found: {0}")]
    ManifestNotFound(Hash),

    /// Channel not found for peer.
    #[error("Channel not found for peer")]
    ChannelNotFound,

    /// Peer not found in store.
    #[error("Peer not found")]
    PeerNotFound,

    /// Identity not found (no stored keypair).
    #[error("Identity not found")]
    IdentityNotFound,

    /// Encryption/decryption error.
    #[error("Encryption error: {0}")]
    Encryption(String),

    /// Hash mismatch during verification.
    #[error("Hash mismatch: expected {expected}, got {got}")]
    HashMismatch { expected: Hash, got: Hash },

    /// Provenance entry not found.
    #[error("Provenance not found for hash: {0}")]
    ProvenanceNotFound(Hash),

    /// Cache entry not found.
    #[error("Cache entry not found: {0}")]
    CacheNotFound(Hash),

    /// Settlement queue error.
    #[error("Settlement error: {0}")]
    Settlement(String),

    /// Schema initialization error.
    #[error("Schema error: {0}")]
    Schema(String),

    /// Invalid data format.
    #[error("Invalid data: {0}")]
    InvalidData(String),

    /// Path error.
    #[error("Path error: {0}")]
    Path(String),

    /// Lock poisoning error.
    #[error("lock poisoned: {0}")]
    LockPoisoned(String),
}

impl StoreError {
    /// Create an encryption error.
    pub fn encryption(msg: impl Into<String>) -> Self {
        StoreError::Encryption(msg.into())
    }

    /// Create a settlement error.
    pub fn settlement(msg: impl Into<String>) -> Self {
        StoreError::Settlement(msg.into())
    }

    /// Create a schema error.
    pub fn schema(msg: impl Into<String>) -> Self {
        StoreError::Schema(msg.into())
    }

    /// Create an invalid data error.
    pub fn invalid_data(msg: impl Into<String>) -> Self {
        StoreError::InvalidData(msg.into())
    }

    /// Create a path error.
    pub fn path(msg: impl Into<String>) -> Self {
        StoreError::Path(msg.into())
    }

    /// Create a lock poisoned error.
    pub fn lock_poisoned(msg: impl Into<String>) -> Self {
        StoreError::LockPoisoned(msg.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nodalync_crypto::content_hash;

    #[test]
    fn test_error_display() {
        let hash = content_hash(b"test");
        let err = StoreError::ContentNotFound(hash);
        let msg = err.to_string();
        assert!(msg.contains("Content not found"));
    }

    #[test]
    fn test_hash_mismatch_error() {
        let expected = content_hash(b"expected");
        let got = content_hash(b"got");
        let err = StoreError::HashMismatch { expected, got };
        let msg = err.to_string();
        assert!(msg.contains("Hash mismatch"));
    }

    #[test]
    fn test_error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let store_err: StoreError = io_err.into();
        assert!(matches!(store_err, StoreError::Io(_)));
    }

    #[test]
    fn test_error_constructors() {
        let err = StoreError::encryption("bad key");
        assert!(matches!(err, StoreError::Encryption(_)));

        let err = StoreError::settlement("batch failed");
        assert!(matches!(err, StoreError::Settlement(_)));

        let err = StoreError::schema("missing table");
        assert!(matches!(err, StoreError::Schema(_)));
    }
}
