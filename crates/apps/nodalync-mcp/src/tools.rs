//! MCP tool input/output types.
//!
//! Defines the request and response types for MCP tools.
//!
//! The MCP server abstracts payment complexity from AI agents:
//! - Agents simply search for content and query by hash
//! - Channel opening, deposits, and settlements happen automatically
//! - Transaction confirmations are returned in responses

use nodalync_types::Hash;
use rmcp::schemars;
use rmcp::schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// ============================================================================
// query_knowledge Tool
// ============================================================================

/// Input for the `query_knowledge` tool.
///
/// Queries content from the Nodalync network. Payment is handled automatically:
/// - Opens payment channels when needed
/// - Auto-deposits if settlement balance is insufficient
/// - Returns transaction confirmations in the response
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct QueryKnowledgeInput {
    /// The query string (natural language or content hash).
    pub query: String,

    /// Maximum budget for this query in HBAR.
    /// If not specified, uses auto-approve threshold.
    #[serde(default)]
    pub budget_hbar: Option<f64>,
}

/// Output from the `query_knowledge` tool.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct QueryKnowledgeOutput {
    /// The retrieved content.
    pub content: String,

    /// Content hash (base58 encoded).
    pub hash: String,

    /// Source hashes (L0 content this derives from).
    pub sources: Vec<String>,

    /// Full provenance chain (all contributing content).
    pub provenance: Vec<String>,

    /// Actual cost of this query in HBAR.
    pub cost_hbar: f64,

    /// Remaining session budget in HBAR.
    pub remaining_budget_hbar: f64,

    /// Payment and settlement transaction details.
    /// Contains all Hedera transactions triggered by this query.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payment: Option<PaymentDetails>,
}

/// Details about payment transactions for a query.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct PaymentDetails {
    /// Whether a new payment channel was opened.
    pub channel_opened: bool,

    /// Channel ID if a channel was opened or used (base58 encoded).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<String>,

    /// Hedera transaction ID for channel opening (if channel was opened on-chain).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_tx_id: Option<String>,

    /// Hedera transaction ID for auto-deposit (if deposit was needed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deposit_tx_id: Option<String>,

    /// Amount auto-deposited in HBAR (if deposit was needed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deposit_amount_hbar: Option<f64>,

    /// Peer ID of the content provider.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_peer_id: Option<String>,

    /// Payment receipt ID from the content server.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payment_receipt_id: Option<String>,

    /// Current Hedera account balance after this query (HBAR).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hedera_balance_hbar: Option<f64>,
}

// ============================================================================
// list_sources Tool
// ============================================================================

/// Input for the `list_sources` tool.
///
/// Lists available content sources, optionally filtered by topic.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ListSourcesInput {
    /// Optional topic filter.
    #[serde(default)]
    pub topic: Option<String>,

    /// Maximum number of results (default: 10, max: 50).
    #[serde(default)]
    pub limit: Option<u32>,

    /// Include content from network peers (default: false).
    /// When true, searches local content + cached announcements + connected peers.
    #[serde(default)]
    pub include_network: Option<bool>,
}

/// A single source in the list output.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct SourceInfo {
    /// Content hash (base58 encoded).
    pub hash: String,

    /// Content title.
    pub title: String,

    /// Price per query in HBAR.
    pub price_hbar: f64,

    /// Short preview (L1 mentions).
    pub preview: String,

    /// Primary topics.
    pub topics: Vec<String>,

    /// Peer ID of the content provider (for opening payment channels).
    /// May be None for locally-owned content or if provider is unknown.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peer_id: Option<String>,
}

/// Output from the `list_sources` tool.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ListSourcesOutput {
    /// Available sources matching the query.
    pub sources: Vec<SourceInfo>,

    /// Total number of sources available (may be > sources.len()).
    pub total_available: u32,
}

// ============================================================================
// status Tool (unified status)
// ============================================================================

/// Output from the unified `status` tool.
/// Combines health, budget, channel, and Hedera status in one response.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct StatusOutput {
    // === Network Status ===
    /// Number of connected peers.
    pub connected_peers: u32,
    /// Whether the node has bootstrapped to the network.
    pub is_bootstrapped: bool,
    /// Node's Nodalync peer ID.
    pub peer_id: String,
    /// Total content items available locally.
    pub local_content_count: u32,

    // === Budget Status ===
    /// Remaining budget for this session in HBAR.
    pub budget_remaining_hbar: f64,
    /// Total session budget in HBAR.
    pub budget_total_hbar: f64,
    /// Amount spent in this session in HBAR.
    pub budget_spent_hbar: f64,

    // === Channel Status ===
    /// Number of open payment channels.
    pub open_channels: u32,
    /// Total balance locked in channels (HBAR).
    pub channel_balance_hbar: f64,

    // === Hedera Status ===
    /// Whether Hedera settlement is configured.
    pub hedera_configured: bool,
    /// Hedera account ID (if configured).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hedera_account_id: Option<String>,
    /// Hedera network (testnet/mainnet).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hedera_network: Option<String>,
    /// On-chain Hedera balance in HBAR (if configured).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hedera_balance_hbar: Option<f64>,
}

// ============================================================================
// search_network Tool
// ============================================================================

/// Input for the `search_network` tool.
///
/// Searches the network for content matching a query.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct SearchNetworkInput {
    /// Search query (matches against titles, descriptions, and tags).
    pub query: String,

    /// Maximum results (default: 10, max: 50).
    #[serde(default)]
    pub limit: Option<u32>,

    /// Filter by content type (L0, L1, L2, L3).
    #[serde(default)]
    pub content_type: Option<String>,
}

/// Output from the `search_network` tool.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct SearchNetworkOutput {
    /// Matching content sources.
    pub results: Vec<SearchResultInfo>,

    /// Total results found.
    pub total: u32,

    /// Number of peers queried.
    pub peers_queried: u32,

    /// Search latency in milliseconds.
    pub latency_ms: u64,
}

/// Individual search result with source attribution.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct SearchResultInfo {
    /// Content hash (base58 encoded).
    pub hash: String,

    /// Content title.
    pub title: String,

    /// Price per query in HBAR.
    pub price_hbar: f64,

    /// Content type (L0, L1, L2, L3).
    pub content_type: String,

    /// Content owner (may be "unknown" for cached announcements).
    pub owner: String,

    /// Where result came from: "local", "cached", or "peer".
    pub source: String,

    /// libp2p peer ID of the publisher (starts with "12D3Koo").
    /// Use this for opening payment channels. None for local content.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peer_id: Option<String>,

    /// Preview of L1 mentions (extracted facts/entities).
    pub preview: Vec<String>,

    /// Primary topics extracted from content.
    pub topics: Vec<String>,
}

// ============================================================================
// Hedera Settlement Tools
// ============================================================================

/// Input for the `deposit_hbar` tool.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct DepositHbarInput {
    /// Amount to deposit in HBAR.
    pub amount_hbar: f64,
}

/// Output from the `deposit_hbar` tool.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct DepositHbarOutput {
    /// Transaction ID.
    pub transaction_id: String,
    /// Amount deposited in tinybars.
    pub amount_tinybars: u64,
    /// New balance in tinybars.
    pub new_balance_tinybars: u64,
}

/// Input for the `open_channel` tool.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct OpenChannelInput {
    /// Peer ID to open channel with.
    /// Can be either:
    /// - A libp2p peer ID (starts with "12D3Koo", from list_sources)
    /// - A Nodalync peer ID (starts with "ndl", 20 bytes base58)
    pub peer_id: String,
    /// Initial deposit in HBAR (default: 100.0, minimum: 100.0).
    /// The deposit is locked in the channel until it's closed.
    #[serde(default = "default_deposit")]
    pub deposit_hbar: f64,
}

fn default_deposit() -> f64 {
    100.0
}

/// Output from the `open_channel` tool.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct OpenChannelOutput {
    /// Channel ID (base58 encoded).
    pub channel_id: String,
    /// Transaction ID from Hedera.
    pub transaction_id: Option<String>,
    /// Initial balance in the channel (tinybars).
    pub balance_tinybars: u64,
    /// Peer ID.
    pub peer_id: String,
}

/// Input for the `close_channel` tool.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct CloseChannelInput {
    /// Peer ID of the channel to close.
    /// Must be a Nodalync peer ID (starts with "ndl", 20 bytes base58).
    pub peer_id: String,
}

/// Output from the `close_channel` tool.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct CloseChannelOutput {
    /// Whether the channel was successfully closed.
    pub success: bool,
    /// Hedera transaction ID for settlement (if on-chain settlement occurred).
    pub transaction_id: Option<String>,
    /// Final balance returned to you (tinybars).
    pub final_balance_tinybars: u64,
    /// Peer ID of the closed channel.
    pub peer_id: String,
    /// Updated Hedera balance after settlement (HBAR).
    pub hedera_balance_hbar: Option<f64>,
}

/// Output from the `reset_channels` tool.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ResetChannelsOutput {
    /// Number of channels that were cleared.
    pub channels_cleared: u32,
    /// Message describing what was reset.
    pub message: String,
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Convert a Hash to a base58 string.
pub fn hash_to_string(hash: &Hash) -> String {
    bs58::encode(&hash.0).into_string()
}

/// Parse a base58 string to a Hash.
pub fn string_to_hash(s: &str) -> Result<Hash, String> {
    let bytes = bs58::decode(s)
        .into_vec()
        .map_err(|e| format!("invalid base58: {}", e))?;

    if bytes.len() != 32 {
        return Err(format!(
            "invalid hash length: expected 32, got {}",
            bytes.len()
        ));
    }

    let mut hash = [0u8; 32];
    hash.copy_from_slice(&bytes);
    Ok(Hash(hash))
}

#[cfg(test)]
mod tests {
    use super::*;
    use nodalync_types::Hash;

    #[test]
    fn test_hash_roundtrip() {
        let original = Hash([
            1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24,
            25, 26, 27, 28, 29, 30, 31, 32,
        ]);

        let encoded = hash_to_string(&original);
        let decoded = string_to_hash(&encoded).unwrap();

        assert_eq!(original, decoded);
    }

    #[test]
    fn test_invalid_hash() {
        // Too short
        let result = string_to_hash("abc");
        assert!(result.is_err());

        // Invalid base58
        let result = string_to_hash("0OIl"); // Invalid base58 characters
        assert!(result.is_err());
    }

    #[test]
    fn test_query_input_deserialization() {
        let json = r#"{"query": "What is Nodalync?"}"#;
        let input: QueryKnowledgeInput = serde_json::from_str(json).unwrap();

        assert_eq!(input.query, "What is Nodalync?");
        assert!(input.budget_hbar.is_none());
    }

    #[test]
    fn test_query_input_with_budget() {
        let json = r#"{"query": "test", "budget_hbar": 0.5}"#;
        let input: QueryKnowledgeInput = serde_json::from_str(json).unwrap();

        assert_eq!(input.budget_hbar, Some(0.5));
    }

    #[test]
    fn test_list_input_defaults() {
        let json = r#"{}"#;
        let input: ListSourcesInput = serde_json::from_str(json).unwrap();

        assert!(input.topic.is_none());
        assert!(input.limit.is_none());
    }
}
