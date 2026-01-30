//! Validation error types for the Nodalync protocol.
//!
//! This module defines the `ValidationError` enum used by all validation
//! functions in this crate. Each variant corresponds to a specific type
//! of validation failure as defined in Protocol Specification §9.

use thiserror::Error;

/// Errors that can occur during validation.
///
/// Each variant includes a descriptive message explaining the validation failure.
/// These errors map to the appropriate `ErrorCode` values when converted to
/// protocol errors.
#[derive(Debug, Error, Clone, PartialEq)]
#[non_exhaustive]
pub enum ValidationError {
    // =========================================================================
    // Content Validation Errors (§9.1)
    // =========================================================================
    /// Content hash does not match the manifest hash
    #[error("content hash mismatch: expected {expected}, got {actual}")]
    HashMismatch {
        /// Expected hash from manifest
        expected: String,
        /// Actual computed hash
        actual: String,
    },

    /// Content size does not match the manifest
    #[error("content size mismatch: expected {expected} bytes, got {actual} bytes")]
    SizeMismatch {
        /// Expected size from manifest
        expected: u64,
        /// Actual content size
        actual: u64,
    },

    /// Title exceeds maximum length
    #[error("title too long: {length} chars exceeds maximum {max}")]
    TitleTooLong {
        /// Actual title length
        length: usize,
        /// Maximum allowed length
        max: usize,
    },

    /// Description exceeds maximum length
    #[error("description too long: {length} chars exceeds maximum {max}")]
    DescriptionTooLong {
        /// Actual description length
        length: usize,
        /// Maximum allowed length
        max: usize,
    },

    /// Too many tags
    #[error("too many tags: {count} exceeds maximum {max}")]
    TooManyTags {
        /// Actual tag count
        count: usize,
        /// Maximum allowed tags
        max: usize,
    },

    /// Tag exceeds maximum length
    #[error("tag too long: '{tag}' is {length} chars, maximum is {max}")]
    TagTooLong {
        /// The offending tag
        tag: String,
        /// Actual tag length
        length: usize,
        /// Maximum allowed length
        max: usize,
    },

    /// Content exceeds maximum size
    #[error("content too large: {size} bytes exceeds maximum {max} bytes")]
    ContentTooLarge {
        /// Actual content size
        size: u64,
        /// Maximum allowed size
        max: u64,
    },

    // =========================================================================
    // Version Validation Errors (§9.2)
    // =========================================================================
    /// First version has a previous hash (should be None)
    #[error("v1 must not have a previous version")]
    V1HasPrevious,

    /// First version root doesn't match content hash
    #[error("v1 root must equal content hash")]
    V1RootMismatch,

    /// Non-first version missing previous hash
    #[error("version {version} must have a previous version")]
    MissingPrevious {
        /// The version number
        version: u32,
    },

    /// Previous hash doesn't match provided manifest
    #[error("previous hash mismatch")]
    PreviousHashMismatch,

    /// Root doesn't match previous version's root
    #[error("root must equal previous version's root")]
    RootMismatch,

    /// Version number doesn't increment by 1
    #[error("version number must increment by 1: expected {expected}, got {actual}")]
    VersionNumberMismatch {
        /// Expected version number
        expected: u32,
        /// Actual version number
        actual: u32,
    },

    /// Timestamp not after previous
    #[error("timestamp must be after previous version")]
    TimestampNotAfterPrevious,

    // =========================================================================
    // Provenance Validation Errors (§9.3)
    // =========================================================================
    /// L0 content must have exactly one root (itself)
    #[error("L0 content must have exactly one root entry (itself)")]
    L0WrongRootCount,

    /// L0 root entry doesn't reference itself
    #[error("L0 root entry must reference the content hash")]
    L0RootNotSelf,

    /// L0 must not have derived_from entries
    #[error("L0 content must not have derived_from entries")]
    L0HasDerivedFrom,

    /// L0 depth must be 0
    #[error("L0 content must have depth 0, got {depth}")]
    L0WrongDepth {
        /// Actual depth
        depth: u32,
    },

    /// L3 must have at least one root
    #[error("L3 content must have at least one root entry")]
    L3NoRoots,

    /// L3 must have at least one derived_from entry
    #[error("L3 content must derive from at least one source")]
    L3NoDerivedFrom,

    /// derived_from references unknown source
    #[error("derived_from references unknown source: {hash}")]
    UnknownSource {
        /// The unknown source hash
        hash: String,
    },

    /// root_l0l1 computation mismatch
    #[error("root_l0l1 entries do not match computed roots from sources")]
    RootEntriesMismatch,

    /// Depth computation mismatch
    #[error("depth mismatch: expected {expected}, got {actual}")]
    DepthMismatch {
        /// Expected depth
        expected: u32,
        /// Actual depth
        actual: u32,
    },

    /// Content derives from itself
    #[error("content cannot derive from itself")]
    SelfReference,

    /// Content is its own root
    #[error("content cannot be its own root")]
    SelfRoot,

    /// Provenance depth exceeds maximum
    #[error("provenance depth {depth} exceeds maximum {max}")]
    DepthTooDeep {
        /// Actual depth
        depth: u32,
        /// Maximum allowed depth
        max: u32,
    },

    // =========================================================================
    // Payment Validation Errors (§9.4)
    // =========================================================================
    /// Payment amount less than price
    #[error("insufficient payment: {amount} < required {price}")]
    InsufficientPayment {
        /// Payment amount
        amount: u64,
        /// Required price
        price: u64,
    },

    /// Payment recipient doesn't match content owner
    #[error("wrong recipient: payment to {payment_recipient}, but owner is {owner}")]
    WrongRecipient {
        /// Payment recipient
        payment_recipient: String,
        /// Content owner
        owner: String,
    },

    /// Query hash doesn't match manifest hash
    #[error("query hash mismatch")]
    QueryHashMismatch,

    /// Channel is not open
    #[error("channel is not open: state is {state}")]
    ChannelNotOpen {
        /// Current channel state
        state: String,
    },

    /// Insufficient channel balance
    #[error("insufficient channel balance: {balance} < required {amount}")]
    InsufficientChannelBalance {
        /// Available balance
        balance: u64,
        /// Required amount
        amount: u64,
    },

    /// Invalid nonce (not greater than previous)
    #[error("invalid nonce: {nonce} must be greater than channel nonce {channel_nonce}")]
    InvalidNonce {
        /// Payment nonce
        nonce: u64,
        /// Current channel nonce
        channel_nonce: u64,
    },

    /// Payment signature is invalid
    #[error("invalid payment signature")]
    InvalidPaymentSignature,

    /// Payment provenance doesn't match manifest
    #[error("payment provenance does not match manifest provenance")]
    ProvenanceMismatch,

    // =========================================================================
    // Message Validation Errors (§9.5)
    // =========================================================================
    /// Unsupported protocol version
    #[error("unsupported protocol version: {version}, expected {expected}")]
    UnsupportedVersion {
        /// Message version
        version: u8,
        /// Expected version
        expected: u8,
    },

    /// Invalid message type
    #[error("invalid message type: {message_type}")]
    InvalidMessageType {
        /// The invalid message type value
        message_type: u16,
    },

    /// Timestamp outside acceptable range
    #[error("timestamp outside acceptable range: skew is {skew_ms}ms, max is {max_skew_ms}ms")]
    TimestampOutOfRange {
        /// Actual clock skew in milliseconds
        skew_ms: u64,
        /// Maximum allowed skew
        max_skew_ms: u64,
    },

    /// Invalid sender peer ID
    #[error("invalid sender peer ID")]
    InvalidSender,

    /// Message signature is invalid
    #[error("invalid message signature")]
    InvalidMessageSignature,

    /// Payload decode failed
    #[error("payload decode failed: {reason}")]
    PayloadDecodeFailed {
        /// Reason for decode failure
        reason: String,
    },

    // =========================================================================
    // Access Validation Errors (§9.6)
    // =========================================================================
    /// Content is private
    #[error("content is private")]
    ContentPrivate,

    /// Peer not in allowlist
    #[error("peer not in allowlist")]
    NotInAllowlist,

    /// Peer in denylist
    #[error("peer is in denylist")]
    InDenylist,

    /// Bond required but not provided
    #[error("bond of {required} required")]
    BondRequired {
        /// Required bond amount
        required: u64,
    },

    // =========================================================================
    // L2 Entity Graph Validation Errors
    // =========================================================================
    /// L2 visibility is not Private
    #[error("L2 visibility must be Private, got {visibility}")]
    L2VisibilityNotPrivate {
        /// The invalid visibility
        visibility: String,
    },

    /// L2 price is not zero
    #[error("L2 price must be 0, got {price}")]
    L2PriceNotZero {
        /// The invalid price
        price: u64,
    },

    /// L2 graph ID doesn't match manifest hash
    #[error("L2 graph ID does not match manifest hash")]
    L2IdMismatch,

    /// L2 has no sources
    #[error("L2 must have at least one source")]
    L2NoSources,

    /// L2 has too many sources
    #[error("L2 has too many sources: {count} exceeds maximum {max}")]
    L2TooManySources {
        /// Actual source count
        count: usize,
        /// Maximum allowed
        max: usize,
    },

    /// L2 has too many entities
    #[error("L2 has too many entities: {count} exceeds maximum {max}")]
    L2TooManyEntities {
        /// Actual entity count
        count: usize,
        /// Maximum allowed
        max: usize,
    },

    /// L2 has too many relationships
    #[error("L2 has too many relationships: {count} exceeds maximum {max}")]
    L2TooManyRelationships {
        /// Actual relationship count
        count: usize,
        /// Maximum allowed
        max: usize,
    },

    /// L2 entity count mismatch
    #[error("L2 entity count mismatch: declared {declared}, actual {actual}")]
    L2EntityCountMismatch {
        /// Declared count
        declared: u32,
        /// Actual count
        actual: u32,
    },

    /// L2 relationship count mismatch
    #[error("L2 relationship count mismatch: declared {declared}, actual {actual}")]
    L2RelationshipCountMismatch {
        /// Declared count
        declared: u32,
        /// Actual count
        actual: u32,
    },

    /// L2 has duplicate entity ID
    #[error("L2 has duplicate entity ID")]
    L2DuplicateEntityId,

    /// L2 invalid entity ID
    #[error("L2 invalid entity ID: {id}")]
    L2InvalidEntityId {
        /// The invalid entity ID
        id: String,
    },

    /// L2 invalid relationship ID
    #[error("L2 invalid relationship ID: {id}")]
    L2InvalidRelationshipId {
        /// The invalid relationship ID
        id: String,
    },

    /// L2 invalid entity reference
    #[error("L2 invalid entity reference: {entity_id} in {context}")]
    L2InvalidEntityRef {
        /// The invalid entity ID
        entity_id: String,
        /// Context where the error occurred
        context: String,
    },

    /// L2 invalid URI
    #[error("L2 invalid URI '{uri}': {reason}")]
    L2InvalidUri {
        /// The invalid URI
        uri: String,
        /// Reason for invalidity
        reason: String,
    },

    /// L2 invalid prefix
    #[error("L2 invalid prefix '{prefix}': {reason}")]
    L2InvalidPrefix {
        /// The invalid prefix
        prefix: String,
        /// Reason for invalidity
        reason: String,
    },

    /// L2 label too long
    #[error("L2 canonical label too long: {length} chars exceeds maximum {max}")]
    L2LabelTooLong {
        /// Actual length
        length: usize,
        /// Maximum allowed
        max: usize,
    },

    /// L2 too many aliases
    #[error("L2 entity has too many aliases: {count} exceeds maximum {max}")]
    L2TooManyAliases {
        /// Actual count
        count: usize,
        /// Maximum allowed
        max: usize,
    },

    /// L2 description too long
    #[error("L2 entity description too long: {length} chars exceeds maximum {max}")]
    L2DescriptionTooLong {
        /// Actual length
        length: usize,
        /// Maximum allowed
        max: usize,
    },

    /// L2 predicate too long
    #[error("L2 predicate too long: {length} chars exceeds maximum {max}")]
    L2PredicateTooLong {
        /// Actual length
        length: usize,
        /// Maximum allowed
        max: usize,
    },

    /// L2 invalid confidence score
    #[error("L2 invalid confidence score: {value} (must be 0.0 to 1.0)")]
    L2InvalidConfidence {
        /// The invalid confidence value
        value: f32,
    },

    /// L2 invalid source type
    #[error("L2 source {hash} has invalid content type: {content_type}")]
    L2InvalidSourceType {
        /// Hash of the invalid source
        hash: String,
        /// The invalid content type
        content_type: String,
    },

    /// L2 cannot be published
    #[error("L2 content cannot be published (must remain private)")]
    L2CannotPublish,

    // =========================================================================
    // Generic Errors
    // =========================================================================
    /// Public key lookup failed
    #[error("public key not found for peer: {peer_id}")]
    PublicKeyNotFound {
        /// The peer ID
        peer_id: String,
    },

    /// Internal validation error
    #[error("internal validation error: {0}")]
    Internal(String),
}

impl ValidationError {
    /// Get the corresponding error code for this validation error.
    pub fn error_code(&self) -> nodalync_types::ErrorCode {
        use nodalync_types::ErrorCode;

        match self {
            // Content validation -> InvalidManifest or specific codes
            Self::HashMismatch { .. } => ErrorCode::InvalidHash,
            Self::SizeMismatch { .. } => ErrorCode::InvalidManifest,
            Self::TitleTooLong { .. } => ErrorCode::InvalidManifest,
            Self::DescriptionTooLong { .. } => ErrorCode::InvalidManifest,
            Self::TooManyTags { .. } => ErrorCode::InvalidManifest,
            Self::TagTooLong { .. } => ErrorCode::InvalidManifest,
            Self::ContentTooLarge { .. } => ErrorCode::ContentTooLarge,

            // Version validation
            Self::V1HasPrevious
            | Self::V1RootMismatch
            | Self::MissingPrevious { .. }
            | Self::PreviousHashMismatch
            | Self::RootMismatch
            | Self::VersionNumberMismatch { .. }
            | Self::TimestampNotAfterPrevious => ErrorCode::InvalidVersion,

            // Provenance validation
            Self::L0WrongRootCount
            | Self::L0RootNotSelf
            | Self::L0HasDerivedFrom
            | Self::L0WrongDepth { .. }
            | Self::L3NoRoots
            | Self::L3NoDerivedFrom
            | Self::UnknownSource { .. }
            | Self::RootEntriesMismatch
            | Self::DepthMismatch { .. }
            | Self::SelfReference
            | Self::SelfRoot
            | Self::DepthTooDeep { .. } => ErrorCode::InvalidProvenance,

            // Payment validation
            Self::InsufficientPayment { .. } => ErrorCode::PaymentInvalid,
            Self::WrongRecipient { .. } => ErrorCode::PaymentInvalid,
            Self::QueryHashMismatch => ErrorCode::PaymentInvalid,
            Self::ChannelNotOpen { .. } => ErrorCode::ChannelClosed,
            Self::InsufficientChannelBalance { .. } => ErrorCode::InsufficientBalance,
            Self::InvalidNonce { .. } => ErrorCode::InvalidNonce,
            Self::InvalidPaymentSignature => ErrorCode::InvalidSignature,
            Self::ProvenanceMismatch => ErrorCode::PaymentInvalid,

            // Message validation
            Self::UnsupportedVersion { .. } => ErrorCode::InvalidManifest,
            Self::InvalidMessageType { .. } => ErrorCode::InvalidManifest,
            Self::TimestampOutOfRange { .. } => ErrorCode::InvalidManifest,
            Self::InvalidSender => ErrorCode::InvalidManifest,
            Self::InvalidMessageSignature => ErrorCode::InvalidSignature,
            Self::PayloadDecodeFailed { .. } => ErrorCode::InvalidManifest,

            // Access validation
            Self::ContentPrivate | Self::NotInAllowlist | Self::InDenylist => {
                ErrorCode::AccessDenied
            }
            Self::BondRequired { .. } => ErrorCode::PaymentRequired,

            // L2 validation
            Self::L2VisibilityNotPrivate { .. }
            | Self::L2PriceNotZero { .. }
            | Self::L2IdMismatch
            | Self::L2NoSources
            | Self::L2TooManySources { .. }
            | Self::L2DuplicateEntityId
            | Self::L2InvalidEntityId { .. }
            | Self::L2InvalidRelationshipId { .. }
            | Self::L2InvalidPrefix { .. }
            | Self::L2LabelTooLong { .. }
            | Self::L2TooManyAliases { .. }
            | Self::L2DescriptionTooLong { .. }
            | Self::L2PredicateTooLong { .. }
            | Self::L2InvalidConfidence { .. }
            | Self::L2InvalidSourceType { .. }
            | Self::L2EntityCountMismatch { .. }
            | Self::L2RelationshipCountMismatch { .. } => ErrorCode::L2InvalidStructure,

            Self::L2TooManyEntities { .. } => ErrorCode::L2EntityLimit,
            Self::L2TooManyRelationships { .. } => ErrorCode::L2RelationshipLimit,
            Self::L2InvalidEntityRef { .. } => ErrorCode::L2InvalidEntityRef,
            Self::L2InvalidUri { .. } => ErrorCode::L2InvalidUri,
            Self::L2CannotPublish => ErrorCode::L2CannotPublish,

            // Generic
            Self::PublicKeyNotFound { .. } => ErrorCode::PeerNotFound,
            Self::Internal(_) => ErrorCode::InternalError,
        }
    }
}

/// Result type for validation operations.
pub type ValidationResult<T> = std::result::Result<T, ValidationError>;

#[cfg(test)]
mod tests {
    use super::*;
    use nodalync_types::ErrorCode;

    #[test]
    fn test_error_display() {
        let err = ValidationError::HashMismatch {
            expected: "abc123".to_string(),
            actual: "def456".to_string(),
        };
        assert!(err.to_string().contains("hash mismatch"));

        let err = ValidationError::TitleTooLong {
            length: 300,
            max: 200,
        };
        assert!(err.to_string().contains("300"));
        assert!(err.to_string().contains("200"));
    }

    #[test]
    fn test_error_codes() {
        assert_eq!(
            ValidationError::HashMismatch {
                expected: "a".into(),
                actual: "b".into()
            }
            .error_code(),
            ErrorCode::InvalidHash
        );

        assert_eq!(
            ValidationError::V1HasPrevious.error_code(),
            ErrorCode::InvalidVersion
        );

        assert_eq!(
            ValidationError::L0WrongRootCount.error_code(),
            ErrorCode::InvalidProvenance
        );

        assert_eq!(
            ValidationError::ContentPrivate.error_code(),
            ErrorCode::AccessDenied
        );

        assert_eq!(
            ValidationError::InvalidPaymentSignature.error_code(),
            ErrorCode::InvalidSignature
        );
    }

    #[test]
    fn test_error_clone() {
        let err = ValidationError::ContentPrivate;
        let cloned = err.clone();
        assert_eq!(err, cloned);
    }
}
