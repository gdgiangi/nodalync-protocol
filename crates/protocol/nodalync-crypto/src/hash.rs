//! Content hashing implementation (Spec §3.1, §3.4)
//!
//! Content hashes are computed using SHA-256 with domain separation:
//! ```text
//! ContentHash(content) = H(0x01 || len(content) as u64be || content)
//! ```

use sha2::{Digest, Sha256};

use crate::Hash;

/// Domain separator for content hashing (Spec Appendix A.2)
/// NOTE: Changed from 0x00 to 0x01 to avoid collision with DOMAIN_KEY (0x00) in identity.rs.
/// This is a breaking change for content hashes (acceptable on testnet).
const DOMAIN_CONTENT: u8 = 0x01;

/// Compute the content hash of the given bytes.
///
/// Uses SHA-256 with domain separation to prevent hash collisions across different uses.
///
/// # Algorithm
/// ```text
/// H(0x00 || len(content) as uint64_be || content)
/// ```
///
/// # Example
/// ```
/// use nodalync_crypto::content_hash;
///
/// let content = b"Hello, Nodalync!";
/// let hash = content_hash(content);
/// assert_eq!(hash.0.len(), 32);
/// ```
pub fn content_hash(content: &[u8]) -> Hash {
    let mut hasher = Sha256::new();

    // Domain separator
    hasher.update([DOMAIN_CONTENT]);

    // Length prefix as big-endian u64
    let len = content.len() as u64;
    hasher.update(len.to_be_bytes());

    // Content
    hasher.update(content);

    let result: [u8; 32] = hasher.finalize().into();
    Hash(result)
}

/// Verify that content matches the expected hash.
///
/// # Returns
/// `true` if `ContentHash(content) == expected`, `false` otherwise.
///
/// # Example
/// ```
/// use nodalync_crypto::{content_hash, verify_content};
///
/// let content = b"Some content";
/// let hash = content_hash(content);
/// assert!(verify_content(content, &hash));
/// assert!(!verify_content(b"Different content", &hash));
/// ```
pub fn verify_content(content: &[u8], expected: &Hash) -> bool {
    let computed = content_hash(content);
    computed.0 == expected.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_hash_deterministic() {
        let content = b"test";
        assert_eq!(content_hash(content).0, content_hash(content).0);
    }

    #[test]
    fn test_content_hash_different_inputs() {
        let hash1 = content_hash(b"test1");
        let hash2 = content_hash(b"test2");
        assert_ne!(hash1.0, hash2.0);
    }

    #[test]
    fn test_verify_content() {
        let content = b"verify me";
        let hash = content_hash(content);
        assert!(verify_content(content, &hash));
        assert!(!verify_content(b"tampered", &hash));
    }

    #[test]
    fn test_domain_separator_no_collision_with_key_domain() {
        // Regression test: DOMAIN_CONTENT (0x01) must differ from DOMAIN_KEY (0x00)
        // so that content hashes and PeerId key hashes never collide for the same input.
        use sha2::{Digest, Sha256};

        let input = [0u8; 32]; // Same input bytes

        // Content hash uses DOMAIN_CONTENT (0x01)
        let content_h = content_hash(&input);

        // Key hash uses DOMAIN_KEY (0x00) — reproduce the identity hash logic
        let mut hasher = Sha256::new();
        hasher.update([0x00u8]); // DOMAIN_KEY
        hasher.update(input);
        let key_h: [u8; 32] = hasher.finalize().into();

        assert_ne!(
            content_h.0, key_h,
            "Content hash and key hash must differ for the same input due to different domain separators"
        );
    }
}
