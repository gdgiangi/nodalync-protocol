//! Content validation (§9.1).
//!
//! This module validates content against its manifest, including:
//! - Hash verification
//! - Size verification
//! - Metadata constraints (title, description, tags)

use nodalync_crypto::content_hash;
use nodalync_types::{
    Manifest, MAX_CONTENT_SIZE, MAX_DESCRIPTION_LENGTH, MAX_TAGS, MAX_TAG_LENGTH,
    MAX_TITLE_LENGTH,
};

use crate::error::{ValidationError, ValidationResult};

/// Validate content against its manifest.
///
/// Checks all content validation rules from §9.1:
/// 1. `ContentHash(content) == manifest.hash`
/// 2. `len(content) == manifest.metadata.content_size`
/// 3. Title length <= MAX_TITLE_LENGTH (200)
/// 4. Description length <= MAX_DESCRIPTION_LENGTH (2000)
/// 5. Tags count <= MAX_TAGS (20), each tag <= MAX_TAG_LENGTH (50)
/// 6. Content size <= MAX_CONTENT_SIZE
///
/// # Arguments
///
/// * `content` - The raw content bytes
/// * `manifest` - The manifest describing the content
///
/// # Returns
///
/// `Ok(())` if all validations pass, or `Err(ValidationError)` describing the failure.
///
/// # Example
///
/// ```
/// use nodalync_valid::validate_content;
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
/// assert!(validate_content(content, &manifest).is_ok());
/// ```
pub fn validate_content(content: &[u8], manifest: &Manifest) -> ValidationResult<()> {
    // 1. Hash matches
    let computed_hash = content_hash(content);
    if computed_hash != manifest.hash {
        return Err(ValidationError::HashMismatch {
            expected: format!("{}", manifest.hash),
            actual: format!("{}", computed_hash),
        });
    }

    // 2. Size matches
    let actual_size = content.len() as u64;
    if actual_size != manifest.metadata.content_size {
        return Err(ValidationError::SizeMismatch {
            expected: manifest.metadata.content_size,
            actual: actual_size,
        });
    }

    // 3. Content not too large
    if actual_size > MAX_CONTENT_SIZE {
        return Err(ValidationError::ContentTooLarge {
            size: actual_size,
            max: MAX_CONTENT_SIZE,
        });
    }

    // Validate metadata constraints
    validate_metadata(manifest)?;

    Ok(())
}

/// Validate manifest metadata constraints.
///
/// Checks:
/// - Title length <= MAX_TITLE_LENGTH
/// - Description length <= MAX_DESCRIPTION_LENGTH (if present)
/// - Tags count <= MAX_TAGS
/// - Each tag length <= MAX_TAG_LENGTH
pub fn validate_metadata(manifest: &Manifest) -> ValidationResult<()> {
    // Title length
    if manifest.metadata.title.len() > MAX_TITLE_LENGTH {
        return Err(ValidationError::TitleTooLong {
            length: manifest.metadata.title.len(),
            max: MAX_TITLE_LENGTH,
        });
    }

    // Description length (if present)
    if let Some(ref desc) = manifest.metadata.description {
        if desc.len() > MAX_DESCRIPTION_LENGTH {
            return Err(ValidationError::DescriptionTooLong {
                length: desc.len(),
                max: MAX_DESCRIPTION_LENGTH,
            });
        }
    }

    // Tags count
    if manifest.metadata.tags.len() > MAX_TAGS {
        return Err(ValidationError::TooManyTags {
            count: manifest.metadata.tags.len(),
            max: MAX_TAGS,
        });
    }

    // Individual tag lengths
    for tag in &manifest.metadata.tags {
        if tag.len() > MAX_TAG_LENGTH {
            return Err(ValidationError::TagTooLong {
                tag: tag.clone(),
                length: tag.len(),
                max: MAX_TAG_LENGTH,
            });
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use nodalync_crypto::{generate_identity, peer_id_from_public_key};
    use nodalync_types::Metadata;

    fn create_test_manifest(content: &[u8], title: &str) -> Manifest {
        let hash = content_hash(content);
        let (_, public_key) = generate_identity();
        let owner = peer_id_from_public_key(&public_key);
        let metadata = Metadata::new(title, content.len() as u64);
        Manifest::new_l0(hash, owner, metadata, 1234567890)
    }

    #[test]
    fn test_valid_content() {
        let content = b"Hello, Nodalync!";
        let manifest = create_test_manifest(content, "Test Content");
        assert!(validate_content(content, &manifest).is_ok());
    }

    #[test]
    fn test_hash_mismatch() {
        let content = b"Hello, Nodalync!";
        let manifest = create_test_manifest(content, "Test Content");
        let different_content = b"Different content";

        let result = validate_content(different_content, &manifest);
        assert!(matches!(result, Err(ValidationError::HashMismatch { .. })));
    }

    #[test]
    fn test_size_mismatch() {
        let content = b"Hello, Nodalync!";
        let hash = content_hash(content);
        let (_, public_key) = generate_identity();
        let owner = peer_id_from_public_key(&public_key);
        // Wrong size in metadata
        let metadata = Metadata::new("Test", 999);
        let manifest = Manifest::new_l0(hash, owner, metadata, 1234567890);

        let result = validate_content(content, &manifest);
        assert!(matches!(result, Err(ValidationError::SizeMismatch { .. })));
    }

    #[test]
    fn test_title_too_long() {
        let content = b"Test";
        let long_title = "a".repeat(MAX_TITLE_LENGTH + 1);
        let hash = content_hash(content);
        let (_, public_key) = generate_identity();
        let owner = peer_id_from_public_key(&public_key);
        let metadata = Metadata::new(&long_title, content.len() as u64);
        let manifest = Manifest::new_l0(hash, owner, metadata, 1234567890);

        let result = validate_content(content, &manifest);
        assert!(matches!(result, Err(ValidationError::TitleTooLong { .. })));
    }

    #[test]
    fn test_title_at_max_length() {
        let content = b"Test";
        let max_title = "a".repeat(MAX_TITLE_LENGTH);
        let hash = content_hash(content);
        let (_, public_key) = generate_identity();
        let owner = peer_id_from_public_key(&public_key);
        let metadata = Metadata::new(&max_title, content.len() as u64);
        let manifest = Manifest::new_l0(hash, owner, metadata, 1234567890);

        assert!(validate_content(content, &manifest).is_ok());
    }

    #[test]
    fn test_description_too_long() {
        let content = b"Test";
        let hash = content_hash(content);
        let (_, public_key) = generate_identity();
        let owner = peer_id_from_public_key(&public_key);
        let long_desc = "a".repeat(MAX_DESCRIPTION_LENGTH + 1);
        let metadata = Metadata::new("Title", content.len() as u64).with_description(&long_desc);
        let manifest = Manifest::new_l0(hash, owner, metadata, 1234567890);

        let result = validate_content(content, &manifest);
        assert!(matches!(
            result,
            Err(ValidationError::DescriptionTooLong { .. })
        ));
    }

    #[test]
    fn test_too_many_tags() {
        let content = b"Test";
        let hash = content_hash(content);
        let (_, public_key) = generate_identity();
        let owner = peer_id_from_public_key(&public_key);
        let tags: Vec<String> = (0..MAX_TAGS + 1).map(|i| format!("tag{}", i)).collect();
        let metadata = Metadata::new("Title", content.len() as u64).with_tags(tags);
        let manifest = Manifest::new_l0(hash, owner, metadata, 1234567890);

        let result = validate_content(content, &manifest);
        assert!(matches!(result, Err(ValidationError::TooManyTags { .. })));
    }

    #[test]
    fn test_tag_too_long() {
        let content = b"Test";
        let hash = content_hash(content);
        let (_, public_key) = generate_identity();
        let owner = peer_id_from_public_key(&public_key);
        let long_tag = "a".repeat(MAX_TAG_LENGTH + 1);
        let metadata = Metadata::new("Title", content.len() as u64).with_tags(vec![long_tag]);
        let manifest = Manifest::new_l0(hash, owner, metadata, 1234567890);

        let result = validate_content(content, &manifest);
        assert!(matches!(result, Err(ValidationError::TagTooLong { .. })));
    }

    #[test]
    fn test_max_tags_ok() {
        let content = b"Test";
        let hash = content_hash(content);
        let (_, public_key) = generate_identity();
        let owner = peer_id_from_public_key(&public_key);
        let tags: Vec<String> = (0..MAX_TAGS).map(|i| format!("tag{}", i)).collect();
        let metadata = Metadata::new("Title", content.len() as u64).with_tags(tags);
        let manifest = Manifest::new_l0(hash, owner, metadata, 1234567890);

        assert!(validate_content(content, &manifest).is_ok());
    }

    #[test]
    fn test_empty_content() {
        let content = b"";
        let manifest = create_test_manifest(content, "Empty Content");
        assert!(validate_content(content, &manifest).is_ok());
    }

    #[test]
    fn test_identical_content_produces_identical_hash() {
        let content1 = b"Identical content";
        let content2 = b"Identical content";
        let hash1 = content_hash(content1);
        let hash2 = content_hash(content2);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_different_content_produces_different_hash() {
        let content1 = b"Content A";
        let content2 = b"Content B";
        let hash1 = content_hash(content1);
        let hash2 = content_hash(content2);
        assert_ne!(hash1, hash2);
    }
}
