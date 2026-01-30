//! Identity management (Spec §3.2)
//!
//! Provides Ed25519 keypair generation and PeerId derivation.
//!
//! PeerId is derived from a public key:
//! ```text
//! PeerId = H(0x00 || public_key)[0:20]
//! ```
//!
//! Human-readable format: `ndl1` + base58(PeerId)

use ed25519_dalek::SigningKey;
use rand::rngs::OsRng;
use sha2::{Digest, Sha256};

use crate::error::CryptoError;
use crate::{PeerId, PrivateKey, PublicKey};

/// Domain separator for key hashing (Ed25519 key type)
const DOMAIN_KEY: u8 = 0x00;

/// Human-readable PeerId prefix
const PEER_ID_PREFIX: &str = "ndl1";

/// Generate a new Ed25519 identity (keypair).
///
/// Uses the operating system's cryptographically secure random number generator.
///
/// # Example
/// ```
/// use nodalync_crypto::generate_identity;
///
/// let (private_key, public_key) = generate_identity();
/// ```
pub fn generate_identity() -> (PrivateKey, PublicKey) {
    let mut csprng = OsRng;
    let signing_key = SigningKey::generate(&mut csprng);
    let verifying_key = signing_key.verifying_key();

    let private_key = PrivateKey::from_signing_key(&signing_key);
    let public_key = PublicKey(verifying_key.to_bytes());

    (private_key, public_key)
}

/// Derive a PeerId from a public key.
///
/// # Algorithm
/// ```text
/// PeerId = H(0x00 || public_key)[0:20]
/// ```
///
/// # Example
/// ```
/// use nodalync_crypto::{generate_identity, peer_id_from_public_key};
///
/// let (_, public_key) = generate_identity();
/// let peer_id = peer_id_from_public_key(&public_key);
/// assert_eq!(peer_id.0.len(), 20);
/// ```
pub fn peer_id_from_public_key(public_key: &PublicKey) -> PeerId {
    let mut hasher = Sha256::new();
    hasher.update([DOMAIN_KEY]);
    hasher.update(public_key.0);
    let hash: [u8; 32] = hasher.finalize().into();

    // Truncate to 20 bytes
    let mut peer_id = [0u8; 20];
    peer_id.copy_from_slice(&hash[..20]);
    PeerId(peer_id)
}

/// Convert a PeerId to its human-readable string format.
///
/// Format: `ndl1` + base58(PeerId)
///
/// # Example
/// ```
/// use nodalync_crypto::{generate_identity, peer_id_from_public_key, peer_id_to_string};
///
/// let (_, public_key) = generate_identity();
/// let peer_id = peer_id_from_public_key(&public_key);
/// let encoded = peer_id_to_string(&peer_id);
/// assert!(encoded.starts_with("ndl1"));
/// ```
pub fn peer_id_to_string(peer_id: &PeerId) -> String {
    let encoded = bs58::encode(&peer_id.0).into_string();
    format!("{}{}", PEER_ID_PREFIX, encoded)
}

/// Parse a human-readable PeerId string.
///
/// # Errors
/// - `InvalidPeerIdPrefix` if the string doesn't start with `ndl1`
/// - `InvalidBase58` if the base58 decoding fails
/// - `InvalidKeyLength` if the decoded data isn't 20 bytes
///
/// # Example
/// ```
/// use nodalync_crypto::{generate_identity, peer_id_from_public_key, peer_id_to_string, peer_id_from_string};
///
/// let (_, public_key) = generate_identity();
/// let peer_id = peer_id_from_public_key(&public_key);
/// let encoded = peer_id_to_string(&peer_id);
/// let decoded = peer_id_from_string(&encoded).unwrap();
/// assert_eq!(peer_id.0, decoded.0);
/// ```
pub fn peer_id_from_string(s: &str) -> Result<PeerId, CryptoError> {
    // Check prefix
    if !s.starts_with(PEER_ID_PREFIX) {
        let prefix = if s.len() >= 4 { &s[..4] } else { s };
        return Err(CryptoError::InvalidPeerIdPrefix(prefix.to_string()));
    }

    // Decode base58 part
    let base58_part = &s[PEER_ID_PREFIX.len()..];
    if base58_part.is_empty() {
        return Err(CryptoError::InvalidPeerIdFormat(
            "Missing data after prefix".to_string(),
        ));
    }

    let decoded = bs58::decode(base58_part)
        .into_vec()
        .map_err(|e| CryptoError::InvalidBase58(e.to_string()))?;

    // Verify length
    if decoded.len() != 20 {
        return Err(CryptoError::InvalidKeyLength {
            expected: 20,
            actual: decoded.len(),
        });
    }

    let mut peer_id = [0u8; 20];
    peer_id.copy_from_slice(&decoded);
    Ok(PeerId(peer_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_identity() {
        let (_private_key, public_key) = generate_identity();
        assert_eq!(public_key.0.len(), 32);
    }

    #[test]
    fn test_peer_id_deterministic() {
        let (_, public_key) = generate_identity();
        let id1 = peer_id_from_public_key(&public_key);
        let id2 = peer_id_from_public_key(&public_key);
        assert_eq!(id1.0, id2.0);
    }

    #[test]
    fn test_peer_id_roundtrip() {
        let (_, public_key) = generate_identity();
        let peer_id = peer_id_from_public_key(&public_key);
        let encoded = peer_id_to_string(&peer_id);
        let decoded = peer_id_from_string(&encoded).unwrap();
        assert_eq!(peer_id.0, decoded.0);
    }

    #[test]
    fn test_invalid_prefix() {
        let result = peer_id_from_string("xyz1abc");
        assert!(matches!(result, Err(CryptoError::InvalidPeerIdPrefix(_))));
    }
}
