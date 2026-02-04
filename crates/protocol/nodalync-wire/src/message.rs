//! Message types and envelope structure for the wire protocol.
//!
//! This module defines the core message types used in the Nodalync protocol
//! as specified in Protocol Specification §6.1.

use nodalync_crypto::{Hash, PeerId, Signature, Timestamp};
use serde::{Deserialize, Serialize};

use crate::error::DecodeError;

/// Message type identifiers for all protocol messages.
///
/// Message types are organized into categories by their high byte:
/// - `0x01xx`: Discovery messages
/// - `0x02xx`: Preview messages
/// - `0x03xx`: Query messages
/// - `0x04xx`: Version messages
/// - `0x05xx`: Channel messages
/// - `0x06xx`: Settlement messages
/// - `0x07xx`: Peer messages
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u16)]
#[non_exhaustive]
pub enum MessageType {
    // =========================================================================
    // Discovery Messages (0x01xx)
    // =========================================================================
    /// Announce content availability to the network
    Announce = 0x0100,

    /// Update an existing announcement (new version)
    AnnounceUpdate = 0x0101,

    /// Search for content (hash-based lookup)
    Search = 0x0110,

    /// Response to a search request
    SearchResponse = 0x0111,

    // =========================================================================
    // Preview Messages (0x02xx)
    // =========================================================================
    /// Request L1 summary preview without payment
    PreviewRequest = 0x0200,

    /// Response with L1 summary
    PreviewResponse = 0x0201,

    // =========================================================================
    // Query Messages (0x03xx)
    // =========================================================================
    /// Request full content with payment
    QueryRequest = 0x0300,

    /// Response with full content
    QueryResponse = 0x0301,

    /// Error response to a query
    QueryError = 0x0302,

    // =========================================================================
    // Version Messages (0x04xx)
    // =========================================================================
    /// Request version history for content
    VersionRequest = 0x0400,

    /// Response with version history
    VersionResponse = 0x0401,

    // =========================================================================
    // Channel Messages (0x05xx)
    // =========================================================================
    /// Open a new payment channel
    ChannelOpen = 0x0500,

    /// Accept a channel open request
    ChannelAccept = 0x0501,

    /// Update channel state with new payments
    ChannelUpdate = 0x0502,

    /// Request to close a channel cooperatively
    ChannelClose = 0x0503,

    /// Initiate a channel dispute
    ChannelDispute = 0x0504,

    /// Acknowledge a cooperative channel close (responder's signature)
    ChannelCloseAck = 0x0505,

    // =========================================================================
    // Settlement Messages (0x06xx)
    // =========================================================================
    /// Submit a batch of settlements
    SettleBatch = 0x0600,

    /// Confirm settlement completion
    SettleConfirm = 0x0601,

    // =========================================================================
    // Peer Messages (0x07xx)
    // =========================================================================
    /// Ping for liveness check
    Ping = 0x0700,

    /// Pong response to ping
    Pong = 0x0701,

    /// Peer information exchange
    PeerInfo = 0x0710,
}

impl MessageType {
    /// Convert a u16 value to a MessageType.
    ///
    /// Returns `Err(DecodeError::InvalidMessageType)` if the value is not a valid message type.
    pub fn from_u16(value: u16) -> Result<Self, DecodeError> {
        match value {
            // Discovery
            0x0100 => Ok(MessageType::Announce),
            0x0101 => Ok(MessageType::AnnounceUpdate),
            0x0110 => Ok(MessageType::Search),
            0x0111 => Ok(MessageType::SearchResponse),
            // Preview
            0x0200 => Ok(MessageType::PreviewRequest),
            0x0201 => Ok(MessageType::PreviewResponse),
            // Query
            0x0300 => Ok(MessageType::QueryRequest),
            0x0301 => Ok(MessageType::QueryResponse),
            0x0302 => Ok(MessageType::QueryError),
            // Version
            0x0400 => Ok(MessageType::VersionRequest),
            0x0401 => Ok(MessageType::VersionResponse),
            // Channel
            0x0500 => Ok(MessageType::ChannelOpen),
            0x0501 => Ok(MessageType::ChannelAccept),
            0x0502 => Ok(MessageType::ChannelUpdate),
            0x0503 => Ok(MessageType::ChannelClose),
            0x0504 => Ok(MessageType::ChannelDispute),
            0x0505 => Ok(MessageType::ChannelCloseAck),
            // Settlement
            0x0600 => Ok(MessageType::SettleBatch),
            0x0601 => Ok(MessageType::SettleConfirm),
            // Peer
            0x0700 => Ok(MessageType::Ping),
            0x0701 => Ok(MessageType::Pong),
            0x0710 => Ok(MessageType::PeerInfo),
            _ => Err(DecodeError::InvalidMessageType(value)),
        }
    }

    /// Convert the message type to its u16 wire format value.
    pub fn to_u16(self) -> u16 {
        self as u16
    }

    /// Check if this is a discovery message (0x01xx).
    pub fn is_discovery(&self) -> bool {
        let code = *self as u16;
        (0x0100..=0x01FF).contains(&code)
    }

    /// Check if this is a preview message (0x02xx).
    pub fn is_preview(&self) -> bool {
        let code = *self as u16;
        (0x0200..=0x02FF).contains(&code)
    }

    /// Check if this is a query message (0x03xx).
    pub fn is_query(&self) -> bool {
        let code = *self as u16;
        (0x0300..=0x03FF).contains(&code)
    }

    /// Check if this is a version message (0x04xx).
    pub fn is_version(&self) -> bool {
        let code = *self as u16;
        (0x0400..=0x04FF).contains(&code)
    }

    /// Check if this is a channel message (0x05xx).
    pub fn is_channel(&self) -> bool {
        let code = *self as u16;
        (0x0500..=0x05FF).contains(&code)
    }

    /// Check if this is a settlement message (0x06xx).
    pub fn is_settlement(&self) -> bool {
        let code = *self as u16;
        (0x0600..=0x06FF).contains(&code)
    }

    /// Check if this is a peer message (0x07xx).
    pub fn is_peer(&self) -> bool {
        let code = *self as u16;
        (0x0700..=0x07FF).contains(&code)
    }

    /// Check if this message type expects a response.
    pub fn expects_response(&self) -> bool {
        matches!(
            self,
            MessageType::Search
                | MessageType::PreviewRequest
                | MessageType::QueryRequest
                | MessageType::VersionRequest
                | MessageType::ChannelOpen
                | MessageType::Ping
        )
    }
}

impl std::fmt::Display for MessageType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MessageType::Announce => write!(f, "ANNOUNCE"),
            MessageType::AnnounceUpdate => write!(f, "ANNOUNCE_UPDATE"),
            MessageType::Search => write!(f, "SEARCH"),
            MessageType::SearchResponse => write!(f, "SEARCH_RESPONSE"),
            MessageType::PreviewRequest => write!(f, "PREVIEW_REQUEST"),
            MessageType::PreviewResponse => write!(f, "PREVIEW_RESPONSE"),
            MessageType::QueryRequest => write!(f, "QUERY_REQUEST"),
            MessageType::QueryResponse => write!(f, "QUERY_RESPONSE"),
            MessageType::QueryError => write!(f, "QUERY_ERROR"),
            MessageType::VersionRequest => write!(f, "VERSION_REQUEST"),
            MessageType::VersionResponse => write!(f, "VERSION_RESPONSE"),
            MessageType::ChannelOpen => write!(f, "CHANNEL_OPEN"),
            MessageType::ChannelAccept => write!(f, "CHANNEL_ACCEPT"),
            MessageType::ChannelUpdate => write!(f, "CHANNEL_UPDATE"),
            MessageType::ChannelClose => write!(f, "CHANNEL_CLOSE"),
            MessageType::ChannelDispute => write!(f, "CHANNEL_DISPUTE"),
            MessageType::ChannelCloseAck => write!(f, "CHANNEL_CLOSE_ACK"),
            MessageType::SettleBatch => write!(f, "SETTLE_BATCH"),
            MessageType::SettleConfirm => write!(f, "SETTLE_CONFIRM"),
            MessageType::Ping => write!(f, "PING"),
            MessageType::Pong => write!(f, "PONG"),
            MessageType::PeerInfo => write!(f, "PEER_INFO"),
        }
    }
}

/// A protocol message envelope.
///
/// All protocol messages share this common envelope structure.
/// The payload is type-specific and encoded as CBOR.
///
/// Spec §6.1: Message Envelope
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    /// Protocol version (currently 0x01)
    pub version: u8,

    /// Type of this message
    pub message_type: MessageType,

    /// Unique message identifier (computed hash)
    pub id: Hash,

    /// Message creation timestamp (milliseconds since Unix epoch)
    pub timestamp: Timestamp,

    /// Sender's peer identifier
    pub sender: PeerId,

    /// Type-specific payload (CBOR encoded)
    pub payload: Vec<u8>,

    /// Signature over the message hash
    ///
    /// Signs `H(version || type || id || timestamp || sender || payload_hash)`
    pub signature: Signature,
}

impl Message {
    /// Create a new message with the given parameters.
    ///
    /// Note: The `id` and `signature` should typically be computed using
    /// `crate::encoding::create_message()` which handles signing.
    pub fn new(
        version: u8,
        message_type: MessageType,
        id: Hash,
        timestamp: Timestamp,
        sender: PeerId,
        payload: Vec<u8>,
        signature: Signature,
    ) -> Self {
        Self {
            version,
            message_type,
            id,
            timestamp,
            sender,
            payload,
            signature,
        }
    }

    /// Get the payload hash (for signature computation).
    pub fn payload_hash(&self) -> Hash {
        crate::encoding::content_hash(&self.payload)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_type_values() {
        // Discovery
        assert_eq!(MessageType::Announce as u16, 0x0100);
        assert_eq!(MessageType::AnnounceUpdate as u16, 0x0101);
        assert_eq!(MessageType::Search as u16, 0x0110);
        assert_eq!(MessageType::SearchResponse as u16, 0x0111);

        // Preview
        assert_eq!(MessageType::PreviewRequest as u16, 0x0200);
        assert_eq!(MessageType::PreviewResponse as u16, 0x0201);

        // Query
        assert_eq!(MessageType::QueryRequest as u16, 0x0300);
        assert_eq!(MessageType::QueryResponse as u16, 0x0301);
        assert_eq!(MessageType::QueryError as u16, 0x0302);

        // Version
        assert_eq!(MessageType::VersionRequest as u16, 0x0400);
        assert_eq!(MessageType::VersionResponse as u16, 0x0401);

        // Channel
        assert_eq!(MessageType::ChannelOpen as u16, 0x0500);
        assert_eq!(MessageType::ChannelAccept as u16, 0x0501);
        assert_eq!(MessageType::ChannelUpdate as u16, 0x0502);
        assert_eq!(MessageType::ChannelClose as u16, 0x0503);
        assert_eq!(MessageType::ChannelDispute as u16, 0x0504);

        // Settlement
        assert_eq!(MessageType::SettleBatch as u16, 0x0600);
        assert_eq!(MessageType::SettleConfirm as u16, 0x0601);

        // Peer
        assert_eq!(MessageType::Ping as u16, 0x0700);
        assert_eq!(MessageType::Pong as u16, 0x0701);
        assert_eq!(MessageType::PeerInfo as u16, 0x0710);
    }

    #[test]
    fn test_message_type_from_u16() {
        assert_eq!(
            MessageType::from_u16(0x0100).unwrap(),
            MessageType::Announce
        );
        assert_eq!(
            MessageType::from_u16(0x0300).unwrap(),
            MessageType::QueryRequest
        );
        assert_eq!(MessageType::from_u16(0x0700).unwrap(), MessageType::Ping);

        assert!(MessageType::from_u16(0x9999).is_err());
        assert!(MessageType::from_u16(0x0000).is_err());
    }

    #[test]
    fn test_message_type_categories() {
        assert!(MessageType::Announce.is_discovery());
        assert!(MessageType::Search.is_discovery());
        assert!(!MessageType::Announce.is_query());

        assert!(MessageType::PreviewRequest.is_preview());
        assert!(MessageType::PreviewResponse.is_preview());

        assert!(MessageType::QueryRequest.is_query());
        assert!(MessageType::QueryError.is_query());

        assert!(MessageType::VersionRequest.is_version());
        assert!(MessageType::VersionResponse.is_version());

        assert!(MessageType::ChannelOpen.is_channel());
        assert!(MessageType::ChannelDispute.is_channel());

        assert!(MessageType::SettleBatch.is_settlement());
        assert!(MessageType::SettleConfirm.is_settlement());

        assert!(MessageType::Ping.is_peer());
        assert!(MessageType::PeerInfo.is_peer());
    }

    #[test]
    fn test_message_type_expects_response() {
        assert!(MessageType::Search.expects_response());
        assert!(MessageType::PreviewRequest.expects_response());
        assert!(MessageType::QueryRequest.expects_response());
        assert!(MessageType::VersionRequest.expects_response());
        assert!(MessageType::ChannelOpen.expects_response());
        assert!(MessageType::Ping.expects_response());

        assert!(!MessageType::SearchResponse.expects_response());
        assert!(!MessageType::Announce.expects_response());
        assert!(!MessageType::Pong.expects_response());
    }

    #[test]
    fn test_message_type_display() {
        assert_eq!(format!("{}", MessageType::Announce), "ANNOUNCE");
        assert_eq!(format!("{}", MessageType::QueryRequest), "QUERY_REQUEST");
        assert_eq!(format!("{}", MessageType::Ping), "PING");
    }

    #[test]
    fn test_message_type_roundtrip() {
        let types = [
            MessageType::Announce,
            MessageType::Search,
            MessageType::QueryRequest,
            MessageType::ChannelOpen,
            MessageType::Ping,
        ];

        for msg_type in types {
            let value = msg_type.to_u16();
            let recovered = MessageType::from_u16(value).unwrap();
            assert_eq!(msg_type, recovered);
        }
    }

    #[test]
    fn test_message_type_all_variants_u16_roundtrip() {
        // Exhaustive test of all message type u16 roundtrips
        let all_types = [
            (0x0100u16, MessageType::Announce),
            (0x0101, MessageType::AnnounceUpdate),
            (0x0110, MessageType::Search),
            (0x0111, MessageType::SearchResponse),
            (0x0200, MessageType::PreviewRequest),
            (0x0201, MessageType::PreviewResponse),
            (0x0300, MessageType::QueryRequest),
            (0x0301, MessageType::QueryResponse),
            (0x0302, MessageType::QueryError),
            (0x0400, MessageType::VersionRequest),
            (0x0401, MessageType::VersionResponse),
            (0x0500, MessageType::ChannelOpen),
            (0x0501, MessageType::ChannelAccept),
            (0x0502, MessageType::ChannelUpdate),
            (0x0503, MessageType::ChannelClose),
            (0x0504, MessageType::ChannelDispute),
            (0x0505, MessageType::ChannelCloseAck),
            (0x0600, MessageType::SettleBatch),
            (0x0601, MessageType::SettleConfirm),
            (0x0700, MessageType::Ping),
            (0x0701, MessageType::Pong),
            (0x0710, MessageType::PeerInfo),
        ];
        for (value, expected) in all_types {
            let parsed = MessageType::from_u16(value).unwrap();
            assert_eq!(parsed, expected, "from_u16({:#06x}) failed", value);
            assert_eq!(parsed.to_u16(), value, "to_u16() failed for {:?}", expected);
        }
    }

    #[test]
    fn test_message_payload_hash_deterministic() {
        let msg = Message::new(
            1,
            MessageType::Ping,
            Hash([0u8; 32]),
            1234567890,
            PeerId::from_bytes([1u8; 20]),
            vec![1, 2, 3, 4, 5],
            Signature::from_bytes([0u8; 64]),
        );
        let h1 = msg.payload_hash();
        let h2 = msg.payload_hash();
        assert_eq!(h1, h2);
        // Different payload should yield different hash
        let msg2 = Message::new(
            1,
            MessageType::Ping,
            Hash([0u8; 32]),
            1234567890,
            PeerId::from_bytes([1u8; 20]),
            vec![5, 4, 3, 2, 1],
            Signature::from_bytes([0u8; 64]),
        );
        assert_ne!(h1, msg2.payload_hash());
    }
}
