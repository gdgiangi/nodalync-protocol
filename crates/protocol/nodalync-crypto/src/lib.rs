//! Cryptographic primitives for the Nodalync protocol.
//!
//! This crate provides all cryptographic functionality required by the Nodalync protocol:
//!
//! - **Content Hashing** (§3.1): SHA-256 with domain separation
//! - **Identity** (§3.2): Ed25519 keypair generation and PeerId derivation
//! - **Signatures** (§3.3): Message signing and verification
//! - **Content Addressing** (§3.4): Content verification by hash
//!
//! # Example
//!
//! ```
//! use nodalync_crypto::{
//!     content_hash, verify_content,
//!     generate_identity, peer_id_from_public_key, peer_id_to_string,
//!     sign, verify, SignedMessage,
//! };
//!
//! // Generate an identity
//! let (private_key, public_key) = generate_identity();
//! let peer_id = peer_id_from_public_key(&public_key);
//! println!("My PeerId: {}", peer_id_to_string(&peer_id));
//!
//! // Hash some content
//! let content = b"Hello, Nodalync!";
//! let hash = content_hash(content);
//! assert!(verify_content(content, &hash));
//!
//! // Sign a message
//! let message = b"Important message";
//! let signature = sign(&private_key, message);
//! assert!(verify(&public_key, message, &signature));
//! ```

mod error;
mod hash;
mod identity;
mod serde_impl;
mod signature;

pub use error::CryptoError;
pub use hash::{content_hash, verify_content};
pub use identity::{
    generate_identity, peer_id_from_public_key, peer_id_from_string, peer_id_to_string,
};
pub use signature::{sign, verify, SignedMessage};

use ed25519_dalek::SigningKey;
use zeroize::{Zeroize, ZeroizeOnDrop};

/// A 32-byte SHA-256 hash.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Hash(pub [u8; 32]);

impl std::fmt::Debug for Hash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Hash({})", hex_string(&self.0[..8]))
    }
}

impl std::fmt::Display for Hash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for byte in &self.0 {
            write!(f, "{:02x}", byte)?;
        }
        Ok(())
    }
}

impl AsRef<[u8]> for Hash {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

/// An Ed25519 private key (32 bytes).
///
/// This should be kept secret and never exposed.
/// Implements Zeroize + ZeroizeOnDrop to clear key material from memory.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct PrivateKey([u8; 32]);

impl PrivateKey {
    /// Create a PrivateKey from raw bytes.
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Get the raw bytes of the private key.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Create from an ed25519-dalek SigningKey.
    pub(crate) fn from_signing_key(key: &SigningKey) -> Self {
        Self(key.to_bytes())
    }

    /// Convert to an ed25519-dalek SigningKey.
    pub(crate) fn to_signing_key(&self) -> SigningKey {
        SigningKey::from_bytes(&self.0)
    }
}

impl std::fmt::Debug for PrivateKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PrivateKey([REDACTED])")
    }
}

/// An Ed25519 public key (32 bytes).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct PublicKey(pub [u8; 32]);

impl PublicKey {
    /// Create a PublicKey from raw bytes.
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Get the raw bytes of the public key.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl std::fmt::Debug for PublicKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PublicKey({})", hex_string(&self.0[..8]))
    }
}

impl AsRef<[u8]> for PublicKey {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

/// An Ed25519 signature (64 bytes).
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Signature(pub [u8; 64]);

impl Signature {
    /// Create a Signature from raw bytes.
    pub fn from_bytes(bytes: [u8; 64]) -> Self {
        Self(bytes)
    }

    /// Get the raw bytes of the signature.
    pub fn as_bytes(&self) -> &[u8; 64] {
        &self.0
    }
}

impl std::fmt::Debug for Signature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Signature({})", hex_string(&self.0[..8]))
    }
}

impl AsRef<[u8]> for Signature {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

/// A peer identifier derived from a public key (20 bytes).
///
/// The PeerId is the first 20 bytes of `H(0x00 || public_key)`.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct PeerId(pub [u8; 20]);

/// Sentinel value representing an unknown peer.
///
/// Used when content is discovered via DHT announcement but the owner's
/// Nodalync identity is not known. For example, when we receive a content
/// announcement via GossipSub, we know the libp2p peer ID but not the
/// Nodalync PeerId.
pub const UNKNOWN_PEER_ID: PeerId = PeerId([0u8; 20]);

impl PeerId {
    /// Create a PeerId from raw bytes.
    pub fn from_bytes(bytes: [u8; 20]) -> Self {
        Self(bytes)
    }

    /// Get the raw bytes of the PeerId.
    pub fn as_bytes(&self) -> &[u8; 20] {
        &self.0
    }
}

impl std::fmt::Debug for PeerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PeerId({})", peer_id_to_string(self))
    }
}

impl std::fmt::Display for PeerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", peer_id_to_string(self))
    }
}

impl AsRef<[u8]> for PeerId {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

/// Milliseconds since Unix epoch.
pub type Timestamp = u64;

/// Helper function to convert bytes to hex string (for Debug output).
fn hex_string(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>()
        + "..."
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_display() {
        let hash = content_hash(b"test");
        let s = format!("{}", hash);
        assert_eq!(s.len(), 64); // 32 bytes as hex
    }

    #[test]
    fn test_peer_id_display() {
        let (_, public_key) = generate_identity();
        let peer_id = peer_id_from_public_key(&public_key);
        let s = format!("{}", peer_id);
        assert!(s.starts_with("ndl1"));
    }

    #[test]
    fn test_types_are_copy() {
        let hash = content_hash(b"test");
        let hash_copy = hash; // This should compile because Hash is Copy
        assert_eq!(hash.0, hash_copy.0);

        let (_, public_key) = generate_identity();
        let pk_copy = public_key; // This should compile because PublicKey is Copy
        assert_eq!(public_key.0, pk_copy.0);
    }

    #[test]
    fn test_private_key_debug_redacted() {
        let (private_key, _) = generate_identity();
        let debug = format!("{:?}", private_key);
        assert!(debug.contains("REDACTED"));
        assert!(!debug.contains(&format!("{:02x}", private_key.0[0])));
    }

    #[test]
    fn test_private_key_implements_zeroize() {
        // Regression test: PrivateKey must implement Zeroize so key material
        // is cleared from memory when dropped.
        use zeroize::Zeroize;

        let (mut private_key, _) = generate_identity();
        // Verify key has non-zero bytes
        assert!(private_key.0.iter().any(|&b| b != 0));

        // Zeroize should clear the key
        private_key.zeroize();
        assert!(
            private_key.0.iter().all(|&b| b == 0),
            "PrivateKey bytes should be zeroed after zeroize()"
        );
    }
}
