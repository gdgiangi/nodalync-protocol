//! Wire protocol serialization for the Nodalync protocol.
//!
//! This crate provides message encoding and decoding for the Nodalync protocol
//! as specified in Protocol Specification §6 and Appendix A.
//!
//! # Overview
//!
//! The wire protocol defines:
//! - **Message envelope**: Common structure for all messages
//! - **Message types**: 17 different message types across 7 categories
//! - **Payload types**: Type-specific payload structures
//! - **Wire format**: Binary encoding using CBOR
//!
//! # Wire Format
//!
//! All messages follow this binary format:
//!
//! ```text
//! [0x00]                  # Protocol magic byte
//! [version: u8]           # Protocol version (0x01)
//! [type: u16 BE]          # Message type
//! [length: u32 BE]        # Payload length
//! [payload: bytes]        # CBOR-encoded payload
//! [signature: 64 bytes]   # Ed25519 signature
//! ```
//!
//! # Message Categories
//!
//! | Category   | Code Range | Messages |
//! |------------|------------|----------|
//! | Discovery  | 0x01xx     | Announce, AnnounceUpdate, Search, SearchResponse |
//! | Preview    | 0x02xx     | PreviewRequest, PreviewResponse |
//! | Query      | 0x03xx     | QueryRequest, QueryResponse, QueryError |
//! | Version    | 0x04xx     | VersionRequest, VersionResponse |
//! | Channel    | 0x05xx     | ChannelOpen, ChannelAccept, ChannelUpdate, ChannelClose, ChannelDispute |
//! | Settlement | 0x06xx     | SettleBatch, SettleConfirm |
//! | Peer       | 0x07xx     | Ping, Pong, PeerInfo |
//!
//! # Example
//!
//! ```
//! use nodalync_wire::{
//!     encode_message, decode_message, create_message,
//!     encode_payload, decode_payload,
//!     Message, MessageType, PingPayload,
//! };
//! use nodalync_crypto::{generate_identity, peer_id_from_public_key};
//!
//! // Generate identity
//! let (private_key, public_key) = generate_identity();
//! let peer_id = peer_id_from_public_key(&public_key);
//!
//! // Create a ping payload
//! let ping = PingPayload { nonce: 12345 };
//! let payload_bytes = encode_payload(&ping).unwrap();
//!
//! // Create and sign the message
//! let timestamp = std::time::SystemTime::now()
//!     .duration_since(std::time::UNIX_EPOCH)
//!     .unwrap()
//!     .as_millis() as u64;
//!
//! let msg = create_message(
//!     MessageType::Ping,
//!     payload_bytes,
//!     peer_id,
//!     timestamp,
//!     &private_key,
//! );
//!
//! // Encode to wire format
//! let wire_bytes = encode_message(&msg).unwrap();
//!
//! // Decode from wire format
//! let decoded = decode_message(&wire_bytes).unwrap();
//! assert_eq!(decoded.message_type, MessageType::Ping);
//!
//! // Decode the payload
//! let decoded_ping: PingPayload = decode_payload(&decoded.payload).unwrap();
//! assert_eq!(decoded_ping.nonce, 12345);
//! ```
//!
//! # Deterministic Encoding
//!
//! CBOR encoding is deterministic (same input always produces same output),
//! which is critical for signature verification. The encoding follows:
//!
//! - Map keys sorted lexicographically
//! - No indefinite-length arrays or maps
//! - Minimal integer encoding
//! - No floating-point for amounts (use u64)
//!
//! # Hash Functions
//!
//! The module provides hash functions with domain separators:
//!
//! - `content_hash()`: Domain separator `0x00` - for content addressing
//! - `message_hash()`: Domain separator `0x01` - for message signing
//! - `channel_state_hash()`: Domain separator `0x02` - for channel state

pub mod encoding;
pub mod error;
pub mod message;
pub mod payload;

// Re-export main types at crate root

// Error types
pub use error::{DecodeError, EncodeError, FormatError};

// Message types
pub use message::{Message, MessageType};

// Encoding functions
pub use encoding::{
    channel_state_hash, content_hash, create_message, decode_message, decode_payload,
    encode_message, encode_payload, message_hash, validate_message_format,
    verify_message_signature,
};

// Payload types - Discovery
pub use payload::{
    AnnouncePayload, AnnounceUpdatePayload, SearchFilters, SearchPayload, SearchResponsePayload,
    SearchResult,
};

// Payload types - Preview
pub use payload::{PreviewRequestPayload, PreviewResponsePayload};

// Payload types - Query
pub use payload::{
    PaymentReceipt, QueryErrorPayload, QueryRequestPayload, QueryResponsePayload, VersionSpec,
};

// Payload types - Version
pub use payload::{VersionInfo, VersionRequestPayload, VersionResponsePayload};

// Payload types - Channel
pub use payload::{
    ChannelAcceptPayload, ChannelBalances, ChannelClosePayload, ChannelDisputePayload,
    ChannelOpenPayload, ChannelUpdatePayload,
};

// Payload types - Settlement
pub use payload::{SettleBatchPayload, SettleConfirmPayload, SettlementEntry};

// Payload types - Peer
pub use payload::{Capability, PeerInfoPayload, PingPayload, PongPayload};

#[cfg(test)]
mod tests {
    use super::*;
    use nodalync_crypto::{
        content_hash as crypto_hash, generate_identity, peer_id_from_public_key,
    };
    use nodalync_types::{ContentType, L1Summary};

    /// Test full message roundtrip: create -> encode -> decode -> verify
    #[test]
    fn test_full_message_roundtrip() {
        let (private_key, public_key, peer_id) = {
            let (pk, pubk) = generate_identity();
            let pid = peer_id_from_public_key(&pubk);
            (pk, pubk, pid)
        };

        // Create payload
        let ping = PingPayload { nonce: 42 };
        let payload_bytes = encode_payload(&ping).unwrap();

        // Create message
        let timestamp = 1234567890000u64;
        let msg = create_message(
            MessageType::Ping,
            payload_bytes.clone(),
            peer_id,
            timestamp,
            &private_key,
        );

        // Encode
        let wire_bytes = encode_message(&msg).unwrap();

        // Decode
        let decoded = decode_message(&wire_bytes).unwrap();

        // Verify fields
        assert_eq!(decoded.version, msg.version);
        assert_eq!(decoded.message_type, MessageType::Ping);
        assert_eq!(decoded.payload, payload_bytes);

        // Verify payload decodes correctly
        let decoded_ping: PingPayload = decode_payload(&decoded.payload).unwrap();
        assert_eq!(decoded_ping.nonce, 42);

        // Verify signature
        assert!(verify_message_signature(&msg, &public_key));
    }

    /// Test that encoding is deterministic (same input -> same output)
    #[test]
    fn test_deterministic_encoding() {
        let hash = crypto_hash(b"test content");
        let payload = AnnouncePayload {
            hash,
            content_type: ContentType::L0,
            title: "Test Title".to_string(),
            l1_summary: L1Summary::empty(hash),
            price: 100,
            addresses: vec!["/ip4/127.0.0.1/tcp/9000".to_string()],
            publisher_peer_id: None,
        };

        let enc1 = encode_payload(&payload).unwrap();
        let enc2 = encode_payload(&payload).unwrap();

        assert_eq!(enc1, enc2, "Encoding must be deterministic for signatures");
    }

    /// Test that invalid magic byte is rejected
    #[test]
    fn test_reject_invalid_magic() {
        // Wire format: magic(1) + version(1) + type(2) + timestamp(8) + sender(20) + length(4) + signature(64) = 100
        let mut bytes = vec![0xFF]; // Wrong magic
        bytes.push(0x01); // Version
        bytes.extend_from_slice(&[0x07, 0x00]); // Ping type
        bytes.extend_from_slice(&[0u8; 8]); // Timestamp
        bytes.extend_from_slice(&[0u8; 20]); // Sender
        bytes.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // Length 0
        bytes.extend_from_slice(&[0u8; 64]); // Signature

        let result = decode_message(&bytes);
        assert!(matches!(
            result,
            Err(DecodeError::InvalidMagic {
                expected: 0x00,
                got: 0xFF
            })
        ));
    }

    /// Test that invalid version is rejected
    #[test]
    fn test_reject_invalid_version() {
        // Wire format: magic(1) + version(1) + type(2) + timestamp(8) + sender(20) + length(4) + signature(64) = 100
        let mut bytes = vec![0x00]; // Correct magic
        bytes.push(0xFF); // Wrong version
        bytes.extend_from_slice(&[0x07, 0x00]); // Ping type
        bytes.extend_from_slice(&[0u8; 8]); // Timestamp
        bytes.extend_from_slice(&[0u8; 20]); // Sender
        bytes.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // Length 0
        bytes.extend_from_slice(&[0u8; 64]); // Signature

        let result = decode_message(&bytes);
        assert!(matches!(
            result,
            Err(DecodeError::InvalidVersion {
                expected: 0x01,
                got: 0xFF
            })
        ));
    }

    /// Test that truncated messages are rejected
    #[test]
    fn test_reject_truncated() {
        let bytes = vec![0x00, 0x01, 0x07]; // Only partial header

        let result = decode_message(&bytes);
        assert!(matches!(result, Err(DecodeError::TruncatedMessage { .. })));
    }

    /// Test that invalid CBOR payload is rejected
    #[test]
    fn test_reject_invalid_cbor() {
        let invalid_cbor = vec![0xFF, 0xFF, 0xFF]; // Invalid CBOR

        let result = decode_payload::<PingPayload>(&invalid_cbor);
        assert!(matches!(result, Err(DecodeError::PayloadDecodeFailed(_))));
    }

    /// Test all message types can be round-tripped
    #[test]
    fn test_all_message_types() {
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
            MessageType::SettleBatch,
            MessageType::SettleConfirm,
            MessageType::Ping,
            MessageType::Pong,
            MessageType::PeerInfo,
        ];

        for msg_type in types {
            let value = msg_type.to_u16();
            let recovered = MessageType::from_u16(value).unwrap();
            assert_eq!(
                msg_type, recovered,
                "MessageType {:?} failed roundtrip",
                msg_type
            );
        }
    }

    /// Test channel state hash is different for different inputs
    #[test]
    fn test_channel_state_hash_uniqueness() {
        let channel1 = crypto_hash(b"channel1");
        let channel2 = crypto_hash(b"channel2");
        let balances = ChannelBalances::new(1000, 500);

        let h1 = channel_state_hash(&channel1, 1, &balances);
        let h2 = channel_state_hash(&channel2, 1, &balances);
        let h3 = channel_state_hash(&channel1, 2, &balances);
        let h4 = channel_state_hash(&channel1, 1, &ChannelBalances::new(2000, 500));

        // All should be different
        assert_ne!(h1, h2);
        assert_ne!(h1, h3);
        assert_ne!(h1, h4);
        assert_ne!(h2, h3);
        assert_ne!(h2, h4);
        assert_ne!(h3, h4);
    }
}
