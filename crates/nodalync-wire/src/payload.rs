//! Payload types for all protocol messages.
//!
//! This module defines the type-specific payloads for each message type
//! as specified in Protocol Specification §6.2-§6.8.

use nodalync_crypto::{Hash, PeerId, PublicKey, Signature, Timestamp};
use nodalync_types::{Amount, ContentType, ErrorCode, L1Summary, Manifest, Payment, Visibility};
use serde::{Deserialize, Serialize};

// =============================================================================
// Discovery Payloads (§6.2)
// =============================================================================

/// Payload for ANNOUNCE messages.
///
/// Announces content availability to the network via DHT.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AnnouncePayload {
    /// Content hash being announced
    pub hash: Hash,
    /// Type of content (L0, L1, L3)
    pub content_type: ContentType,
    /// Content title
    pub title: String,
    /// L1 summary for preview
    pub l1_summary: L1Summary,
    /// Query price
    pub price: Amount,
    /// Multiaddrs where content can be retrieved
    pub addresses: Vec<String>,
    /// The libp2p peer ID of the publisher (base58 encoded)
    /// Used to dial the publisher directly when retrieving content
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publisher_peer_id: Option<String>,
}

/// Payload for ANNOUNCE_UPDATE messages.
///
/// Announces a new version of existing content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AnnounceUpdatePayload {
    /// Stable version root identifier
    pub version_root: Hash,
    /// New version hash
    pub new_hash: Hash,
    /// Version number
    pub version_number: u32,
    /// Updated title
    pub title: String,
    /// Updated L1 summary
    pub l1_summary: L1Summary,
    /// Updated price
    pub price: Amount,
}

/// Payload for SEARCH messages.
///
/// Requests content by hash lookup in the DHT.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SearchPayload {
    /// Search query (typically a hash for lookup)
    pub query: String,
    /// Optional search filters
    pub filters: Option<SearchFilters>,
    /// Maximum results to return
    pub limit: u32,
    /// Offset for pagination
    pub offset: u32,
}

/// Filters for search queries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct SearchFilters {
    /// Filter by content types
    pub content_types: Option<Vec<ContentType>>,
    /// Maximum price filter
    pub max_price: Option<Amount>,
    /// Minimum reputation score
    pub min_reputation: Option<i64>,
    /// Created after timestamp
    pub created_after: Option<Timestamp>,
    /// Created before timestamp
    pub created_before: Option<Timestamp>,
    /// Filter by tags
    pub tags: Option<Vec<String>>,
}

/// Payload for SEARCH_RESPONSE messages.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SearchResponsePayload {
    /// Search results
    pub results: Vec<SearchResult>,
    /// Total matching results (may be more than returned)
    pub total_count: u64,
}

/// A single search result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SearchResult {
    /// Content hash
    pub hash: Hash,
    /// Content type
    pub content_type: ContentType,
    /// Content title
    pub title: String,
    /// Content owner
    pub owner: PeerId,
    /// L1 summary preview
    pub l1_summary: L1Summary,
    /// Query price
    pub price: Amount,
    /// Total queries served
    pub total_queries: u64,
    /// Relevance score (0.0 to 1.0)
    pub relevance_score: f64,
}

// =============================================================================
// Preview Payloads (§6.3)
// =============================================================================

/// Payload for PREVIEW_REQUEST messages.
///
/// Requests L1 summary without payment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PreviewRequestPayload {
    /// Content hash to preview
    pub hash: Hash,
}

/// Payload for PREVIEW_RESPONSE messages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PreviewResponsePayload {
    /// Content hash
    pub hash: Hash,
    /// Content manifest (without full content)
    pub manifest: Manifest,
    /// L1 summary with preview mentions
    pub l1_summary: L1Summary,
}

// =============================================================================
// Query Payloads (§6.4)
// =============================================================================

/// Payload for QUERY_REQUEST messages.
///
/// Requests full content with payment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct QueryRequestPayload {
    /// Content hash to query
    pub hash: Hash,
    /// Optional natural language query (for future use)
    pub query: Option<String>,
    /// Payment for this query
    pub payment: Payment,
    /// Optional version specification
    pub version_spec: Option<VersionSpec>,
    /// Payment nonce for replay protection (must be > channel nonce)
    #[serde(default)]
    pub payment_nonce: u64,
}

/// Specification for which version to retrieve.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VersionSpec {
    /// Get the latest version
    #[default]
    Latest,
    /// Get a specific version number
    Number(u32),
    /// Get a specific version by hash
    Hash(Hash),
}

/// Payload for QUERY_RESPONSE messages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct QueryResponsePayload {
    /// Content hash
    pub hash: Hash,
    /// Full content bytes
    pub content: Vec<u8>,
    /// Content manifest
    pub manifest: Manifest,
    /// Payment receipt
    pub payment_receipt: PaymentReceipt,
}

/// Receipt confirming payment was processed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PaymentReceipt {
    /// Unique payment identifier
    pub payment_id: Hash,
    /// Amount paid
    pub amount: Amount,
    /// Receipt timestamp
    pub timestamp: Timestamp,
    /// Channel nonce at time of payment
    pub channel_nonce: u64,
    /// Signature from content distributor
    pub distributor_signature: Signature,
}

/// Payload for QUERY_ERROR messages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct QueryErrorPayload {
    /// Content hash that was queried
    pub hash: Hash,
    /// Error code
    pub error_code: ErrorCode,
    /// Optional human-readable message
    pub message: Option<String>,
}

// =============================================================================
// Version Payloads (§6.5)
// =============================================================================

/// Payload for VERSION_REQUEST messages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct VersionRequestPayload {
    /// Stable version root identifier
    pub version_root: Hash,
}

/// Payload for VERSION_RESPONSE messages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct VersionResponsePayload {
    /// Version root hash
    pub version_root: Hash,
    /// All available versions
    pub versions: Vec<VersionInfo>,
    /// Hash of the latest version
    pub latest: Hash,
}

/// Information about a single version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct VersionInfo {
    /// Version content hash
    pub hash: Hash,
    /// Version number
    pub number: u32,
    /// Creation timestamp
    pub timestamp: Timestamp,
    /// Visibility level
    pub visibility: Visibility,
    /// Query price
    pub price: Amount,
}

// =============================================================================
// Channel Payloads (§6.6)
// =============================================================================

/// Payload for CHANNEL_OPEN messages.
///
/// Initiates opening a payment channel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ChannelOpenPayload {
    /// Unique channel identifier
    pub channel_id: Hash,
    /// Initial deposit from opener
    pub initial_balance: Amount,
    /// Optional on-chain funding transaction
    pub funding_tx: Option<Vec<u8>>,
}

/// Payload for CHANNEL_ACCEPT messages.
///
/// Accepts a channel open request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ChannelAcceptPayload {
    /// Channel identifier being accepted
    pub channel_id: Hash,
    /// Initial deposit from acceptor
    pub initial_balance: Amount,
    /// Optional on-chain funding transaction
    pub funding_tx: Option<Vec<u8>>,
}

/// Payload for CHANNEL_UPDATE messages.
///
/// Updates channel state with new payments.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ChannelUpdatePayload {
    /// Channel identifier
    pub channel_id: Hash,
    /// Monotonically increasing nonce
    pub nonce: u64,
    /// Current balances
    pub balances: ChannelBalances,
    /// Payments in this update
    pub payments: Vec<Payment>,
    /// Signature over the state
    pub signature: Signature,
}

/// Balance distribution in a channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ChannelBalances {
    /// Balance of the channel initiator
    pub initiator: Amount,
    /// Balance of the channel responder
    pub responder: Amount,
}

impl ChannelBalances {
    /// Create new channel balances.
    pub fn new(initiator: Amount, responder: Amount) -> Self {
        Self {
            initiator,
            responder,
        }
    }

    /// Get the total balance in the channel.
    pub fn total(&self) -> Amount {
        self.initiator + self.responder
    }
}

/// Payload for CHANNEL_CLOSE messages.
///
/// Requests cooperative channel close.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ChannelClosePayload {
    /// Channel identifier
    pub channel_id: Hash,
    /// Final agreed balances
    pub final_balances: ChannelBalances,
    /// Proposed on-chain settlement transaction
    pub settlement_tx: Vec<u8>,
}

/// Payload for CHANNEL_DISPUTE messages.
///
/// Initiates an on-chain dispute.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ChannelDisputePayload {
    /// Channel identifier
    pub channel_id: Hash,
    /// Highest known channel state
    pub claimed_state: ChannelUpdatePayload,
    /// Supporting evidence (signed states, etc.)
    pub evidence: Vec<Vec<u8>>,
}

// =============================================================================
// Settlement Payloads (§6.7)
// =============================================================================

/// Payload for SETTLE_BATCH messages.
///
/// Submits a batch of settlements.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SettleBatchPayload {
    /// Unique batch identifier
    pub batch_id: Hash,
    /// Settlement entries
    pub entries: Vec<SettlementEntry>,
    /// Merkle root of entries for verification
    pub merkle_root: Hash,
    /// Signature from batch creator
    pub signature: Signature,
}

/// A single entry in a settlement batch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SettlementEntry {
    /// Recipient peer ID
    pub recipient: PeerId,
    /// Total amount to settle
    pub amount: Amount,
    /// Content hashes for audit trail
    pub provenance_hashes: Vec<Hash>,
    /// Payment IDs included in this entry
    pub payment_ids: Vec<Hash>,
}

/// Payload for SETTLE_CONFIRM messages.
///
/// Confirms settlement completion on-chain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SettleConfirmPayload {
    /// Batch that was settled
    pub batch_id: Hash,
    /// On-chain transaction ID
    pub transaction_id: String,
    /// Block number containing the transaction
    pub block_number: u64,
    /// Confirmation timestamp
    pub timestamp: Timestamp,
}

// =============================================================================
// Peer Payloads (§6.8)
// =============================================================================

/// Payload for PING messages.
///
/// Liveness check with nonce for round-trip verification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PingPayload {
    /// Random nonce to be echoed in PONG
    pub nonce: u64,
}

/// Payload for PONG messages.
///
/// Response to PING with echoed nonce.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PongPayload {
    /// Echoed nonce from PING
    pub nonce: u64,
}

/// Payload for PEER_INFO messages.
///
/// Exchanges peer information and capabilities.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PeerInfoPayload {
    /// Peer identifier
    pub peer_id: PeerId,
    /// Peer's public key
    pub public_key: PublicKey,
    /// Multiaddrs for this peer
    pub addresses: Vec<String>,
    /// Supported capabilities
    pub capabilities: Vec<Capability>,
    /// Number of content items hosted
    pub content_count: u64,
    /// Uptime in seconds
    pub uptime: u64,
}

/// Peer capabilities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
#[non_exhaustive]
pub enum Capability {
    /// Can serve query requests
    Query = 0x01,
    /// Supports payment channels
    Channel = 0x02,
    /// Can initiate settlements
    Settle = 0x04,
    /// Participates in DHT indexing
    Index = 0x08,
}

impl Capability {
    /// Convert a u8 value to a Capability.
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0x01 => Some(Capability::Query),
            0x02 => Some(Capability::Channel),
            0x04 => Some(Capability::Settle),
            0x08 => Some(Capability::Index),
            _ => None,
        }
    }

    /// Convert to u8 value.
    pub fn to_u8(self) -> u8 {
        self as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nodalync_crypto::content_hash;

    fn test_hash(data: &[u8]) -> Hash {
        content_hash(data)
    }

    fn test_l1_summary() -> L1Summary {
        L1Summary::empty(test_hash(b"source"))
    }

    #[test]
    fn test_announce_payload_serialization() {
        let payload = AnnouncePayload {
            hash: test_hash(b"content"),
            content_type: ContentType::L0,
            title: "Test Content".to_string(),
            l1_summary: test_l1_summary(),
            price: 100,
            addresses: vec!["/ip4/127.0.0.1/tcp/9000".to_string()],
            publisher_peer_id: None,
        };

        let json = serde_json::to_string(&payload).unwrap();
        let deserialized: AnnouncePayload = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.title, payload.title);
        assert_eq!(deserialized.price, payload.price);
    }

    #[test]
    fn test_announce_payload_with_publisher_peer_id_cbor() {
        // Test CBOR roundtrip with publisher_peer_id set to Some
        let payload = AnnouncePayload {
            hash: test_hash(b"content"),
            content_type: ContentType::L0,
            title: "Test Content".to_string(),
            l1_summary: test_l1_summary(),
            price: 100,
            addresses: vec!["/ip4/127.0.0.1/tcp/9000".to_string()],
            publisher_peer_id: Some(
                "12D3KooWLvP5fP18r2B1xLV21eq9JyMzkySxvdTdWvuaxzVcs289".to_string(),
            ),
        };

        // Test CBOR encoding/decoding (what the wire uses)
        let mut cbor_buf = Vec::new();
        ciborium::into_writer(&payload, &mut cbor_buf).unwrap();
        let decoded: AnnouncePayload = ciborium::from_reader(&cbor_buf[..]).unwrap();

        assert_eq!(decoded.title, payload.title);
        assert_eq!(decoded.price, payload.price);
        assert_eq!(decoded.publisher_peer_id, payload.publisher_peer_id);
        assert_eq!(
            decoded.publisher_peer_id,
            Some("12D3KooWLvP5fP18r2B1xLV21eq9JyMzkySxvdTdWvuaxzVcs289".to_string())
        );
    }

    #[test]
    fn test_announce_payload_backwards_compatible() {
        // Test that decoding an old CBOR payload (without publisher_peer_id) works
        // by encoding a payload with None and verifying it decodes correctly
        let payload = AnnouncePayload {
            hash: test_hash(b"content"),
            content_type: ContentType::L0,
            title: "Test".to_string(),
            l1_summary: test_l1_summary(),
            price: 0,
            addresses: vec![],
            publisher_peer_id: None,
        };

        // Encode without publisher_peer_id
        let mut cbor_buf = Vec::new();
        ciborium::into_writer(&payload, &mut cbor_buf).unwrap();

        // Decode should work and have None for publisher_peer_id
        let decoded: AnnouncePayload = ciborium::from_reader(&cbor_buf[..]).unwrap();
        assert_eq!(decoded.publisher_peer_id, None);
    }

    #[test]
    fn test_search_filters_default() {
        let filters = SearchFilters::default();
        assert!(filters.content_types.is_none());
        assert!(filters.max_price.is_none());
        assert!(filters.tags.is_none());
    }

    #[test]
    fn test_version_spec_default() {
        let spec = VersionSpec::default();
        assert!(matches!(spec, VersionSpec::Latest));
    }

    #[test]
    fn test_channel_balances() {
        let balances = ChannelBalances::new(1000, 500);
        assert_eq!(balances.total(), 1500);
    }

    #[test]
    fn test_capability_values() {
        assert_eq!(Capability::Query as u8, 0x01);
        assert_eq!(Capability::Channel as u8, 0x02);
        assert_eq!(Capability::Settle as u8, 0x04);
        assert_eq!(Capability::Index as u8, 0x08);
    }

    #[test]
    fn test_capability_from_u8() {
        assert_eq!(Capability::from_u8(0x01), Some(Capability::Query));
        assert_eq!(Capability::from_u8(0x02), Some(Capability::Channel));
        assert_eq!(Capability::from_u8(0x04), Some(Capability::Settle));
        assert_eq!(Capability::from_u8(0x08), Some(Capability::Index));
        assert_eq!(Capability::from_u8(0xFF), None);
    }

    #[test]
    fn test_ping_pong_roundtrip() {
        let ping = PingPayload { nonce: 12345 };
        let json = serde_json::to_string(&ping).unwrap();
        let deserialized: PingPayload = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.nonce, ping.nonce);

        let pong = PongPayload { nonce: ping.nonce };
        assert_eq!(pong.nonce, ping.nonce);
    }

    #[test]
    fn test_query_error_payload() {
        let payload = QueryErrorPayload {
            hash: test_hash(b"content"),
            error_code: ErrorCode::NotFound,
            message: Some("Content not found".to_string()),
        };

        let json = serde_json::to_string(&payload).unwrap();
        let deserialized: QueryErrorPayload = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.error_code, ErrorCode::NotFound);
    }
}
