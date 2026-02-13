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

    /// Get the protocol error code for this error.
    pub fn error_code(&self) -> nodalync_types::ErrorCode {
        use nodalync_types::ErrorCode;
        match self {
            Self::ContentNotFound(_) => ErrorCode::NotFound,
            Self::ManifestNotFound(_) => ErrorCode::NotFound,
            Self::ChannelNotFound => ErrorCode::ChannelNotFound,
            Self::PeerNotFound => ErrorCode::PeerNotFound,
            Self::IdentityNotFound => ErrorCode::InternalError,
            Self::HashMismatch { .. } => ErrorCode::InvalidHash,
            Self::ProvenanceNotFound(_) => ErrorCode::InvalidProvenance,
            Self::CacheNotFound(_) => ErrorCode::NotFound,
            Self::InvalidData(_) => ErrorCode::InvalidManifest,
            Self::Io(_) => ErrorCode::InternalError,
            Self::Database(_) => ErrorCode::InternalError,
            Self::Serialization(_) => ErrorCode::InvalidManifest,
            Self::Encryption(_) => ErrorCode::InternalError,
            Self::Settlement(_) => ErrorCode::InternalError,
            Self::Schema(_) => ErrorCode::InternalError,
            Self::Path(_) => ErrorCode::InternalError,
            Self::LockPoisoned(_) => ErrorCode::InternalError,
        }
    }

    /// Get a user-friendly suggestion for recovering from this error.
    pub fn suggestion(&self) -> &'static str {
        match self {
            Self::ContentNotFound(_) => {
                "Content not in local store. Use 'nodalync search' to find and query it from the network."
            }
            Self::ManifestNotFound(_) => {
                "Manifest not found. The content may not have been published yet."
            }
            Self::ChannelNotFound => {
                "No payment channel found. Open one with 'nodalync channel open'."
            }
            Self::PeerNotFound => {
                "Peer not in local store. Connect to the network to discover peers."
            }
            Self::IdentityNotFound => {
                "No identity found. Run 'nodalync init' to create one."
            }
            Self::HashMismatch { .. } => {
                "Content hash doesn't match. The data may be corrupted. Re-download from the network."
            }
            Self::ProvenanceNotFound(_) => {
                "Provenance chain not found. The source content may need to be queried first."
            }
            Self::CacheNotFound(_) => {
                "Cache miss. The content will be fetched from the network on next query."
            }
            Self::Io(_) => {
                "I/O error. Check disk space, file permissions, and that the data directory is accessible."
            }
            Self::Database(_) => {
                "Database error. The database may be corrupted. Try 'nodalync repair' or restore from backup."
            }
            Self::Serialization(_) => {
                "Data serialization error. The stored data may be from an incompatible version."
            }
            Self::Encryption(_) => {
                "Encryption/decryption failed. Check that the correct key is being used."
            }
            Self::Settlement(_) => {
                "Settlement queue error. Check Hedera configuration and network connectivity."
            }
            Self::Schema(_) => {
                "Database schema error. The database may need migration. Try upgrading the CLI."
            }
            Self::InvalidData(_) => {
                "Invalid data format. The data may be corrupted or from an incompatible version."
            }
            Self::Path(_) => {
                "Invalid path. Check that the data directory exists and is writable."
            }
            Self::LockPoisoned(_) => {
                "Internal lock error. Restart the application."
            }
        }
    }

    /// Returns true if this error is transient and the operation may succeed on retry.
    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            Self::Io(_) | Self::Database(_) | Self::LockPoisoned(_) | Self::Settlement(_)
        )
    }

    /// Suggested retry delay in milliseconds for transient errors.
    ///
    /// Returns `None` for non-transient errors.
    pub fn retry_delay_ms(&self) -> Option<u64> {
        match self {
            Self::Io(_) => Some(1_000),
            Self::Database(_) => Some(500),
            Self::LockPoisoned(_) => Some(100),
            Self::Settlement(_) => Some(5_000),
            _ => None,
        }
    }

    /// Metric labels for monitoring integration.
    pub fn metric_labels(&self) -> (&'static str, &'static str) {
        match self {
            Self::ContentNotFound(_) => ("store", "content_not_found"),
            Self::ManifestNotFound(_) => ("store", "manifest_not_found"),
            Self::ChannelNotFound => ("store", "channel_not_found"),
            Self::PeerNotFound => ("store", "peer_not_found"),
            Self::IdentityNotFound => ("store", "identity_not_found"),
            Self::HashMismatch { .. } => ("store", "hash_mismatch"),
            Self::ProvenanceNotFound(_) => ("store", "provenance_not_found"),
            Self::CacheNotFound(_) => ("store", "cache_not_found"),
            Self::Io(_) => ("store", "io"),
            Self::Database(_) => ("store", "database"),
            Self::Serialization(_) => ("store", "serialization"),
            Self::Encryption(_) => ("store", "encryption"),
            Self::Settlement(_) => ("store", "settlement"),
            Self::Schema(_) => ("store", "schema"),
            Self::InvalidData(_) => ("store", "invalid_data"),
            Self::Path(_) => ("store", "path"),
            Self::LockPoisoned(_) => ("store", "lock_poisoned"),
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
    fn test_suggestion() {
        let hash = content_hash(b"test");
        let err = StoreError::ContentNotFound(hash);
        assert!(err.suggestion().contains("search"));

        let err = StoreError::IdentityNotFound;
        assert!(err.suggestion().contains("init"));

        let err = StoreError::ChannelNotFound;
        assert!(err.suggestion().contains("channel"));
    }

    #[test]
    fn test_is_transient() {
        let io_err = std::io::Error::new(std::io::ErrorKind::Other, "test");
        assert!(StoreError::Io(io_err).is_transient());
        assert!(StoreError::Settlement("test".into()).is_transient());

        let hash = content_hash(b"test");
        assert!(!StoreError::ContentNotFound(hash).is_transient());
        assert!(!StoreError::IdentityNotFound.is_transient());
    }

    #[test]
    fn test_retry_delay() {
        let io_err = std::io::Error::new(std::io::ErrorKind::Other, "test");
        assert_eq!(StoreError::Io(io_err).retry_delay_ms(), Some(1_000));
        assert_eq!(
            StoreError::Settlement("test".into()).retry_delay_ms(),
            Some(5_000)
        );

        let hash = content_hash(b"test");
        assert_eq!(StoreError::ContentNotFound(hash).retry_delay_ms(), None);
    }

    #[test]
    fn test_error_code() {
        use nodalync_types::ErrorCode;
        let hash = content_hash(b"test");
        assert_eq!(
            StoreError::ContentNotFound(hash).error_code(),
            ErrorCode::NotFound
        );
        assert_eq!(
            StoreError::ChannelNotFound.error_code(),
            ErrorCode::ChannelNotFound
        );
        assert_eq!(
            StoreError::PeerNotFound.error_code(),
            ErrorCode::PeerNotFound
        );
    }

    #[test]
    fn test_metric_labels() {
        let hash = content_hash(b"test");
        let (cat, var) = StoreError::ContentNotFound(hash).metric_labels();
        assert_eq!(cat, "store");
        assert_eq!(var, "content_not_found");

        let (cat, var) = StoreError::ChannelNotFound.metric_labels();
        assert_eq!(cat, "store");
        assert_eq!(var, "channel_not_found");
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
