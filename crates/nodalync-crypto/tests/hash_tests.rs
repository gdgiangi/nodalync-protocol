//! Hash function tests for nodalync-crypto (Spec §3.1, §3.4)

use nodalync_crypto::{content_hash, verify_content};
use sha2::{Digest, Sha256};

/// §3.1 Test: Same content produces identical hash
#[test]
fn same_content_produces_identical_hash() {
    let content = b"Hello, Nodalync!";
    let hash1 = content_hash(content);
    let hash2 = content_hash(content);
    assert_eq!(hash1.0, hash2.0, "Same content should produce identical hash");
}

/// §3.1 Test: Different content produces different hash
#[test]
fn different_content_produces_different_hash() {
    let content1 = b"Hello, Nodalync!";
    let content2 = b"Hello, Nodalync?";
    let hash1 = content_hash(content1);
    let hash2 = content_hash(content2);
    assert_ne!(hash1.0, hash2.0, "Different content should produce different hash");
}

/// §3.1 Test: Domain separation - ContentHash(x) != H(x)
#[test]
fn domain_separation_prevents_collision() {
    let content = b"test content";

    // Raw SHA-256 hash
    let mut hasher = Sha256::new();
    hasher.update(content);
    let raw_hash: [u8; 32] = hasher.finalize().into();

    // Content hash with domain separator
    let content_hash_result = content_hash(content);

    assert_ne!(
        raw_hash, content_hash_result.0,
        "ContentHash should differ from raw H(x) due to domain separation"
    );
}

/// §3.4 Test: Verify content succeeds for valid content
#[test]
fn verify_content_succeeds_for_valid_content() {
    let content = b"Some valid content for verification";
    let hash = content_hash(content);
    assert!(
        verify_content(content, &hash),
        "Verify should succeed for valid content"
    );
}

/// §3.4 Test: Verify content fails for tampered content
#[test]
fn verify_content_fails_for_tampered_content() {
    let original = b"Original content";
    let hash = content_hash(original);

    let tampered = b"Original contenT"; // Changed last character
    assert!(
        !verify_content(tampered, &hash),
        "Verify should fail for tampered content"
    );
}

/// Test empty content hashing
#[test]
fn empty_content_produces_valid_hash() {
    let content: &[u8] = b"";
    let hash = content_hash(content);
    // Should produce a valid 32-byte hash
    assert_eq!(hash.0.len(), 32);
    // Verify roundtrip
    assert!(verify_content(content, &hash));
}

/// Test large content hashing
#[test]
fn large_content_produces_valid_hash() {
    let content = vec![0xABu8; 1_000_000]; // 1MB of data
    let hash = content_hash(&content);
    assert!(verify_content(&content, &hash));
}

/// Test that length prefix affects hash
#[test]
fn length_prefix_affects_hash() {
    // Two different contents that would produce same hash without length prefix
    let content1 = b"ab";
    let content2 = b"a";

    let hash1 = content_hash(content1);
    let hash2 = content_hash(content2);

    assert_ne!(hash1.0, hash2.0, "Different length content should have different hashes");
}

/// Test Hash debug display
#[test]
fn hash_debug_display() {
    let content = b"test";
    let hash = content_hash(content);
    let debug = format!("{:?}", hash);
    assert!(debug.contains("Hash"));
}
