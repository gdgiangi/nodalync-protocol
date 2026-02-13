//! Network error types.
//!
//! This module defines all error types for the nodalync-net crate.

use thiserror::Error;

/// Network-specific errors.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum NetworkError {
    /// Transport layer error (TCP, etc.).
    #[error("transport error: {0}")]
    Transport(String),

    /// Failed to connect to peer.
    #[error("connection failed: {0}")]
    ConnectionFailed(String),

    /// Operation timed out.
    #[error("timeout: {0}")]
    Timeout(String),

    /// Peer not found in routing table.
    #[error("peer not found: {0}")]
    PeerNotFound(String),

    /// DHT operation failed.
    #[error("DHT error: {0}")]
    DhtError(String),

    /// Record not found in DHT.
    #[error("record not found for key")]
    RecordNotFound,

    /// Failed to encode message.
    #[error("encoding error: {0}")]
    Encoding(String),

    /// Failed to decode message.
    #[error("decoding error: {0}")]
    Decoding(String),

    /// Received unexpected response type.
    #[error("invalid response type: expected {expected}, got {got}")]
    InvalidResponseType { expected: String, got: String },

    /// Internal channel closed unexpectedly.
    #[error("channel closed")]
    ChannelClosed,

    /// Maximum retry attempts exceeded.
    #[error("max retries exceeded after {attempts} attempts")]
    MaxRetriesExceeded { attempts: u32 },

    /// GossipSub protocol error.
    #[error("gossipsub error: {0}")]
    GossipSubError(String),

    /// Swarm is not running.
    #[error("swarm not running")]
    SwarmNotRunning,

    /// Peer ID mapping not found.
    #[error("peer ID mapping not found for {0}")]
    PeerIdMappingNotFound(String),

    /// Already listening on address.
    #[error("already listening on {0}")]
    AlreadyListening(String),

    /// Bootstrap failed.
    #[error("bootstrap failed: {0}")]
    BootstrapFailed(String),

    /// Dial error.
    #[error("dial error: {0}")]
    DialError(String),

    /// IO error.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Payment channel required error from server.
    /// Contains server's peer IDs so client can open a channel and retry.
    #[error("payment channel required")]
    ChannelRequired {
        /// Server's Nodalync peer ID (20 bytes, for opening channel).
        nodalync_peer_id: Option<[u8; 20]>,
        /// Server's libp2p peer ID (base58 string, for dialing).
        libp2p_peer_id: Option<String>,
    },

    /// Query error returned by server.
    #[error("query error: {code:?} - {message}")]
    QueryError {
        /// Error code from server.
        code: nodalync_types::ErrorCode,
        /// Error message from server.
        message: String,
    },
}

impl NetworkError {
    /// Get the protocol error code for this error.
    pub fn error_code(&self) -> nodalync_types::ErrorCode {
        use nodalync_types::ErrorCode;
        match self {
            Self::Transport(_) => ErrorCode::ConnectionFailed,
            Self::ConnectionFailed(_) => ErrorCode::ConnectionFailed,
            Self::Timeout(_) => ErrorCode::Timeout,
            Self::PeerNotFound(_) => ErrorCode::PeerNotFound,
            Self::DhtError(_) => ErrorCode::ConnectionFailed,
            Self::RecordNotFound => ErrorCode::NotFound,
            Self::Encoding(_) | Self::Decoding(_) => ErrorCode::InvalidManifest,
            Self::InvalidResponseType { .. } => ErrorCode::InvalidManifest,
            Self::ChannelClosed => ErrorCode::ConnectionFailed,
            Self::MaxRetriesExceeded { .. } => ErrorCode::Timeout,
            Self::GossipSubError(_) => ErrorCode::ConnectionFailed,
            Self::SwarmNotRunning => ErrorCode::ConnectionFailed,
            Self::PeerIdMappingNotFound(_) => ErrorCode::PeerNotFound,
            Self::AlreadyListening(_) => ErrorCode::InternalError,
            Self::BootstrapFailed(_) => ErrorCode::ConnectionFailed,
            Self::DialError(_) => ErrorCode::ConnectionFailed,
            Self::Io(_) => ErrorCode::InternalError,
            Self::ChannelRequired { .. } => ErrorCode::ChannelNotFound,
            Self::QueryError { code, .. } => *code,
        }
    }

    /// Get a user-friendly suggestion for recovering from this error.
    pub fn suggestion(&self) -> &'static str {
        match self {
            Self::Transport(_) | Self::ConnectionFailed(_) => {
                "Check network connectivity. Verify the peer address is reachable."
            }
            Self::Timeout(_) => {
                "The operation timed out. Try again or increase the timeout. Check if the peer is online."
            }
            Self::PeerNotFound(_) | Self::PeerIdMappingNotFound(_) => {
                "Peer not found in routing table. Wait for discovery or add the peer manually."
            }
            Self::DhtError(_) => {
                "DHT operation failed. This is usually transient — retry after a few seconds."
            }
            Self::RecordNotFound => {
                "Record not found in the DHT. The content may not be published yet."
            }
            Self::Encoding(_) | Self::Decoding(_) => {
                "Message encoding/decoding failed. This may indicate a protocol version mismatch."
            }
            Self::InvalidResponseType { .. } => {
                "Received unexpected response type. Check protocol compatibility with the peer."
            }
            Self::ChannelClosed => {
                "Internal channel closed. The network event loop may have stopped. Restart the node."
            }
            Self::MaxRetriesExceeded { .. } => {
                "Maximum retries exceeded. The peer may be offline or overloaded. Try again later."
            }
            Self::GossipSubError(_) => {
                "GossipSub protocol error. This is usually transient — the mesh will self-heal."
            }
            Self::SwarmNotRunning => {
                "Network is not started. Call start_network() or enable auto-start."
            }
            Self::AlreadyListening(_) => {
                "Already listening on this address. Use a different port or stop the existing listener."
            }
            Self::BootstrapFailed(_) => {
                "Bootstrap failed. Check bootstrap node addresses and network connectivity."
            }
            Self::DialError(_) => {
                "Failed to dial peer. Check the multiaddress format and network connectivity."
            }
            Self::Io(_) => {
                "I/O error. Check file permissions and disk space."
            }
            Self::ChannelRequired { .. } => {
                "Payment channel required. Open a channel with the content provider first."
            }
            Self::QueryError { .. } => {
                "Query failed on the remote peer. Check the error code for details."
            }
        }
    }

    /// Returns true if this error is transient and the operation may succeed on retry.
    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            Self::Timeout(_)
                | Self::DhtError(_)
                | Self::MaxRetriesExceeded { .. }
                | Self::GossipSubError(_)
                | Self::ChannelClosed
                | Self::Transport(_)
                | Self::ConnectionFailed(_)
                | Self::BootstrapFailed(_)
                | Self::DialError(_)
        )
    }

    /// Suggested retry delay in milliseconds for transient errors.
    ///
    /// Returns `None` for non-transient errors. For transient errors,
    /// returns a suggested base delay (callers should add jitter/backoff).
    pub fn retry_delay_ms(&self) -> Option<u64> {
        match self {
            Self::Timeout(_) => Some(5_000),
            Self::DhtError(_) => Some(2_000),
            Self::MaxRetriesExceeded { .. } => Some(10_000),
            Self::GossipSubError(_) => Some(1_000),
            Self::ChannelClosed => Some(5_000),
            Self::Transport(_) | Self::ConnectionFailed(_) => Some(3_000),
            Self::BootstrapFailed(_) => Some(10_000),
            Self::DialError(_) => Some(3_000),
            _ => None,
        }
    }

    /// Metric labels for monitoring integration.
    ///
    /// Returns `(category, variant)` suitable for use as metric labels.
    pub fn metric_labels(&self) -> (&'static str, &'static str) {
        match self {
            Self::Transport(_) => ("network", "transport"),
            Self::ConnectionFailed(_) => ("network", "connection_failed"),
            Self::Timeout(_) => ("network", "timeout"),
            Self::PeerNotFound(_) => ("network", "peer_not_found"),
            Self::DhtError(_) => ("network", "dht_error"),
            Self::RecordNotFound => ("network", "record_not_found"),
            Self::Encoding(_) => ("network", "encoding"),
            Self::Decoding(_) => ("network", "decoding"),
            Self::InvalidResponseType { .. } => ("network", "invalid_response"),
            Self::ChannelClosed => ("network", "channel_closed"),
            Self::MaxRetriesExceeded { .. } => ("network", "max_retries"),
            Self::GossipSubError(_) => ("network", "gossipsub"),
            Self::SwarmNotRunning => ("network", "swarm_not_running"),
            Self::PeerIdMappingNotFound(_) => ("network", "peer_id_mapping"),
            Self::AlreadyListening(_) => ("network", "already_listening"),
            Self::BootstrapFailed(_) => ("network", "bootstrap_failed"),
            Self::DialError(_) => ("network", "dial_error"),
            Self::Io(_) => ("network", "io"),
            Self::ChannelRequired { .. } => ("network", "channel_required"),
            Self::QueryError { .. } => ("network", "query_error"),
        }
    }
}

/// Result type alias using NetworkError.
pub type NetworkResult<T> = Result<T, NetworkError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = NetworkError::Transport("connection reset".to_string());
        assert_eq!(format!("{}", err), "transport error: connection reset");

        let err = NetworkError::Timeout("30s elapsed".to_string());
        assert_eq!(format!("{}", err), "timeout: 30s elapsed");

        let err = NetworkError::MaxRetriesExceeded { attempts: 3 };
        assert_eq!(format!("{}", err), "max retries exceeded after 3 attempts");
    }

    #[test]
    fn test_error_debug() {
        let err = NetworkError::RecordNotFound;
        assert!(format!("{:?}", err).contains("RecordNotFound"));
    }

    #[test]
    fn test_is_transient() {
        assert!(NetworkError::Timeout("test".into()).is_transient());
        assert!(NetworkError::DhtError("test".into()).is_transient());
        assert!(NetworkError::ConnectionFailed("test".into()).is_transient());
        assert!(NetworkError::BootstrapFailed("test".into()).is_transient());
        assert!(NetworkError::MaxRetriesExceeded { attempts: 3 }.is_transient());

        assert!(!NetworkError::RecordNotFound.is_transient());
        assert!(!NetworkError::SwarmNotRunning.is_transient());
        assert!(!NetworkError::PeerNotFound("test".into()).is_transient());
        assert!(!NetworkError::Encoding("test".into()).is_transient());
    }

    #[test]
    fn test_retry_delay() {
        assert_eq!(
            NetworkError::Timeout("test".into()).retry_delay_ms(),
            Some(5_000)
        );
        assert_eq!(
            NetworkError::DhtError("test".into()).retry_delay_ms(),
            Some(2_000)
        );
        assert_eq!(NetworkError::RecordNotFound.retry_delay_ms(), None);
        assert_eq!(NetworkError::SwarmNotRunning.retry_delay_ms(), None);
    }

    #[test]
    fn test_suggestion() {
        let err = NetworkError::Timeout("test".into());
        assert!(err.suggestion().contains("timed out"));

        let err = NetworkError::SwarmNotRunning;
        assert!(err.suggestion().contains("start_network"));
    }

    #[test]
    fn test_error_code() {
        use nodalync_types::ErrorCode;
        assert_eq!(
            NetworkError::Timeout("test".into()).error_code(),
            ErrorCode::Timeout
        );
        assert_eq!(
            NetworkError::PeerNotFound("test".into()).error_code(),
            ErrorCode::PeerNotFound
        );
        assert_eq!(
            NetworkError::RecordNotFound.error_code(),
            ErrorCode::NotFound
        );
    }

    #[test]
    fn test_metric_labels() {
        let (cat, var) = NetworkError::Timeout("test".into()).metric_labels();
        assert_eq!(cat, "network");
        assert_eq!(var, "timeout");

        let (cat, var) = NetworkError::DhtError("test".into()).metric_labels();
        assert_eq!(cat, "network");
        assert_eq!(var, "dht_error");
    }
}
