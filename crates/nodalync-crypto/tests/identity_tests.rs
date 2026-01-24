//! Identity tests for nodalync-crypto (Spec §3.2)

use nodalync_crypto::{
    generate_identity, peer_id_from_public_key, peer_id_from_string, peer_id_to_string,
};

/// §3.2 Test: Same public key produces same PeerId
#[test]
fn same_public_key_produces_same_peer_id() {
    let (_, public_key) = generate_identity();
    let peer_id1 = peer_id_from_public_key(&public_key);
    let peer_id2 = peer_id_from_public_key(&public_key);
    assert_eq!(peer_id1.0, peer_id2.0, "Same public key should produce same PeerId");
}

/// §3.2 Test: Different public keys produce different PeerIds
#[test]
fn different_public_keys_produce_different_peer_ids() {
    let (_, public_key1) = generate_identity();
    let (_, public_key2) = generate_identity();
    let peer_id1 = peer_id_from_public_key(&public_key1);
    let peer_id2 = peer_id_from_public_key(&public_key2);
    assert_ne!(
        peer_id1.0, peer_id2.0,
        "Different public keys should produce different PeerIds"
    );
}

/// §3.2 Test: Human-readable encoding roundtrips
#[test]
fn human_readable_encoding_roundtrips() {
    let (_, public_key) = generate_identity();
    let peer_id = peer_id_from_public_key(&public_key);

    let encoded = peer_id_to_string(&peer_id);
    let decoded = peer_id_from_string(&encoded).expect("Should decode valid peer id string");

    assert_eq!(peer_id.0, decoded.0, "Roundtrip should preserve PeerId");
}

/// §3.2 Test: Human-readable format starts with ndl1 prefix
#[test]
fn human_readable_has_correct_prefix() {
    let (_, public_key) = generate_identity();
    let peer_id = peer_id_from_public_key(&public_key);
    let encoded = peer_id_to_string(&peer_id);

    assert!(
        encoded.starts_with("ndl1"),
        "Human-readable PeerId should start with 'ndl1' prefix, got: {}",
        encoded
    );
}

/// §3.2 Test: Invalid prefix rejected
#[test]
fn invalid_prefix_rejected() {
    // Valid base58 but wrong prefix
    let invalid = "abc1qpzry9x8gf2tvdw0s3jn54khce6mua7l";
    let result = peer_id_from_string(invalid);
    assert!(result.is_err(), "Invalid prefix should be rejected");
}

/// §3.2 Test: Invalid checksum rejected
#[test]
fn invalid_data_rejected() {
    // Correct prefix but garbage data
    let invalid = "ndl1INVALID!!!";
    let result = peer_id_from_string(invalid);
    assert!(result.is_err(), "Invalid data should be rejected");
}

/// Test PeerId is exactly 20 bytes
#[test]
fn peer_id_is_20_bytes() {
    let (_, public_key) = generate_identity();
    let peer_id = peer_id_from_public_key(&public_key);
    assert_eq!(peer_id.0.len(), 20, "PeerId should be exactly 20 bytes");
}

/// Test multiple roundtrips are consistent
#[test]
fn multiple_roundtrips_consistent() {
    let (_, public_key) = generate_identity();
    let peer_id = peer_id_from_public_key(&public_key);

    for _ in 0..10 {
        let encoded = peer_id_to_string(&peer_id);
        let decoded = peer_id_from_string(&encoded).unwrap();
        assert_eq!(peer_id.0, decoded.0);
    }
}

/// Test identity generation produces valid keys
#[test]
fn identity_generation_produces_valid_keys() {
    let (_private_key, public_key) = generate_identity();
    // Private key should be 32 bytes (accessed via accessor in implementation)
    // Public key should be 32 bytes
    assert_eq!(public_key.0.len(), 32, "Public key should be 32 bytes");
}

/// Test empty string rejected
#[test]
fn empty_string_rejected() {
    let result = peer_id_from_string("");
    assert!(result.is_err(), "Empty string should be rejected");
}

/// Test just prefix rejected
#[test]
fn just_prefix_rejected() {
    let result = peer_id_from_string("ndl1");
    assert!(result.is_err(), "Just prefix without data should be rejected");
}

/// Test PeerId debug display
#[test]
fn peer_id_debug_display() {
    let (_, public_key) = generate_identity();
    let peer_id = peer_id_from_public_key(&public_key);
    let debug = format!("{:?}", peer_id);
    assert!(debug.contains("PeerId"));
}
