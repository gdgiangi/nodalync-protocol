//! MCP tool input/output types.
//!
//! Defines the request and response types for MCP tools.

use nodalync_types::Hash;
use rmcp::schemars;
use rmcp::schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// ============================================================================
// query_knowledge Tool
// ============================================================================

/// Input for the `query_knowledge` tool.
///
/// Queries content from the Nodalync network and pays automatically.
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

    /// Content hash.
    pub hash: String,

    /// Source hashes (L0 content this derives from).
    pub sources: Vec<String>,

    /// Full provenance chain (all contributing content).
    pub provenance: Vec<String>,

    /// Actual cost of this query in HBAR.
    pub cost_hbar: f64,

    /// Remaining session budget in HBAR.
    pub remaining_budget_hbar: f64,
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
// health_status Tool
// ============================================================================

/// Output from the `health_status` tool.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct HealthStatusOutput {
    /// Number of connected peers.
    pub connected_peers: u32,

    /// Whether the node has bootstrapped to the network.
    pub is_bootstrapped: bool,

    /// Remaining budget for this session in HBAR.
    pub budget_remaining_hbar: f64,

    /// Total session budget in HBAR.
    pub budget_total_hbar: f64,

    /// Amount spent in this session in HBAR.
    pub budget_spent_hbar: f64,

    /// Total content items available locally.
    pub local_content_count: u32,

    /// Node peer ID.
    pub peer_id: String,
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

    /// Preview of L1 mentions (extracted facts/entities).
    pub preview: Vec<String>,

    /// Primary topics extracted from content.
    pub topics: Vec<String>,
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
