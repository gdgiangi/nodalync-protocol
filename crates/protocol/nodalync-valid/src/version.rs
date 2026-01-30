//! Version validation (§9.2).
//!
//! This module validates version constraints:
//! - v1: previous is None, root equals content hash
//! - v2+: previous is Some, root equals previous.root
//! - Version number increments by 1
//! - Timestamp is after previous timestamp

use nodalync_types::Manifest;

use crate::error::{ValidationError, ValidationResult};

/// Validate version constraints for a manifest.
///
/// Checks all version validation rules from §9.2:
///
/// For v1 (first version):
/// - `previous` must be `None`
/// - `root` must equal the content hash
///
/// For v2+ (subsequent versions):
/// - `previous` must be `Some`
/// - `root` must equal `previous.root`
/// - `number` must equal `previous.number + 1`
/// - `timestamp` must be greater than `previous.timestamp`
///
/// # Arguments
///
/// * `manifest` - The manifest to validate
/// * `previous` - The previous version's manifest (None for v1)
///
/// # Returns
///
/// `Ok(())` if version constraints are satisfied, or `Err(ValidationError)`.
///
/// # Example
///
/// ```
/// use nodalync_valid::validate_version;
/// use nodalync_types::{Manifest, Metadata};
/// use nodalync_crypto::{content_hash, generate_identity, peer_id_from_public_key};
///
/// let content = b"Hello, Nodalync!";
/// let hash = content_hash(content);
/// let (_, public_key) = generate_identity();
/// let owner = peer_id_from_public_key(&public_key);
/// let metadata = Metadata::new("Test", content.len() as u64);
/// let manifest = Manifest::new_l0(hash, owner, metadata, 1234567890);
///
/// // v1 validation (no previous)
/// assert!(validate_version(&manifest, None).is_ok());
/// ```
pub fn validate_version(manifest: &Manifest, previous: Option<&Manifest>) -> ValidationResult<()> {
    let v = &manifest.version;

    if v.number == 1 {
        // First version constraints
        validate_v1(manifest)?;
    } else {
        // Subsequent version constraints
        validate_v2_plus(manifest, previous)?;
    }

    Ok(())
}

/// Validate first version (v1) constraints.
fn validate_v1(manifest: &Manifest) -> ValidationResult<()> {
    let v = &manifest.version;

    // v1 must have no previous
    if v.previous.is_some() {
        return Err(ValidationError::V1HasPrevious);
    }

    // v1 root must equal content hash
    if v.root != manifest.hash {
        return Err(ValidationError::V1RootMismatch);
    }

    Ok(())
}

/// Validate subsequent version (v2+) constraints.
fn validate_v2_plus(manifest: &Manifest, previous: Option<&Manifest>) -> ValidationResult<()> {
    let v = &manifest.version;

    // v2+ must have previous
    if v.previous.is_none() {
        return Err(ValidationError::MissingPrevious { version: v.number });
    }

    // If previous manifest is provided, validate against it
    if let Some(prev) = previous {
        // Previous hash must match
        if v.previous.as_ref() != Some(&prev.hash) {
            return Err(ValidationError::PreviousHashMismatch);
        }

        // Root must equal previous root
        if v.root != prev.version.root {
            return Err(ValidationError::RootMismatch);
        }

        // Version number must increment by 1
        let expected_number = prev.version.number + 1;
        if v.number != expected_number {
            return Err(ValidationError::VersionNumberMismatch {
                expected: expected_number,
                actual: v.number,
            });
        }

        // Timestamp must be after previous
        if v.timestamp <= prev.version.timestamp {
            return Err(ValidationError::TimestampNotAfterPrevious);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use nodalync_crypto::{content_hash, generate_identity, peer_id_from_public_key};
    use nodalync_types::{Metadata, Version};

    fn create_test_manifest(content: &[u8], timestamp: u64) -> Manifest {
        let hash = content_hash(content);
        let (_, public_key) = generate_identity();
        let owner = peer_id_from_public_key(&public_key);
        let metadata = Metadata::new("Test", content.len() as u64);
        Manifest::new_l0(hash, owner, metadata, timestamp)
    }

    #[test]
    fn test_valid_v1() {
        let manifest = create_test_manifest(b"Content v1", 1000);
        assert!(validate_version(&manifest, None).is_ok());
    }

    #[test]
    fn test_v1_with_previous_fails() {
        let content = b"Content v1";
        let hash = content_hash(content);
        let (_, public_key) = generate_identity();
        let owner = peer_id_from_public_key(&public_key);
        let metadata = Metadata::new("Test", content.len() as u64);

        let mut manifest = Manifest::new_l0(hash, owner, metadata, 1000);
        // Incorrectly set previous for v1
        manifest.version.previous = Some(hash);

        let result = validate_version(&manifest, None);
        assert!(matches!(result, Err(ValidationError::V1HasPrevious)));
    }

    #[test]
    fn test_v1_root_mismatch() {
        let content = b"Content v1";
        let hash = content_hash(content);
        let different_hash = content_hash(b"different");
        let (_, public_key) = generate_identity();
        let owner = peer_id_from_public_key(&public_key);
        let metadata = Metadata::new("Test", content.len() as u64);

        let mut manifest = Manifest::new_l0(hash, owner, metadata, 1000);
        // Incorrectly set root to different hash
        manifest.version.root = different_hash;

        let result = validate_version(&manifest, None);
        assert!(matches!(result, Err(ValidationError::V1RootMismatch)));
    }

    #[test]
    fn test_valid_v2() {
        let v1_content = b"Content v1";
        let v1_manifest = create_test_manifest(v1_content, 1000);

        // Create v2 manifest
        let v2_content = b"Content v2";
        let v2_hash = content_hash(v2_content);
        let (_, public_key) = generate_identity();
        let owner = peer_id_from_public_key(&public_key);
        let metadata = Metadata::new("Test v2", v2_content.len() as u64);

        let mut v2_manifest = Manifest::new_l0(v2_hash, owner, metadata, 2000);
        v2_manifest.version =
            Version::new_from_previous(&v1_manifest.version, v1_manifest.hash, 2000);

        assert!(validate_version(&v2_manifest, Some(&v1_manifest)).is_ok());
    }

    #[test]
    fn test_v2_missing_previous() {
        let content = b"Content";
        let hash = content_hash(content);
        let (_, public_key) = generate_identity();
        let owner = peer_id_from_public_key(&public_key);
        let metadata = Metadata::new("Test", content.len() as u64);

        let mut manifest = Manifest::new_l0(hash, owner, metadata, 1000);
        // Set to v2 but leave previous as None
        manifest.version.number = 2;

        let result = validate_version(&manifest, None);
        assert!(matches!(
            result,
            Err(ValidationError::MissingPrevious { version: 2 })
        ));
    }

    #[test]
    fn test_v2_previous_hash_mismatch() {
        let v1_manifest = create_test_manifest(b"Content v1", 1000);

        // Create v2 with wrong previous hash
        let v2_content = b"Content v2";
        let v2_hash = content_hash(v2_content);
        let (_, public_key) = generate_identity();
        let owner = peer_id_from_public_key(&public_key);
        let metadata = Metadata::new("Test v2", v2_content.len() as u64);

        let mut v2_manifest = Manifest::new_l0(v2_hash, owner, metadata, 2000);
        v2_manifest.version =
            Version::new_from_previous(&v1_manifest.version, v1_manifest.hash, 2000);
        // Set wrong previous hash
        v2_manifest.version.previous = Some(content_hash(b"wrong"));

        let result = validate_version(&v2_manifest, Some(&v1_manifest));
        assert!(matches!(result, Err(ValidationError::PreviousHashMismatch)));
    }

    #[test]
    fn test_v2_root_mismatch() {
        let v1_manifest = create_test_manifest(b"Content v1", 1000);

        let v2_content = b"Content v2";
        let v2_hash = content_hash(v2_content);
        let (_, public_key) = generate_identity();
        let owner = peer_id_from_public_key(&public_key);
        let metadata = Metadata::new("Test v2", v2_content.len() as u64);

        let mut v2_manifest = Manifest::new_l0(v2_hash, owner, metadata, 2000);
        v2_manifest.version =
            Version::new_from_previous(&v1_manifest.version, v1_manifest.hash, 2000);
        // Set wrong root
        v2_manifest.version.root = content_hash(b"wrong root");

        let result = validate_version(&v2_manifest, Some(&v1_manifest));
        assert!(matches!(result, Err(ValidationError::RootMismatch)));
    }

    #[test]
    fn test_v2_version_number_mismatch() {
        let v1_manifest = create_test_manifest(b"Content v1", 1000);

        let v2_content = b"Content v2";
        let v2_hash = content_hash(v2_content);
        let (_, public_key) = generate_identity();
        let owner = peer_id_from_public_key(&public_key);
        let metadata = Metadata::new("Test v2", v2_content.len() as u64);

        let mut v2_manifest = Manifest::new_l0(v2_hash, owner, metadata, 2000);
        v2_manifest.version =
            Version::new_from_previous(&v1_manifest.version, v1_manifest.hash, 2000);
        // Set wrong version number
        v2_manifest.version.number = 5;

        let result = validate_version(&v2_manifest, Some(&v1_manifest));
        assert!(matches!(
            result,
            Err(ValidationError::VersionNumberMismatch {
                expected: 2,
                actual: 5
            })
        ));
    }

    #[test]
    fn test_v2_timestamp_not_after_previous() {
        let v1_manifest = create_test_manifest(b"Content v1", 2000);

        let v2_content = b"Content v2";
        let v2_hash = content_hash(v2_content);
        let (_, public_key) = generate_identity();
        let owner = peer_id_from_public_key(&public_key);
        let metadata = Metadata::new("Test v2", v2_content.len() as u64);

        let mut v2_manifest = Manifest::new_l0(v2_hash, owner, metadata, 1000);
        v2_manifest.version =
            Version::new_from_previous(&v1_manifest.version, v1_manifest.hash, 1000);

        let result = validate_version(&v2_manifest, Some(&v1_manifest));
        assert!(matches!(
            result,
            Err(ValidationError::TimestampNotAfterPrevious)
        ));
    }

    #[test]
    fn test_valid_version_chain() {
        let v1 = create_test_manifest(b"v1", 1000);
        assert!(validate_version(&v1, None).is_ok());

        // Create v2
        let v2_hash = content_hash(b"v2");
        let (_, pk) = generate_identity();
        let owner = peer_id_from_public_key(&pk);
        let mut v2 = Manifest::new_l0(v2_hash, owner, Metadata::new("v2", 2), 2000);
        v2.version = Version::new_from_previous(&v1.version, v1.hash, 2000);
        assert!(validate_version(&v2, Some(&v1)).is_ok());

        // Create v3
        let v3_hash = content_hash(b"v3");
        let (_, pk) = generate_identity();
        let owner = peer_id_from_public_key(&pk);
        let mut v3 = Manifest::new_l0(v3_hash, owner, Metadata::new("v3", 2), 3000);
        v3.version = Version::new_from_previous(&v2.version, v2.hash, 3000);
        assert!(validate_version(&v3, Some(&v2)).is_ok());

        // v3's root should still be v1's hash
        assert_eq!(v3.version.root, v1.hash);
    }
}
