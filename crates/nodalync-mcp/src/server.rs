//! MCP server implementation for Nodalync.
//!
//! Uses the RMCP SDK to expose Nodalync knowledge querying to AI assistants.

use std::sync::Arc;

use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::*,
    service::{RequestContext, RoleServer},
    tool, tool_handler, tool_router, ErrorData as McpError,
};
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

use nodalync_ops::DefaultNodeOperations;
use nodalync_store::{ManifestFilter, ManifestStore, NodeState, NodeStateConfig};
use nodalync_types::Visibility;

use crate::budget::{hbar_to_tinybars, tinybars_to_hbar, BudgetTracker};
use crate::tools::{
    hash_to_string, string_to_hash, ListSourcesInput, ListSourcesOutput, QueryKnowledgeInput,
    QueryKnowledgeOutput, SourceInfo,
};

/// Configuration for the MCP server.
#[derive(Debug, Clone)]
pub struct McpServerConfig {
    /// Session budget in HBAR.
    pub budget_hbar: f64,
    /// Auto-approve threshold in HBAR.
    pub auto_approve_hbar: f64,
    /// Data directory for node state.
    pub data_dir: std::path::PathBuf,
}

impl Default for McpServerConfig {
    fn default() -> Self {
        Self {
            budget_hbar: 1.0,
            auto_approve_hbar: 0.01,
            data_dir: directories::ProjectDirs::from("", "", "nodalync")
                .map(|d| d.data_dir().to_path_buf())
                .unwrap_or_else(|| std::path::PathBuf::from("~/.nodalync")),
        }
    }
}

/// Nodalync MCP Server.
///
/// Implements the MCP server handler with `query_knowledge` and `list_sources` tools.
#[derive(Clone)]
pub struct NodalyncMcpServer {
    /// Node operations instance.
    ops: Arc<Mutex<DefaultNodeOperations>>,
    /// Budget tracker.
    budget: Arc<BudgetTracker>,
    /// Tool router for MCP.
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl NodalyncMcpServer {
    /// Create a new MCP server with the given configuration.
    pub fn new(config: McpServerConfig) -> Result<Self, Box<dyn std::error::Error>> {
        // Initialize node state
        let state_config = NodeStateConfig::new(&config.data_dir);
        let state = NodeState::open(state_config)?;

        // Generate or load identity
        let (_, public_key) = nodalync_crypto::generate_identity();
        let peer_id = nodalync_crypto::peer_id_from_public_key(&public_key);

        // Create operations instance
        let ops = DefaultNodeOperations::with_defaults(state, peer_id);

        // Create budget tracker
        let budget = BudgetTracker::with_auto_approve(config.budget_hbar, config.auto_approve_hbar);

        info!(
            budget_hbar = config.budget_hbar,
            auto_approve_hbar = config.auto_approve_hbar,
            "MCP server initialized"
        );

        Ok(Self {
            ops: Arc::new(Mutex::new(ops)),
            budget: Arc::new(budget),
            tool_router: Self::tool_router(),
        })
    }

    /// Create a server with default configuration.
    pub fn with_defaults() -> Result<Self, Box<dyn std::error::Error>> {
        Self::new(McpServerConfig::default())
    }

    /// Query knowledge from the Nodalync network.
    ///
    /// Retrieves content matching the query, pays the content owner,
    /// and returns the content with provenance information.
    #[tool(
        description = "Query knowledge from the Nodalync network. Returns content with provenance and automatically handles payment. Query must be a base58-encoded content hash (use list_sources to discover hashes)."
    )]
    async fn query_knowledge(
        &self,
        Parameters(input): Parameters<QueryKnowledgeInput>,
    ) -> Result<CallToolResult, McpError> {
        debug!(query = %input.query, "Processing query_knowledge request");

        // Parse query as hash (natural language search not yet supported)
        let hash = match string_to_hash(&input.query) {
            Ok(h) => h,
            Err(_) => {
                return Ok(CallToolResult::error(vec![Content::text(
                    "Query must be a base58-encoded content hash. Use list_sources to discover available content hashes.",
                )]));
            }
        };

        // Get preview to check price
        let mut ops = self.ops.lock().await;
        let preview = match ops.preview_content(&hash).await {
            Ok(p) => p,
            Err(e) => {
                warn!(hash = %hash_to_string(&hash), error = %e, "Content not found");
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Content not found: {}",
                    e
                ))]));
            }
        };

        let price = preview.manifest.economics.price;
        let price_hbar = tinybars_to_hbar(price);

        // Check per-query budget limit
        let max_budget = input
            .budget_hbar
            .map(hbar_to_tinybars)
            .unwrap_or(self.budget.remaining());
        if price > max_budget {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "Content costs {:.6} HBAR but query budget is {:.6} HBAR",
                price_hbar,
                tinybars_to_hbar(max_budget)
            ))]));
        }

        // Check if auto-approve or needs confirmation
        let auto_approved = self.budget.can_auto_approve(price);
        if !auto_approved && !self.budget.can_afford(price) {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "Insufficient budget: content costs {:.6} HBAR but only {:.6} HBAR remaining",
                price_hbar,
                self.budget.remaining_hbar()
            ))]));
        }

        // Reserve budget BEFORE executing query (atomic spend returns None if insufficient)
        if price > 0 && self.budget.spend(price).is_none() {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "Budget reservation failed: content costs {:.6} HBAR but only {:.6} HBAR remaining",
                price_hbar,
                self.budget.remaining_hbar()
            ))]));
        }

        // Execute query (budget already reserved)
        let response = match ops.query_content(&hash, price, None).await {
            Ok(r) => r,
            Err(e) => {
                // Refund budget on failure (best effort - log if this somehow fails)
                if price > 0 {
                    self.budget.refund(price);
                }
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Query failed: {}",
                    e
                ))]));
            }
        };

        // Build output
        let content_str = String::from_utf8_lossy(&response.content).to_string();

        // Sources are the root L0/L1 content this derives from
        let sources: Vec<String> = response
            .manifest
            .provenance
            .root_l0l1
            .iter()
            .map(|e| hash_to_string(&e.hash))
            .collect();

        // Provenance includes both root sources and direct parents
        let mut provenance: Vec<String> = sources.clone();
        for h in &response.manifest.provenance.derived_from {
            let hash_str = hash_to_string(h);
            if !provenance.contains(&hash_str) {
                provenance.push(hash_str);
            }
        }

        let output = QueryKnowledgeOutput {
            content: content_str,
            hash: hash_to_string(&response.manifest.hash),
            sources,
            provenance,
            cost_hbar: price_hbar,
            remaining_budget_hbar: self.budget.remaining_hbar(),
        };

        info!(
            hash = %hash_to_string(&hash),
            cost_hbar = price_hbar,
            remaining_hbar = self.budget.remaining_hbar(),
            "Query completed successfully"
        );

        let json = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    /// List available knowledge sources.
    ///
    /// Returns a list of content items that can be queried, optionally
    /// filtered by topic.
    #[tool(
        description = "List available knowledge sources in the Nodalync network. Returns content metadata including titles, prices, and topics. Use this to discover what knowledge is available before querying."
    )]
    async fn list_sources(
        &self,
        Parameters(input): Parameters<ListSourcesInput>,
    ) -> Result<CallToolResult, McpError> {
        debug!(topic = ?input.topic, limit = ?input.limit, "Processing list_sources request");

        let limit = input.limit.unwrap_or(10).min(50);
        let ops = self.ops.lock().await;

        // Build filter for shared content
        let filter = ManifestFilter::new()
            .with_visibility(Visibility::Shared)
            .limit(limit);

        // Get manifests matching filter
        let manifests = match ops.state.manifests.list(filter) {
            Ok(m) => m,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to list sources: {}",
                    e
                ))]));
            }
        };

        // Convert to SourceInfo, applying topic filter if provided
        let sources: Vec<SourceInfo> = manifests
            .into_iter()
            .filter(|m| {
                // Apply topic filter if provided
                if let Some(ref topic) = input.topic {
                    let topic_lower = topic.to_lowercase();
                    m.metadata.title.to_lowercase().contains(&topic_lower)
                        || m.metadata
                            .tags
                            .iter()
                            .any(|t| t.to_lowercase().contains(&topic_lower))
                        || m.metadata
                            .description
                            .as_ref()
                            .is_some_and(|d| d.to_lowercase().contains(&topic_lower))
                } else {
                    true
                }
            })
            .map(|m| {
                // Generate preview from manifest
                let preview = m
                    .metadata
                    .description
                    .clone()
                    .unwrap_or_else(|| format!("{} bytes of content", m.metadata.content_size));

                SourceInfo {
                    hash: hash_to_string(&m.hash),
                    title: m.metadata.title.clone(),
                    price_hbar: tinybars_to_hbar(m.economics.price),
                    preview,
                    topics: m.metadata.tags.clone(),
                }
            })
            .collect();

        let total_available = sources.len() as u32;

        let output = ListSourcesOutput {
            sources,
            total_available,
        };

        info!(count = output.sources.len(), "Listed sources");

        let json = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    /// Get the current budget status.
    #[tool(
        description = "Get the current budget status for this session. Returns JSON with total, spent, remaining, and auto-approve threshold in HBAR."
    )]
    async fn budget_status(&self) -> Result<CallToolResult, McpError> {
        let status = self.budget.status_json();
        let json = serde_json::to_string_pretty(&status)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }
}

/// Knowledge resource URI prefix.
const KNOWLEDGE_URI_PREFIX: &str = "knowledge://";

#[tool_handler]
impl rmcp::ServerHandler for NodalyncMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::LATEST,
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
            server_info: Implementation::from_build_env(),
            instructions: Some(
                "Nodalync MCP Server - Query decentralized knowledge with automatic payments. \
                 Use `list_sources` to discover available content, then `query_knowledge` \
                 to retrieve content. You can also access content directly via `knowledge://{hash}` resources. \
                 Payments are handled automatically within your session budget."
                    .into(),
            ),
        }
    }

    /// List available resource templates.
    ///
    /// Exposes the `knowledge://{hash}` URI template for direct content access.
    #[allow(clippy::manual_async_fn)]
    fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParam>,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListResourceTemplatesResult, McpError>> + Send + '_
    {
        async move {
            let template = RawResourceTemplate {
                uri_template: format!("{}{}", KNOWLEDGE_URI_PREFIX, "{hash}"),
                name: "knowledge".to_string(),
                title: Some("Nodalync Knowledge".to_string()),
                description: Some(
                    "Access knowledge content directly by hash. Use list_sources to discover available hashes.".to_string(),
                ),
                mime_type: Some("text/plain".to_string()),
            };

            Ok(ListResourceTemplatesResult {
                resource_templates: vec![Annotated::new(template, None)],
                next_cursor: None,
            })
        }
    }

    /// Read a knowledge resource by URI.
    ///
    /// Handles `knowledge://{hash}` URIs by fetching and paying for content.
    #[allow(clippy::manual_async_fn)]
    fn read_resource(
        &self,
        request: ReadResourceRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ReadResourceResult, McpError>> + Send + '_ {
        async move {
            let uri = &request.uri;
            debug!(uri = %uri, "Reading knowledge resource");

            // Parse knowledge:// URI
            let hash_str = uri.strip_prefix(KNOWLEDGE_URI_PREFIX).ok_or_else(|| {
                McpError::invalid_params(
                    format!(
                        "Invalid URI scheme. Expected '{}' prefix, got: {}",
                        KNOWLEDGE_URI_PREFIX, uri
                    ),
                    None,
                )
            })?;

            let hash = string_to_hash(hash_str).map_err(|e| {
                McpError::invalid_params(format!("Invalid content hash: {}", e), None)
            })?;

            // Get content preview to check price
            let mut ops = self.ops.lock().await;
            let preview = ops
                .preview_content(&hash)
                .await
                .map_err(|e| McpError::invalid_params(format!("Content not found: {}", e), None))?;

            let price = preview.manifest.economics.price;
            let price_hbar = tinybars_to_hbar(price);

            // Reserve budget before query
            if price > 0 && self.budget.spend(price).is_none() {
                return Err(McpError::invalid_request(
                    format!(
                        "Insufficient budget: content costs {:.6} HBAR but only {:.6} HBAR remaining",
                        price_hbar,
                        self.budget.remaining_hbar()
                    ),
                    None,
                ));
            }

            // Execute query
            let response = match ops.query_content(&hash, price, None).await {
                Ok(r) => r,
                Err(e) => {
                    // Refund on failure
                    if price > 0 {
                        self.budget.refund(price);
                    }
                    return Err(McpError::internal_error(
                        format!("Query failed: {}", e),
                        None,
                    ));
                }
            };

            let content_str = String::from_utf8_lossy(&response.content).to_string();

            info!(
                uri = %uri,
                cost_hbar = price_hbar,
                remaining_hbar = self.budget.remaining_hbar(),
                "Resource read successfully"
            );

            Ok(ReadResourceResult {
                contents: vec![ResourceContents::text(content_str, uri.clone())],
            })
        }
    }
}

/// Run the MCP server on stdio transport.
pub async fn run_server(config: McpServerConfig) -> Result<(), Box<dyn std::error::Error>> {
    use rmcp::{transport::stdio, ServiceExt};

    info!("Starting Nodalync MCP server");

    let server = NodalyncMcpServer::new(config)?;
    let service = server.serve(stdio()).await?;
    service.waiting().await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_config(temp_dir: &TempDir) -> McpServerConfig {
        McpServerConfig {
            budget_hbar: 1.0,
            auto_approve_hbar: 0.01,
            data_dir: temp_dir.path().to_path_buf(),
        }
    }

    #[test]
    fn test_server_creation() {
        let temp_dir = TempDir::new().unwrap();
        let config = test_config(&temp_dir);

        let server = NodalyncMcpServer::new(config);
        assert!(server.is_ok());
    }

    #[tokio::test]
    async fn test_budget_status() {
        let temp_dir = TempDir::new().unwrap();
        let config = test_config(&temp_dir);

        let server = NodalyncMcpServer::new(config).unwrap();
        let result = server.budget_status().await.unwrap();

        // Should return success with budget info
        assert!(!result.is_error.unwrap_or(false));
    }

    #[tokio::test]
    async fn test_list_sources_empty() {
        let temp_dir = TempDir::new().unwrap();
        let config = test_config(&temp_dir);

        let server = NodalyncMcpServer::new(config).unwrap();
        let input = ListSourcesInput {
            topic: None,
            limit: None,
        };

        let result = server.list_sources(Parameters(input)).await.unwrap();

        // Should succeed even with no sources
        assert!(!result.is_error.unwrap_or(false));
    }
}
