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

#[allow(clippy::manual_is_multiple_of)]
fn from_hex(s: &str) -> Result<Vec<u8>, String> {
    if s.len() % 2 != 0 {
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

/// Read a hex string in human-readable formats (JSON) or raw bytes in binary formats (CBOR).
fn deserialize_hex_or_bytes<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
where
    D: Deserializer<'de>,
{
    if deserializer.is_human_readable() {
        let s = String::deserialize(deserializer)?;
        from_hex(&s).map_err(de::Error::custom)
    } else {
        Vec::deserialize(deserializer)
    }
}

/// Copy `bytes` into a fixed-size array, naming `type_name` in the error when the length is wrong.
fn to_fixed_array<const N: usize, E: de::Error>(
    bytes: &[u8],
    type_name: &str,
) -> Result<[u8; N], E> {
    <[u8; N]>::try_from(bytes).map_err(|_| {
        E::custom(format!(
            "{} must be {} bytes, got {}",
            type_name,
            N,
            bytes.len()
        ))
    })
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
        let bytes = deserialize_hex_or_bytes(deserializer)?;
        to_fixed_array(&bytes, "Hash").map(Hash)
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
            to_fixed_array(&bytes, "PeerId").map(PeerId)
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
        let bytes = deserialize_hex_or_bytes(deserializer)?;
        to_fixed_array(&bytes, "PublicKey").map(PublicKey)
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
        let bytes = deserialize_hex_or_bytes(deserializer)?;
        to_fixed_array(&bytes, "Signature").map(Signature)
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
    fn test_invalid_length_errors_name_type_and_sizes() {
        let hash: Result<Hash, _> = serde_json::from_str("\"aabbccdd\"");
        assert!(hash
            .unwrap_err()
            .to_string()
            .contains("Hash must be 32 bytes, got 4"));

        let key: Result<PublicKey, _> = serde_json::from_str("\"aabb\"");
        assert!(key
            .unwrap_err()
            .to_string()
            .contains("PublicKey must be 32 bytes, got 2"));

        let sig: Result<Signature, _> = serde_json::from_str("\"aa\"");
        assert!(sig
            .unwrap_err()
            .to_string()
            .contains("Signature must be 64 bytes, got 1"));
    }

    #[test]
    fn test_invalid_hex() {
        let result: Result<Hash, _> = serde_json::from_str("\"not_valid_hex_at_all_!@#$%\"");
        assert!(result.is_err());
    }
}
