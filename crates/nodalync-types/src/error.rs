//! Error types for the Nodalync protocol.
//!
//! This module defines protocol-level error codes (Appendix C) and
//! the main error type used across all Nodalync crates.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Protocol error codes from Appendix C.
///
/// These codes are used in protocol messages to communicate error conditions
/// between peers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u16)]
#[non_exhaustive]
pub enum ErrorCode {
    // =========================================================================
    // Query Errors (0x0001 - 0x00FF)
    // =========================================================================
    /// Content not found for the given hash
    NotFound = 0x0001,
    /// Access denied due to visibility or access control
    AccessDenied = 0x0002,
    /// Payment required to access this content
    PaymentRequired = 0x0003,
    /// Payment validation failed
    PaymentInvalid = 0x0004,
    /// Rate limit exceeded for this peer
    RateLimited = 0x0005,
    /// Requested version not found
    VersionNotFound = 0x0006,

    // =========================================================================
    // Channel Errors (0x0100 - 0x01FF)
    // =========================================================================
    /// Channel does not exist
    ChannelNotFound = 0x0100,
    /// Channel is already closed
    ChannelClosed = 0x0101,
    /// Insufficient balance in channel
    InsufficientBalance = 0x0102,
    /// Invalid nonce (must be greater than previous)
    InvalidNonce = 0x0103,
    /// Signature verification failed
    InvalidSignature = 0x0104,

    // =========================================================================
    // Validation Errors (0x0200 - 0x02FF)
    // =========================================================================
    /// Content hash does not match
    InvalidHash = 0x0200,
    /// Provenance chain is invalid
    InvalidProvenance = 0x0201,
    /// Version constraints violated
    InvalidVersion = 0x0202,
    /// Manifest validation failed
    InvalidManifest = 0x0203,
    /// Content exceeds size limit
    ContentTooLarge = 0x0204,

    // L2 Entity Graph Errors (0x0210 - 0x021F)
    /// L2 entity graph structure is invalid
    L2InvalidStructure = 0x0210,
    /// L2 is missing required source content
    L2MissingSource = 0x0211,
    /// L2 exceeds maximum entity count
    L2EntityLimit = 0x0212,
    /// L2 exceeds maximum relationship count
    L2RelationshipLimit = 0x0213,
    /// L2 contains invalid entity reference
    L2InvalidEntityRef = 0x0214,
    /// L2 entity graph contains a cycle
    L2CycleDetected = 0x0215,
    /// L2 contains invalid URI or CURIE
    L2InvalidUri = 0x0216,
    /// L2 content cannot be published (must remain private)
    L2CannotPublish = 0x0217,

    // =========================================================================
    // Network Errors (0x0300 - 0x03FF)
    // =========================================================================
    /// Peer not found in network
    PeerNotFound = 0x0300,
    /// Failed to establish connection
    ConnectionFailed = 0x0301,
    /// Operation timed out
    Timeout = 0x0302,

    // =========================================================================
    // Internal Errors
    // =========================================================================
    /// Internal server error
    InternalError = 0xFFFF,
}

impl ErrorCode {
    /// Returns true if this is a query-related error (0x0001-0x00FF)
    pub fn is_query_error(&self) -> bool {
        let code = *self as u16;
        (0x0001..=0x00FF).contains(&code)
    }

    /// Returns true if this is a channel-related error (0x0100-0x01FF)
    pub fn is_channel_error(&self) -> bool {
        let code = *self as u16;
        (0x0100..=0x01FF).contains(&code)
    }

    /// Returns true if this is a validation error (0x0200-0x02FF)
    pub fn is_validation_error(&self) -> bool {
        let code = *self as u16;
        (0x0200..=0x02FF).contains(&code)
    }

    /// Returns true if this is a network error (0x0300-0x03FF)
    pub fn is_network_error(&self) -> bool {
        let code = *self as u16;
        (0x0300..=0x03FF).contains(&code)
    }

    /// Get the numeric code value
    pub fn code(&self) -> u16 {
        *self as u16
    }
}

impl std::fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErrorCode::NotFound => write!(f, "NOT_FOUND"),
            ErrorCode::AccessDenied => write!(f, "ACCESS_DENIED"),
            ErrorCode::PaymentRequired => write!(f, "PAYMENT_REQUIRED"),
            ErrorCode::PaymentInvalid => write!(f, "PAYMENT_INVALID"),
            ErrorCode::RateLimited => write!(f, "RATE_LIMITED"),
            ErrorCode::VersionNotFound => write!(f, "VERSION_NOT_FOUND"),
            ErrorCode::ChannelNotFound => write!(f, "CHANNEL_NOT_FOUND"),
            ErrorCode::ChannelClosed => write!(f, "CHANNEL_CLOSED"),
            ErrorCode::InsufficientBalance => write!(f, "INSUFFICIENT_BALANCE"),
            ErrorCode::InvalidNonce => write!(f, "INVALID_NONCE"),
            ErrorCode::InvalidSignature => write!(f, "INVALID_SIGNATURE"),
            ErrorCode::InvalidHash => write!(f, "INVALID_HASH"),
            ErrorCode::InvalidProvenance => write!(f, "INVALID_PROVENANCE"),
            ErrorCode::InvalidVersion => write!(f, "INVALID_VERSION"),
            ErrorCode::InvalidManifest => write!(f, "INVALID_MANIFEST"),
            ErrorCode::ContentTooLarge => write!(f, "CONTENT_TOO_LARGE"),
            ErrorCode::L2InvalidStructure => write!(f, "L2_INVALID_STRUCTURE"),
            ErrorCode::L2MissingSource => write!(f, "L2_MISSING_SOURCE"),
            ErrorCode::L2EntityLimit => write!(f, "L2_ENTITY_LIMIT"),
            ErrorCode::L2RelationshipLimit => write!(f, "L2_RELATIONSHIP_LIMIT"),
            ErrorCode::L2InvalidEntityRef => write!(f, "L2_INVALID_ENTITY_REF"),
            ErrorCode::L2CycleDetected => write!(f, "L2_CYCLE_DETECTED"),
            ErrorCode::L2InvalidUri => write!(f, "L2_INVALID_URI"),
            ErrorCode::L2CannotPublish => write!(f, "L2_CANNOT_PUBLISH"),
            ErrorCode::PeerNotFound => write!(f, "PEER_NOT_FOUND"),
            ErrorCode::ConnectionFailed => write!(f, "CONNECTION_FAILED"),
            ErrorCode::Timeout => write!(f, "TIMEOUT"),
            ErrorCode::InternalError => write!(f, "INTERNAL_ERROR"),
        }
    }
}

/// Main error type for all Nodalync operations.
///
/// This error type is used across all Nodalync crates to provide
/// consistent error handling and reporting.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum NodalyncError {
    /// Content validation failed
    #[error("content validation failed: {0}")]
    ContentValidation(String),

    /// Provenance validation failed
    #[error("provenance validation failed: {0}")]
    ProvenanceValidation(String),

    /// Version validation failed
    #[error("version validation failed: {0}")]
    VersionValidation(String),

    /// Payment validation failed
    #[error("payment validation failed: {0}")]
    PaymentValidation(String),

    /// Message validation failed
    #[error("message validation failed: {0}")]
    MessageValidation(String),

    /// Access validation failed
    #[error("access denied: {0}")]
    AccessDenied(String),

    /// Storage operation failed
    #[error("storage error: {0}")]
    Storage(String),

    /// Network operation failed
    #[error("network error: {0}")]
    Network(String),

    /// Settlement operation failed
    #[error("settlement error: {0}")]
    Settlement(String),

    /// Cryptographic operation failed
    #[error("crypto error: {0}")]
    Crypto(String),

    /// Serialization/deserialization failed
    #[error("serialization error: {0}")]
    Serialization(String),

    /// Channel operation failed
    #[error("channel error: {0}")]
    Channel(String),

    /// Protocol error with code (for wire format)
    #[error("protocol error {code}: {message}")]
    Protocol {
        /// The error code
        code: ErrorCode,
        /// Human-readable message
        message: String,
    },

    /// Not found
    #[error("not found: {0}")]
    NotFound(String),

    /// Invalid input
    #[error("invalid input: {0}")]
    InvalidInput(String),
}

impl NodalyncError {
    /// Create a protocol error from an error code
    pub fn protocol(code: ErrorCode, message: impl Into<String>) -> Self {
        Self::Protocol {
            code,
            message: message.into(),
        }
    }

    /// Get the error code if this is a protocol error
    pub fn error_code(&self) -> Option<ErrorCode> {
        match self {
            NodalyncError::Protocol { code, .. } => Some(*code),
            NodalyncError::NotFound(_) => Some(ErrorCode::NotFound),
            NodalyncError::AccessDenied(_) => Some(ErrorCode::AccessDenied),
            NodalyncError::ContentValidation(_) => Some(ErrorCode::InvalidManifest),
            NodalyncError::ProvenanceValidation(_) => Some(ErrorCode::InvalidProvenance),
            NodalyncError::VersionValidation(_) => Some(ErrorCode::InvalidVersion),
            NodalyncError::PaymentValidation(_) => Some(ErrorCode::PaymentInvalid),
            NodalyncError::Channel(_) => Some(ErrorCode::ChannelNotFound),
            NodalyncError::Network(_) => Some(ErrorCode::ConnectionFailed),
            _ => None,
        }
    }
}

/// Result type alias for Nodalync operations
pub type Result<T> = std::result::Result<T, NodalyncError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_code_values() {
        // Query errors
        assert_eq!(ErrorCode::NotFound as u16, 0x0001);
        assert_eq!(ErrorCode::AccessDenied as u16, 0x0002);
        assert_eq!(ErrorCode::PaymentRequired as u16, 0x0003);
        assert_eq!(ErrorCode::PaymentInvalid as u16, 0x0004);
        assert_eq!(ErrorCode::RateLimited as u16, 0x0005);
        assert_eq!(ErrorCode::VersionNotFound as u16, 0x0006);

        // Channel errors
        assert_eq!(ErrorCode::ChannelNotFound as u16, 0x0100);
        assert_eq!(ErrorCode::ChannelClosed as u16, 0x0101);
        assert_eq!(ErrorCode::InsufficientBalance as u16, 0x0102);
        assert_eq!(ErrorCode::InvalidNonce as u16, 0x0103);
        assert_eq!(ErrorCode::InvalidSignature as u16, 0x0104);

        // Validation errors
        assert_eq!(ErrorCode::InvalidHash as u16, 0x0200);
        assert_eq!(ErrorCode::InvalidProvenance as u16, 0x0201);
        assert_eq!(ErrorCode::InvalidVersion as u16, 0x0202);
        assert_eq!(ErrorCode::InvalidManifest as u16, 0x0203);
        assert_eq!(ErrorCode::ContentTooLarge as u16, 0x0204);

        // L2 errors
        assert_eq!(ErrorCode::L2InvalidStructure as u16, 0x0210);
        assert_eq!(ErrorCode::L2MissingSource as u16, 0x0211);
        assert_eq!(ErrorCode::L2EntityLimit as u16, 0x0212);
        assert_eq!(ErrorCode::L2RelationshipLimit as u16, 0x0213);
        assert_eq!(ErrorCode::L2InvalidEntityRef as u16, 0x0214);
        assert_eq!(ErrorCode::L2CycleDetected as u16, 0x0215);
        assert_eq!(ErrorCode::L2InvalidUri as u16, 0x0216);
        assert_eq!(ErrorCode::L2CannotPublish as u16, 0x0217);

        // Network errors
        assert_eq!(ErrorCode::PeerNotFound as u16, 0x0300);
        assert_eq!(ErrorCode::ConnectionFailed as u16, 0x0301);
        assert_eq!(ErrorCode::Timeout as u16, 0x0302);

        // Internal error
        assert_eq!(ErrorCode::InternalError as u16, 0xFFFF);
    }

    #[test]
    fn test_error_code_categories() {
        assert!(ErrorCode::NotFound.is_query_error());
        assert!(ErrorCode::PaymentRequired.is_query_error());
        assert!(!ErrorCode::NotFound.is_channel_error());

        assert!(ErrorCode::ChannelNotFound.is_channel_error());
        assert!(ErrorCode::InsufficientBalance.is_channel_error());
        assert!(!ErrorCode::ChannelNotFound.is_query_error());

        assert!(ErrorCode::InvalidHash.is_validation_error());
        assert!(ErrorCode::InvalidProvenance.is_validation_error());
        assert!(!ErrorCode::InvalidHash.is_network_error());

        assert!(ErrorCode::PeerNotFound.is_network_error());
        assert!(ErrorCode::Timeout.is_network_error());
        assert!(!ErrorCode::Timeout.is_validation_error());

        // L2 errors are validation errors
        assert!(ErrorCode::L2InvalidStructure.is_validation_error());
        assert!(ErrorCode::L2CannotPublish.is_validation_error());
        assert!(!ErrorCode::L2InvalidStructure.is_channel_error());
    }

    #[test]
    fn test_error_code_display() {
        assert_eq!(format!("{}", ErrorCode::NotFound), "NOT_FOUND");
        assert_eq!(format!("{}", ErrorCode::AccessDenied), "ACCESS_DENIED");
        assert_eq!(format!("{}", ErrorCode::InternalError), "INTERNAL_ERROR");
    }

    #[test]
    fn test_nodalync_error_display() {
        let err = NodalyncError::ContentValidation("hash mismatch".to_string());
        assert_eq!(
            format!("{}", err),
            "content validation failed: hash mismatch"
        );

        let err = NodalyncError::protocol(ErrorCode::NotFound, "content not found");
        assert_eq!(
            format!("{}", err),
            "protocol error NOT_FOUND: content not found"
        );
    }

    #[test]
    fn test_nodalync_error_code_extraction() {
        let err = NodalyncError::protocol(ErrorCode::NotFound, "test");
        assert_eq!(err.error_code(), Some(ErrorCode::NotFound));

        let err = NodalyncError::NotFound("test".to_string());
        assert_eq!(err.error_code(), Some(ErrorCode::NotFound));

        let err = NodalyncError::Crypto("test".to_string());
        assert_eq!(err.error_code(), None);
    }

    #[test]
    fn test_error_code_serialization() {
        let code = ErrorCode::PaymentRequired;
        let json = serde_json::to_string(&code).unwrap();
        let deserialized: ErrorCode = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, code);
    }

    #[test]
    fn test_error_code_copy() {
        let code = ErrorCode::NotFound;
        let code_copy = code; // Copy
        assert_eq!(code, code_copy);
    }
}
