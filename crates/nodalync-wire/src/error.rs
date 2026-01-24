//! Error types for the wire protocol module.
//!
//! This module defines errors that can occur during message encoding,
//! decoding, and format validation.

use thiserror::Error;

/// Errors that can occur when encoding a message or payload.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum EncodeError {
    /// CBOR encoding failed
    #[error("CBOR encoding failed: {0}")]
    Cbor(String),

    /// Payload exceeds maximum allowed size
    #[error("payload too large: {size} bytes exceeds maximum {max} bytes")]
    PayloadTooLarge {
        /// Actual size of the payload
        size: usize,
        /// Maximum allowed size
        max: usize,
    },
}

impl From<ciborium::ser::Error<std::io::Error>> for EncodeError {
    fn from(err: ciborium::ser::Error<std::io::Error>) -> Self {
        EncodeError::Cbor(err.to_string())
    }
}

/// Errors that can occur when decoding a message.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum DecodeError {
    /// Invalid protocol magic byte
    #[error("invalid magic byte: expected {expected:#04x}, got {got:#04x}")]
    InvalidMagic {
        /// Expected magic byte
        expected: u8,
        /// Received magic byte
        got: u8,
    },

    /// Invalid protocol version
    #[error("invalid protocol version: expected {expected:#04x}, got {got:#04x}")]
    InvalidVersion {
        /// Expected version
        expected: u8,
        /// Received version
        got: u8,
    },

    /// Invalid or unknown message type
    #[error("invalid message type: {0:#06x}")]
    InvalidMessageType(u16),

    /// Failed to decode CBOR payload
    #[error("payload decode failed: {0}")]
    PayloadDecodeFailed(String),

    /// Message was truncated (not enough bytes)
    #[error("truncated message: expected at least {expected} bytes, got {got}")]
    TruncatedMessage {
        /// Minimum expected bytes
        expected: usize,
        /// Actual bytes received
        got: usize,
    },

    /// Signature verification failed
    #[error("signature mismatch")]
    SignatureMismatch,

    /// Message ID mismatch during decode
    #[error("message ID mismatch")]
    IdMismatch,

    /// Generic IO error during decode
    #[error("IO error: {0}")]
    Io(String),
}

impl From<ciborium::de::Error<std::io::Error>> for DecodeError {
    fn from(err: ciborium::de::Error<std::io::Error>) -> Self {
        DecodeError::PayloadDecodeFailed(err.to_string())
    }
}

impl From<std::io::Error> for DecodeError {
    fn from(err: std::io::Error) -> Self {
        DecodeError::Io(err.to_string())
    }
}

/// Errors that can occur when validating message format.
///
/// These are distinct from semantic validation (e.g., business logic)
/// and focus on wire format correctness.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum FormatError {
    /// Protocol version is not supported
    #[error("unsupported protocol version: {0}")]
    InvalidVersion(u8),

    /// Message type is not recognized
    #[error("invalid message type")]
    InvalidType,

    /// Timestamp is outside acceptable range (> ±5 minutes from current time)
    #[error("timestamp out of range: {0}")]
    TimestampOutOfRange(u64),

    /// Sender PeerId is invalid
    #[error("invalid sender")]
    InvalidSender,

    /// Message signature is invalid
    #[error("invalid signature")]
    InvalidSignature,

    /// Message ID does not match computed hash
    #[error("message ID mismatch")]
    InvalidId,

    /// Payload does not match expected structure for message type
    #[error("invalid payload for message type")]
    InvalidPayload,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_error_display() {
        let err = EncodeError::Cbor("test error".to_string());
        assert!(format!("{}", err).contains("CBOR encoding failed"));

        let err = EncodeError::PayloadTooLarge {
            size: 1000,
            max: 500,
        };
        assert!(format!("{}", err).contains("1000 bytes"));
        assert!(format!("{}", err).contains("500 bytes"));
    }

    #[test]
    fn test_decode_error_display() {
        let err = DecodeError::InvalidMagic {
            expected: 0x00,
            got: 0xFF,
        };
        assert!(format!("{}", err).contains("0x00"));
        assert!(format!("{}", err).contains("0xff"));

        let err = DecodeError::InvalidVersion {
            expected: 0x01,
            got: 0x02,
        };
        assert!(format!("{}", err).contains("0x01"));
        assert!(format!("{}", err).contains("0x02"));

        let err = DecodeError::InvalidMessageType(0x9999);
        assert!(format!("{}", err).contains("0x9999"));

        let err = DecodeError::TruncatedMessage {
            expected: 100,
            got: 50,
        };
        assert!(format!("{}", err).contains("100"));
        assert!(format!("{}", err).contains("50"));
    }

    #[test]
    fn test_format_error_display() {
        let err = FormatError::InvalidVersion(0x99);
        // 0x99 = 153 in decimal
        assert!(format!("{}", err).contains("153"));

        let err = FormatError::TimestampOutOfRange(123456);
        assert!(format!("{}", err).contains("123456"));
    }
}
