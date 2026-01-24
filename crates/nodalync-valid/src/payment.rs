//! Payment validation (§9.4).
//!
//! This module validates payment parameters:
//! - Amount >= price
//! - Correct recipient
//! - Channel state and balance
//! - Nonce validity
//! - Signature verification
//! - Provenance matching

use nodalync_crypto::{verify, PublicKey};
use nodalync_types::{Channel, ChannelState, Manifest, Payment, PeerId, ProvenanceEntry};

use crate::error::{ValidationError, ValidationResult};

/// Callback trait for looking up public keys by peer ID.
///
/// This abstraction allows validation to work without direct storage access.
pub trait PublicKeyLookup {
    /// Look up the public key for a peer.
    fn lookup(&self, peer_id: &PeerId) -> Option<PublicKey>;
}

/// Callback trait for checking if a peer has a bond.
pub trait BondChecker {
    /// Check if a peer has posted the required bond amount.
    fn has_bond(&self, peer_id: &PeerId, amount: u64) -> bool;
}

/// Validate a payment against channel and manifest.
///
/// Checks all payment validation rules from §9.4:
/// 1. `amount >= manifest.economics.price`
/// 2. `recipient == manifest.owner`
/// 3. `query_hash == manifest.hash`
/// 4. `channel.state == Open`
/// 5. `channel.their_balance >= amount`
/// 6. Payment nonce > channel nonce (replay prevention)
/// 7. Signature is valid
/// 8. Provenance matches manifest
///
/// # Arguments
///
/// * `payment` - The payment to validate
/// * `channel` - The payment channel
/// * `manifest` - The manifest for the queried content
/// * `payer_pubkey` - The payer's public key for signature verification
/// * `payment_nonce` - The payment's nonce value
///
/// # Returns
///
/// `Ok(())` if payment is valid, or `Err(ValidationError)`.
pub fn validate_payment(
    payment: &Payment,
    channel: &Channel,
    manifest: &Manifest,
    payer_pubkey: Option<&PublicKey>,
    payment_nonce: u64,
) -> ValidationResult<()> {
    // 1. Amount sufficient
    if payment.amount < manifest.economics.price {
        return Err(ValidationError::InsufficientPayment {
            amount: payment.amount,
            price: manifest.economics.price,
        });
    }

    // 2. Correct recipient
    if payment.recipient != manifest.owner {
        return Err(ValidationError::WrongRecipient {
            payment_recipient: format!("{}", payment.recipient),
            owner: format!("{}", manifest.owner),
        });
    }

    // 3. Query hash matches
    if payment.query_hash != manifest.hash {
        return Err(ValidationError::QueryHashMismatch);
    }

    // 4. Channel is open
    if channel.state != ChannelState::Open {
        return Err(ValidationError::ChannelNotOpen {
            state: format!("{:?}", channel.state),
        });
    }

    // 5. Sufficient balance
    if channel.their_balance < payment.amount {
        return Err(ValidationError::InsufficientChannelBalance {
            balance: channel.their_balance,
            amount: payment.amount,
        });
    }

    // 6. Valid nonce (prevents replay)
    if payment_nonce <= channel.nonce {
        return Err(ValidationError::InvalidNonce {
            nonce: payment_nonce,
            channel_nonce: channel.nonce,
        });
    }

    // 7. Verify signature (if public key provided)
    if let Some(pubkey) = payer_pubkey {
        if !verify_payment_signature(pubkey, payment) {
            return Err(ValidationError::InvalidPaymentSignature);
        }
    }

    // 8. Provenance matches manifest
    if !provenance_matches(&payment.provenance, &manifest.provenance.root_l0l1) {
        return Err(ValidationError::ProvenanceMismatch);
    }

    Ok(())
}

/// Validate payment without signature verification.
///
/// Use this for quick validation when the signature has already been verified
/// or when the payer's public key is not available.
pub fn validate_payment_basic(
    payment: &Payment,
    channel: &Channel,
    manifest: &Manifest,
    payment_nonce: u64,
) -> ValidationResult<()> {
    validate_payment(payment, channel, manifest, None, payment_nonce)
}

/// Verify a payment signature.
///
/// The signature covers the payment data (excluding the signature itself).
fn verify_payment_signature(pubkey: &PublicKey, payment: &Payment) -> bool {
    // Construct the message that was signed
    // This should match the signing process in nodalync-ops
    let message = construct_payment_message(payment);
    verify(pubkey, &message, &payment.signature)
}

/// Construct the message bytes for payment signing/verification.
fn construct_payment_message(payment: &Payment) -> Vec<u8> {
    // The payment message includes all fields except the signature
    // Format: channel_id || amount (u64 BE) || recipient || query_hash || timestamp (u64 BE)
    let mut message = Vec::new();
    message.extend_from_slice(payment.channel_id.as_ref());
    message.extend_from_slice(&payment.amount.to_be_bytes());
    message.extend_from_slice(payment.recipient.as_ref());
    message.extend_from_slice(payment.query_hash.as_ref());
    message.extend_from_slice(&payment.timestamp.to_be_bytes());
    message
}

/// Check if payment provenance matches manifest provenance.
///
/// The payment's provenance entries should match the manifest's root_l0l1 entries.
fn provenance_matches(payment_prov: &[ProvenanceEntry], manifest_prov: &[ProvenanceEntry]) -> bool {
    use std::collections::HashSet;

    // Build set of hashes from each
    let payment_hashes: HashSet<_> = payment_prov.iter().map(|e| e.hash).collect();
    let manifest_hashes: HashSet<_> = manifest_prov.iter().map(|e| e.hash).collect();

    payment_hashes == manifest_hashes
}

#[cfg(test)]
mod tests {
    use super::*;
    use nodalync_crypto::{content_hash, generate_identity, peer_id_from_public_key, sign};
    use nodalync_types::{Economics, Metadata, Signature, Visibility};

    fn test_peer_id() -> PeerId {
        let (_, public_key) = generate_identity();
        peer_id_from_public_key(&public_key)
    }

    fn create_test_manifest(content: &[u8], price: u64) -> Manifest {
        let hash = content_hash(content);
        let owner = test_peer_id();
        let metadata = Metadata::new("Test", content.len() as u64);
        let mut manifest = Manifest::new_l0(hash, owner, metadata, 1234567890);
        manifest.economics = Economics::with_price(price);
        manifest
    }

    fn create_test_channel(peer: PeerId, their_balance: u64) -> Channel {
        let channel_id = content_hash(b"channel");
        let mut channel = Channel::new(channel_id, peer, 1000, 1000);
        channel.mark_open(their_balance, 2000);
        channel
    }

    fn create_test_payment(manifest: &Manifest, channel: &Channel, amount: u64) -> (Payment, u64) {
        let payment_id = content_hash(b"payment");
        let provenance = manifest.provenance.root_l0l1.clone();
        let nonce = channel.nonce + 1;

        let payment = Payment::new(
            payment_id,
            channel.channel_id,
            amount,
            manifest.owner,
            manifest.hash,
            provenance,
            1234567890,
            Signature([0u8; 64]), // Dummy signature
        );

        (payment, nonce)
    }

    #[test]
    fn test_valid_payment() {
        let manifest = create_test_manifest(b"Content", 100);
        let channel = create_test_channel(manifest.owner, 1000);
        let (payment, nonce) = create_test_payment(&manifest, &channel, 100);

        let result = validate_payment_basic(&payment, &channel, &manifest, nonce);
        assert!(result.is_ok());
    }

    #[test]
    fn test_insufficient_payment() {
        let manifest = create_test_manifest(b"Content", 100);
        let channel = create_test_channel(manifest.owner, 1000);
        let (payment, nonce) = create_test_payment(&manifest, &channel, 50); // Less than price

        let result = validate_payment_basic(&payment, &channel, &manifest, nonce);
        assert!(matches!(
            result,
            Err(ValidationError::InsufficientPayment {
                amount: 50,
                price: 100
            })
        ));
    }

    #[test]
    fn test_payment_exceeds_price_ok() {
        let manifest = create_test_manifest(b"Content", 100);
        let channel = create_test_channel(manifest.owner, 1000);
        let (payment, nonce) = create_test_payment(&manifest, &channel, 200); // More than price

        let result = validate_payment_basic(&payment, &channel, &manifest, nonce);
        assert!(result.is_ok());
    }

    #[test]
    fn test_wrong_recipient() {
        let manifest = create_test_manifest(b"Content", 100);
        let channel = create_test_channel(manifest.owner, 1000);
        let (mut payment, nonce) = create_test_payment(&manifest, &channel, 100);
        payment.recipient = test_peer_id(); // Wrong recipient

        let result = validate_payment_basic(&payment, &channel, &manifest, nonce);
        assert!(matches!(
            result,
            Err(ValidationError::WrongRecipient { .. })
        ));
    }

    #[test]
    fn test_query_hash_mismatch() {
        let manifest = create_test_manifest(b"Content", 100);
        let channel = create_test_channel(manifest.owner, 1000);
        let (mut payment, nonce) = create_test_payment(&manifest, &channel, 100);
        payment.query_hash = content_hash(b"different"); // Wrong hash

        let result = validate_payment_basic(&payment, &channel, &manifest, nonce);
        assert!(matches!(result, Err(ValidationError::QueryHashMismatch)));
    }

    #[test]
    fn test_channel_not_open() {
        let manifest = create_test_manifest(b"Content", 100);
        let mut channel = create_test_channel(manifest.owner, 1000);
        channel.mark_closing(3000); // Close the channel
        let (payment, nonce) = create_test_payment(&manifest, &channel, 100);

        let result = validate_payment_basic(&payment, &channel, &manifest, nonce);
        assert!(matches!(
            result,
            Err(ValidationError::ChannelNotOpen { .. })
        ));
    }

    #[test]
    fn test_insufficient_channel_balance() {
        let manifest = create_test_manifest(b"Content", 100);
        let channel = create_test_channel(manifest.owner, 50); // Only 50 balance
        let (payment, nonce) = create_test_payment(&manifest, &channel, 100);

        let result = validate_payment_basic(&payment, &channel, &manifest, nonce);
        assert!(matches!(
            result,
            Err(ValidationError::InsufficientChannelBalance {
                balance: 50,
                amount: 100
            })
        ));
    }

    #[test]
    fn test_invalid_nonce() {
        let manifest = create_test_manifest(b"Content", 100);
        let mut channel = create_test_channel(manifest.owner, 1000);
        channel.nonce = 5; // Set channel nonce
        let (payment, _) = create_test_payment(&manifest, &channel, 100);
        let old_nonce = 3; // Nonce <= channel nonce

        let result = validate_payment_basic(&payment, &channel, &manifest, old_nonce);
        assert!(matches!(
            result,
            Err(ValidationError::InvalidNonce {
                nonce: 3,
                channel_nonce: 5
            })
        ));
    }

    #[test]
    fn test_nonce_equal_to_channel_nonce_fails() {
        let manifest = create_test_manifest(b"Content", 100);
        let mut channel = create_test_channel(manifest.owner, 1000);
        channel.nonce = 5;
        let (payment, _) = create_test_payment(&manifest, &channel, 100);

        let result = validate_payment_basic(&payment, &channel, &manifest, 5);
        assert!(matches!(result, Err(ValidationError::InvalidNonce { .. })));
    }

    #[test]
    fn test_provenance_mismatch() {
        let manifest = create_test_manifest(b"Content", 100);
        let channel = create_test_channel(manifest.owner, 1000);
        let (mut payment, nonce) = create_test_payment(&manifest, &channel, 100);
        // Change provenance
        payment.provenance = vec![ProvenanceEntry::new(
            content_hash(b"different"),
            test_peer_id(),
            Visibility::Shared,
        )];

        let result = validate_payment_basic(&payment, &channel, &manifest, nonce);
        assert!(matches!(result, Err(ValidationError::ProvenanceMismatch)));
    }

    #[test]
    fn test_payment_signature_verification() {
        let (private_key, public_key) = generate_identity();
        let _payer = peer_id_from_public_key(&public_key);
        let owner = test_peer_id();

        let manifest = create_test_manifest(b"Content", 100);
        let channel = create_test_channel(owner, 1000);

        // Create payment and sign it
        let payment_id = content_hash(b"payment");
        let provenance = manifest.provenance.root_l0l1.clone();
        let _nonce = channel.nonce + 1;

        let mut payment = Payment::new(
            payment_id,
            channel.channel_id,
            100,
            manifest.owner,
            manifest.hash,
            provenance,
            1234567890,
            Signature([0u8; 64]),
        );

        // Sign the payment
        let message = construct_payment_message(&payment);
        payment.signature = sign(&private_key, &message);

        // Verify with correct key
        assert!(verify_payment_signature(&public_key, &payment));

        // Verify with wrong key fails
        let (_, wrong_key) = generate_identity();
        assert!(!verify_payment_signature(&wrong_key, &payment));
    }

    #[test]
    fn test_provenance_matches_function() {
        let hash1 = content_hash(b"hash1");
        let hash2 = content_hash(b"hash2");
        let owner = test_peer_id();

        let prov1 = vec![
            ProvenanceEntry::new(hash1, owner, Visibility::Shared),
            ProvenanceEntry::new(hash2, owner, Visibility::Shared),
        ];

        let prov2 = vec![
            ProvenanceEntry::new(hash2, owner, Visibility::Unlisted), // Different visibility OK
            ProvenanceEntry::new(hash1, owner, Visibility::Shared),   // Different order OK
        ];

        // Same hashes should match
        assert!(provenance_matches(&prov1, &prov2));

        // Different hashes should not match
        let prov3 = vec![ProvenanceEntry::new(hash1, owner, Visibility::Shared)];
        assert!(!provenance_matches(&prov1, &prov3));
    }
}
