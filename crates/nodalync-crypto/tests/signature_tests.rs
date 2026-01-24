//! Signature tests for nodalync-crypto (Spec §3.3)

use nodalync_crypto::{generate_identity, sign, verify, Signature, SignedMessage};

/// §3.3 Test: Valid signature verifies
#[test]
fn valid_signature_verifies() {
    let (private_key, public_key) = generate_identity();
    let message = b"This is a test message to sign";

    let signature = sign(&private_key, message);
    assert!(
        verify(&public_key, message, &signature),
        "Valid signature should verify"
    );
}

/// §3.3 Test: Tampered message fails verification
#[test]
fn tampered_message_fails_verification() {
    let (private_key, public_key) = generate_identity();
    let original = b"Original message";
    let tampered = b"Tampered message";

    let signature = sign(&private_key, original);
    assert!(
        !verify(&public_key, tampered, &signature),
        "Tampered message should fail verification"
    );
}

/// §3.3 Test: Wrong key fails verification
#[test]
fn wrong_key_fails_verification() {
    let (private_key1, _public_key1) = generate_identity();
    let (_private_key2, public_key2) = generate_identity();
    let message = b"Message signed by key 1";

    let signature = sign(&private_key1, message);
    assert!(
        !verify(&public_key2, message, &signature),
        "Wrong public key should fail verification"
    );
}

/// §3.3 Test: Truncated signature fails
#[test]
fn truncated_signature_fails() {
    let (private_key, public_key) = generate_identity();
    let message = b"Test message";

    let signature = sign(&private_key, message);

    // Create truncated signature (only first 32 bytes, padded with zeros)
    let mut truncated = [0u8; 64];
    truncated[..32].copy_from_slice(&signature.0[..32]);
    let truncated_sig = Signature(truncated);

    // This should fail because the signature is corrupted
    assert!(
        !verify(&public_key, message, &truncated_sig),
        "Truncated/corrupted signature should fail verification"
    );
}

/// Test signing empty message
#[test]
fn sign_empty_message() {
    let (private_key, public_key) = generate_identity();
    let message: &[u8] = b"";

    let signature = sign(&private_key, message);
    assert!(
        verify(&public_key, message, &signature),
        "Empty message signature should verify"
    );
}

/// Test signing large message
#[test]
fn sign_large_message() {
    let (private_key, public_key) = generate_identity();
    let message = vec![0xFFu8; 1_000_000]; // 1MB message

    let signature = sign(&private_key, &message);
    assert!(
        verify(&public_key, &message, &signature),
        "Large message signature should verify"
    );
}

/// Test signature is deterministic (same message, same key = same signature)
/// Note: Ed25519 in dalek is deterministic
#[test]
fn signature_is_deterministic() {
    let (private_key, _public_key) = generate_identity();
    let message = b"Deterministic test";

    let sig1 = sign(&private_key, message);
    let sig2 = sign(&private_key, message);

    assert_eq!(
        sig1.0, sig2.0,
        "Same message and key should produce same signature"
    );
}

/// Test SignedMessage construction and verification
#[test]
fn signed_message_construction() {
    let (private_key, public_key) = generate_identity();
    let payload = b"Payload data".to_vec();

    let signature = sign(&private_key, &payload);
    let peer_id = nodalync_crypto::peer_id_from_public_key(&public_key);

    let signed_message = SignedMessage {
        payload: payload.clone(),
        signer: peer_id,
        signature,
    };

    // Verify the signed message
    assert!(
        verify(
            &public_key,
            &signed_message.payload,
            &signed_message.signature
        ),
        "SignedMessage should be verifiable"
    );
}

/// Test signature on message with special characters
#[test]
fn sign_message_with_special_chars() {
    let (private_key, public_key) = generate_identity();
    let message = "Unicode: \u{1F600} \u{1F60D} \u{1F389}".as_bytes();

    let signature = sign(&private_key, message);
    assert!(verify(&public_key, message, &signature));
}

/// Test that flipping a single bit in signature invalidates it
#[test]
fn single_bit_flip_invalidates_signature() {
    let (private_key, public_key) = generate_identity();
    let message = b"Bit flip test";

    let signature = sign(&private_key, message);

    // Flip a bit in the signature
    let mut corrupted = signature.0;
    corrupted[0] ^= 0x01;
    let corrupted_sig = Signature(corrupted);

    assert!(
        !verify(&public_key, message, &corrupted_sig),
        "Single bit flip should invalidate signature"
    );
}

/// Test Signature debug display
#[test]
fn signature_debug_display() {
    let (private_key, _) = generate_identity();
    let signature = sign(&private_key, b"test");
    let debug = format!("{:?}", signature);
    assert!(debug.contains("Signature"));
}
