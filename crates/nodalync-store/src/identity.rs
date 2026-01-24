//! Identity storage with encryption.
//!
//! This module provides encrypted storage for Ed25519 private keys.
//! Keys are encrypted at rest using AES-256-GCM with a key derived
//! from a user password using Argon2id.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use argon2::{password_hash::SaltString, Argon2, PasswordHasher};
use nodalync_crypto::{generate_identity, peer_id_from_public_key, PeerId, PrivateKey, PublicKey};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};

use crate::error::{Result, StoreError};

/// Nonce length for AES-GCM.
const NONCE_LEN: usize = 12;

/// Stored identity format.
#[derive(Serialize, Deserialize)]
struct StoredIdentity {
    /// Argon2 salt (base64 encoded).
    salt: String,
    /// AES-GCM nonce (base64 encoded).
    nonce: String,
    /// Encrypted private key (base64 encoded).
    ciphertext: String,
    /// Public key (for verification).
    public_key: [u8; 32],
}

/// Identity store for encrypted key management.
///
/// Stores and retrieves Ed25519 keypairs with encryption at rest.
pub struct IdentityStore {
    /// Directory for identity files.
    identity_dir: PathBuf,
}

impl IdentityStore {
    /// Create a new identity store.
    ///
    /// The directory will be created if it doesn't exist.
    pub fn new(identity_dir: impl AsRef<Path>) -> Result<Self> {
        let identity_dir = identity_dir.as_ref().to_path_buf();
        fs::create_dir_all(&identity_dir)?;
        Ok(Self { identity_dir })
    }

    /// Path to the keypair file.
    fn keypair_path(&self) -> PathBuf {
        self.identity_dir.join("keypair.key")
    }

    /// Path to the peer ID file.
    fn peer_id_path(&self) -> PathBuf {
        self.identity_dir.join("peer_id")
    }

    /// Check if an identity exists.
    pub fn exists(&self) -> bool {
        self.keypair_path().exists()
    }

    /// Generate and store a new identity.
    ///
    /// Creates a new Ed25519 keypair, encrypts the private key with the
    /// provided password, and stores it.
    ///
    /// Returns the peer ID of the new identity.
    pub fn generate(&self, password: &str) -> Result<PeerId> {
        if self.exists() {
            return Err(StoreError::encryption("Identity already exists"));
        }

        let (private_key, public_key) = generate_identity();
        let peer_id = peer_id_from_public_key(&public_key);

        self.store_keypair(&private_key, &public_key, password)?;

        // Store peer_id in plaintext for quick lookup
        let mut file = File::create(self.peer_id_path())?;
        file.write_all(&peer_id.0)?;

        Ok(peer_id)
    }

    /// Store an existing keypair.
    ///
    /// Encrypts the private key with the provided password and stores it.
    pub fn store_keypair(
        &self,
        private_key: &PrivateKey,
        public_key: &PublicKey,
        password: &str,
    ) -> Result<()> {
        // Generate salt for Argon2
        let salt = SaltString::generate(&mut OsRng);

        // Derive encryption key using Argon2id
        let argon2 = Argon2::default();
        let password_hash = argon2
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| StoreError::encryption(format!("Key derivation failed: {}", e)))?;

        // Extract 32 bytes from the hash for AES-256
        let hash_bytes = password_hash
            .hash
            .ok_or_else(|| StoreError::encryption("Failed to extract hash bytes"))?;
        let key_bytes = hash_bytes.as_bytes();

        // Ensure we have at least 32 bytes
        if key_bytes.len() < 32 {
            return Err(StoreError::encryption("Derived key too short"));
        }
        let mut encryption_key = [0u8; 32];
        encryption_key.copy_from_slice(&key_bytes[..32]);

        // Generate random nonce for AES-GCM
        let mut nonce_bytes = [0u8; NONCE_LEN];
        rand::Rng::fill(&mut OsRng, &mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        // Encrypt the private key
        let cipher = Aes256Gcm::new_from_slice(&encryption_key)
            .map_err(|e| StoreError::encryption(format!("Cipher init failed: {}", e)))?;
        let ciphertext = cipher
            .encrypt(nonce, private_key.as_bytes().as_ref())
            .map_err(|e| StoreError::encryption(format!("Encryption failed: {}", e)))?;

        // Create stored identity structure
        let stored = StoredIdentity {
            salt: salt.to_string(),
            nonce: base64_encode(&nonce_bytes),
            ciphertext: base64_encode(&ciphertext),
            public_key: public_key.0,
        };

        // Write to file
        let json = serde_json::to_string_pretty(&stored)?;
        let mut file = File::create(self.keypair_path())?;
        file.write_all(json.as_bytes())?;

        // Store peer_id in plaintext for quick lookup
        let peer_id = peer_id_from_public_key(public_key);
        let mut peer_file = File::create(self.peer_id_path())?;
        peer_file.write_all(&peer_id.0)?;

        Ok(())
    }

    /// Load the keypair, decrypting with the provided password.
    ///
    /// Returns (private_key, public_key) if successful.
    pub fn load(&self, password: &str) -> Result<(PrivateKey, PublicKey)> {
        if !self.exists() {
            return Err(StoreError::IdentityNotFound);
        }

        // Read stored identity
        let mut file = File::open(self.keypair_path())?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        let stored: StoredIdentity = serde_json::from_str(&contents)?;

        // Parse salt
        let salt = SaltString::from_b64(&stored.salt)
            .map_err(|e| StoreError::encryption(format!("Invalid salt: {}", e)))?;

        // Derive decryption key
        let argon2 = Argon2::default();
        let password_hash = argon2
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| StoreError::encryption(format!("Key derivation failed: {}", e)))?;

        let hash_bytes = password_hash
            .hash
            .ok_or_else(|| StoreError::encryption("Failed to extract hash bytes"))?;
        let key_bytes = hash_bytes.as_bytes();

        if key_bytes.len() < 32 {
            return Err(StoreError::encryption("Derived key too short"));
        }
        let mut encryption_key = [0u8; 32];
        encryption_key.copy_from_slice(&key_bytes[..32]);

        // Parse nonce
        let nonce_bytes = base64_decode(&stored.nonce)?;
        let nonce = Nonce::from_slice(&nonce_bytes);

        // Parse ciphertext
        let ciphertext = base64_decode(&stored.ciphertext)?;

        // Decrypt
        let cipher = Aes256Gcm::new_from_slice(&encryption_key)
            .map_err(|e| StoreError::encryption(format!("Cipher init failed: {}", e)))?;
        let plaintext = cipher
            .decrypt(nonce, ciphertext.as_ref())
            .map_err(|_| StoreError::encryption("Decryption failed - wrong password?"))?;

        // Reconstruct private key
        if plaintext.len() != 32 {
            return Err(StoreError::encryption("Invalid decrypted key length"));
        }
        let mut key_bytes = [0u8; 32];
        key_bytes.copy_from_slice(&plaintext);
        let private_key = PrivateKey::from_bytes(key_bytes);
        let public_key = PublicKey::from_bytes(stored.public_key);

        Ok((private_key, public_key))
    }

    /// Get the peer ID without decrypting the private key.
    ///
    /// This is a quick lookup that doesn't require the password.
    pub fn peer_id(&self) -> Result<PeerId> {
        let path = self.peer_id_path();
        if !path.exists() {
            return Err(StoreError::IdentityNotFound);
        }

        let mut file = File::open(path)?;
        let mut bytes = [0u8; 20];
        file.read_exact(&mut bytes)?;
        Ok(PeerId::from_bytes(bytes))
    }

    /// Get the public key without decrypting the private key.
    pub fn public_key(&self) -> Result<PublicKey> {
        if !self.exists() {
            return Err(StoreError::IdentityNotFound);
        }

        let mut file = File::open(self.keypair_path())?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        let stored: StoredIdentity = serde_json::from_str(&contents)?;

        Ok(PublicKey::from_bytes(stored.public_key))
    }

    /// Delete the stored identity.
    ///
    /// This is irreversible and will destroy the keypair.
    pub fn delete(&self) -> Result<()> {
        if self.keypair_path().exists() {
            fs::remove_file(self.keypair_path())?;
        }
        if self.peer_id_path().exists() {
            fs::remove_file(self.peer_id_path())?;
        }
        Ok(())
    }
}

/// Base64 encode bytes.
fn base64_encode(bytes: &[u8]) -> String {
    use std::io::Write;
    let mut buf = Vec::new();
    {
        let mut encoder = base64_encoder(&mut buf);
        encoder.write_all(bytes).unwrap();
    }
    String::from_utf8(buf).unwrap()
}

fn base64_encoder(output: &mut Vec<u8>) -> impl std::io::Write + '_ {
    struct Base64Writer<'a>(&'a mut Vec<u8>);
    impl<'a> std::io::Write for Base64Writer<'a> {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            const ALPHABET: &[u8; 64] =
                b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

            for chunk in buf.chunks(3) {
                let b0 = chunk[0] as usize;
                let b1 = chunk.get(1).copied().unwrap_or(0) as usize;
                let b2 = chunk.get(2).copied().unwrap_or(0) as usize;

                self.0.push(ALPHABET[b0 >> 2]);
                self.0.push(ALPHABET[((b0 & 0x03) << 4) | (b1 >> 4)]);

                if chunk.len() > 1 {
                    self.0.push(ALPHABET[((b1 & 0x0f) << 2) | (b2 >> 6)]);
                } else {
                    self.0.push(b'=');
                }

                if chunk.len() > 2 {
                    self.0.push(ALPHABET[b2 & 0x3f]);
                } else {
                    self.0.push(b'=');
                }
            }
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    Base64Writer(output)
}

/// Base64 decode string to bytes.
fn base64_decode(s: &str) -> Result<Vec<u8>> {
    const DECODE_TABLE: [i8; 128] = [
        -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
        -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, 62, -1, -1,
        -1, 63, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, -1, -1, -1, -1, -1, -1, -1, 0, 1, 2, 3, 4,
        5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, -1, -1, -1,
        -1, -1, -1, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45,
        46, 47, 48, 49, 50, 51, -1, -1, -1, -1, -1,
    ];

    let bytes: Vec<u8> = s.bytes().filter(|&b| b != b'=').collect();
    let mut result = Vec::with_capacity(bytes.len() * 3 / 4);

    for chunk in bytes.chunks(4) {
        let mut buf = [0u8; 4];
        for (i, &b) in chunk.iter().enumerate() {
            if b >= 128 {
                return Err(StoreError::encryption("Invalid base64 character"));
            }
            let val = DECODE_TABLE[b as usize];
            if val < 0 {
                return Err(StoreError::encryption("Invalid base64 character"));
            }
            buf[i] = val as u8;
        }

        result.push((buf[0] << 2) | (buf[1] >> 4));
        if chunk.len() > 2 {
            result.push((buf[1] << 4) | (buf[2] >> 2));
        }
        if chunk.len() > 3 {
            result.push((buf[2] << 6) | buf[3]);
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_identity_generate_and_load() {
        let temp_dir = TempDir::new().unwrap();
        let store = IdentityStore::new(temp_dir.path()).unwrap();

        assert!(!store.exists());

        let password = "test_password_123";
        let peer_id = store.generate(password).unwrap();

        assert!(store.exists());

        // Load with correct password
        let (_, public_key) = store.load(password).unwrap();
        let loaded_peer_id = peer_id_from_public_key(&public_key);
        assert_eq!(peer_id, loaded_peer_id);

        // Quick peer_id lookup without password
        let quick_peer_id = store.peer_id().unwrap();
        assert_eq!(peer_id, quick_peer_id);
    }

    #[test]
    fn test_identity_wrong_password() {
        let temp_dir = TempDir::new().unwrap();
        let store = IdentityStore::new(temp_dir.path()).unwrap();

        let password = "correct_password";
        store.generate(password).unwrap();

        // Try loading with wrong password
        let result = store.load("wrong_password");
        assert!(result.is_err());
    }

    #[test]
    fn test_identity_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let store = IdentityStore::new(temp_dir.path()).unwrap();

        let result = store.load("any_password");
        assert!(matches!(result, Err(StoreError::IdentityNotFound)));
    }

    #[test]
    fn test_identity_delete() {
        let temp_dir = TempDir::new().unwrap();
        let store = IdentityStore::new(temp_dir.path()).unwrap();

        store.generate("password").unwrap();
        assert!(store.exists());

        store.delete().unwrap();
        assert!(!store.exists());
    }

    #[test]
    fn test_identity_duplicate() {
        let temp_dir = TempDir::new().unwrap();
        let store = IdentityStore::new(temp_dir.path()).unwrap();

        store.generate("password").unwrap();

        // Trying to generate again should fail
        let result = store.generate("password");
        assert!(result.is_err());
    }

    #[test]
    fn test_base64_roundtrip() {
        let original = b"Hello, World! This is a test of base64 encoding.";
        let encoded = base64_encode(original);
        let decoded = base64_decode(&encoded).unwrap();
        assert_eq!(original.as_slice(), decoded.as_slice());
    }

    #[test]
    fn test_base64_various_lengths() {
        // Test various lengths to cover padding cases
        for len in 1..=20 {
            let original: Vec<u8> = (0..len).map(|i| i as u8).collect();
            let encoded = base64_encode(&original);
            let decoded = base64_decode(&encoded).unwrap();
            assert_eq!(original, decoded);
        }
    }

    #[test]
    fn test_public_key_lookup() {
        let temp_dir = TempDir::new().unwrap();
        let store = IdentityStore::new(temp_dir.path()).unwrap();

        let password = "test_password";
        store.generate(password).unwrap();

        // Get public key without password
        let public_key = store.public_key().unwrap();

        // Verify it matches what we'd get by loading
        let (_, loaded_pk) = store.load(password).unwrap();
        assert_eq!(public_key.0, loaded_pk.0);
    }
}
