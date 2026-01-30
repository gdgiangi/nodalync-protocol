//! Validation rules for the Nodalync protocol.
//!
//! This crate implements all validation rules from Protocol Specification §9.
//! It provides both standalone validation functions and a `Validator` trait
//! for combining all validations.
//!
//! # Validation Categories
//!
//! - **Content Validation** (§9.1): Hash, size, and metadata constraints
//! - **Version Validation** (§9.2): Version chain rules
//! - **Provenance Validation** (§9.3): Derivation and depth rules
//! - **Payment Validation** (§9.4): Amount, channel, and signature rules
//! - **Message Validation** (§9.5): Protocol version, timestamp, and signature rules
//! - **Access Validation** (§9.6): Visibility, allowlist/denylist, and bond rules
//!
//! # Usage
//!
//! ## Using standalone functions
//!
//! ```
//! use nodalync_valid::{validate_content, validate_version, validate_provenance};
//! use nodalync_types::{Manifest, Metadata};
//! use nodalync_crypto::{content_hash, generate_identity, peer_id_from_public_key};
//!
//! let content = b"Hello, Nodalync!";
//! let hash = content_hash(content);
//! let (_, public_key) = generate_identity();
//! let owner = peer_id_from_public_key(&public_key);
//! let metadata = Metadata::new("Test", content.len() as u64);
//! let manifest = Manifest::new_l0(hash, owner, metadata, 1234567890);
//!
//! // Validate content
//! assert!(validate_content(content, &manifest).is_ok());
//!
//! // Validate version (v1)
//! assert!(validate_version(&manifest, None).is_ok());
//!
//! // Validate provenance (L0)
//! assert!(validate_provenance(&manifest, &[]).is_ok());
//! ```
//!
//! ## Using the Validator trait
//!
//! ```
//! use nodalync_valid::{Validator, DefaultValidator};
//! use nodalync_types::{Manifest, Metadata};
//! use nodalync_crypto::{content_hash, generate_identity, peer_id_from_public_key};
//!
//! let validator = DefaultValidator::new();
//!
//! let content = b"Hello, Nodalync!";
//! let hash = content_hash(content);
//! let (_, public_key) = generate_identity();
//! let owner = peer_id_from_public_key(&public_key);
//! let metadata = Metadata::new("Test", content.len() as u64);
//! let manifest = Manifest::new_l0(hash, owner, metadata, 1234567890);
//!
//! assert!(validator.validate_content(content, &manifest).is_ok());
//! ```
//!
//! # Error Handling
//!
//! All validation functions return `Result<(), ValidationError>`. The
//! `ValidationError` enum provides detailed error information and can
//! be converted to protocol error codes.
//!
//! ```
//! use nodalync_valid::{validate_content, ValidationError};
//! use nodalync_types::{Manifest, Metadata, ErrorCode};
//! use nodalync_crypto::{content_hash, generate_identity, peer_id_from_public_key};
//!
//! let content = b"Content";
//! let hash = content_hash(content);
//! let (_, public_key) = generate_identity();
//! let owner = peer_id_from_public_key(&public_key);
//! // Wrong size in metadata
//! let metadata = Metadata::new("Test", 999);
//! let manifest = Manifest::new_l0(hash, owner, metadata, 1234567890);
//!
//! match validate_content(content, &manifest) {
//!     Ok(()) => println!("Valid!"),
//!     Err(e) => {
//!         println!("Validation failed: {}", e);
//!         println!("Error code: {:?}", e.error_code());
//!     }
//! }
//! ```

pub mod access;
pub mod content;
pub mod error;
pub mod l2;
pub mod message;
pub mod payment;
pub mod provenance;
pub mod validator;
pub mod version;

// Re-export main types and functions
pub use error::{ValidationError, ValidationResult};

// Re-export standalone validation functions
pub use access::{
    is_owner, validate_access, validate_access_basic, validate_access_with_owner_bypass,
};
pub use content::{validate_content, validate_metadata};
pub use l2::{
    expand_curie, is_valid_uri, validate_l2_content, validate_l2_provenance, validate_l2_publish,
};
pub use message::{is_valid_message_type, validate_message, validate_message_basic};
pub use payment::{
    construct_payment_message, validate_payment, validate_payment_basic, BondChecker,
    PublicKeyLookup,
};
pub use provenance::validate_provenance;
pub use version::validate_version;

// Re-export validator trait and implementations
pub use validator::{
    DefaultValidator, NoopBondChecker, NoopPublicKeyLookup, PermissiveBondChecker, Validator,
    ValidatorConfig,
};

#[cfg(test)]
mod tests {
    use super::*;
    use nodalync_crypto::{content_hash, generate_identity, peer_id_from_public_key};
    use nodalync_types::{Metadata, Visibility};

    /// Integration test: Create and validate L0 content
    #[test]
    fn test_full_l0_validation() {
        let content = b"This is some L0 content for testing.";
        let hash = content_hash(content);
        let (_, public_key) = generate_identity();
        let owner = peer_id_from_public_key(&public_key);
        let metadata = nodalync_types::Metadata::new("Test Document", content.len() as u64)
            .with_description("A test document for validation")
            .with_tags(vec!["test".to_string(), "validation".to_string()]);
        let mut manifest = nodalync_types::Manifest::new_l0(hash, owner, metadata, 1234567890);
        manifest.visibility = Visibility::Shared;

        // Validate content
        assert!(validate_content(content, &manifest).is_ok());

        // Validate version
        assert!(validate_version(&manifest, None).is_ok());

        // Validate provenance
        assert!(validate_provenance(&manifest, &[]).is_ok());

        // Validate access (should pass for Shared visibility)
        let requester = {
            let (_, pk) = generate_identity();
            peer_id_from_public_key(&pk)
        };
        assert!(validate_access_basic(&requester, &manifest).is_ok());
    }

    /// Integration test: Version chain
    #[test]
    fn test_version_chain_validation() {
        // Create v1
        let content_v1 = b"Version 1 content";
        let hash_v1 = content_hash(content_v1);
        let (_, pk) = generate_identity();
        let owner = peer_id_from_public_key(&pk);
        let metadata_v1 = Metadata::new("Doc v1", content_v1.len() as u64);
        let manifest_v1 = nodalync_types::Manifest::new_l0(hash_v1, owner, metadata_v1, 1000);

        assert!(validate_content(content_v1, &manifest_v1).is_ok());
        assert!(validate_version(&manifest_v1, None).is_ok());

        // Create v2
        let content_v2 = b"Version 2 content with updates";
        let hash_v2 = content_hash(content_v2);
        let metadata_v2 = Metadata::new("Doc v2", content_v2.len() as u64);
        let mut manifest_v2 = nodalync_types::Manifest::new_l0(hash_v2, owner, metadata_v2, 2000);
        manifest_v2.version =
            nodalync_types::Version::new_from_previous(&manifest_v1.version, hash_v1, 2000);

        assert!(validate_content(content_v2, &manifest_v2).is_ok());
        assert!(validate_version(&manifest_v2, Some(&manifest_v1)).is_ok());
    }

    /// Integration test: Use Validator trait
    #[test]
    fn test_validator_trait_integration() {
        let validator = DefaultValidator::new();

        let content = b"Content for validator trait test";
        let hash = content_hash(content);
        let (_, pk) = generate_identity();
        let owner = peer_id_from_public_key(&pk);
        let metadata = Metadata::new("Test", content.len() as u64);
        let mut manifest = nodalync_types::Manifest::new_l0(hash, owner, metadata, 1234567890);
        manifest.visibility = Visibility::Shared;

        assert!(validator.validate_content(content, &manifest).is_ok());
        assert!(validator.validate_version(&manifest, None).is_ok());
        assert!(validator.validate_provenance(&manifest, &[]).is_ok());

        let requester = {
            let (_, pk) = generate_identity();
            peer_id_from_public_key(&pk)
        };
        assert!(validator.validate_access(&requester, &manifest).is_ok());
    }
}
