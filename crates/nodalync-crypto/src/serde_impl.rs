//! Serde serialization implementations for crypto types.
//!
//! All types are serialized as hex strings for human readability in JSON,
//! while maintaining efficiency in binary formats like CBOR.

use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

use crate::{Hash, PeerId, PublicKey, Signature};

// Helper functions for hex encoding/decoding
fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn from_hex(s: &str) -> Result<Vec<u8>, String> {
    if !s.len().is_multiple_of(2) {
        return Err("Hex string must have even length".to_string());
    }
    (0..s.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&s[i..i + 2], 16)
                .map_err(|e| format!("Invalid hex character: {}", e))
        })
        .collect()
}

// Hash serialization
impl Serialize for Hash {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if serializer.is_human_readable() {
            serializer.serialize_str(&to_hex(&self.0))
        } else {
            serializer.serialize_bytes(&self.0)
        }
    }
}

impl<'de> Deserialize<'de> for Hash {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        if deserializer.is_human_readable() {
            let s = String::deserialize(deserializer)?;
            let bytes = from_hex(&s).map_err(de::Error::custom)?;
            if bytes.len() != 32 {
                return Err(de::Error::custom(format!(
                    "Hash must be 32 bytes, got {}",
                    bytes.len()
                )));
            }
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&bytes);
            Ok(Hash(arr))
        } else {
            let bytes: Vec<u8> = Vec::deserialize(deserializer)?;
            if bytes.len() != 32 {
                return Err(de::Error::custom(format!(
                    "Hash must be 32 bytes, got {}",
                    bytes.len()
                )));
            }
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&bytes);
            Ok(Hash(arr))
        }
    }
}

// PeerId serialization
impl Serialize for PeerId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if serializer.is_human_readable() {
            // Use the human-readable format (ndl1...)
            serializer.serialize_str(&crate::peer_id_to_string(self))
        } else {
            serializer.serialize_bytes(&self.0)
        }
    }
}

impl<'de> Deserialize<'de> for PeerId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        if deserializer.is_human_readable() {
            let s = String::deserialize(deserializer)?;
            crate::peer_id_from_string(&s).map_err(de::Error::custom)
        } else {
            let bytes: Vec<u8> = Vec::deserialize(deserializer)?;
            if bytes.len() != 20 {
                return Err(de::Error::custom(format!(
                    "PeerId must be 20 bytes, got {}",
                    bytes.len()
                )));
            }
            let mut arr = [0u8; 20];
            arr.copy_from_slice(&bytes);
            Ok(PeerId(arr))
        }
    }
}

// PublicKey serialization
impl Serialize for PublicKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if serializer.is_human_readable() {
            serializer.serialize_str(&to_hex(&self.0))
        } else {
            serializer.serialize_bytes(&self.0)
        }
    }
}

impl<'de> Deserialize<'de> for PublicKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        if deserializer.is_human_readable() {
            let s = String::deserialize(deserializer)?;
            let bytes = from_hex(&s).map_err(de::Error::custom)?;
            if bytes.len() != 32 {
                return Err(de::Error::custom(format!(
                    "PublicKey must be 32 bytes, got {}",
                    bytes.len()
                )));
            }
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&bytes);
            Ok(PublicKey(arr))
        } else {
            let bytes: Vec<u8> = Vec::deserialize(deserializer)?;
            if bytes.len() != 32 {
                return Err(de::Error::custom(format!(
                    "PublicKey must be 32 bytes, got {}",
                    bytes.len()
                )));
            }
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&bytes);
            Ok(PublicKey(arr))
        }
    }
}

// Signature serialization
impl Serialize for Signature {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if serializer.is_human_readable() {
            serializer.serialize_str(&to_hex(&self.0))
        } else {
            serializer.serialize_bytes(&self.0)
        }
    }
}

impl<'de> Deserialize<'de> for Signature {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        if deserializer.is_human_readable() {
            let s = String::deserialize(deserializer)?;
            let bytes = from_hex(&s).map_err(de::Error::custom)?;
            if bytes.len() != 64 {
                return Err(de::Error::custom(format!(
                    "Signature must be 64 bytes, got {}",
                    bytes.len()
                )));
            }
            let mut arr = [0u8; 64];
            arr.copy_from_slice(&bytes);
            Ok(Signature(arr))
        } else {
            let bytes: Vec<u8> = Vec::deserialize(deserializer)?;
            if bytes.len() != 64 {
                return Err(de::Error::custom(format!(
                    "Signature must be 64 bytes, got {}",
                    bytes.len()
                )));
            }
            let mut arr = [0u8; 64];
            arr.copy_from_slice(&bytes);
            Ok(Signature(arr))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{content_hash, generate_identity, peer_id_from_public_key, sign};

    #[test]
    fn test_hash_serde_json() {
        let hash = content_hash(b"test content");
        let json = serde_json::to_string(&hash).unwrap();

        // Should be a hex string
        assert!(json.starts_with('"'));
        assert_eq!(json.len(), 66); // 64 hex chars + 2 quotes

        let deserialized: Hash = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, hash);
    }

    #[test]
    fn test_peer_id_serde_json() {
        let (_, public_key) = generate_identity();
        let peer_id = peer_id_from_public_key(&public_key);
        let json = serde_json::to_string(&peer_id).unwrap();

        // Should be ndl1... format
        assert!(json.contains("ndl1"));

        let deserialized: PeerId = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, peer_id);
    }

    #[test]
    fn test_public_key_serde_json() {
        let (_, public_key) = generate_identity();
        let json = serde_json::to_string(&public_key).unwrap();

        // Should be a hex string
        assert!(json.starts_with('"'));
        assert_eq!(json.len(), 66); // 64 hex chars + 2 quotes

        let deserialized: PublicKey = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, public_key);
    }

    #[test]
    fn test_signature_serde_json() {
        let (private_key, _) = generate_identity();
        let signature = sign(&private_key, b"test message");
        let json = serde_json::to_string(&signature).unwrap();

        // Should be a hex string
        assert!(json.starts_with('"'));
        assert_eq!(json.len(), 130); // 128 hex chars + 2 quotes

        let deserialized: Signature = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, signature);
    }

    #[test]
    fn test_hash_roundtrip() {
        let hash = content_hash(b"roundtrip test");

        // JSON roundtrip
        let json = serde_json::to_string(&hash).unwrap();
        let from_json: Hash = serde_json::from_str(&json).unwrap();
        assert_eq!(from_json, hash);
    }

    #[test]
    fn test_invalid_hash_length() {
        let result: Result<Hash, _> = serde_json::from_str("\"aabbccdd\"");
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_hex() {
        let result: Result<Hash, _> = serde_json::from_str("\"not_valid_hex_at_all_!@#$%\"");
        assert!(result.is_err());
    }
}
