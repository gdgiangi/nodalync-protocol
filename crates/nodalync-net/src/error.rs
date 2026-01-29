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
}
