//! Wire format encoding and decoding.
//!
//! This module implements the wire format specified in Protocol Specification
//! Appendix A, including:
//!
//! - CBOR encoding with deterministic serialization
//! - Message wire format (magic, version, type, length, payload, signature)
//! - Hash computation with domain separators
//!
//! # Wire Format
//!
//! ```text
//! [0x00]                  # Protocol magic byte
//! [version: u8]           # Protocol version
//! [type: u16 BE]          # Message type
//! [length: u32 BE]        # Payload length
//! [payload: bytes]        # CBOR-encoded payload
//! [signature: 64 bytes]   # Ed25519 signature
//! ```

use nodalync_crypto::{Hash, PeerId, PrivateKey, Signature, Timestamp};
use nodalync_types::constants::{MAX_MESSAGE_SIZE, PROTOCOL_MAGIC, PROTOCOL_VERSION};
use serde::{de::DeserializeOwned, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{DecodeError, EncodeError, FormatError};
use crate::message::{Message, MessageType};
use crate::payload::ChannelBalances;

/// Minimum message size: magic(1) + version(1) + type(2) + timestamp(8) + sender(20) + length(4) + signature(64) = 100 bytes
const MIN_MESSAGE_SIZE: usize = 1 + 1 + 2 + 8 + 20 + 4 + 64;

/// Domain separator for content hashing
const DOMAIN_CONTENT: u8 = 0x00;

/// Domain separator for message hashing
const DOMAIN_MESSAGE: u8 = 0x01;

/// Domain separator for channel state hashing
const DOMAIN_CHANNEL_STATE: u8 = 0x02;

// =============================================================================
// Hash Functions
// =============================================================================

/// Compute content hash with domain separator.
///
/// Uses domain separator `0x00` per spec §3.1.
///
/// `H(0x00 || len(content) || content)`
pub fn content_hash(content: &[u8]) -> Hash {
    let mut hasher = Sha256::new();
    hasher.update([DOMAIN_CONTENT]);
    hasher.update((content.len() as u64).to_be_bytes());
    hasher.update(content);
    Hash(hasher.finalize().into())
}

/// Compute message hash for signing.
///
/// Uses domain separator `0x01` per Appendix A.
///
/// `H(0x01 || version || type || id || timestamp || sender || payload_hash)`
pub fn message_hash(msg: &Message) -> Hash {
    let mut hasher = Sha256::new();
    hasher.update([DOMAIN_MESSAGE]);
    hasher.update([msg.version]);
    hasher.update(msg.message_type.to_u16().to_be_bytes());
    hasher.update(msg.id.0);
    hasher.update(msg.timestamp.to_be_bytes());
    hasher.update(msg.sender.0);
    hasher.update(content_hash(&msg.payload).0);
    Hash(hasher.finalize().into())
}

/// Compute channel state hash for dispute resolution.
///
/// Uses domain separator `0x02` per Appendix A.
///
/// `H(0x02 || channel_id || nonce || initiator_balance || responder_balance)`
pub fn channel_state_hash(channel_id: &Hash, nonce: u64, balances: &ChannelBalances) -> Hash {
    let mut hasher = Sha256::new();
    hasher.update([DOMAIN_CHANNEL_STATE]);
    hasher.update(channel_id.0);
    hasher.update(nonce.to_be_bytes());
    hasher.update(balances.initiator.to_be_bytes());
    hasher.update(balances.responder.to_be_bytes());
    Hash(hasher.finalize().into())
}

// =============================================================================
// Payload Encoding/Decoding
// =============================================================================

/// Encode a payload to deterministic CBOR.
///
/// The resulting bytes are suitable for inclusion in a message.
pub fn encode_payload<T: Serialize>(payload: &T) -> Result<Vec<u8>, EncodeError> {
    let mut buf = Vec::new();
    ciborium::into_writer(payload, &mut buf)?;

    // Check size limit
    if buf.len() > MAX_MESSAGE_SIZE as usize {
        return Err(EncodeError::PayloadTooLarge {
            size: buf.len(),
            max: MAX_MESSAGE_SIZE as usize,
        });
    }

    Ok(buf)
}

/// Decode a CBOR payload.
pub fn decode_payload<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, DecodeError> {
    ciborium::from_reader(bytes).map_err(|e| DecodeError::PayloadDecodeFailed(e.to_string()))
}

// =============================================================================
// Message Encoding/Decoding
// =============================================================================

/// Encode a message to wire format.
///
/// Wire format (v2 - includes sender and timestamp):
/// ```text
/// [0x00]                  # Protocol magic byte
/// [version: u8]           # Protocol version
/// [type: u16 BE]          # Message type
/// [timestamp: u64 BE]     # Message timestamp (millis since epoch)
/// [sender: 20 bytes]      # Sender's Nodalync peer ID
/// [length: u32 BE]        # Payload length
/// [payload: bytes]        # CBOR-encoded payload
/// [signature: 64 bytes]   # Ed25519 signature
/// ```
pub fn encode_message(msg: &Message) -> Result<Vec<u8>, EncodeError> {
    let payload_len = msg.payload.len();
    // Header: magic(1) + version(1) + type(2) + timestamp(8) + sender(20) + length(4) + signature(64) = 100
    let total_len = 100 + payload_len;

    let mut buf = Vec::with_capacity(total_len);

    // Magic byte
    buf.push(PROTOCOL_MAGIC);

    // Version
    buf.push(msg.version);

    // Message type (big-endian)
    buf.extend_from_slice(&msg.message_type.to_u16().to_be_bytes());

    // Timestamp (big-endian)
    buf.extend_from_slice(&msg.timestamp.to_be_bytes());

    // Sender peer ID (20 bytes)
    buf.extend_from_slice(&msg.sender.0);

    // Payload length (big-endian)
    buf.extend_from_slice(&(payload_len as u32).to_be_bytes());

    // Payload
    buf.extend_from_slice(&msg.payload);

    // Signature
    buf.extend_from_slice(&msg.signature.0);

    Ok(buf)
}

/// Decode a message from wire format.
///
/// Wire format (v2 - includes sender and timestamp):
/// ```text
/// [0x00]                  # Protocol magic byte
/// [version: u8]           # Protocol version
/// [type: u16 BE]          # Message type
/// [timestamp: u64 BE]     # Message timestamp (millis since epoch)
/// [sender: 20 bytes]      # Sender's Nodalync peer ID
/// [length: u32 BE]        # Payload length
/// [payload: bytes]        # CBOR-encoded payload
/// [signature: 64 bytes]   # Ed25519 signature
/// ```
pub fn decode_message(bytes: &[u8]) -> Result<Message, DecodeError> {
    // Check minimum size
    if bytes.len() < MIN_MESSAGE_SIZE {
        return Err(DecodeError::TruncatedMessage {
            expected: MIN_MESSAGE_SIZE,
            got: bytes.len(),
        });
    }

    let mut cursor = 0;

    // Magic byte
    let magic = bytes[cursor];
    cursor += 1;
    if magic != PROTOCOL_MAGIC {
        return Err(DecodeError::InvalidMagic {
            expected: PROTOCOL_MAGIC,
            got: magic,
        });
    }

    // Version
    let version = bytes[cursor];
    cursor += 1;
    if version != PROTOCOL_VERSION {
        return Err(DecodeError::InvalidVersion {
            expected: PROTOCOL_VERSION,
            got: version,
        });
    }

    // Message type (big-endian)
    let type_bytes: [u8; 2] =
        bytes[cursor..cursor + 2]
            .try_into()
            .map_err(|_| DecodeError::TruncatedMessage {
                expected: cursor + 2,
                got: bytes.len(),
            })?;
    cursor += 2;
    let message_type = MessageType::from_u16(u16::from_be_bytes(type_bytes))?;

    // Timestamp (big-endian)
    let ts_bytes: [u8; 8] =
        bytes[cursor..cursor + 8]
            .try_into()
            .map_err(|_| DecodeError::TruncatedMessage {
                expected: cursor + 8,
                got: bytes.len(),
            })?;
    cursor += 8;
    let timestamp = u64::from_be_bytes(ts_bytes);

    // Sender peer ID (20 bytes)
    let sender_bytes: [u8; 20] =
        bytes[cursor..cursor + 20]
            .try_into()
            .map_err(|_| DecodeError::TruncatedMessage {
                expected: cursor + 20,
                got: bytes.len(),
            })?;
    cursor += 20;
    let sender = PeerId::from_bytes(sender_bytes);

    // Payload length (big-endian)
    let len_bytes: [u8; 4] =
        bytes[cursor..cursor + 4]
            .try_into()
            .map_err(|_| DecodeError::TruncatedMessage {
                expected: cursor + 4,
                got: bytes.len(),
            })?;
    cursor += 4;
    let payload_len = u32::from_be_bytes(len_bytes) as usize;

    // Check we have enough bytes for payload + signature
    let expected_total = cursor + payload_len + 64;
    if bytes.len() < expected_total {
        return Err(DecodeError::TruncatedMessage {
            expected: expected_total,
            got: bytes.len(),
        });
    }

    // Payload
    let payload = bytes[cursor..cursor + payload_len].to_vec();
    cursor += payload_len;

    // Signature
    let sig_bytes: [u8; 64] =
        bytes[cursor..cursor + 64]
            .try_into()
            .map_err(|_| DecodeError::TruncatedMessage {
                expected: cursor + 64,
                got: bytes.len(),
            })?;
    let signature = Signature::from_bytes(sig_bytes);

    // Compute message ID as hash of the header + payload
    let id = compute_message_id(version, message_type, &payload);

    Ok(Message {
        version,
        message_type,
        id,
        timestamp,
        sender,
        payload,
        signature,
    })
}

/// Compute a message ID from its components.
///
/// The ID is a hash of the version, type, and payload.
fn compute_message_id(version: u8, message_type: MessageType, payload: &[u8]) -> Hash {
    let mut hasher = Sha256::new();
    hasher.update([version]);
    hasher.update(message_type.to_u16().to_be_bytes());
    hasher.update(payload);
    Hash(hasher.finalize().into())
}

// =============================================================================
// Message Creation Helpers
// =============================================================================

/// Create and sign a new message.
///
/// This is the primary way to create messages for sending.
pub fn create_message(
    message_type: MessageType,
    payload: Vec<u8>,
    sender: PeerId,
    timestamp: Timestamp,
    private_key: &PrivateKey,
) -> Message {
    // Compute message ID
    let id = compute_message_id(PROTOCOL_VERSION, message_type, &payload);

    // Create unsigned message
    let mut msg = Message {
        version: PROTOCOL_VERSION,
        message_type,
        id,
        timestamp,
        sender,
        payload,
        signature: Signature::from_bytes([0u8; 64]), // Placeholder
    };

    // Compute hash for signing
    let hash = message_hash(&msg);

    // Sign the hash
    let signature = nodalync_crypto::sign(private_key, hash.as_ref());
    msg.signature = signature;

    msg
}

// =============================================================================
// Validation
// =============================================================================

/// Validate message format (not semantic validity).
///
/// Checks:
/// - Protocol version is supported
/// - Message type is valid
/// - Timestamp is within acceptable range (±5 minutes)
/// - Sender is non-zero
/// - Signature format is valid
pub fn validate_message_format(msg: &Message, current_time: Timestamp) -> Result<(), FormatError> {
    // Check version
    if msg.version != PROTOCOL_VERSION {
        return Err(FormatError::InvalidVersion(msg.version));
    }

    // Check timestamp is within ±5 minutes (300,000 ms)
    let max_skew = nodalync_types::constants::MAX_CLOCK_SKEW_MS;
    let time_diff = msg.timestamp.abs_diff(current_time);
    if time_diff > max_skew {
        return Err(FormatError::TimestampOutOfRange(msg.timestamp));
    }

    // Check sender is not all zeros
    if msg.sender.0 == [0u8; 20] {
        return Err(FormatError::InvalidSender);
    }

    // Verify signature format (actual verification requires public key)
    // The signature should not be all zeros
    if msg.signature.0 == [0u8; 64] {
        return Err(FormatError::InvalidSignature);
    }

    Ok(())
}

/// Verify message signature.
///
/// Returns true if the signature is valid for the given public key.
pub fn verify_message_signature(msg: &Message, public_key: &nodalync_crypto::PublicKey) -> bool {
    let hash = message_hash(msg);
    nodalync_crypto::verify(public_key, hash.as_ref(), &msg.signature)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nodalync_crypto::{
        content_hash as crypto_hash, generate_identity, peer_id_from_public_key,
    };

    fn test_keypair() -> (PrivateKey, nodalync_crypto::PublicKey, PeerId) {
        let (private_key, public_key) = generate_identity();
        let peer_id = peer_id_from_public_key(&public_key);
        (private_key, public_key, peer_id)
    }

    #[test]
    fn test_content_hash_deterministic() {
        let data = b"hello world";
        let h1 = content_hash(data);
        let h2 = content_hash(data);
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_content_hash_different_data() {
        let h1 = content_hash(b"hello");
        let h2 = content_hash(b"world");
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_channel_state_hash_deterministic() {
        let channel_id = crypto_hash(b"channel");
        let balances = ChannelBalances::new(1000, 500);
        let h1 = channel_state_hash(&channel_id, 1, &balances);
        let h2 = channel_state_hash(&channel_id, 1, &balances);
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_channel_state_hash_changes_with_nonce() {
        let channel_id = crypto_hash(b"channel");
        let balances = ChannelBalances::new(1000, 500);
        let h1 = channel_state_hash(&channel_id, 1, &balances);
        let h2 = channel_state_hash(&channel_id, 2, &balances);
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_encode_decode_payload_roundtrip() {
        use crate::payload::PingPayload;

        let original = PingPayload { nonce: 12345 };
        let encoded = encode_payload(&original).unwrap();
        let decoded: PingPayload = decode_payload(&encoded).unwrap();
        assert_eq!(decoded.nonce, original.nonce);
    }

    #[test]
    fn test_encode_decode_message_roundtrip() {
        use crate::payload::PingPayload;

        let (private_key, _public_key, peer_id) = test_keypair();

        let payload = PingPayload { nonce: 42 };
        let payload_bytes = encode_payload(&payload).unwrap();

        let msg = create_message(
            MessageType::Ping,
            payload_bytes,
            peer_id,
            1234567890000,
            &private_key,
        );

        let encoded = encode_message(&msg).unwrap();
        let decoded = decode_message(&encoded).unwrap();

        assert_eq!(decoded.version, msg.version);
        assert_eq!(decoded.message_type, msg.message_type);
        assert_eq!(decoded.payload, msg.payload);
        assert_eq!(decoded.signature, msg.signature);
    }

    #[test]
    fn test_decode_invalid_magic() {
        // Format: magic(1) + version(1) + type(2) + timestamp(8) + sender(20) + length(4) + signature(64) = 100
        let mut bytes = vec![0xFF]; // Invalid magic
        bytes.push(0x01); // version
        bytes.extend_from_slice(&[0x07, 0x00]); // type
        bytes.extend_from_slice(&[0u8; 8]); // timestamp
        bytes.extend_from_slice(&[0u8; 20]); // sender
        bytes.extend_from_slice(&[0u8; 4]); // length
        bytes.extend_from_slice(&[0u8; 64]); // signature

        let result = decode_message(&bytes);
        assert!(matches!(result, Err(DecodeError::InvalidMagic { .. })));
    }

    #[test]
    fn test_decode_invalid_version() {
        // Format: magic(1) + version(1) + type(2) + timestamp(8) + sender(20) + length(4) + signature(64) = 100
        let mut bytes = vec![0x00]; // magic
        bytes.push(0xFF); // Invalid version
        bytes.extend_from_slice(&[0x07, 0x00]); // type
        bytes.extend_from_slice(&[0u8; 8]); // timestamp
        bytes.extend_from_slice(&[0u8; 20]); // sender
        bytes.extend_from_slice(&[0u8; 4]); // length
        bytes.extend_from_slice(&[0u8; 64]); // signature

        let result = decode_message(&bytes);
        assert!(matches!(result, Err(DecodeError::InvalidVersion { .. })));
    }

    #[test]
    fn test_decode_truncated() {
        let bytes = vec![0x00, 0x01]; // Only magic and version

        let result = decode_message(&bytes);
        assert!(matches!(result, Err(DecodeError::TruncatedMessage { .. })));
    }

    #[test]
    fn test_decode_invalid_message_type() {
        // Format: magic(1) + version(1) + type(2) + timestamp(8) + sender(20) + length(4) + signature(64) = 100
        let mut bytes = vec![0x00]; // magic
        bytes.push(0x01); // version
        bytes.extend_from_slice(&[0x99, 0x99]); // Invalid type
        bytes.extend_from_slice(&[0u8; 8]); // timestamp
        bytes.extend_from_slice(&[0u8; 20]); // sender
        bytes.extend_from_slice(&[0u8; 4]); // length
        bytes.extend_from_slice(&[0u8; 64]); // signature

        let result = decode_message(&bytes);
        assert!(matches!(result, Err(DecodeError::InvalidMessageType(_))));
    }

    #[test]
    fn test_message_signature_verification() {
        use crate::payload::PingPayload;

        let (private_key, public_key, peer_id) = test_keypair();

        let payload = PingPayload { nonce: 42 };
        let payload_bytes = encode_payload(&payload).unwrap();

        let msg = create_message(
            MessageType::Ping,
            payload_bytes,
            peer_id,
            1234567890000,
            &private_key,
        );

        // Verify with correct public key
        assert!(verify_message_signature(&msg, &public_key));

        // Verify with wrong public key fails
        let (_, wrong_key, _) = test_keypair();
        assert!(!verify_message_signature(&msg, &wrong_key));
    }

    #[test]
    fn test_validate_message_format_valid() {
        let (private_key, _, peer_id) = test_keypair();

        let current_time = 1234567890000u64;
        let msg = create_message(
            MessageType::Ping,
            vec![],
            peer_id,
            current_time,
            &private_key,
        );

        let result = validate_message_format(&msg, current_time);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_message_format_timestamp_out_of_range() {
        let (private_key, _, peer_id) = test_keypair();

        let current_time = 1234567890000u64;
        let msg_time = current_time + 600_000; // 10 minutes in future

        let msg = create_message(MessageType::Ping, vec![], peer_id, msg_time, &private_key);

        let result = validate_message_format(&msg, current_time);
        assert!(matches!(result, Err(FormatError::TimestampOutOfRange(_))));
    }

    #[test]
    fn test_validate_message_format_zero_sender() {
        let (private_key, _, _) = test_keypair();
        let zero_peer = PeerId::from_bytes([0u8; 20]);

        let current_time = 1234567890000u64;
        let msg = create_message(
            MessageType::Ping,
            vec![],
            zero_peer,
            current_time,
            &private_key,
        );

        let result = validate_message_format(&msg, current_time);
        assert!(matches!(result, Err(FormatError::InvalidSender)));
    }

    #[test]
    fn test_deterministic_encoding() {
        use crate::payload::AnnouncePayload;
        use nodalync_types::{ContentType, L1Summary};

        let hash = crypto_hash(b"content");
        let payload = AnnouncePayload {
            hash,
            content_type: ContentType::L0,
            title: "Test".to_string(),
            l1_summary: L1Summary::empty(hash),
            price: 100,
            addresses: vec!["/ip4/127.0.0.1/tcp/9000".to_string()],
            publisher_peer_id: None,
        };

        // Encode multiple times - should be identical
        let enc1 = encode_payload(&payload).unwrap();
        let enc2 = encode_payload(&payload).unwrap();
        assert_eq!(enc1, enc2, "CBOR encoding should be deterministic");
    }

    #[test]
    fn test_content_hash_empty_data() {
        let h1 = content_hash(b"");
        let h2 = content_hash(b"");
        assert_eq!(h1, h2);
        // Empty data should still produce a valid hash (not zero)
        assert_ne!(h1, Hash([0u8; 32]));
    }

    #[test]
    fn test_channel_state_hash_changes_with_balances() {
        let channel_id = crypto_hash(b"channel");
        let b1 = ChannelBalances::new(1000, 500);
        let b2 = ChannelBalances::new(900, 600);
        let h1 = channel_state_hash(&channel_id, 1, &b1);
        let h2 = channel_state_hash(&channel_id, 1, &b2);
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_validate_message_format_zero_signature() {
        // A message with all-zero signature should fail validation
        let peer_id = PeerId::from_bytes([1u8; 20]);
        let current_time = 1234567890000u64;
        let msg = Message::new(
            nodalync_types::constants::PROTOCOL_VERSION,
            MessageType::Ping,
            Hash([0u8; 32]),
            current_time,
            peer_id,
            vec![],
            Signature::from_bytes([0u8; 64]),
        );
        let result = validate_message_format(&msg, current_time);
        assert!(matches!(result, Err(FormatError::InvalidSignature)));
    }

    #[test]
    fn test_encode_decode_large_payload() {
        // Test with a message containing a large payload
        let (private_key, _public_key, peer_id) = test_keypair();
        let large_data = vec![0xABu8; 10000];
        let msg = create_message(
            MessageType::Announce,
            large_data.clone(),
            peer_id,
            1234567890000,
            &private_key,
        );
        let encoded = encode_message(&msg).unwrap();
        let decoded = decode_message(&encoded).unwrap();
        assert_eq!(decoded.payload, large_data);
        assert_eq!(decoded.message_type, MessageType::Announce);
    }

    #[test]
    fn test_encode_message_all_types_roundtrip() {
        let (private_key, _public_key, peer_id) = test_keypair();
        let types = [
            MessageType::Announce,
            MessageType::AnnounceUpdate,
            MessageType::Search,
            MessageType::SearchResponse,
            MessageType::PreviewRequest,
            MessageType::PreviewResponse,
            MessageType::QueryRequest,
            MessageType::QueryResponse,
            MessageType::QueryError,
            MessageType::VersionRequest,
            MessageType::VersionResponse,
            MessageType::ChannelOpen,
            MessageType::ChannelAccept,
            MessageType::ChannelUpdate,
            MessageType::ChannelClose,
            MessageType::ChannelDispute,
            MessageType::ChannelCloseAck,
            MessageType::SettleBatch,
            MessageType::SettleConfirm,
            MessageType::Ping,
            MessageType::Pong,
            MessageType::PeerInfo,
        ];
        for msg_type in types {
            let msg = create_message(
                msg_type,
                vec![1, 2, 3],
                peer_id,
                1234567890000,
                &private_key,
            );
            let encoded = encode_message(&msg).unwrap();
            let decoded = decode_message(&encoded).unwrap();
            assert_eq!(decoded.message_type, msg_type, "Failed for {:?}", msg_type);
            assert_eq!(decoded.payload, vec![1, 2, 3]);
        }
    }
}
