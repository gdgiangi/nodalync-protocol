//! Message validation (§9.5).
//!
//! This module validates protocol messages:
//! - Protocol version
//! - Message type validity
//! - Timestamp within acceptable range
//! - Sender validity
//! - Signature verification
//! - Payload decoding

use nodalync_crypto::{verify, PublicKey, Timestamp};
use nodalync_types::{MAX_CLOCK_SKEW_MS, PROTOCOL_VERSION};
use nodalync_wire::Message;

use crate::error::{ValidationError, ValidationResult};

/// Validate a protocol message.
///
/// Checks all message validation rules from §9.5:
/// 1. `version == PROTOCOL_VERSION`
/// 2. `message_type` is valid
/// 3. `timestamp` within ±MAX_CLOCK_SKEW_MS (5 minutes)
/// 4. `sender` is valid PeerId
/// 5. Signature is valid
/// 6. Payload decodes correctly for message type
///
/// # Arguments
///
/// * `message` - The message to validate
/// * `current_time` - The current timestamp (milliseconds since Unix epoch)
/// * `sender_pubkey` - The sender's public key for signature verification (optional)
///
/// # Returns
///
/// `Ok(())` if message is valid, or `Err(ValidationError)`.
pub fn validate_message(
    message: &Message,
    current_time: Timestamp,
    sender_pubkey: Option<&PublicKey>,
) -> ValidationResult<()> {
    // 1. Protocol version
    if message.version != PROTOCOL_VERSION {
        return Err(ValidationError::UnsupportedVersion {
            version: message.version,
            expected: PROTOCOL_VERSION,
        });
    }

    // 2. Message type is valid (already validated by MessageType parsing)
    // The message_type field is a MessageType enum, so it's always valid if parsed

    // 3. Timestamp within acceptable range
    validate_timestamp(message.timestamp, current_time)?;

    // 4. Sender is valid PeerId
    // PeerId is a fixed 20-byte array, so structural validity is guaranteed
    // Semantic validity (known peer) depends on context

    // 5. Verify signature (if public key provided)
    if let Some(pubkey) = sender_pubkey {
        if !verify_message_signature(pubkey, message) {
            return Err(ValidationError::InvalidMessageSignature);
        }
    }

    // 6. Payload decoding is checked at message receipt time by nodalync-wire
    // If we want to validate payload decodes for type, we'd need the type-specific
    // decode logic here. For now, we trust the wire layer.

    Ok(())
}

/// Validate message without signature verification.
///
/// Use when the sender's public key is not available or signature
/// has already been verified.
pub fn validate_message_basic(message: &Message, current_time: Timestamp) -> ValidationResult<()> {
    validate_message(message, current_time, None)
}

/// Validate message timestamp against current time.
fn validate_timestamp(message_time: Timestamp, current_time: Timestamp) -> ValidationResult<()> {
    let skew = message_time.abs_diff(current_time);

    if skew > MAX_CLOCK_SKEW_MS {
        return Err(ValidationError::TimestampOutOfRange {
            skew_ms: skew,
            max_skew_ms: MAX_CLOCK_SKEW_MS,
        });
    }

    Ok(())
}

/// Verify message signature.
///
/// The signature covers the message hash: H(version || type || id || timestamp || sender || payload_hash)
fn verify_message_signature(pubkey: &PublicKey, message: &Message) -> bool {
    let msg_bytes = construct_message_for_signing(message);
    verify(pubkey, &msg_bytes, &message.signature)
}

/// Construct the message bytes for signing/verification.
fn construct_message_for_signing(message: &Message) -> Vec<u8> {
    let mut bytes = Vec::new();

    // version (1 byte)
    bytes.push(message.version);

    // message type (2 bytes, big-endian)
    bytes.extend_from_slice(&message.message_type.to_u16().to_be_bytes());

    // message id (32 bytes)
    bytes.extend_from_slice(message.id.as_ref());

    // timestamp (8 bytes, big-endian)
    bytes.extend_from_slice(&message.timestamp.to_be_bytes());

    // sender (20 bytes)
    bytes.extend_from_slice(message.sender.as_ref());

    // payload hash (32 bytes)
    bytes.extend_from_slice(message.payload_hash().as_ref());

    bytes
}

/// Check if a message type value is valid.
///
/// Used for raw u16 values before parsing into MessageType.
pub fn is_valid_message_type(type_value: u16) -> bool {
    nodalync_wire::MessageType::from_u16(type_value).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use nodalync_crypto::{
        content_hash, generate_identity, peer_id_from_public_key, sign, Signature,
    };
    use nodalync_wire::MessageType;

    fn create_test_message(timestamp: Timestamp) -> Message {
        let (_, public_key) = generate_identity();
        let sender = peer_id_from_public_key(&public_key);

        Message::new(
            PROTOCOL_VERSION,
            MessageType::Ping,
            content_hash(b"message_id"),
            timestamp,
            sender,
            vec![],
            Signature([0u8; 64]),
        )
    }

    fn create_signed_message(timestamp: Timestamp) -> (Message, PublicKey) {
        let (private_key, public_key) = generate_identity();
        let sender = peer_id_from_public_key(&public_key);

        let mut message = Message::new(
            PROTOCOL_VERSION,
            MessageType::Ping,
            content_hash(b"message_id"),
            timestamp,
            sender,
            vec![],
            Signature([0u8; 64]),
        );

        // Sign the message
        let msg_bytes = construct_message_for_signing(&message);
        message.signature = sign(&private_key, &msg_bytes);

        (message, public_key)
    }

    #[test]
    fn test_valid_message() {
        let current_time = 1000000u64;
        let message = create_test_message(current_time);

        let result = validate_message_basic(&message, current_time);
        assert!(result.is_ok());
    }

    #[test]
    fn test_unsupported_version() {
        let current_time = 1000000u64;
        let mut message = create_test_message(current_time);
        message.version = 0xFF; // Wrong version

        let result = validate_message_basic(&message, current_time);
        assert!(matches!(
            result,
            Err(ValidationError::UnsupportedVersion {
                version: 0xFF,
                expected: PROTOCOL_VERSION
            })
        ));
    }

    #[test]
    fn test_timestamp_in_future_within_skew() {
        let current_time = 1000000u64;
        let future_time = current_time + MAX_CLOCK_SKEW_MS - 1000;
        let message = create_test_message(future_time);

        let result = validate_message_basic(&message, current_time);
        assert!(result.is_ok());
    }

    #[test]
    fn test_timestamp_in_past_within_skew() {
        let current_time = 1000000u64;
        let past_time = current_time - MAX_CLOCK_SKEW_MS + 1000;
        let message = create_test_message(past_time);

        let result = validate_message_basic(&message, current_time);
        assert!(result.is_ok());
    }

    #[test]
    fn test_timestamp_too_far_in_future() {
        let current_time = 1000000u64;
        let future_time = current_time + MAX_CLOCK_SKEW_MS + 1000;
        let message = create_test_message(future_time);

        let result = validate_message_basic(&message, current_time);
        assert!(matches!(
            result,
            Err(ValidationError::TimestampOutOfRange { .. })
        ));
    }

    #[test]
    fn test_timestamp_too_far_in_past() {
        let current_time = 1000000u64;
        let past_time = current_time - MAX_CLOCK_SKEW_MS - 1000;
        let message = create_test_message(past_time);

        let result = validate_message_basic(&message, current_time);
        assert!(matches!(
            result,
            Err(ValidationError::TimestampOutOfRange { .. })
        ));
    }

    #[test]
    fn test_timestamp_at_exact_boundary() {
        let current_time = 1000000u64;

        // Exactly at max skew should pass
        let boundary_time = current_time + MAX_CLOCK_SKEW_MS;
        let message = create_test_message(boundary_time);
        assert!(validate_message_basic(&message, current_time).is_ok());

        // One ms over should fail
        let over_boundary = current_time + MAX_CLOCK_SKEW_MS + 1;
        let message = create_test_message(over_boundary);
        assert!(validate_message_basic(&message, current_time).is_err());
    }

    #[test]
    fn test_valid_signature() {
        let current_time = 1000000u64;
        let (message, pubkey) = create_signed_message(current_time);

        let result = validate_message(&message, current_time, Some(&pubkey));
        assert!(result.is_ok());
    }

    #[test]
    fn test_invalid_signature() {
        let current_time = 1000000u64;
        let (message, _) = create_signed_message(current_time);

        // Use a different public key
        let (_, wrong_pubkey) = generate_identity();

        let result = validate_message(&message, current_time, Some(&wrong_pubkey));
        assert!(matches!(
            result,
            Err(ValidationError::InvalidMessageSignature)
        ));
    }

    #[test]
    fn test_tampered_message_fails_signature() {
        let current_time = 1000000u64;
        let (mut message, pubkey) = create_signed_message(current_time);

        // Tamper with the message
        message.payload = vec![0xFF, 0xFF];

        let result = validate_message(&message, current_time, Some(&pubkey));
        assert!(matches!(
            result,
            Err(ValidationError::InvalidMessageSignature)
        ));
    }

    #[test]
    fn test_is_valid_message_type() {
        // Valid types
        assert!(is_valid_message_type(0x0100)); // Announce
        assert!(is_valid_message_type(0x0300)); // QueryRequest
        assert!(is_valid_message_type(0x0700)); // Ping

        // Invalid types
        assert!(!is_valid_message_type(0x0000));
        assert!(!is_valid_message_type(0x9999));
        assert!(!is_valid_message_type(0xFFFF));
    }

    #[test]
    fn test_all_message_types() {
        let valid_types = [
            0x0100, 0x0101, 0x0110, 0x0111, // Discovery
            0x0200, 0x0201, // Preview
            0x0300, 0x0301, 0x0302, // Query
            0x0400, 0x0401, // Version
            0x0500, 0x0501, 0x0502, 0x0503, 0x0504, // Channel
            0x0600, 0x0601, // Settlement
            0x0700, 0x0701, 0x0710, // Peer
        ];

        for type_val in valid_types {
            assert!(
                is_valid_message_type(type_val),
                "Type {:04x} should be valid",
                type_val
            );
        }
    }

    #[test]
    fn test_construct_message_for_signing() {
        let message = create_test_message(1234567890);
        let bytes = construct_message_for_signing(&message);

        // Verify structure: 1 + 2 + 32 + 8 + 20 + 32 = 95 bytes
        assert_eq!(bytes.len(), 95);

        // Verify version
        assert_eq!(bytes[0], PROTOCOL_VERSION);

        // Verify message type
        let type_bytes = &bytes[1..3];
        assert_eq!(
            u16::from_be_bytes([type_bytes[0], type_bytes[1]]),
            MessageType::Ping.to_u16()
        );
    }
}
