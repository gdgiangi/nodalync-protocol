//! Data structures for the Nodalync protocol.
//!
//! This crate provides all data types used across the Nodalync protocol,
//! as defined in Protocol Specification §4. It contains no business logic,
//! only type definitions with serialization support.
//!
//! # Module Organization
//!
//! - [`enums`] - Enumeration types (ContentType, Visibility, etc.)
//! - [`constants`] - Protocol constants (limits, timing, economics)
//! - [`error`] - Error codes and the main error type
//! - [`manifest`] - Content manifest and metadata types
//! - [`provenance`] - Provenance chain types
//! - [`content`] - L1 mentions and summaries
//! - [`channel`] - Payment channel types
//! - [`settlement`] - On-chain settlement types
//!
//! # Example
//!
//! ```
//! use nodalync_types::{
//!     ContentType, Visibility, Manifest, Metadata, Provenance,
//!     Amount, constants::MAX_CONTENT_SIZE,
//! };
//! use nodalync_crypto::{content_hash, generate_identity, peer_id_from_public_key};
//!
//! // Create a new L0 manifest
//! let content = b"Hello, Nodalync!";
//! let hash = content_hash(content);
//! let (_, public_key) = generate_identity();
//! let owner = peer_id_from_public_key(&public_key);
//! let metadata = Metadata::new("My Document", content.len() as u64);
//!
//! let manifest = Manifest::new_l0(hash, owner, metadata, 1234567890);
//!
//! assert_eq!(manifest.content_type, ContentType::L0);
//! assert_eq!(manifest.visibility, Visibility::Private);
//! ```
//!
//! # Type Conventions
//!
//! All types follow these conventions:
//!
//! - Derive `Debug`, `Clone`, `PartialEq`, `Eq` where appropriate
//! - Derive `Copy` for small types (enums, simple structs)
//! - Derive `Serialize`, `Deserialize` for wire format
//! - Use `#[serde(rename_all = "snake_case")]` for consistent JSON
//! - Use `#[repr(u8)]` or `#[repr(u16)]` for enums with defined wire values
//! - Use `#[non_exhaustive]` for enums to allow future extension

/// Protocol version (from Cargo.toml).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub mod channel;
pub mod constants;
pub mod content;
pub mod enums;
pub mod error;
pub mod l2;
pub mod manifest;
pub mod provenance;
pub mod settlement;

// Re-export all public types at the crate root for convenience

// Enums
pub use enums::{
    ChannelState, Classification, Confidence, ContentType, Currency, LocationType,
    ResolutionMethod, Visibility,
};

// Constants
pub use constants::*;

// Error types
pub use error::{ErrorCode, NodalyncError, Result};

// Manifest types
pub use manifest::{AccessControl, Economics, Manifest, Metadata, Version};

// Provenance types
pub use provenance::{Provenance, ProvenanceEntry};

// Content types
pub use content::{L1Summary, Mention, SourceLocation};

// Channel types
pub use channel::{Channel, Payment};

// Settlement types
pub use settlement::{Distribution, SettlementBatch, SettlementEntry};

// L2 Entity Graph types
pub use l2::{
    ConflictResolution, Entity, L1Reference, L2BuildConfig, L2EntityGraph, L2MergeConfig,
    LiteralValue, MentionRef, PrefixEntry, PrefixMap, Relationship, RelationshipObject, Uri,
};

/// Amount in tinybars (10^-8 HBAR).
///
/// This is the standard type for all monetary values in the protocol.
/// One HBAR equals 100,000,000 (10^8) tinybars.
pub type Amount = u64;

// Re-export crypto types that are commonly used with types
pub use nodalync_crypto::{Hash, PeerId, Signature, Timestamp};

#[cfg(test)]
mod tests {
    use super::*;
    use nodalync_crypto::{content_hash, generate_identity, peer_id_from_public_key};

    #[test]
    fn test_full_manifest_creation() {
        // Generate identity
        let (_, public_key) = generate_identity();
        let owner = peer_id_from_public_key(&public_key);

        // Create content and hash
        let content = b"Test document content";
        let hash = content_hash(content);

        // Create metadata
        let metadata = Metadata::new("Test Document", content.len() as u64)
            .with_description("A test document")
            .with_tags(vec!["test".to_string(), "example".to_string()])
            .with_mime_type("text/plain");

        // Create manifest
        let timestamp = 1234567890u64;
        let manifest = Manifest::new_l0(hash, owner, metadata, timestamp);

        // Verify
        assert_eq!(manifest.hash, hash);
        assert_eq!(manifest.owner, owner);
        assert_eq!(manifest.content_type, ContentType::L0);
        assert_eq!(manifest.visibility, Visibility::Private);
        assert!(manifest.version.is_first_version());
        assert!(manifest.provenance.is_l0());
        assert_eq!(manifest.economics.price, 0);
    }

    #[test]
    fn test_types_interop() {
        // Test that types work together correctly
        let (_, public_key) = generate_identity();
        let owner = peer_id_from_public_key(&public_key);
        let hash = content_hash(b"content");

        // Create provenance
        let provenance = Provenance::new_l0(hash, owner);
        assert!(provenance.is_valid(&hash));

        // Create provenance entry
        let entry = ProvenanceEntry::new(hash, owner, Visibility::Shared);
        assert_eq!(entry.weight, 1);

        // Create version
        let version = Version::new_v1(hash, 1234567890);
        assert!(version.is_valid(&hash));
    }

    #[test]
    fn test_amount_type() {
        let amount: Amount = 100_000_000; // 1 HBAR
        assert_eq!(amount, 100_000_000u64);

        // Test with economics
        let mut economics = Economics::with_price(1000);
        economics.record_query(1000);
        assert_eq!(economics.total_revenue, 1000);
    }

    #[test]
    fn test_constants_available() {
        // Verify constants are accessible
        assert_eq!(MAX_CONTENT_SIZE, 104_857_600);
        assert_eq!(MIN_PRICE, 1);
        assert_eq!(DHT_BUCKET_SIZE, 20);
        assert_eq!(PROTOCOL_VERSION, 0x01);
    }

    #[test]
    fn test_error_types() {
        let err = NodalyncError::protocol(ErrorCode::NotFound, "content not found");
        assert_eq!(err.error_code(), Some(ErrorCode::NotFound));

        let err = NodalyncError::ContentValidation("hash mismatch".to_string());
        assert!(matches!(err.error_code(), Some(ErrorCode::InvalidManifest)));
    }
}
