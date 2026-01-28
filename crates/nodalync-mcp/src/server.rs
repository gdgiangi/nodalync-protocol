//! MCP server implementation for Nodalync.
//!
//! Uses the RMCP SDK to expose Nodalync knowledge querying to AI assistants.

use std::sync::Arc;
use std::time::Duration;

use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::*,
    service::{RequestContext, RoleServer},
    tool, tool_handler, tool_router, ErrorData as McpError,
};
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

use nodalync_crypto::{peer_id_from_public_key, PeerId as NodalyncPeerId};
use nodalync_net::{Multiaddr, Network, NetworkConfig, NetworkNode, PeerId as LibP2pPeerId};
use nodalync_ops::DefaultNodeOperations;
use nodalync_store::{ManifestFilter, ManifestStore, NodeState, NodeStateConfig};
use nodalync_types::{ContentType, Visibility};

use crate::budget::{hbar_to_tinybars, tinybars_to_hbar, BudgetTracker};
use crate::tools::{
    hash_to_string, string_to_hash, HealthStatusOutput, ListSourcesInput, ListSourcesOutput,
    QueryKnowledgeInput, QueryKnowledgeOutput, SearchNetworkInput, SearchNetworkOutput,
    SearchResultInfo, SourceInfo,
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
    /// Enable network connectivity for live peer search.
    pub enable_network: bool,
    /// Bootstrap nodes to connect to (multiaddr strings).
    pub bootstrap_nodes: Vec<String>,
}

/// Default bootstrap node address.
const DEFAULT_BOOTSTRAP_NODE: &str = "/dns4/nodalync-bootstrap.eastus.azurecontainer.io/tcp/9000/p2p/12D3KooWMqrUmZm4e1BJTRMWqKHCe1TSX9Vu83uJLEyCGr2dUjYm";

impl Default for McpServerConfig {
    fn default() -> Self {
        Self {
            budget_hbar: 1.0,
            auto_approve_hbar: 0.01,
            data_dir: directories::ProjectDirs::from("", "", "nodalync")
                .map(|d| d.data_dir().to_path_buf())
                .unwrap_or_else(|| std::path::PathBuf::from("~/.nodalync")),
            enable_network: false,
            bootstrap_nodes: vec![DEFAULT_BOOTSTRAP_NODE.to_string()],
        }
    }
}

/// Nodalync MCP Server.
///
/// Implements the MCP server handler with `query_knowledge`, `list_sources`,
/// and `search_network` tools.
#[derive(Clone)]
pub struct NodalyncMcpServer {
    /// Node operations instance.
    ops: Arc<Mutex<DefaultNodeOperations>>,
    /// Budget tracker.
    budget: Arc<BudgetTracker>,
    /// Tool router for MCP.
    tool_router: ToolRouter<Self>,
    /// Optional network node for live peer search.
    network: Option<Arc<NetworkNode>>,
}

#[tool_router]
impl NodalyncMcpServer {
    /// Create a new MCP server with the given configuration.
    pub async fn new(
        config: McpServerConfig,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        // Initialize node state
        let state_config = NodeStateConfig::new(&config.data_dir);
        let state = NodeState::open(state_config)?;

        // Load or generate identity
        let (private_key, public_key, peer_id) = load_identity(&state, &config)?;

        // Optionally create network
        let network = if config.enable_network {
            info!("Initializing network for MCP server...");

            let net_config = build_network_config(&config, &private_key, &public_key)?;
            let libp2p_keypair = to_libp2p_keypair(&private_key)?;

            let node = NetworkNode::with_keypair(
                private_key.clone(),
                public_key,
                libp2p_keypair,
                net_config,
            )
            .await?;

            // Subscribe to announcements
            if let Err(e) = node.subscribe_announcements().await {
                warn!("Failed to subscribe to announcements: {}", e);
            }

            // Bootstrap with timeout
            info!("Bootstrapping to network...");
            match tokio::time::timeout(Duration::from_secs(15), node.bootstrap()).await {
                Ok(Ok(())) => {
                    info!(
                        "Bootstrap complete, connected to {} peer(s)",
                        node.connected_peers().len()
                    );
                }
                Ok(Err(e)) => {
                    warn!(
                        "Bootstrap failed: {} - continuing with limited connectivity",
                        e
                    );
                }
                Err(_) => {
                    warn!("Bootstrap timed out - continuing with limited connectivity");
                }
            }

            Some(Arc::new(node))
        } else {
            None
        };

        // Create operations with or without network
        let ops = if let Some(ref net) = network {
            DefaultNodeOperations::with_defaults_and_network(
                state,
                peer_id,
                Arc::clone(net) as Arc<dyn nodalync_net::Network>,
            )
        } else {
            DefaultNodeOperations::with_defaults(state, peer_id)
        };

        // Create budget tracker
        let budget = BudgetTracker::with_auto_approve(config.budget_hbar, config.auto_approve_hbar);

        info!(
            budget_hbar = config.budget_hbar,
            auto_approve_hbar = config.auto_approve_hbar,
            network_enabled = config.enable_network,
            "MCP server initialized"
        );

        Ok(Self {
            ops: Arc::new(Mutex::new(ops)),
            budget: Arc::new(budget),
            tool_router: Self::tool_router(),
            network,
        })
    }

    /// Create a server with default configuration.
    pub async fn with_defaults() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Self::new(McpServerConfig::default()).await
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
        description = "List available knowledge sources in the Nodalync network. Returns content metadata including titles, prices, and topics. Use this to discover what knowledge is available before querying. Set include_network=true to also search connected peers in real-time."
    )]
    async fn list_sources(
        &self,
        Parameters(input): Parameters<ListSourcesInput>,
    ) -> Result<CallToolResult, McpError> {
        debug!(topic = ?input.topic, limit = ?input.limit, include_network = ?input.include_network, "Processing list_sources request");

        let limit = input.limit.unwrap_or(10).min(50);
        let include_network = input.include_network.unwrap_or(false);
        let mut ops = self.ops.lock().await;

        let mut sources: Vec<SourceInfo> = Vec::new();
        let mut seen_hashes = std::collections::HashSet::new();

        // 1. If network enabled, do a live peer search first (this also caches results)
        if include_network && ops.has_network() {
            let query = input.topic.as_deref().unwrap_or("");
            if let Ok(results) = ops.search_network(query, None, limit).await {
                for r in results {
                    if seen_hashes.insert(r.hash) {
                        let preview = if !r.l1_summary.preview_mentions.is_empty() {
                            r.l1_summary
                                .preview_mentions
                                .iter()
                                .take(2)
                                .map(|m| m.content.clone())
                                .collect::<Vec<_>>()
                                .join("; ")
                        } else {
                            r.l1_summary.summary.clone()
                        };

                        sources.push(SourceInfo {
                            hash: hash_to_string(&r.hash),
                            title: r.title.clone(),
                            price_hbar: tinybars_to_hbar(r.price),
                            preview,
                            topics: r.l1_summary.primary_topics.clone(),
                        });
                    }
                }
            }
        } else {
            // 2. Local-only: Get local manifests
            let filter = ManifestFilter::new()
                .with_visibility(Visibility::Shared)
                .limit(limit);

            if let Ok(manifests) = ops.state.manifests.list(filter) {
                for m in manifests {
                    if seen_hashes.insert(m.hash) {
                        // Apply topic filter if provided
                        let matches_topic = if let Some(ref topic) = input.topic {
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
                        };

                        if matches_topic {
                            let preview = m.metadata.description.clone().unwrap_or_else(|| {
                                format!("{} bytes of content", m.metadata.content_size)
                            });

                            sources.push(SourceInfo {
                                hash: hash_to_string(&m.hash),
                                title: m.metadata.title.clone(),
                                price_hbar: tinybars_to_hbar(m.economics.price),
                                preview,
                                topics: m.metadata.tags.clone(),
                            });
                        }
                    }
                }
            }

            // 3. Include cached network announcements if requested (but no live peers)
            if include_network {
                let query = input.topic.as_deref().unwrap_or("");
                let announcements = ops.state.search_announcements(query, None, limit);

                for announce in announcements {
                    if seen_hashes.insert(announce.hash) {
                        let preview = if !announce.l1_summary.preview_mentions.is_empty() {
                            announce
                                .l1_summary
                                .preview_mentions
                                .iter()
                                .map(|m| m.content.clone())
                                .collect::<Vec<_>>()
                                .join("; ")
                        } else {
                            format!("{} mentions extracted", announce.l1_summary.mention_count)
                        };

                        sources.push(SourceInfo {
                            hash: hash_to_string(&announce.hash),
                            title: announce.title.clone(),
                            price_hbar: tinybars_to_hbar(announce.price),
                            preview,
                            topics: vec![],
                        });
                    }
                }
            }
        }

        // Truncate to limit
        sources.truncate(limit as usize);

        let total_available = sources.len() as u32;

        let output = ListSourcesOutput {
            sources,
            total_available,
        };

        info!(
            count = output.sources.len(),
            include_network = include_network,
            "Listed sources"
        );

        let json = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    /// Search the Nodalync network for knowledge sources.
    ///
    /// Queries local content, cached announcements, AND connected peers in real-time.
    #[tool(
        description = "Search the Nodalync network for knowledge sources. Queries local content, cached announcements, AND connected peers in real-time. Returns results with source attribution ('local', 'cached', or 'peer'). Use this to discover content before querying. Requires --enable-network flag for live peer search."
    )]
    async fn search_network(
        &self,
        Parameters(input): Parameters<SearchNetworkInput>,
    ) -> Result<CallToolResult, McpError> {
        let start = std::time::Instant::now();
        let limit = input.limit.unwrap_or(10).min(50);

        debug!(query = %input.query, limit = limit, "Processing search_network request");

        // Parse content type filter
        let content_type = input
            .content_type
            .as_ref()
            .and_then(|s| parse_content_type(s));

        let mut ops = self.ops.lock().await;

        // Check if network is available for live search
        let has_network = ops.has_network();
        if !has_network {
            debug!("Network not enabled - searching local and cached only");
        }

        // Call search_network (searches local + cached + peers if network available)
        let results = ops
            .search_network(&input.query, content_type, limit)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let peers_queried = if let Some(ref network) = self.network {
            network.connected_peers().len().min(5) as u32
        } else {
            0
        };

        let output = SearchNetworkOutput {
            results: results
                .iter()
                .map(|r| {
                    // Extract preview mentions from L1 summary
                    let preview: Vec<String> = r
                        .l1_summary
                        .preview_mentions
                        .iter()
                        .map(|m| m.content.clone())
                        .collect();

                    SearchResultInfo {
                        hash: hash_to_string(&r.hash),
                        title: r.title.clone(),
                        price_hbar: tinybars_to_hbar(r.price),
                        content_type: format!("{:?}", r.content_type),
                        owner: if r.owner == nodalync_crypto::UNKNOWN_PEER_ID {
                            "unknown".to_string()
                        } else {
                            r.owner.to_string()
                        },
                        source: r.source.to_string(),
                        preview,
                        topics: r.l1_summary.primary_topics.clone(),
                    }
                })
                .collect(),
            total: results.len() as u32,
            peers_queried,
            latency_ms: start.elapsed().as_millis() as u64,
        };

        info!(
            query = %input.query,
            results = output.total,
            peers_queried = peers_queried,
            latency_ms = output.latency_ms,
            "Network search completed"
        );

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

    /// Get the health status of the Nodalync node.
    #[tool(
        description = "Get health status of the Nodalync node. Returns connection status, peer count, bootstrap status, budget info, and local content count. Use this for diagnostics."
    )]
    async fn health_status(&self) -> Result<CallToolResult, McpError> {
        let ops = self.ops.lock().await;

        // Count local content
        let filter = ManifestFilter::new();
        let local_content_count = match ops.state.manifests.list(filter) {
            Ok(manifests) => manifests.len() as u32,
            Err(_) => 0,
        };

        // Get peer ID from ops
        let peer_id = ops.peer_id().to_string();

        // Get actual network status if network is enabled
        let (connected_peers, is_bootstrapped) = if let Some(ref network) = self.network {
            let peers = network.connected_peers().len() as u32;
            (peers, peers > 0)
        } else {
            (0, false)
        };

        let output = HealthStatusOutput {
            connected_peers,
            is_bootstrapped,
            budget_remaining_hbar: self.budget.remaining_hbar(),
            budget_total_hbar: self.budget.total_budget_hbar(),
            budget_spent_hbar: self.budget.spent_hbar(),
            local_content_count,
            peer_id,
        };

        info!(
            local_content = local_content_count,
            connected_peers = connected_peers,
            is_bootstrapped = is_bootstrapped,
            budget_remaining = output.budget_remaining_hbar,
            "Health status requested"
        );

        let json = serde_json::to_string_pretty(&output)
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
pub async fn run_server(
    config: McpServerConfig,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use rmcp::{transport::stdio, ServiceExt};

    info!("Starting Nodalync MCP server");

    let server = NodalyncMcpServer::new(config).await?;
    let service = server.serve(stdio()).await?;
    service.waiting().await?;

    Ok(())
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Load or generate identity for the MCP server.
///
/// Tries to load from environment variable NODALYNC_PASSWORD, falls back to
/// generating an ephemeral identity if not available.
fn load_identity(
    state: &NodeState,
    _config: &McpServerConfig,
) -> Result<
    (
        nodalync_crypto::PrivateKey,
        nodalync_crypto::PublicKey,
        NodalyncPeerId,
    ),
    Box<dyn std::error::Error + Send + Sync>,
> {
    // Try loading from environment variable
    if let Ok(password) = std::env::var("NODALYNC_PASSWORD") {
        if state.identity.exists() {
            match state.identity.load(&password) {
                Ok((private_key, public_key)) => {
                    let peer_id = peer_id_from_public_key(&public_key);
                    info!("Loaded identity from keystore");
                    return Ok((private_key, public_key, peer_id));
                }
                Err(e) => {
                    warn!("Failed to load identity: {} - generating ephemeral", e);
                }
            }
        }
    }

    // Generate ephemeral identity
    let (private_key, public_key) = nodalync_crypto::generate_identity();
    let peer_id = peer_id_from_public_key(&public_key);
    warn!("Using ephemeral identity - set NODALYNC_PASSWORD for persistent identity");
    Ok((private_key, public_key, peer_id))
}

/// Build network configuration from MCP server config.
fn build_network_config(
    config: &McpServerConfig,
    _private_key: &nodalync_crypto::PrivateKey,
    _public_key: &nodalync_crypto::PublicKey,
) -> Result<NetworkConfig, Box<dyn std::error::Error + Send + Sync>> {
    // Parse bootstrap nodes
    let mut bootstrap_nodes = Vec::new();
    for node_str in &config.bootstrap_nodes {
        if let Some((peer_id, addr)) = parse_bootstrap_address(node_str) {
            bootstrap_nodes.push((peer_id, addr));
        } else {
            warn!("Invalid bootstrap node address: {}", node_str);
        }
    }

    // Use random port for MCP server (don't conflict with main node)
    Ok(NetworkConfig {
        listen_addresses: vec!["/ip4/127.0.0.1/tcp/0".parse()?],
        bootstrap_nodes,
        ..Default::default()
    })
}

/// Parse a bootstrap address string into peer ID and multiaddr.
///
/// Expected format: /dns4/host/tcp/port/p2p/peer_id or /ip4/addr/tcp/port/p2p/peer_id
fn parse_bootstrap_address(addr_str: &str) -> Option<(LibP2pPeerId, Multiaddr)> {
    use nodalync_net::multiaddr::Protocol;

    let addr: Multiaddr = addr_str.parse().ok()?;

    // Extract peer ID from the address
    let mut peer_id = None;
    for proto in addr.iter() {
        if let Protocol::P2p(pid) = proto {
            peer_id = Some(pid);
        }
    }

    let peer_id = peer_id?;

    // Build address without peer ID for dialing
    let mut dial_addr = Multiaddr::empty();
    for proto in addr.iter() {
        if !matches!(proto, Protocol::P2p(_)) {
            dial_addr.push(proto);
        }
    }

    Some((peer_id, dial_addr))
}

/// Convert a Nodalync private key to a libp2p keypair.
fn to_libp2p_keypair(
    private_key: &nodalync_crypto::PrivateKey,
) -> Result<nodalync_net::identity::Keypair, Box<dyn std::error::Error + Send + Sync>> {
    // Create ed25519 secret key from our 32-byte seed
    let secret =
        nodalync_net::identity::ed25519::SecretKey::try_from_bytes(private_key.as_bytes().to_vec())
            .map_err(|e| format!("Failed to create ed25519 secret key: {}", e))?;
    let keypair = nodalync_net::identity::ed25519::Keypair::from(secret);
    Ok(nodalync_net::identity::Keypair::from(keypair))
}

/// Parse a content type string to ContentType enum.
fn parse_content_type(s: &str) -> Option<ContentType> {
    match s.to_uppercase().as_str() {
        "L0" => Some(ContentType::L0),
        "L1" => Some(ContentType::L1),
        "L2" => Some(ContentType::L2),
        "L3" => Some(ContentType::L3),
        _ => None,
    }
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
            enable_network: false,
            bootstrap_nodes: vec![],
        }
    }

    #[tokio::test]
    async fn test_server_creation() {
        let temp_dir = TempDir::new().unwrap();
        let config = test_config(&temp_dir);

        let server = NodalyncMcpServer::new(config).await;
        assert!(server.is_ok());
    }

    #[tokio::test]
    async fn test_budget_status() {
        let temp_dir = TempDir::new().unwrap();
        let config = test_config(&temp_dir);

        let server = NodalyncMcpServer::new(config).await.unwrap();
        let result = server.budget_status().await.unwrap();

        // Should return success with budget info
        assert!(!result.is_error.unwrap_or(false));
    }

    #[tokio::test]
    async fn test_list_sources_empty() {
        let temp_dir = TempDir::new().unwrap();
        let config = test_config(&temp_dir);

        let server = NodalyncMcpServer::new(config).await.unwrap();
        let input = ListSourcesInput {
            topic: None,
            limit: None,
            include_network: None,
        };

        let result = server.list_sources(Parameters(input)).await.unwrap();

        // Should succeed even with no sources
        assert!(!result.is_error.unwrap_or(false));
    }

    #[tokio::test]
    async fn test_search_network_without_network() {
        let temp_dir = TempDir::new().unwrap();
        let config = test_config(&temp_dir);

        let server = NodalyncMcpServer::new(config).await.unwrap();
        let input = SearchNetworkInput {
            query: "test".to_string(),
            limit: None,
            content_type: None,
        };

        // Should succeed even without network (searches local only)
        let result = server.search_network(Parameters(input)).await.unwrap();
        assert!(!result.is_error.unwrap_or(false));
    }

    #[tokio::test]
    async fn test_health_status_without_network() {
        let temp_dir = TempDir::new().unwrap();
        let config = test_config(&temp_dir);

        let server = NodalyncMcpServer::new(config).await.unwrap();
        let result = server.health_status().await.unwrap();

        // Should return success
        assert!(!result.is_error.unwrap_or(false));

        // Verify content contains expected fields
        if let Some(content) = result.content.first() {
            if let RawContent::Text(RawTextContent { text, .. }) = &content.raw {
                // Without network, should show 0 connected peers
                assert!(text.contains("\"connected_peers\": 0"));
                assert!(text.contains("\"is_bootstrapped\": false"));
            }
        }
    }

    #[test]
    fn test_parse_content_type() {
        assert_eq!(parse_content_type("L0"), Some(ContentType::L0));
        assert_eq!(parse_content_type("l1"), Some(ContentType::L1));
        assert_eq!(parse_content_type("L2"), Some(ContentType::L2));
        assert_eq!(parse_content_type("l3"), Some(ContentType::L3));
        assert_eq!(parse_content_type("invalid"), None);
    }

    #[test]
    fn test_parse_bootstrap_address() {
        let addr =
            "/dns4/example.com/tcp/9000/p2p/12D3KooWMqrUmZm4e1BJTRMWqKHCe1TSX9Vu83uJLEyCGr2dUjYm";
        let result = parse_bootstrap_address(addr);
        assert!(result.is_some());

        let (peer_id, dial_addr) = result.unwrap();
        assert_eq!(
            peer_id.to_string(),
            "12D3KooWMqrUmZm4e1BJTRMWqKHCe1TSX9Vu83uJLEyCGr2dUjYm"
        );
        assert_eq!(dial_addr.to_string(), "/dns4/example.com/tcp/9000");
    }

    #[test]
    fn test_search_network_input_parsing() {
        let json = r#"{"query": "protocol", "limit": 20}"#;
        let input: SearchNetworkInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.query, "protocol");
        assert_eq!(input.limit, Some(20));
        assert!(input.content_type.is_none());
    }

    #[test]
    fn test_search_network_input_with_content_type() {
        let json = r#"{"query": "test", "content_type": "L0"}"#;
        let input: SearchNetworkInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.query, "test");
        assert_eq!(input.content_type, Some("L0".to_string()));
    }
}
