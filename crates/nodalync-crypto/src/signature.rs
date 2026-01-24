//! Signature creation and verification (Spec §3.3)
//!
//! All protocol messages requiring authentication are signed using Ed25519.
//!
//! Signatures are computed over the hash of the message:
//! ```text
//! signature = Ed25519_Sign(private_key, H(message))
//! ```

use ed25519_dalek::{Signature as DalekSignature, Signer, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};

use crate::{PeerId, PrivateKey, PublicKey, Signature};

/// Sign a message with a private key.
///
/// The message is first hashed, then the hash is signed.
///
/// # Algorithm
/// ```text
/// Ed25519_Sign(private_key, H(message))
/// ```
///
/// # Example
/// ```
/// use nodalync_crypto::{generate_identity, sign, verify};
///
/// let (private_key, public_key) = generate_identity();
/// let message = b"Hello, world!";
/// let signature = sign(&private_key, message);
/// assert!(verify(&public_key, message, &signature));
/// ```
pub fn sign(private_key: &PrivateKey, message: &[u8]) -> Signature {
    // Hash the message first
    let mut hasher = Sha256::new();
    hasher.update(message);
    let message_hash: [u8; 32] = hasher.finalize().into();

    // Sign the hash
    let signing_key = private_key.to_signing_key();
    let sig: DalekSignature = signing_key.sign(&message_hash);

    Signature(sig.to_bytes())
}

/// Verify a signature against a public key and message.
///
/// # Returns
/// `true` if the signature is valid, `false` otherwise.
///
/// # Example
/// ```
/// use nodalync_crypto::{generate_identity, sign, verify};
///
/// let (private_key, public_key) = generate_identity();
/// let message = b"Hello, world!";
/// let signature = sign(&private_key, message);
///
/// assert!(verify(&public_key, message, &signature));
/// assert!(!verify(&public_key, b"Different message", &signature));
/// ```
pub fn verify(public_key: &PublicKey, message: &[u8], signature: &Signature) -> bool {
    // Hash the message first
    let mut hasher = Sha256::new();
    hasher.update(message);
    let message_hash: [u8; 32] = hasher.finalize().into();

    // Convert to dalek types
    let Ok(verifying_key) = VerifyingKey::from_bytes(&public_key.0) else {
        return false;
    };

    let sig = DalekSignature::from_bytes(&signature.0);

    // Verify
    verifying_key.verify(&message_hash, &sig).is_ok()
}

/// A message with its signature and signer information.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedMessage {
    /// The message payload
    pub payload: Vec<u8>,
    /// The PeerId of the signer
    pub signer: PeerId,
    /// The signature over the payload
    pub signature: Signature,
}

impl SignedMessage {
    /// Create a new signed message.
    ///
    /// # Example
    /// ```
    /// use nodalync_crypto::{generate_identity, peer_id_from_public_key, SignedMessage};
    ///
    /// let (private_key, public_key) = generate_identity();
    /// let peer_id = peer_id_from_public_key(&public_key);
    /// let payload = b"Message payload".to_vec();
    ///
    /// let signed = SignedMessage::new(&private_key, peer_id, payload);
    /// ```
    pub fn new(private_key: &PrivateKey, signer: PeerId, payload: Vec<u8>) -> Self {
        let signature = sign(private_key, &payload);
        Self {
            payload,
            signer,
            signature,
        }
    }

    /// Verify this signed message against the given public key.
    ///
    /// # Returns
    /// `true` if the signature is valid for the payload and public key.
    pub fn verify(&self, public_key: &PublicKey) -> bool {
        verify(public_key, &self.payload, &self.signature)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{generate_identity, peer_id_from_public_key};

    #[test]
    fn test_sign_verify() {
        let (private_key, public_key) = generate_identity();
        let message = b"test message";
        let signature = sign(&private_key, message);
        assert!(verify(&public_key, message, &signature));
    }

    #[test]
    fn test_wrong_key_fails() {
        let (private_key1, _) = generate_identity();
        let (_, public_key2) = generate_identity();
        let message = b"test message";
        let signature = sign(&private_key1, message);
        assert!(!verify(&public_key2, message, &signature));
    }

    #[test]
    fn test_tampered_message_fails() {
        let (private_key, public_key) = generate_identity();
        let message = b"test message";
        let signature = sign(&private_key, message);
        assert!(!verify(&public_key, b"different message", &signature));
    }

    #[test]
    fn test_signed_message() {
        let (private_key, public_key) = generate_identity();
        let peer_id = peer_id_from_public_key(&public_key);
        let payload = b"payload".to_vec();

        let signed = SignedMessage::new(&private_key, peer_id, payload);
        assert!(signed.verify(&public_key));
    }
}
