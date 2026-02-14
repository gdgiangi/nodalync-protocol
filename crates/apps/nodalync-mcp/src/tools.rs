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
///
/// For x402 payment flow:
/// 1. Call without `x402_payment` — if content requires payment and x402 is enabled,
///    returns a 402 response with payment requirements instead of content.
/// 2. Call again with `x402_payment` containing the base64-encoded payment header —
///    payment is verified via the facilitator and content is delivered.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct QueryKnowledgeInput {
    /// The query string (natural language or content hash).
    pub query: String,

    /// Maximum budget for this query in HBAR.
    /// If not specified, uses auto-approve threshold.
    #[serde(default)]
    pub budget_hbar: Option<f64>,

    /// x402 payment header (base64-encoded).
    /// When provided, the server validates the payment via the Blocky402 facilitator
    /// and delivers content on successful settlement.
    /// When omitted and content has a price, the server returns payment requirements.
    #[serde(default)]
    pub x402_payment: Option<String>,
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
    /// This is the on-chain account balance, not the settlement contract deposit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hedera_account_balance_hbar: Option<f64>,
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

/// Information about a single payment channel.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ChannelInfo {
    /// Channel ID (base58 encoded).
    pub channel_id: String,
    /// Nodalync peer ID of the channel peer (starts with "ndl").
    pub peer_id: String,
    /// libp2p peer ID of the channel peer (starts with "12D3Koo"), if known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub libp2p_peer_id: Option<String>,
    /// Current channel state (Opening, Open, Closing, Closed, Disputed).
    pub state: String,
    /// Our balance in the channel (HBAR).
    pub my_balance_hbar: f64,
    /// Their balance in the channel (HBAR).
    pub their_balance_hbar: f64,
    /// Number of pending payments not yet settled.
    pub pending_payments: u32,
    /// Last update timestamp (Unix milliseconds).
    pub last_update: u64,
}

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
    /// Detailed information about each open channel.
    pub channels: Vec<ChannelInfo>,

    // === Hedera Status ===
    /// Whether Hedera settlement is configured.
    pub hedera_configured: bool,
    /// Hedera account ID (if configured).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hedera_account_id: Option<String>,
    /// Hedera network (testnet/mainnet).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hedera_network: Option<String>,
    /// Hedera account balance in HBAR - total HBAR owned by this account on-chain.
    /// This is the spendable balance for gas fees, deposits, and transfers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hedera_account_balance_hbar: Option<f64>,
    /// Settlement contract balance in HBAR - amount deposited into the smart contract.
    /// This funds payment channels and is separate from the account balance.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hedera_contract_balance_hbar: Option<f64>,
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
    /// Can be either:
    /// - A libp2p peer ID (starts with "12D3Koo")
    /// - A Nodalync peer ID (starts with "ndl", 20 bytes base58)
    pub peer_id: String,
}

/// Output from the `close_channel` tool.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct CloseChannelOutput {
    /// Whether the channel was successfully closed.
    pub success: bool,
    /// How the channel was closed: "cooperative", "dispute_initiated", or "off_chain".
    pub close_method: String,
    /// Hedera transaction ID for settlement (if on-chain settlement occurred).
    pub transaction_id: Option<String>,
    /// Final balance returned to you (tinybars).
    pub final_balance_tinybars: u64,
    /// Peer ID of the closed channel.
    pub peer_id: String,
    /// Updated Hedera account balance after settlement (HBAR).
    /// This is the on-chain account balance after funds are returned.
    pub hedera_account_balance_hbar: Option<f64>,
}

/// Result for a single channel close attempt in close_all_channels.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ChannelCloseResult {
    /// Peer ID of the channel.
    pub peer_id: String,
    /// Whether the close succeeded.
    pub success: bool,
    /// How the channel was closed: "cooperative", "dispute_initiated", or "failed".
    pub close_method: String,
    /// Transaction ID if on-chain operation occurred.
    pub transaction_id: Option<String>,
    /// Error message if close failed.
    pub error: Option<String>,
}

/// Output from the `close_all_channels` tool.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct CloseAllChannelsOutput {
    /// Number of channels that were processed.
    pub channels_processed: u32,
    /// Number of channels successfully closed.
    pub channels_closed: u32,
    /// Number of channels where disputes were initiated.
    pub disputes_initiated: u32,
    /// Number of channels that failed to close.
    pub channels_failed: u32,
    /// Details for each channel close attempt.
    pub results: Vec<ChannelCloseResult>,
    /// Updated Hedera account balance after all operations (HBAR).
    pub hedera_account_balance_hbar: Option<f64>,
}

// ============================================================================
// publish_content Tool
// ============================================================================

/// Default visibility value for serde deserialization.
fn default_visibility() -> String {
    "shared".to_string()
}

/// Input for the `publish_content` tool.
///
/// Publishes new content to the Nodalync network.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct PublishContentInput {
    /// The content to publish (text).
    pub content: String,

    /// Content title.
    pub title: String,

    /// Optional content description.
    #[serde(default)]
    pub description: Option<String>,

    /// Price per query in HBAR (default: 0, free).
    #[serde(default)]
    pub price_hbar: Option<f64>,

    /// Visibility: "private", "unlisted", or "shared" (default: "shared").
    #[serde(default = "default_visibility")]
    pub visibility: String,

    /// Optional MIME type (default: "text/plain").
    #[serde(default)]
    pub mime_type: Option<String>,

    /// Optional tags for content discovery.
    #[serde(default)]
    pub tags: Option<Vec<String>>,
}

/// Output from the `publish_content` tool.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct PublishContentOutput {
    /// Content hash (base58 encoded).
    pub hash: String,
    /// Content title.
    pub title: String,
    /// Content type (always "L0" for published content).
    pub content_type: String,
    /// Visibility level.
    pub visibility: String,
    /// Price per query in HBAR.
    pub price_hbar: f64,
    /// Content size in bytes.
    pub size_bytes: usize,
    /// Number of L1 mentions extracted.
    pub mentions_extracted: u32,
    /// Primary topics extracted from content.
    pub topics: Vec<String>,
}

// ============================================================================
// preview_content Tool
// ============================================================================

/// Input for the `preview_content` tool.
///
/// Previews content metadata without paying for the full content.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct PreviewContentInput {
    /// Content hash (base58 encoded).
    pub hash: String,
}

/// Output from the `preview_content` tool.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct PreviewContentOutput {
    /// Content hash (base58 encoded).
    pub hash: String,
    /// Content title.
    pub title: String,
    /// Content owner (peer ID string).
    pub owner: String,
    /// Price per query in HBAR.
    pub price_hbar: f64,
    /// Content type (L0, L1, L2, L3).
    pub content_type: String,
    /// Visibility level.
    pub visibility: String,
    /// Content size in bytes.
    pub size_bytes: u64,
    /// Number of L1 mentions.
    pub mention_count: u32,
    /// Preview of L1 mentions.
    pub preview_mentions: Vec<String>,
    /// Primary topics.
    pub topics: Vec<String>,
    /// Summary text.
    pub summary: String,
    /// Provider peer ID (libp2p), if known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_peer_id: Option<String>,
}

// ============================================================================
// synthesize_content Tool
// ============================================================================

/// Input for the `synthesize_content` tool.
///
/// Creates L3 synthesized content from multiple sources.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct SynthesizeContentInput {
    /// The synthesized content (text).
    pub content: String,

    /// Content title.
    pub title: String,

    /// Source content hashes (base58 encoded). At least one required.
    pub sources: Vec<String>,

    /// Optional description.
    #[serde(default)]
    pub description: Option<String>,

    /// Whether to publish immediately (default: false).
    #[serde(default)]
    pub publish: Option<bool>,

    /// Price per query in HBAR (used if publish is true).
    #[serde(default)]
    pub price_hbar: Option<f64>,

    /// Visibility (used if publish is true): "private", "unlisted", or "shared" (default: "shared").
    #[serde(default = "default_visibility")]
    pub visibility: String,
}

/// Output from the `synthesize_content` tool.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct SynthesizeContentOutput {
    /// Content hash (base58 encoded).
    pub hash: String,
    /// Content title.
    pub title: String,
    /// Content type (always "L3" for synthesized content).
    pub content_type: String,
    /// Source content hashes used.
    pub sources: Vec<String>,
    /// Provenance depth.
    pub provenance_depth: u32,
    /// Number of root (L0/L1) sources in the provenance chain.
    pub root_source_count: usize,
    /// Whether the content was published.
    pub published: bool,
    /// Visibility (if published).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visibility: Option<String>,
    /// Price in HBAR (if published).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price_hbar: Option<f64>,
}

// ============================================================================
// update_content Tool
// ============================================================================

/// Input for the `update_content` tool.
///
/// Creates a new version of existing content.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct UpdateContentInput {
    /// Hash of the previous version (base58 encoded).
    pub previous_hash: String,

    /// New content (text).
    pub content: String,

    /// Optional new title (inherits from previous if not set).
    #[serde(default)]
    pub title: Option<String>,

    /// Optional new description (inherits from previous if not set).
    #[serde(default)]
    pub description: Option<String>,
}

/// Output from the `update_content` tool.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct UpdateContentOutput {
    /// New content hash (base58 encoded).
    pub hash: String,
    /// Previous version hash.
    pub previous_hash: String,
    /// Version number.
    pub version_number: u32,
    /// Content title.
    pub title: String,
    /// Content size in bytes.
    pub size_bytes: usize,
    /// Visibility level.
    pub visibility: String,
}

// ============================================================================
// delete_content Tool
// ============================================================================

/// Input for the `delete_content` tool.
///
/// Deletes content and sets visibility to Offline.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct DeleteContentInput {
    /// Content hash (base58 encoded).
    pub hash: String,
}

/// Output from the `delete_content` tool.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct DeleteContentOutput {
    /// Content hash that was deleted.
    pub hash: String,
    /// Title of deleted content.
    pub title: String,
    /// Whether content bytes were removed.
    pub content_removed: bool,
    /// New visibility (always "Offline").
    pub visibility: String,
}

// ============================================================================
// set_visibility Tool
// ============================================================================

/// Input for the `set_visibility` tool.
///
/// Changes the visibility of content.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct SetVisibilityInput {
    /// Content hash (base58 encoded).
    pub hash: String,

    /// New visibility: "private", "unlisted", or "shared".
    pub visibility: String,
}

/// Output from the `set_visibility` tool.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct SetVisibilityOutput {
    /// Content hash.
    pub hash: String,
    /// New visibility.
    pub visibility: String,
    /// Previous visibility.
    pub previous_visibility: String,
}

// ============================================================================
// list_versions Tool
// ============================================================================

/// Input for the `list_versions` tool.
///
/// Lists all versions of a content item.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ListVersionsInput {
    /// Content hash (base58 encoded). Can be any version's hash.
    pub hash: String,
}

/// A single version entry.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct VersionEntry {
    /// Version content hash (base58 encoded).
    pub hash: String,
    /// Version number.
    pub version_number: u32,
    /// Creation timestamp (Unix milliseconds).
    pub timestamp: u64,
    /// Visibility level.
    pub visibility: String,
    /// Price per query in HBAR.
    pub price_hbar: f64,
}

/// Output from the `list_versions` tool.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ListVersionsOutput {
    /// Root hash (stable identifier across all versions).
    pub root_hash: String,
    /// All versions.
    pub versions: Vec<VersionEntry>,
    /// Total number of versions.
    pub total_versions: u32,
}

// ============================================================================
// get_earnings Tool
// ============================================================================

/// Input for the `get_earnings` tool.
///
/// Gets earnings information for published content.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct GetEarningsInput {
    /// Maximum number of content items to return (default: 20).
    #[serde(default)]
    pub limit: Option<u32>,

    /// Filter by content type (L0, L1, L2, L3).
    #[serde(default)]
    pub content_type: Option<String>,
}

/// Earnings for a single content item.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ContentEarnings {
    /// Content hash (base58 encoded).
    pub hash: String,
    /// Content title.
    pub title: String,
    /// Content type (L0, L1, L2, L3).
    pub content_type: String,
    /// Total queries served.
    pub total_queries: u64,
    /// Total revenue in HBAR.
    pub total_revenue_hbar: f64,
    /// Price per query in HBAR.
    pub price_hbar: f64,
    /// Visibility level.
    pub visibility: String,
}

/// Output from the `get_earnings` tool.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct GetEarningsOutput {
    /// Earnings per content item.
    pub items: Vec<ContentEarnings>,
    /// Total revenue across all content in HBAR.
    pub total_revenue_hbar: f64,
    /// Total queries across all content.
    pub total_queries: u64,
    /// Number of content items with earnings.
    pub content_count: u32,
}

// ============================================================================
// x402 Payment Protocol Types
// ============================================================================

/// Output when content requires x402 payment.
///
/// Returned instead of content when:
/// - Content has a price > 0
/// - x402 is enabled on the server
/// - No `x402_payment` header was provided in the request
///
/// The client should construct a payment using the `accepts` requirements,
/// then retry with the `x402_payment` field populated.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct X402PaymentRequiredOutput {
    /// Always "payment_required" — signals the client to pay.
    pub status: String,

    /// x402 protocol version.
    pub x402_version: u32,

    /// Content hash (base58 encoded) — what you're paying for.
    pub content_hash: String,

    /// Content title.
    pub title: String,

    /// Content description / preview.
    pub description: String,

    /// Base content price in HBAR (before app fee).
    pub price_hbar: f64,

    /// Total payment required in HBAR (price + app fee).
    pub total_required_hbar: f64,

    /// Application fee percentage applied on top of content price.
    pub app_fee_percent: u8,

    /// Payment requirements the client must satisfy.
    /// Typically one entry for HBAR on Hedera testnet/mainnet.
    pub accepts: Vec<X402PaymentRequirement>,

    /// Hint: retry `query_knowledge` with this hash and the `x402_payment` field.
    pub instruction: String,
}

/// A single accepted payment method for x402.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct X402PaymentRequirement {
    /// Payment scheme (e.g., "exact").
    pub scheme: String,

    /// Network identifier (CAIP-2 format, e.g., "hedera:testnet").
    pub network: String,

    /// Required payment amount in smallest unit (tinybars for HBAR).
    pub amount: String,

    /// Asset identifier ("HBAR" for native Hedera).
    pub asset: String,

    /// Address to pay to (Hedera account ID).
    pub pay_to: String,

    /// Maximum time in seconds the payment is valid after creation.
    pub max_timeout_seconds: u64,
}

/// Output from the `x402_status` tool.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct X402StatusOutput {
    /// Whether x402 payments are enabled on this node.
    pub enabled: bool,

    /// Settlement network (e.g., "hedera:testnet").
    pub network: String,

    /// Facilitator URL (Blocky402 endpoint).
    pub facilitator_url: String,

    /// Receiving Hedera account ID.
    pub account_id: String,

    /// Application fee percentage.
    pub app_fee_percent: u8,

    /// Total x402 transactions processed.
    pub total_transactions: usize,

    /// Total payment volume in tinybars.
    pub total_volume: u64,

    /// Total application fees collected in tinybars.
    pub total_app_fees: u64,

    /// Total volume in HBAR.
    pub total_volume_hbar: f64,

    /// Total app fees in HBAR.
    pub total_app_fees_hbar: f64,
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

    #[test]
    fn test_deposit_hbar_input_deserialization() {
        let json = r#"{"amount_hbar": 5.0}"#;
        let input: DepositHbarInput = serde_json::from_str(json).unwrap();
        assert!((input.amount_hbar - 5.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_open_channel_input_deserialization() {
        let json = r#"{"peer_id": "12D3KooWTest"}"#;
        let input: OpenChannelInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.peer_id, "12D3KooWTest");
        assert!((input.deposit_hbar - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_open_channel_input_with_custom_deposit() {
        let json = r#"{"peer_id": "12D3KooWTest", "deposit_hbar": 250.0}"#;
        let input: OpenChannelInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.peer_id, "12D3KooWTest");
        assert!((input.deposit_hbar - 250.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_close_channel_input_deserialization() {
        let json = r#"{"peer_id": "12D3KooWTest"}"#;
        let input: CloseChannelInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.peer_id, "12D3KooWTest");
    }

    #[test]
    fn test_status_output_serialization() {
        let status = StatusOutput {
            connected_peers: 3,
            is_bootstrapped: true,
            peer_id: "ndl1TestPeerId".to_string(),
            local_content_count: 42,
            budget_remaining_hbar: 0.75,
            budget_total_hbar: 1.0,
            budget_spent_hbar: 0.25,
            open_channels: 2,
            channel_balance_hbar: 200.0,
            channels: vec![ChannelInfo {
                channel_id: "ch_abc123".to_string(),
                peer_id: "ndl1Peer1".to_string(),
                libp2p_peer_id: Some("12D3KooWPeer1".to_string()),
                state: "Open".to_string(),
                my_balance_hbar: 90.0,
                their_balance_hbar: 10.0,
                pending_payments: 1,
                last_update: 1700000000000,
            }],
            hedera_configured: true,
            hedera_account_id: Some("0.0.7703962".to_string()),
            hedera_network: Some("testnet".to_string()),
            hedera_account_balance_hbar: Some(500.0),
            hedera_contract_balance_hbar: Some(200.0),
        };

        let json_str = serde_json::to_string(&status).unwrap();
        let json: serde_json::Value = serde_json::from_str(&json_str).unwrap();

        assert_eq!(json["connected_peers"], 3);
        assert_eq!(json["is_bootstrapped"], true);
        assert_eq!(json["peer_id"], "ndl1TestPeerId");
        assert_eq!(json["local_content_count"], 42);
        assert_eq!(json["open_channels"], 2);
        assert_eq!(json["hedera_configured"], true);
        assert_eq!(json["hedera_account_id"], "0.0.7703962");
        assert_eq!(json["hedera_network"], "testnet");
    }

    #[test]
    fn test_payment_details_serialization() {
        let payment = PaymentDetails {
            channel_opened: true,
            channel_id: Some("ch_test123".to_string()),
            channel_tx_id: Some("0.0.7703962@1700000000.000".to_string()),
            deposit_tx_id: Some("0.0.7703962@1700000001.000".to_string()),
            deposit_amount_hbar: Some(100.0),
            provider_peer_id: Some("12D3KooWProvider".to_string()),
            payment_receipt_id: Some("receipt_001".to_string()),
            hedera_account_balance_hbar: Some(400.0),
        };

        let json_str = serde_json::to_string(&payment).unwrap();
        let json: serde_json::Value = serde_json::from_str(&json_str).unwrap();

        assert_eq!(json["channel_opened"], true);
        assert_eq!(json["channel_id"], "ch_test123");
        assert_eq!(json["deposit_amount_hbar"], 100.0);
        assert_eq!(json["provider_peer_id"], "12D3KooWProvider");
    }

    #[test]
    fn test_source_info_serialization() {
        let source = SourceInfo {
            hash: "QmTestHash123456789abcdef".to_string(),
            title: "Nodalync Protocol Overview".to_string(),
            price_hbar: 0.01,
            preview: "A protocol for fair knowledge economics".to_string(),
            topics: vec!["protocol".to_string(), "knowledge".to_string()],
            peer_id: Some("12D3KooWSourcePeer".to_string()),
        };

        let json_str = serde_json::to_string(&source).unwrap();
        let json: serde_json::Value = serde_json::from_str(&json_str).unwrap();

        assert_eq!(json["hash"], "QmTestHash123456789abcdef");
        assert_eq!(json["title"], "Nodalync Protocol Overview");
        assert_eq!(json["price_hbar"], 0.01);
        assert_eq!(json["topics"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn test_channel_info_serialization() {
        let channel = ChannelInfo {
            channel_id: "ch_channel456".to_string(),
            peer_id: "ndl1ChannelPeer".to_string(),
            libp2p_peer_id: Some("12D3KooWChannelPeer".to_string()),
            state: "Open".to_string(),
            my_balance_hbar: 85.5,
            their_balance_hbar: 14.5,
            pending_payments: 3,
            last_update: 1700000500000,
        };

        let json_str = serde_json::to_string(&channel).unwrap();
        let json: serde_json::Value = serde_json::from_str(&json_str).unwrap();

        assert_eq!(json["channel_id"], "ch_channel456");
        assert_eq!(json["peer_id"], "ndl1ChannelPeer");
        assert_eq!(json["libp2p_peer_id"], "12D3KooWChannelPeer");
        assert_eq!(json["state"], "Open");
        assert_eq!(json["my_balance_hbar"], 85.5);
        assert_eq!(json["their_balance_hbar"], 14.5);
        assert_eq!(json["pending_payments"], 3);
        assert_eq!(json["last_update"], 1700000500000u64);
    }

    #[test]
    fn test_close_all_channels_output_serialization() {
        let output = CloseAllChannelsOutput {
            channels_processed: 5,
            channels_closed: 3,
            disputes_initiated: 1,
            channels_failed: 1,
            results: vec![
                ChannelCloseResult {
                    peer_id: "ndl1Peer1".to_string(),
                    success: true,
                    close_method: "cooperative".to_string(),
                    transaction_id: Some("0.0.7703962@1700000010.000".to_string()),
                    error: None,
                },
                ChannelCloseResult {
                    peer_id: "ndl1Peer2".to_string(),
                    success: false,
                    close_method: "failed".to_string(),
                    transaction_id: None,
                    error: Some("peer unreachable".to_string()),
                },
            ],
            hedera_account_balance_hbar: Some(550.0),
        };

        let json_str = serde_json::to_string(&output).unwrap();
        let json: serde_json::Value = serde_json::from_str(&json_str).unwrap();

        assert_eq!(json["channels_processed"], 5);
        assert_eq!(json["channels_closed"], 3);
        assert_eq!(json["disputes_initiated"], 1);
        assert_eq!(json["channels_failed"], 1);
        assert_eq!(json["results"].as_array().unwrap().len(), 2);
        assert_eq!(json["hedera_account_balance_hbar"], 550.0);
    }

    #[test]
    fn test_search_result_info_serialization() {
        let result = SearchResultInfo {
            hash: "QmSearchResult789".to_string(),
            title: "AI Agent Economics".to_string(),
            price_hbar: 0.05,
            content_type: "L1".to_string(),
            owner: "ndl1Owner123".to_string(),
            source: "peer".to_string(),
            peer_id: Some("12D3KooWSearchPeer".to_string()),
            preview: vec!["AI agents pay for knowledge".to_string()],
            topics: vec!["economics".to_string(), "ai".to_string()],
        };

        let json_str = serde_json::to_string(&result).unwrap();
        let json: serde_json::Value = serde_json::from_str(&json_str).unwrap();

        assert_eq!(json["hash"], "QmSearchResult789");
        assert_eq!(json["title"], "AI Agent Economics");
        assert_eq!(json["price_hbar"], 0.05);
        assert_eq!(json["content_type"], "L1");
        assert_eq!(json["owner"], "ndl1Owner123");
        assert_eq!(json["source"], "peer");
        assert_eq!(json["peer_id"], "12D3KooWSearchPeer");
        assert_eq!(json["preview"].as_array().unwrap().len(), 1);
        assert_eq!(json["topics"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn test_search_network_input_defaults() {
        let json = r#"{"query": "test"}"#;
        let input: SearchNetworkInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.query, "test");
        assert!(input.limit.is_none());
        assert!(input.content_type.is_none());
    }

    #[test]
    fn test_search_network_input_with_filters() {
        let json = r#"{"query": "protocol", "limit": 25, "content_type": "L2"}"#;
        let input: SearchNetworkInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.query, "protocol");
        assert_eq!(input.limit, Some(25));
        assert_eq!(input.content_type, Some("L2".to_string()));
    }

    // ====================================================================
    // Content Production Tool Tests
    // ====================================================================

    #[test]
    fn test_publish_content_input_deserialization() {
        let json = r#"{"content": "Hello world", "title": "Test"}"#;
        let input: PublishContentInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.content, "Hello world");
        assert_eq!(input.title, "Test");
        assert_eq!(input.visibility, "shared");
        assert!(input.description.is_none());
        assert!(input.price_hbar.is_none());
        assert!(input.tags.is_none());
    }

    #[test]
    fn test_publish_content_output_serialization() {
        let output = PublishContentOutput {
            hash: "QmTestHash".to_string(),
            title: "Test Content".to_string(),
            content_type: "L0".to_string(),
            visibility: "Shared".to_string(),
            price_hbar: 0.01,
            size_bytes: 1024,
            mentions_extracted: 5,
            topics: vec!["test".to_string()],
        };
        let json_str = serde_json::to_string(&output).unwrap();
        let json: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(json["hash"], "QmTestHash");
        assert_eq!(json["content_type"], "L0");
        assert_eq!(json["size_bytes"], 1024);
        assert_eq!(json["mentions_extracted"], 5);
    }

    #[test]
    fn test_preview_content_input_deserialization() {
        let json = r#"{"hash": "QmTestHash123"}"#;
        let input: PreviewContentInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.hash, "QmTestHash123");
    }

    #[test]
    fn test_preview_content_output_serialization() {
        let output = PreviewContentOutput {
            hash: "QmTestHash".to_string(),
            title: "Test".to_string(),
            owner: "ndl1Owner".to_string(),
            price_hbar: 0.05,
            content_type: "L0".to_string(),
            visibility: "Shared".to_string(),
            size_bytes: 2048,
            mention_count: 3,
            preview_mentions: vec!["mention1".to_string()],
            topics: vec!["topic1".to_string()],
            summary: "A test summary".to_string(),
            provider_peer_id: Some("12D3KooWProvider".to_string()),
        };
        let json_str = serde_json::to_string(&output).unwrap();
        let json: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(json["hash"], "QmTestHash");
        assert_eq!(json["mention_count"], 3);
        assert_eq!(json["provider_peer_id"], "12D3KooWProvider");
    }

    #[test]
    fn test_synthesize_content_input_deserialization() {
        let json =
            r#"{"content": "Synthesis", "title": "L3 Content", "sources": ["hash1", "hash2"]}"#;
        let input: SynthesizeContentInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.content, "Synthesis");
        assert_eq!(input.sources.len(), 2);
        assert_eq!(input.visibility, "shared");
        assert!(input.publish.is_none());
    }

    #[test]
    fn test_synthesize_content_output_serialization() {
        let output = SynthesizeContentOutput {
            hash: "QmSynthHash".to_string(),
            title: "Synthesis".to_string(),
            content_type: "L3".to_string(),
            sources: vec!["hash1".to_string(), "hash2".to_string()],
            provenance_depth: 2,
            root_source_count: 3,
            published: true,
            visibility: Some("Shared".to_string()),
            price_hbar: Some(0.02),
        };
        let json_str = serde_json::to_string(&output).unwrap();
        let json: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(json["content_type"], "L3");
        assert_eq!(json["provenance_depth"], 2);
        assert_eq!(json["root_source_count"], 3);
        assert_eq!(json["published"], true);
    }

    #[test]
    fn test_update_content_input_deserialization() {
        let json = r#"{"previous_hash": "QmOldHash", "content": "Updated text"}"#;
        let input: UpdateContentInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.previous_hash, "QmOldHash");
        assert_eq!(input.content, "Updated text");
        assert!(input.title.is_none());
        assert!(input.description.is_none());
    }

    #[test]
    fn test_update_content_output_serialization() {
        let output = UpdateContentOutput {
            hash: "QmNewHash".to_string(),
            previous_hash: "QmOldHash".to_string(),
            version_number: 2,
            title: "Updated Title".to_string(),
            size_bytes: 512,
            visibility: "Shared".to_string(),
        };
        let json_str = serde_json::to_string(&output).unwrap();
        let json: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(json["hash"], "QmNewHash");
        assert_eq!(json["previous_hash"], "QmOldHash");
        assert_eq!(json["version_number"], 2);
    }

    #[test]
    fn test_delete_content_input_deserialization() {
        let json = r#"{"hash": "QmDeleteMe"}"#;
        let input: DeleteContentInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.hash, "QmDeleteMe");
    }

    #[test]
    fn test_delete_content_output_serialization() {
        let output = DeleteContentOutput {
            hash: "QmDeleted".to_string(),
            title: "Deleted Content".to_string(),
            content_removed: true,
            visibility: "Offline".to_string(),
        };
        let json_str = serde_json::to_string(&output).unwrap();
        let json: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(json["content_removed"], true);
        assert_eq!(json["visibility"], "Offline");
    }

    #[test]
    fn test_set_visibility_input_deserialization() {
        let json = r#"{"hash": "QmTestHash", "visibility": "private"}"#;
        let input: SetVisibilityInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.hash, "QmTestHash");
        assert_eq!(input.visibility, "private");
    }

    #[test]
    fn test_set_visibility_output_serialization() {
        let output = SetVisibilityOutput {
            hash: "QmTestHash".to_string(),
            visibility: "Private".to_string(),
            previous_visibility: "Shared".to_string(),
        };
        let json_str = serde_json::to_string(&output).unwrap();
        let json: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(json["visibility"], "Private");
        assert_eq!(json["previous_visibility"], "Shared");
    }

    #[test]
    fn test_list_versions_input_deserialization() {
        let json = r#"{"hash": "QmVersionRoot"}"#;
        let input: ListVersionsInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.hash, "QmVersionRoot");
    }

    #[test]
    fn test_list_versions_output_serialization() {
        let output = ListVersionsOutput {
            root_hash: "QmRoot".to_string(),
            versions: vec![VersionEntry {
                hash: "QmV1".to_string(),
                version_number: 1,
                timestamp: 1700000000000,
                visibility: "Shared".to_string(),
                price_hbar: 0.01,
            }],
            total_versions: 1,
        };
        let json_str = serde_json::to_string(&output).unwrap();
        let json: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(json["root_hash"], "QmRoot");
        assert_eq!(json["total_versions"], 1);
        assert_eq!(json["versions"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn test_get_earnings_input_deserialization() {
        let json = r#"{}"#;
        let input: GetEarningsInput = serde_json::from_str(json).unwrap();
        assert!(input.limit.is_none());
        assert!(input.content_type.is_none());
    }

    #[test]
    fn test_get_earnings_output_serialization() {
        let output = GetEarningsOutput {
            items: vec![ContentEarnings {
                hash: "QmEarnings".to_string(),
                title: "Earning Content".to_string(),
                content_type: "L0".to_string(),
                total_queries: 100,
                total_revenue_hbar: 1.0,
                price_hbar: 0.01,
                visibility: "Shared".to_string(),
            }],
            total_revenue_hbar: 1.0,
            total_queries: 100,
            content_count: 1,
        };
        let json_str = serde_json::to_string(&output).unwrap();
        let json: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(json["total_revenue_hbar"], 1.0);
        assert_eq!(json["total_queries"], 100);
        assert_eq!(json["content_count"], 1);
        assert_eq!(json["items"].as_array().unwrap().len(), 1);
    }
}
