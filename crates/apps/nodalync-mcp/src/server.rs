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

use nodalync_crypto::{
    content_hash, peer_id_from_public_key, PeerId as NodalyncPeerId, UNKNOWN_PEER_ID,
};
use nodalync_net::{Multiaddr, Network, NetworkNode, PeerId as LibP2pPeerId};
use nodalync_ops::DefaultNodeOperations;
use nodalync_store::{
    ChannelStore, ContentStore, ManifestFilter, ManifestStore, NodeState, NodeStateConfig,
};
use nodalync_types::{ContentType, Visibility};

use crate::budget::{hbar_to_tinybars, tinybars_to_hbar, BudgetTracker};
use crate::error::McpError as NodalyncMcpError;
use crate::startup;
use crate::tools::{
    hash_to_string, string_to_hash, ChannelCloseResult, ChannelInfo, CloseAllChannelsOutput,
    CloseChannelInput, ContentEarnings, DeleteContentInput, DeleteContentOutput, DepositHbarInput,
    DepositHbarOutput, GetEarningsInput, GetEarningsOutput, ListSourcesInput, ListSourcesOutput,
    ListVersionsInput, ListVersionsOutput, OpenChannelInput, OpenChannelOutput, PaymentDetails,
    PreviewContentInput, PreviewContentOutput, PublishContentInput, PublishContentOutput,
    QueryKnowledgeInput, QueryKnowledgeOutput, SearchNetworkInput, SearchNetworkOutput,
    SearchResultInfo, SetVisibilityInput, SetVisibilityOutput, SourceInfo, StatusOutput,
    SynthesizeContentInput, SynthesizeContentOutput, UpdateContentInput, UpdateContentOutput,
    VersionEntry,
};

/// Create a standardized error response for MCP tools.
///
/// Returns a JSON-formatted error with error code, message, and recovery suggestion.
fn tool_error(error: &NodalyncMcpError) -> CallToolResult {
    let code = error.error_code();
    let response = serde_json::json!({
        "error": code.to_string(),
        "code": code.code(),
        "message": error.to_string(),
        "suggestion": code.suggestion(),
    });
    CallToolResult::error(vec![Content::text(response.to_string())])
}

/// Convert a Nodalync PeerId to a base58 string.
fn peer_id_to_string(peer_id: &NodalyncPeerId) -> String {
    bs58::encode(&peer_id.0).into_string()
}

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
    /// Optional Hedera configuration for on-chain settlement.
    pub hedera: Option<HederaConfig>,
}

/// Configuration for Hedera settlement integration.
#[derive(Debug, Clone)]
pub struct HederaConfig {
    /// Hedera account ID (e.g., "0.0.7703962").
    pub account_id: String,
    /// Path to Hedera private key file.
    pub private_key_path: std::path::PathBuf,
    /// Settlement contract ID (e.g., "0.0.7729011").
    pub contract_id: String,
    /// Hedera network (testnet, mainnet, previewnet).
    pub network: String,
}

/// Default bootstrap node addresses (US, EU, Asia).
const DEFAULT_BOOTSTRAP_NODES: &[&str] = &[
    "/dns4/nodalync-bootstrap.eastus.azurecontainer.io/tcp/9000/p2p/12D3KooWMqrUmZm4e1BJTRMWqKHCe1TSX9Vu83uJLEyCGr2dUjYm",
    "/dns4/nodalync-eu.northeurope.azurecontainer.io/tcp/9000/p2p/12D3KooWQiK8uHf877wena9MAPHHprXkmGRhAmXAYakRsMfdnk7P",
    "/dns4/nodalync-asia.southeastasia.azurecontainer.io/tcp/9000/p2p/12D3KooWFojioE6LXFs3qqBdKQeCFuMr2obsMrvXGY69jmhheLfk",
];

impl Default for McpServerConfig {
    fn default() -> Self {
        Self {
            budget_hbar: 1.0,
            auto_approve_hbar: 0.01,
            data_dir: nodalync_store::default_data_dir(),
            enable_network: false,
            bootstrap_nodes: DEFAULT_BOOTSTRAP_NODES
                .iter()
                .map(|s| s.to_string())
                .collect(),
            hedera: None,
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
    /// Optional Hedera settlement for on-chain operations.
    settlement: Option<Arc<dyn nodalync_settle::Settlement>>,
    /// Hedera configuration (if enabled).
    hedera_config: Option<HederaConfig>,
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
        let network = startup::start_network(&config, &private_key, public_key).await?;

        // Create Hedera settlement if configured
        let settlement = startup::create_settlement(config.hedera.as_ref()).await?;

        // Create operations with network and/or settlement
        let mut ops = startup::build_ops(state, peer_id, network.as_ref(), settlement.as_ref());

        // Set the private key for signing payments
        ops.set_private_key(private_key);

        // Wrap ops in Arc<Mutex> for sharing
        let ops = Arc::new(Mutex::new(ops));

        // Spawn background event processor, cleanup and settlement if network is enabled
        if let Some(ref net) = network {
            startup::spawn_background_tasks(&ops, net);
        }

        // Create budget tracker
        let budget = BudgetTracker::with_auto_approve(config.budget_hbar, config.auto_approve_hbar);

        info!(
            budget_hbar = config.budget_hbar,
            auto_approve_hbar = config.auto_approve_hbar,
            network_enabled = config.enable_network,
            hedera_enabled = config.hedera.is_some(),
            "MCP server initialized"
        );

        Ok(Self {
            ops,
            budget: Arc::new(budget),
            tool_router: Self::tool_router(),
            network,
            settlement,
            hedera_config: config.hedera.clone(),
        })
    }

    /// Create a server with default configuration.
    pub async fn with_defaults() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Self::new(McpServerConfig::default()).await
    }

    /// Gracefully shutdown the MCP server, closing all payment channels.
    ///
    /// This should be called before dropping the server to ensure all payment
    /// channels are properly closed and settled. For cooperative channels, this
    /// settles immediately. For unresponsive peers, this initiates a dispute.
    ///
    /// Returns the number of channels that were processed.
    pub async fn shutdown(&self) -> u32 {
        info!("MCP server shutting down, closing all payment channels...");

        // Get list of open channels and private key
        let (channels, private_key) = {
            let ops = self.ops.lock().await;
            let channels = ops.state.channels.list_open().unwrap_or_default();
            let private_key = ops.private_key().cloned();
            (channels, private_key)
        };

        if channels.is_empty() {
            info!("No open payment channels to close");
            return 0;
        }

        let Some(private_key) = private_key else {
            warn!("Private key not available, cannot close channels");
            return 0;
        };

        let channels_count = channels.len() as u32;
        info!(
            channels_count = channels_count,
            "Closing payment channels on shutdown"
        );

        let mut closed = 0u32;
        let mut disputed = 0u32;
        let mut failed = 0u32;

        for (peer_id, _channel) in channels {
            let peer_id_str = peer_id_to_string(&peer_id);

            // Try cooperative close with short timeout
            let close_result = {
                let mut ops = self.ops.lock().await;
                tokio::time::timeout(
                    Duration::from_secs(3),
                    ops.close_payment_channel(&peer_id, &private_key),
                )
                .await
            };

            match close_result {
                Ok(Ok(nodalync_ops::CloseResult::Success { .. }))
                | Ok(Ok(nodalync_ops::CloseResult::SuccessOffChain { .. })) => {
                    closed += 1;
                    debug!(peer_id = %peer_id_str, "Channel closed on shutdown");
                }
                Ok(Ok(nodalync_ops::CloseResult::PeerUnresponsive { .. }))
                | Ok(Ok(nodalync_ops::CloseResult::OnChainFailed { .. }))
                | Ok(Err(_))
                | Err(_) => {
                    // Peer unresponsive or error - initiate dispute
                    let dispute_result = {
                        let mut ops = self.ops.lock().await;
                        ops.dispute_payment_channel(&peer_id, &private_key).await
                    };

                    match dispute_result {
                        Ok(_tx_id) => {
                            disputed += 1;
                            debug!(peer_id = %peer_id_str, "Dispute initiated on shutdown");
                        }
                        Err(e) => {
                            failed += 1;
                            warn!(
                                peer_id = %peer_id_str,
                                error = %e,
                                "Failed to close or dispute channel on shutdown"
                            );
                        }
                    }
                }
            }
        }

        info!(
            closed = closed,
            disputed = disputed,
            failed = failed,
            "Shutdown channel cleanup complete"
        );

        channels_count
    }

    /// Query knowledge from the Nodalync network.
    ///
    /// Retrieves content matching the query. Payment is fully automated:
    /// - Auto-deposits HBAR if settlement balance is insufficient
    /// - Auto-opens payment channels when needed
    /// - Returns all transaction confirmations in the response
    #[tool(
        description = "Query knowledge from the Nodalync network. Returns content with provenance and full transaction details. Payment is fully automated - channels are opened and deposits are made as needed. Query by content hash (use search_network to find content first)."
    )]
    async fn query_knowledge(
        &self,
        Parameters(input): Parameters<QueryKnowledgeInput>,
    ) -> Result<CallToolResult, McpError> {
        debug!(query = %input.query, "Processing query_knowledge request");
        tool_outcome(self.run_query_knowledge(&input).await)
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
        let topic = input.topic.as_deref();

        // 1. If network enabled, do a live peer search first (this also caches results)
        if include_network && ops.has_network() {
            add_network_sources(&mut ops, topic, limit, &mut seen_hashes, &mut sources).await;
        } else {
            // 2. Local-only: Get local manifests
            add_local_sources(&ops, topic, limit, &mut seen_hashes, &mut sources);

            // 3. Include cached network announcements if requested (but no live peers)
            if include_network {
                add_cached_announcement_sources(&ops, topic, limit, &mut seen_hashes, &mut sources);
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

        json_result(&output)
    }

    /// Search the Nodalync network for knowledge sources.
    ///
    /// Step 1 of the knowledge query workflow: Find content by searching.
    /// Step 2: Use query_knowledge with the hash from search results.
    #[tool(
        description = "Search the Nodalync network for knowledge. Returns a list of available content with hashes, titles, prices, and previews. Use the 'hash' field from results to query content with query_knowledge. Supports filtering by content_type (L0=raw documents, L3=synthesized insights)."
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
                        peer_id: r.publisher_peer_id.clone(),
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

    /// Get comprehensive status of the Nodalync node.
    ///
    /// Returns network, budget, channel, and Hedera status in a single response.
    /// This is the recommended way to check node status.
    #[tool(
        description = "Get comprehensive status of the Nodalync node including network connectivity, session budget, payment channels, and Hedera balance. Use this as the primary status check."
    )]
    async fn status(&self) -> Result<CallToolResult, McpError> {
        // Collect data from ops while holding lock, then release before async calls
        let (peer_id, local_content_count, open_channels, channel_balance_tinybars, channels_info) = {
            let ops = self.ops.lock().await;

            let peer_id = ops.peer_id().to_string();

            // Local content count
            let filter = ManifestFilter::new();
            let local_content_count = match ops.state.manifests.list(filter) {
                Ok(manifests) => manifests.len() as u32,
                Err(_) => 0,
            };

            // Channel status with detailed info
            let channels = ops.state.channels.list_open().unwrap_or_default();
            let open_channels = channels.len() as u32;
            let channel_balance_tinybars: u64 = channels
                .iter()
                .map(|(_, c)| c.my_balance + c.their_balance)
                .sum();

            // Build detailed channel info
            let channels_info: Vec<ChannelInfo> = channels
                .iter()
                .map(|(nodalync_peer_id, channel)| {
                    // Try to look up the libp2p peer ID from the network
                    let libp2p_peer_id = if let Some(ref network) = self.network {
                        network
                            .libp2p_peer_id(nodalync_peer_id)
                            .map(|pid| pid.to_string())
                    } else {
                        None
                    };

                    ChannelInfo {
                        channel_id: hash_to_string(&channel.channel_id),
                        peer_id: peer_id_to_string(nodalync_peer_id),
                        libp2p_peer_id,
                        state: format!("{:?}", channel.state),
                        my_balance_hbar: tinybars_to_hbar(channel.my_balance),
                        their_balance_hbar: tinybars_to_hbar(channel.their_balance),
                        pending_payments: channel.pending_payments.len() as u32,
                        last_update: channel.last_update,
                    }
                })
                .collect();

            (
                peer_id,
                local_content_count,
                open_channels,
                channel_balance_tinybars,
                channels_info,
            )
        }; // ops lock released here

        // Network status (no lock needed - network methods are thread-safe)
        let (connected_peers, is_bootstrapped) = if let Some(ref network) = self.network {
            let peers = network.connected_peers().len() as u32;
            (peers, peers > 0)
        } else {
            (0, false)
        };

        // Hedera status - async calls done without holding ops lock
        // Fetch both account balance (on-chain HBAR) and contract balance (deposited funds)
        let (
            hedera_account_id,
            hedera_network,
            hedera_account_balance_hbar,
            hedera_contract_balance_hbar,
        ) = if let (Some(config), Some(settlement)) = (&self.hedera_config, &self.settlement) {
            // Fetch both balances in parallel for efficiency
            let (account_balance, contract_balance) =
                tokio::join!(settlement.get_account_balance(), settlement.get_balance());
            (
                Some(config.account_id.clone()),
                Some(config.network.clone()),
                account_balance.ok().map(tinybars_to_hbar),
                contract_balance.ok().map(tinybars_to_hbar),
            )
        } else {
            (None, None, None, None)
        };

        let output = StatusOutput {
            // Network
            connected_peers,
            is_bootstrapped,
            peer_id,
            local_content_count,
            // Budget
            budget_remaining_hbar: self.budget.remaining_hbar(),
            budget_total_hbar: self.budget.total_budget_hbar(),
            budget_spent_hbar: self.budget.spent_hbar(),
            // Channels
            open_channels,
            channel_balance_hbar: tinybars_to_hbar(channel_balance_tinybars),
            channels: channels_info,
            // Hedera
            hedera_configured: self.settlement.is_some(),
            hedera_account_id,
            hedera_network,
            hedera_account_balance_hbar,
            hedera_contract_balance_hbar,
        };

        info!(
            connected_peers = connected_peers,
            open_channels = open_channels,
            budget_remaining = output.budget_remaining_hbar,
            hedera_account_balance = ?output.hedera_account_balance_hbar,
            hedera_contract_balance = ?output.hedera_contract_balance_hbar,
            "Status requested"
        );

        let json = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    /// Deposit HBAR to the settlement contract.
    ///
    /// Deposits funds to enable payment channel operations.
    #[tool(
        description = "Deposit HBAR to the Nodalync settlement contract. This funds your account for opening payment channels and settling transactions. Requires Hedera to be configured."
    )]
    async fn deposit_hbar(
        &self,
        Parameters(input): Parameters<DepositHbarInput>,
    ) -> Result<CallToolResult, McpError> {
        let Some(settlement) = &self.settlement else {
            return Ok(tool_error(&NodalyncMcpError::internal(
                "Hedera settlement is not configured.",
            )));
        };

        let amount_tinybars = hbar_to_tinybars(input.amount_hbar);

        let tx_id = settlement
            .deposit(amount_tinybars)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let new_balance = settlement
            .get_balance()
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let output = DepositHbarOutput {
            transaction_id: tx_id.to_string(),
            amount_tinybars,
            new_balance_tinybars: new_balance,
        };

        info!(
            amount_hbar = input.amount_hbar,
            new_balance_hbar = tinybars_to_hbar(new_balance),
            "Deposit completed"
        );

        let json = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    /// Open a payment channel with a peer.
    ///
    /// Creates a new payment channel for off-chain micropayments.
    #[tool(
        description = "Open a payment channel with a peer. Channels enable fast off-chain micropayments for content queries. The deposit is locked until the channel is closed. Use the peer_id from list_sources or search_network results. Minimum deposit: 100 HBAR."
    )]
    async fn open_channel(
        &self,
        Parameters(input): Parameters<OpenChannelInput>,
    ) -> Result<CallToolResult, McpError> {
        tool_outcome(self.run_open_channel(input).await)
    }

    /// Close a payment channel and settle on-chain.
    ///
    /// Closes an open payment channel with a peer, settling the final balance
    /// on Hedera. Any remaining funds are returned to your account.
    /// If the peer is unresponsive, initiates a dispute (24-hour settlement period).
    #[tool(
        description = "Close a payment channel and settle the final balance on Hedera. Closes the channel with the specified peer and returns any remaining funds. Use status to see open channels first. Accepts both libp2p peer IDs (12D3Koo...) and Nodalync peer IDs (ndl...)."
    )]
    async fn close_channel(
        &self,
        Parameters(input): Parameters<CloseChannelInput>,
    ) -> Result<CallToolResult, McpError> {
        tool_outcome(self.run_close_channel(input).await)
    }

    /// Close all open payment channels.
    ///
    /// Attempts to cooperatively close all open channels. For any unresponsive
    /// peers, initiates a dispute to ensure settlement (24-hour waiting period).
    #[tool(
        description = "Close all open payment channels and settle balances on Hedera. Attempts cooperative close first; if a peer is unresponsive, automatically initiates a dispute to ensure payment. Use this before ending a session to settle all pending payments."
    )]
    async fn close_all_channels(&self) -> Result<CallToolResult, McpError> {
        tool_outcome(self.run_close_all_channels().await)
    }

    /// Publish new content to the Nodalync network.
    ///
    /// Creates L0 content, extracts L1 summary, and publishes with the given visibility and price.
    #[tool(
        description = "Publish new content to the Nodalync network. Creates L0 content with automatic L1 fact extraction, sets pricing and visibility. Returns the content hash for future reference. Use set_visibility or delete_content to manage content after publishing."
    )]
    async fn publish_content(
        &self,
        Parameters(input): Parameters<PublishContentInput>,
    ) -> Result<CallToolResult, McpError> {
        debug!(title = %input.title, "Processing publish_content request");
        tool_outcome(self.run_publish_content(input).await)
    }

    /// Preview content metadata without paying.
    ///
    /// Returns L1 summary, pricing, and provenance information.
    #[tool(
        description = "Preview content metadata including title, price, mentions, and topics without paying for the full content. Use this to inspect content before querying."
    )]
    async fn preview_content(
        &self,
        Parameters(input): Parameters<PreviewContentInput>,
    ) -> Result<CallToolResult, McpError> {
        debug!(hash = %input.hash, "Processing preview_content request");

        let hash = match string_to_hash(&input.hash) {
            Ok(h) => h,
            Err(e) => return Ok(tool_error(&NodalyncMcpError::InvalidHash(e))),
        };

        let mut ops = self.ops.lock().await;
        let preview = match ops.preview_content(&hash).await {
            Ok(p) => p,
            Err(e) => return Ok(tool_error(&NodalyncMcpError::Ops(e))),
        };

        let manifest = &preview.manifest;
        let l1 = &preview.l1_summary;

        let owner = if manifest.owner == UNKNOWN_PEER_ID {
            "unknown".to_string()
        } else {
            peer_id_to_string(&manifest.owner)
        };

        let preview_mentions: Vec<String> = l1
            .preview_mentions
            .iter()
            .map(|m| m.content.clone())
            .collect();

        let output = PreviewContentOutput {
            hash: hash_to_string(&manifest.hash),
            title: manifest.metadata.title.clone(),
            owner,
            price_hbar: tinybars_to_hbar(manifest.economics.price),
            content_type: format!("{:?}", manifest.content_type),
            visibility: format!("{:?}", manifest.visibility),
            size_bytes: manifest.metadata.content_size,
            mention_count: l1.mention_count,
            preview_mentions,
            topics: l1.primary_topics.clone(),
            summary: l1.summary.clone(),
            provider_peer_id: preview.provider_peer_id.clone(),
        };

        let json = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    /// Synthesize L3 content from multiple sources.
    ///
    /// Creates derived content with full provenance tracking.
    #[tool(
        description = "Create L3 synthesized content from multiple sources. Tracks provenance so 95% of query revenue flows to original sources. Optionally publish immediately with pricing. Sources must be valid content hashes."
    )]
    async fn synthesize_content(
        &self,
        Parameters(input): Parameters<SynthesizeContentInput>,
    ) -> Result<CallToolResult, McpError> {
        debug!(title = %input.title, sources = ?input.sources, "Processing synthesize_content request");
        tool_outcome(self.run_synthesize_content(input).await)
    }

    /// Update existing content with a new version.
    ///
    /// Creates a new version linked to the previous one via version chain.
    #[tool(
        description = "Create a new version of existing content. The new version is linked to the previous via the version chain. Title, description, and other metadata are inherited from the previous version unless overridden."
    )]
    async fn update_content(
        &self,
        Parameters(input): Parameters<UpdateContentInput>,
    ) -> Result<CallToolResult, McpError> {
        debug!(previous_hash = %input.previous_hash, "Processing update_content request");
        tool_outcome(self.run_update_content(input).await)
    }

    /// Delete content and set visibility to Offline.
    ///
    /// Removes content bytes and marks the manifest as offline.
    /// The manifest is preserved for provenance tracking.
    #[tool(
        description = "Delete content from your node. Removes the content bytes and sets visibility to Offline. The manifest is preserved for provenance tracking. Only works on content you own."
    )]
    async fn delete_content(
        &self,
        Parameters(input): Parameters<DeleteContentInput>,
    ) -> Result<CallToolResult, McpError> {
        debug!(hash = %input.hash, "Processing delete_content request");
        tool_outcome(self.run_delete_content(input).await)
    }

    /// Change content visibility.
    ///
    /// Sets the visibility of content to private, unlisted, or shared.
    #[tool(
        description = "Change the visibility of your content. Options: 'private' (local only), 'unlisted' (served if hash known), 'shared' (announced to network). Only works on content you own."
    )]
    async fn set_visibility(
        &self,
        Parameters(input): Parameters<SetVisibilityInput>,
    ) -> Result<CallToolResult, McpError> {
        debug!(hash = %input.hash, visibility = %input.visibility, "Processing set_visibility request");

        let hash = match string_to_hash(&input.hash) {
            Ok(h) => h,
            Err(e) => return Ok(tool_error(&NodalyncMcpError::InvalidHash(e))),
        };

        let visibility = match parse_visibility(&input.visibility) {
            Some(v) => v,
            None => {
                return Ok(tool_error(&NodalyncMcpError::Internal(format!(
                    "Invalid visibility '{}'. Use 'private', 'unlisted', or 'shared'.",
                    input.visibility
                ))));
            }
        };

        let mut ops = self.ops.lock().await;

        // Get previous visibility
        let manifest = match ops.get_content_manifest(&hash) {
            Ok(Some(m)) => m,
            Ok(None) => {
                return Ok(tool_error(&NodalyncMcpError::NotFound(hash_to_string(
                    &hash,
                ))));
            }
            Err(e) => return Ok(tool_error(&NodalyncMcpError::Ops(e))),
        };

        let previous_visibility = format!("{:?}", manifest.visibility);

        // Set new visibility (includes ownership check)
        ops.set_content_visibility(&hash, visibility)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let output = SetVisibilityOutput {
            hash: hash_to_string(&hash),
            visibility: format!("{:?}", visibility),
            previous_visibility,
        };

        info!(
            hash = %output.hash,
            from = %output.previous_visibility,
            to = %output.visibility,
            "Visibility changed"
        );

        let json = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    /// List all versions of a content item.
    ///
    /// Returns the version history including timestamps, visibility, and pricing.
    #[tool(
        description = "List all versions of a content item. Accepts any version's hash and returns the full version history. Shows version numbers, timestamps, visibility, and pricing for each version."
    )]
    async fn list_versions(
        &self,
        Parameters(input): Parameters<ListVersionsInput>,
    ) -> Result<CallToolResult, McpError> {
        debug!(hash = %input.hash, "Processing list_versions request");

        let hash = match string_to_hash(&input.hash) {
            Ok(h) => h,
            Err(e) => return Ok(tool_error(&NodalyncMcpError::InvalidHash(e))),
        };

        let ops = self.ops.lock().await;

        // Load manifest to find version root
        let manifest = match ops.get_content_manifest(&hash) {
            Ok(Some(m)) => m,
            Ok(None) => {
                return Ok(tool_error(&NodalyncMcpError::NotFound(hash_to_string(
                    &hash,
                ))));
            }
            Err(e) => return Ok(tool_error(&NodalyncMcpError::Ops(e))),
        };

        let root_hash = manifest.version.root;

        // Get all versions
        let versions = ops
            .get_content_versions(&root_hash)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let version_entries: Vec<VersionEntry> = versions
            .iter()
            .map(|v| VersionEntry {
                hash: hash_to_string(&v.hash),
                version_number: v.number,
                timestamp: v.timestamp,
                visibility: format!("{:?}", v.visibility),
                price_hbar: tinybars_to_hbar(v.price),
            })
            .collect();

        let total_versions = version_entries.len() as u32;

        let output = ListVersionsOutput {
            root_hash: hash_to_string(&root_hash),
            versions: version_entries,
            total_versions,
        };

        let json = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    /// Get earnings information for published content.
    ///
    /// Returns revenue and query statistics for content you own.
    #[tool(
        description = "Get earnings information for your published content. Shows total queries, revenue, and pricing for each content item. Optionally filter by content type (L0, L1, L2, L3)."
    )]
    async fn get_earnings(
        &self,
        Parameters(input): Parameters<GetEarningsInput>,
    ) -> Result<CallToolResult, McpError> {
        debug!(limit = ?input.limit, content_type = ?input.content_type, "Processing get_earnings request");

        tool_outcome(self.run_get_earnings(input).await)
    }
}

/// Knowledge resource URI prefix.
const KNOWLEDGE_URI_PREFIX: &str = "knowledge://";

/// Steps of the `query_knowledge` tool. Each returns the tool's error response
/// (`Err(CallToolResult)`) when the query cannot go on.
impl NodalyncMcpServer {
    /// Look up the content, pay for it as needed and query it.
    async fn run_query_knowledge(
        &self,
        input: &QueryKnowledgeInput,
    ) -> Result<QueryKnowledgeOutput, CallToolResult> {
        // Parse query as hash
        let hash = string_to_hash(&input.query)
            .map_err(|e| tool_error(&NodalyncMcpError::InvalidHash(e)))?;

        // Track all payment operations for the response
        let mut payment_details = empty_payment_details();

        // Get preview to check price and find provider
        let mut ops = self.ops.lock().await;
        let preview = ops.preview_content(&hash).await.map_err(|e| {
            warn!(hash = %hash_to_string(&hash), error = %e, "Content not found");
            tool_error(&NodalyncMcpError::Ops(e))
        })?;
        let price = preview.manifest.economics.price;

        self.prepare_query_payment(&mut ops, &preview, input.budget_hbar, &mut payment_details)
            .await?;

        // Execute query with automatic retry on channel requirement
        let response = self
            .execute_query(&mut ops, &hash, price, &mut payment_details)
            .await?;

        // Record payment receipt
        payment_details.payment_receipt_id = Some(hash_to_string(&response.receipt.payment_id));

        // Get updated Hedera account balance (live on-chain balance)
        self.record_account_balance(&mut payment_details).await;

        let output = build_query_output(
            response,
            price,
            payment_details,
            self.budget.remaining_hbar(),
        );
        info!(
            hash = %hash_to_string(&hash),
            cost_hbar = output.cost_hbar,
            remaining_hbar = self.budget.remaining_hbar(),
            "Query completed successfully"
        );
        Ok(output)
    }

    /// Check the budgets, then for paid content: auto-deposit, auto-open a payment
    /// channel with the provider, and reserve the price from the session budget.
    async fn prepare_query_payment(
        &self,
        ops: &mut DefaultNodeOperations,
        preview: &nodalync_ops::PreviewResponse,
        budget_hbar: Option<f64>,
        payment_details: &mut PaymentDetails,
    ) -> Result<(), CallToolResult> {
        let price = preview.manifest.economics.price;
        self.check_query_budget(price, budget_hbar)?;
        if price == 0 {
            return Ok(());
        }

        // === AUTO-DEPOSIT IF NEEDED ===
        self.auto_deposit_for_query(price, payment_details).await;

        // === AUTO-OPEN PAYMENT CHANNEL IF NEEDED ===
        self.ensure_provider_channel(ops, preview, payment_details)
            .await;

        // === RESERVE BUDGET ===
        self.reserve_budget(price)
    }

    /// The price must fit the per-query limit (the session's remaining budget by
    /// default) and the session budget.
    fn check_query_budget(
        &self,
        price: u64,
        budget_hbar: Option<f64>,
    ) -> Result<(), CallToolResult> {
        // Check per-query budget limit
        let max_budget = budget_hbar
            .map(hbar_to_tinybars)
            .unwrap_or(self.budget.remaining());
        if price > max_budget {
            return Err(tool_error(&NodalyncMcpError::BudgetExceeded {
                cost: price,
                remaining: max_budget,
            }));
        }

        // Check session budget
        if !self.budget.can_afford(price) {
            return Err(tool_error(&NodalyncMcpError::BudgetExceeded {
                cost: price,
                remaining: self.budget.remaining(),
            }));
        }
        Ok(())
    }

    /// Spend `price` from the session budget, failing if it no longer fits.
    fn reserve_budget(&self, price: u64) -> Result<(), CallToolResult> {
        if self.budget.spend(price).is_none() {
            return Err(tool_error(&NodalyncMcpError::BudgetExceeded {
                cost: price,
                remaining: self.budget.remaining(),
            }));
        }
        Ok(())
    }

    /// For paid content, ensure we have enough in the settlement contract: deposit
    /// 10 HBAR when the balance is below max(price × 10, 1 HBAR). A failed deposit is logged.
    async fn auto_deposit_for_query(&self, price: u64, payment_details: &mut PaymentDetails) {
        let Some(ref settlement) = self.settlement else {
            return;
        };
        // Check current balance
        let Ok(balance) = settlement.get_balance().await else {
            return;
        };
        // Minimum deposit: max of (price * 10, 1 HBAR) to avoid frequent deposits
        let min_required = (price * 10).max(hbar_to_tinybars(1.0));
        if balance >= min_required {
            return;
        }

        let deposit_amount = hbar_to_tinybars(10.0); // Deposit 10 HBAR
        info!(
            current_balance_hbar = tinybars_to_hbar(balance),
            deposit_hbar = 10.0,
            "Auto-depositing HBAR for payment operations"
        );

        match settlement.deposit(deposit_amount).await {
            Ok(tx_id) => {
                info!(tx_id = %tx_id, "Auto-deposit successful");
                payment_details.deposit_tx_id = Some(tx_id.to_string());
                payment_details.deposit_amount_hbar = Some(10.0);
            }
            Err(e) => {
                warn!(error = %e, "Auto-deposit failed, continuing anyway");
            }
        }
    }

    /// Open a payment channel with the content's provider unless one is open: by the
    /// provider's libp2p peer ID when the preview has one, else by the owner's Nodalync
    /// peer ID when it is known.
    async fn ensure_provider_channel(
        &self,
        ops: &mut DefaultNodeOperations,
        preview: &nodalync_ops::PreviewResponse,
        payment_details: &mut PaymentDetails,
    ) {
        let libp2p_peer_opt = preview
            .provider_peer_id
            .as_ref()
            .and_then(|s| s.parse::<LibP2pPeerId>().ok());

        if let Some(libp2p_peer) = libp2p_peer_opt {
            self.ensure_libp2p_channel(ops, libp2p_peer, payment_details)
                .await;
        } else if preview.manifest.owner != UNKNOWN_PEER_ID {
            // Fallback to Nodalync peer ID
            ensure_direct_channel(ops, preview.manifest.owner, payment_details).await;
        }
    }

    /// Open a channel with a provider known by libp2p peer ID, unless one is open.
    async fn ensure_libp2p_channel(
        &self,
        ops: &mut DefaultNodeOperations,
        libp2p_peer: LibP2pPeerId,
        payment_details: &mut PaymentDetails,
    ) {
        // Check if we have an existing channel
        let existing_nodalync_id = self
            .network
            .as_ref()
            .and_then(|n| n.nodalync_peer_id(&libp2p_peer));

        let has_channel = existing_nodalync_id
            .map(|id| ops.has_open_channel(&id).unwrap_or(false))
            .unwrap_or(false);

        if has_channel {
            payment_details.provider_peer_id = existing_nodalync_id.map(|p| peer_id_to_string(&p));
            return;
        }

        // Open channel with on-chain funding
        let channel_deposit = hbar_to_tinybars(1.0);
        info!(
            provider_libp2p = %libp2p_peer,
            deposit_hbar = 1.0,
            "Auto-opening payment channel"
        );

        match ops
            .open_payment_channel_to_libp2p(libp2p_peer, channel_deposit)
            .await
        {
            Ok((channel, remote_nodalync_id)) => {
                info!(
                    channel_id = %hash_to_string(&channel.channel_id),
                    provider = %peer_id_to_string(&remote_nodalync_id),
                    "Payment channel opened successfully"
                );
                record_opened_channel(payment_details, &channel, Some(&remote_nodalync_id));
            }
            Err(e) => {
                warn!(error = %e, "Failed to open payment channel");
            }
        }
    }

    /// Query the content. If the provider requires a payment channel, open one with
    /// the peer it names and retry once. A failed query refunds the reserved price.
    async fn execute_query(
        &self,
        ops: &mut DefaultNodeOperations,
        hash: &nodalync_crypto::Hash,
        price: u64,
        payment_details: &mut PaymentDetails,
    ) -> Result<nodalync_ops::QueryResponse, CallToolResult> {
        match ops.query_content(hash, price, None).await {
            Ok(r) => Ok(r),
            Err(nodalync_ops::OpsError::ChannelRequiredWithPeerInfo {
                nodalync_peer_id,
                libp2p_peer_id,
            }) => {
                info!("Server requires payment channel - auto-opening and retrying");
                open_required_channel(ops, nodalync_peer_id, libp2p_peer_id, payment_details).await;

                // Retry query
                ops.query_content(hash, price, None)
                    .await
                    .map_err(|e| self.refund_query(price, e))
            }
            Err(e) => Err(self.refund_query(price, e)),
        }
    }

    /// Refund a paid query's reserved price and return the error response for `e`.
    fn refund_query(&self, price: u64, e: nodalync_ops::OpsError) -> CallToolResult {
        self.refund_paid(price);
        tool_error(&NodalyncMcpError::Ops(e))
    }

    /// Record the live Hedera account balance, when settlement is configured and it can be read.
    async fn record_account_balance(&self, payment_details: &mut PaymentDetails) {
        if let Some(ref settlement) = self.settlement {
            if let Ok(balance) = settlement.get_account_balance().await {
                payment_details.hedera_account_balance_hbar = Some(tinybars_to_hbar(balance));
            }
        }
    }
}

/// Payment details before any payment operation has happened.
fn empty_payment_details() -> PaymentDetails {
    PaymentDetails {
        channel_opened: false,
        channel_id: None,
        channel_tx_id: None,
        deposit_tx_id: None,
        deposit_amount_hbar: None,
        provider_peer_id: None,
        payment_receipt_id: None,
        hedera_account_balance_hbar: None,
    }
}

/// Record a newly opened channel (and the provider, when given) in the payment details.
fn record_opened_channel(
    payment_details: &mut PaymentDetails,
    channel: &nodalync_types::Channel,
    provider: Option<&NodalyncPeerId>,
) {
    payment_details.channel_opened = true;
    payment_details.channel_id = Some(hash_to_string(&channel.channel_id));
    if let Some(provider) = provider {
        payment_details.provider_peer_id = Some(peer_id_to_string(provider));
    }
    // Get on-chain tx ID if available
    if let Some(tx_id) = &channel.funding_tx_id {
        payment_details.channel_tx_id = Some(tx_id.clone());
    }
}

/// Open a channel with a provider known by Nodalync peer ID, unless one is open.
async fn ensure_direct_channel(
    ops: &mut DefaultNodeOperations,
    peer: NodalyncPeerId,
    payment_details: &mut PaymentDetails,
) {
    payment_details.provider_peer_id = Some(peer_id_to_string(&peer));
    if ops.has_open_channel(&peer).unwrap_or(false) {
        return;
    }

    let channel_deposit = hbar_to_tinybars(1.0);
    match ops.open_payment_channel(&peer, channel_deposit).await {
        Ok(channel) => {
            record_opened_channel(payment_details, &channel, None);
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        Err(e) => {
            warn!(error = %e, "Failed to open payment channel");
        }
    }
}

/// Open the channel a provider asked for: by libp2p peer ID when given (a failure is
/// ignored), else by Nodalync peer ID.
async fn open_required_channel(
    ops: &mut DefaultNodeOperations,
    nodalync_peer_id: Option<NodalyncPeerId>,
    libp2p_peer_id: Option<String>,
    payment_details: &mut PaymentDetails,
) {
    let channel_deposit = hbar_to_tinybars(1.0);

    // Try libp2p peer ID first
    if let Some(libp2p_str) = &libp2p_peer_id {
        let Ok(libp2p_peer) = libp2p_str.parse::<LibP2pPeerId>() else {
            return;
        };
        if let Ok((channel, remote_id)) = ops
            .open_payment_channel_to_libp2p(libp2p_peer, channel_deposit)
            .await
        {
            record_opened_channel(payment_details, &channel, Some(&remote_id));
        }
    } else if let Some(nodalync_id) = &nodalync_peer_id {
        if let Ok(channel) = ops.open_payment_channel(nodalync_id, channel_deposit).await {
            record_opened_channel(payment_details, &channel, Some(nodalync_id));
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }
}

/// The tool output for a completed query: content, its root sources, its full
/// provenance (roots, then derived-from hashes not already listed), and payment
/// details when it cost anything.
fn build_query_output(
    response: nodalync_ops::QueryResponse,
    price: u64,
    payment_details: PaymentDetails,
    remaining_budget_hbar: f64,
) -> QueryKnowledgeOutput {
    let sources: Vec<String> = response
        .manifest
        .provenance
        .root_l0l1
        .iter()
        .map(|e| hash_to_string(&e.hash))
        .collect();

    let mut provenance: Vec<String> = sources.clone();
    for h in &response.manifest.provenance.derived_from {
        let hash_str = hash_to_string(h);
        if !provenance.contains(&hash_str) {
            provenance.push(hash_str);
        }
    }

    QueryKnowledgeOutput {
        content: String::from_utf8_lossy(&response.content).to_string(),
        hash: hash_to_string(&response.manifest.hash),
        sources,
        provenance,
        cost_hbar: tinybars_to_hbar(price),
        remaining_budget_hbar,
        // Only include payment details if there was a cost
        payment: (price > 0).then_some(payment_details),
    }
}

/// Add sources found by a live network search (which also caches the results).
async fn add_network_sources(
    ops: &mut DefaultNodeOperations,
    topic: Option<&str>,
    limit: u32,
    seen_hashes: &mut std::collections::HashSet<nodalync_crypto::Hash>,
    sources: &mut Vec<SourceInfo>,
) {
    let query = topic.unwrap_or("");
    let Ok(results) = ops.search_network(query, None, limit).await else {
        return;
    };
    for r in results {
        if seen_hashes.insert(r.hash) {
            sources.push(SourceInfo {
                hash: hash_to_string(&r.hash),
                title: r.title.clone(),
                price_hbar: tinybars_to_hbar(r.price),
                preview: network_result_preview(&r.l1_summary),
                topics: r.l1_summary.primary_topics.clone(),
                peer_id: r.publisher_peer_id.clone(),
            });
        }
    }
}

/// A network result's preview: its first two mentions, else its summary text.
fn network_result_preview(l1_summary: &nodalync_types::L1Summary) -> String {
    if l1_summary.preview_mentions.is_empty() {
        return l1_summary.summary.clone();
    }
    l1_summary
        .preview_mentions
        .iter()
        .take(2)
        .map(|m| m.content.clone())
        .collect::<Vec<_>>()
        .join("; ")
}

/// Add shared local manifests that match `topic` (every manifest when there is none).
fn add_local_sources(
    ops: &DefaultNodeOperations,
    topic: Option<&str>,
    limit: u32,
    seen_hashes: &mut std::collections::HashSet<nodalync_crypto::Hash>,
    sources: &mut Vec<SourceInfo>,
) {
    let filter = ManifestFilter::new()
        .with_visibility(Visibility::Shared)
        .limit(limit);

    let Ok(manifests) = ops.state.manifests.list(filter) else {
        return;
    };
    for m in manifests {
        // Apply topic filter if provided
        if seen_hashes.insert(m.hash) && manifest_matches_topic(&m, topic) {
            sources.push(local_source(&m));
        }
    }
}

/// Whether a manifest's title, tags or description contain `topic` (case-insensitive).
/// With no topic, every manifest matches.
fn manifest_matches_topic(m: &nodalync_types::Manifest, topic: Option<&str>) -> bool {
    let Some(topic) = topic else {
        return true;
    };
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
}

/// A source for a local manifest; its owner is the peer to query.
fn local_source(m: &nodalync_types::Manifest) -> SourceInfo {
    let preview = m
        .metadata
        .description
        .clone()
        .unwrap_or_else(|| format!("{} bytes of content", m.metadata.content_size));

    // For local manifests, use the owner as peer_id
    SourceInfo {
        hash: hash_to_string(&m.hash),
        title: m.metadata.title.clone(),
        price_hbar: tinybars_to_hbar(m.economics.price),
        preview,
        topics: m.metadata.tags.clone(),
        peer_id: Some(peer_id_to_string(&m.owner)),
    }
}

/// Add cached network announcements matching `topic`.
fn add_cached_announcement_sources(
    ops: &DefaultNodeOperations,
    topic: Option<&str>,
    limit: u32,
    seen_hashes: &mut std::collections::HashSet<nodalync_crypto::Hash>,
    sources: &mut Vec<SourceInfo>,
) {
    let query = topic.unwrap_or("");
    let announcements = ops.state.search_announcements(query, None, limit);

    for announce in announcements {
        if seen_hashes.insert(announce.hash) {
            sources.push(SourceInfo {
                hash: hash_to_string(&announce.hash),
                title: announce.title.clone(),
                price_hbar: tinybars_to_hbar(announce.price),
                preview: announcement_preview(&announce.l1_summary),
                topics: vec![],
                peer_id: announce.publisher_peer_id.clone(),
            });
        }
    }
}

/// A cached announcement's preview: all its previewed mentions, else its mention count.
fn announcement_preview(l1_summary: &nodalync_types::L1Summary) -> String {
    if l1_summary.preview_mentions.is_empty() {
        return format!("{} mentions extracted", l1_summary.mention_count);
    }
    l1_summary
        .preview_mentions
        .iter()
        .map(|m| m.content.clone())
        .collect::<Vec<_>>()
        .join("; ")
}

/// Steps of the `open_channel` tool.
impl NodalyncMcpServer {
    /// Open a channel with a peer given by libp2p peer ID (preferred) or Nodalync peer ID.
    async fn run_open_channel(
        &self,
        input: OpenChannelInput,
    ) -> Result<OpenChannelOutput, CallToolResult> {
        check_channel_deposit(input.deposit_hbar)?;

        let deposit_tinybars = hbar_to_tinybars(input.deposit_hbar);
        let mut ops = self.ops.lock().await;

        // Check if network is available
        if !ops.has_network() {
            warn!("Cannot open channel: network not available");
            return Err(tool_error(&NodalyncMcpError::Internal(
                "Network not available. Ensure MCP server is started with --enable-network"
                    .to_string(),
            )));
        }

        // Try to parse as libp2p peer ID first (starts with "12D3Koo" or similar)
        if let Ok(libp2p_peer) = input.peer_id.parse::<LibP2pPeerId>() {
            return open_channel_via_libp2p(&mut ops, libp2p_peer, &input, deposit_tinybars).await;
        }

        // Try to parse as Nodalync peer ID (20 bytes base58, starts with "ndl")
        let peer_id = parse_nodalync_peer_id(
            &input.peer_id,
            "Invalid peer ID format. Use the peer_id from list_sources (starts with 12D3Koo).",
            |len| {
                format!("Invalid peer ID. Expected libp2p peer ID (from list_sources, starts with 12D3Koo) or Nodalync peer ID (20 bytes). Got {} bytes.", len)
            },
        )?;
        open_channel_via_nodalync(&mut ops, peer_id, input, deposit_tinybars).await
    }
}

/// Steps of the `close_channel` and `close_all_channels` tools.
impl NodalyncMcpServer {
    /// Close the channel with one peer: cooperatively, else by dispute when the peer
    /// is unresponsive.
    async fn run_close_channel(
        &self,
        input: CloseChannelInput,
    ) -> Result<crate::tools::CloseChannelOutput, ToolExit> {
        // Parse peer ID - accept both libp2p (12D3KooW...) and Nodalync (ndl...) formats
        let peer_id = self.resolve_channel_peer(&input.peer_id)?;

        let mut ops = self.ops.lock().await;

        // Get channel info before closing
        let channel = ops
            .state
            .channels
            .get(&peer_id)
            .map_err(|e| McpError::internal_error(format!("Failed to get channel: {}", e), None))?
            .ok_or_else(|| {
                tool_error(&NodalyncMcpError::Internal(
                    "No channel exists with this peer.".to_string(),
                ))
            })?;

        let final_balance = channel.my_balance;

        info!(
            peer_id = %peer_id_to_string(&peer_id),
            balance_tinybars = final_balance,
            "Closing payment channel"
        );

        // Get the private key for signing
        let private_key = ops.private_key().cloned().ok_or_else(|| {
            tool_error(&NodalyncMcpError::Internal(
                "Private key not available. Cannot sign channel close.".to_string(),
            ))
        })?;

        // Attempt cooperative close with proper signature
        let result = ops.close_payment_channel(&peer_id, &private_key).await;
        drop(ops); // Release lock before async Hedera calls

        // Get updated Hedera account balance
        let hedera_balance = self.hedera_balance_hbar().await;

        let closed = ClosedChannel {
            peer_id,
            peer_id_str: input.peer_id,
            final_balance,
            hedera_balance,
        };
        self.close_channel_output(result, closed, &private_key)
            .await
    }

    /// The Nodalync peer ID for a peer given by libp2p peer ID (looked up through the
    /// network) or by Nodalync peer ID.
    fn resolve_channel_peer(&self, peer_id: &str) -> Result<NodalyncPeerId, ToolExit> {
        if !peer_id.starts_with("12D3KooW") {
            // Nodalync peer ID (ndl...)
            return Ok(parse_nodalync_peer_id(
                peer_id,
                "Invalid peer ID format. Use the peer_id from status or channel details.",
                |len| {
                    format!(
                        "Invalid peer ID. Expected Nodalync peer ID (20 bytes base58, starts with 'ndl') \
                         or libp2p peer ID (starts with '12D3KooW'). Got {} bytes.",
                        len
                    )
                },
            )?);
        }

        // libp2p peer ID - look up Nodalync peer ID from network
        let Some(ref network) = self.network else {
            return Err(tool_error(&NodalyncMcpError::Internal(
                "Network not available. Cannot look up Nodalync peer ID from libp2p ID."
                    .to_string(),
            ))
            .into());
        };

        let libp2p_peer: LibP2pPeerId = peer_id
            .parse()
            .map_err(|_| McpError::internal_error("Invalid libp2p peer ID format", None))?;

        // Look up the Nodalync peer ID
        network.nodalync_peer_id(&libp2p_peer).ok_or_else(|| {
            tool_error(&NodalyncMcpError::InvalidHash(
                "No Nodalync peer ID mapping found for this libp2p peer. \
                 The peer may not have an open channel with you."
                    .to_string(),
            ))
            .into()
        })
    }

    /// The tool output for a close attempt; an unresponsive peer gets a dispute.
    async fn close_channel_output(
        &self,
        result: nodalync_ops::OpsResult<nodalync_ops::CloseResult>,
        closed: ClosedChannel,
        private_key: &nodalync_crypto::PrivateKey,
    ) -> Result<crate::tools::CloseChannelOutput, ToolExit> {
        use nodalync_ops::CloseResult;

        match result {
            Ok(CloseResult::Success {
                transaction_id,
                final_balances,
            }) => {
                let output = closed.output("cooperative", Some(transaction_id), final_balances.0);
                info!(
                    peer_id = %closed.peer_id_str,
                    close_method = "cooperative",
                    tx_id = ?output.transaction_id,
                    "Channel closed successfully"
                );
                Ok(output)
            }
            Ok(CloseResult::SuccessOffChain { final_balances }) => {
                info!(
                    peer_id = %closed.peer_id_str,
                    close_method = "off_chain",
                    "Channel closed off-chain"
                );
                Ok(closed.output("off_chain", None, final_balances.0))
            }
            Ok(CloseResult::PeerUnresponsive { .. }) => {
                self.dispute_unresponsive_peer(closed, private_key).await
            }
            Ok(CloseResult::OnChainFailed { error }) => {
                warn!(error = %error, "On-chain close failed");
                Err(tool_error(&NodalyncMcpError::Internal(format!(
                    "On-chain settlement failed: {}",
                    error
                )))
                .into())
            }
            Err(e) => {
                warn!(error = %e, "Failed to close channel");
                Err(tool_error(&NodalyncMcpError::Internal(format!(
                    "Failed to close channel: {}",
                    e
                )))
                .into())
            }
        }
    }

    /// Peer didn't respond - initiate dispute.
    async fn dispute_unresponsive_peer(
        &self,
        closed: ClosedChannel,
        private_key: &nodalync_crypto::PrivateKey,
    ) -> Result<crate::tools::CloseChannelOutput, ToolExit> {
        info!(
            peer_id = %closed.peer_id_str,
            "Peer unresponsive, initiating dispute"
        );

        let mut ops = self.ops.lock().await;
        match ops
            .dispute_payment_channel(&closed.peer_id, private_key)
            .await
        {
            Ok(tx_id) => {
                let output = closed.output("dispute_initiated", Some(tx_id), closed.final_balance);
                info!(
                    peer_id = %closed.peer_id_str,
                    close_method = "dispute_initiated",
                    tx_id = ?output.transaction_id,
                    "Dispute initiated - settlement in 24 hours"
                );
                Ok(output)
            }
            Err(e) => {
                warn!(error = %e, "Failed to initiate dispute");
                Err(tool_error(&NodalyncMcpError::Internal(format!(
                    "Peer unresponsive and dispute failed: {}",
                    e
                )))
                .into())
            }
        }
    }

    /// Close every open channel, disputing for unresponsive peers.
    async fn run_close_all_channels(&self) -> Result<CloseAllChannelsOutput, ToolExit> {
        // Get list of open channels and private key
        let (channels, private_key) = {
            let ops = self.ops.lock().await;
            let channels = ops.state.channels.list_open().unwrap_or_default();
            let private_key = ops.private_key().cloned();
            (channels, private_key)
        };

        let private_key = private_key.ok_or_else(|| {
            tool_error(&NodalyncMcpError::Internal(
                "Private key not available. Cannot sign channel closes.".to_string(),
            ))
        })?;

        let channels_processed = channels.len() as u32;

        info!(
            channels_count = channels_processed,
            "Closing all payment channels"
        );

        // Close each channel
        let mut tally = CloseTally::default();
        for (peer_id, channel) in channels {
            let was_funded_on_chain = channel.funding_tx_id.is_some();
            let result = self
                .close_one_channel(&peer_id, was_funded_on_chain, &private_key)
                .await;
            tally.record(result);
        }

        // Get updated Hedera balance
        let hedera_balance = self.hedera_balance_hbar().await;

        let output = CloseAllChannelsOutput {
            channels_processed,
            channels_closed: tally.closed,
            disputes_initiated: tally.disputes,
            channels_failed: tally.failed,
            results: tally.results,
            hedera_account_balance_hbar: hedera_balance,
        };

        info!(
            processed = channels_processed,
            closed = output.channels_closed,
            disputes = output.disputes_initiated,
            failed = output.channels_failed,
            "Close all channels complete"
        );
        Ok(output)
    }

    /// Close one channel cooperatively (5 second timeout); if that does not succeed,
    /// dispute it, or clear it if it was never funded on-chain.
    async fn close_one_channel(
        &self,
        peer_id: &NodalyncPeerId,
        was_funded_on_chain: bool,
        private_key: &nodalync_crypto::PrivateKey,
    ) -> ChannelCloseResult {
        let peer_id_str = peer_id_to_string(peer_id);

        // Try cooperative close with timeout
        let close_result = {
            let mut ops = self.ops.lock().await;
            tokio::time::timeout(
                Duration::from_secs(5),
                ops.close_payment_channel(peer_id, private_key),
            )
            .await
        };

        match close_result {
            Ok(Ok(nodalync_ops::CloseResult::Success { transaction_id, .. })) => {
                info!(peer_id = %peer_id_str, "Channel closed cooperatively");
                close_entry(&peer_id_str, "cooperative", Some(transaction_id), None)
            }
            Ok(Ok(nodalync_ops::CloseResult::SuccessOffChain { .. })) => {
                info!(peer_id = %peer_id_str, "Channel closed off-chain");
                close_entry(&peer_id_str, "off_chain", None, None)
            }
            Ok(Ok(nodalync_ops::CloseResult::OnChainFailed { error })) => {
                // On-chain failed - try dispute
                self.dispute_or_clear(peer_id, was_funded_on_chain, private_key, Some(error))
                    .await
            }
            // Peer unresponsive or error - initiate dispute
            _ => {
                self.dispute_or_clear(peer_id, was_funded_on_chain, private_key, None)
                    .await
            }
        }
    }

    /// Dispute a channel that did not close. If the dispute fails and the channel was
    /// never funded on-chain, just remove it from local state.
    ///
    /// `on_chain_error` is the on-chain close failure, when that is why it did not close.
    async fn dispute_or_clear(
        &self,
        peer_id: &NodalyncPeerId,
        was_funded_on_chain: bool,
        private_key: &nodalync_crypto::PrivateKey,
        on_chain_error: Option<String>,
    ) -> ChannelCloseResult {
        let peer_id_str = peer_id_to_string(peer_id);
        let dispute_result = {
            let mut ops = self.ops.lock().await;
            ops.dispute_payment_channel(peer_id, private_key).await
        };

        match dispute_result {
            Ok(tx_id) => {
                if on_chain_error.is_none() {
                    info!(
                        peer_id = %peer_id_str,
                        "Dispute initiated for unresponsive peer"
                    );
                }
                close_entry(&peer_id_str, "dispute_initiated", Some(tx_id), None)
            }
            // If the channel was never funded on-chain, just remove it
            Err(_) if !was_funded_on_chain => {
                let mut ops = self.ops.lock().await;
                let _ = ops.state.channels.delete(peer_id);
                info!(
                    peer_id = %peer_id_str,
                    "Cleared unfunded channel from local state"
                );
                close_entry(&peer_id_str, "cleared_unfunded", None, None)
            }
            Err(e) => {
                let error = match on_chain_error {
                    Some(error) => format!(
                        "On-chain close failed: {}. Dispute also failed: {}",
                        error, e
                    ),
                    None => {
                        warn!(
                            peer_id = %peer_id_str,
                            error = %e,
                            "Failed to close or dispute channel"
                        );
                        e.to_string()
                    }
                };
                close_entry(&peer_id_str, "failed", None, Some(error))
            }
        }
    }

    /// The live Hedera account balance in HBAR, when settlement is configured and it can be read.
    async fn hedera_balance_hbar(&self) -> Option<f64> {
        let settlement = self.settlement.as_ref()?;
        settlement
            .get_account_balance()
            .await
            .ok()
            .map(tinybars_to_hbar)
    }
}

/// A channel being closed by the `close_channel` tool.
struct ClosedChannel {
    peer_id: NodalyncPeerId,
    /// The peer ID as the caller gave it.
    peer_id_str: String,
    /// Our balance before closing.
    final_balance: u64,
    hedera_balance: Option<f64>,
}

impl ClosedChannel {
    fn output(
        &self,
        close_method: &str,
        transaction_id: Option<String>,
        final_balance_tinybars: u64,
    ) -> crate::tools::CloseChannelOutput {
        crate::tools::CloseChannelOutput {
            success: true,
            close_method: close_method.to_string(),
            transaction_id,
            final_balance_tinybars,
            peer_id: self.peer_id_str.clone(),
            hedera_account_balance_hbar: self.hedera_balance,
        }
    }
}

/// One channel's result in `close_all_channels`; it succeeded unless the method is "failed".
fn close_entry(
    peer_id: &str,
    close_method: &str,
    transaction_id: Option<String>,
    error: Option<String>,
) -> ChannelCloseResult {
    ChannelCloseResult {
        peer_id: peer_id.to_string(),
        success: close_method != "failed",
        close_method: close_method.to_string(),
        transaction_id,
        error,
    }
}

/// Counts of `close_all_channels` results by outcome.
#[derive(Default)]
struct CloseTally {
    results: Vec<ChannelCloseResult>,
    closed: u32,
    disputes: u32,
    failed: u32,
}

impl CloseTally {
    fn record(&mut self, result: ChannelCloseResult) {
        match result.close_method.as_str() {
            "dispute_initiated" => self.disputes += 1,
            "failed" => self.failed += 1,
            _ => self.closed += 1,
        }
        self.results.push(result);
    }
}

/// Why a tool step stopped early: an error result for the caller, or a protocol error.
enum ToolExit {
    Result(CallToolResult),
    Protocol(McpError),
}

impl From<CallToolResult> for ToolExit {
    fn from(result: CallToolResult) -> Self {
        ToolExit::Result(result)
    }
}

impl From<McpError> for ToolExit {
    fn from(error: McpError) -> Self {
        ToolExit::Protocol(error)
    }
}

/// Steps of the content tools: publish, synthesize, update and delete.
impl NodalyncMcpServer {
    /// Create L0 content, extract its L1 summary and publish it.
    async fn run_publish_content(
        &self,
        input: PublishContentInput,
    ) -> Result<PublishContentOutput, ToolExit> {
        // Validate content
        check_content_size(&input.content)?;

        // Parse visibility
        let visibility = parse_visibility_input(&input.visibility)?;

        let content_bytes = input.content.as_bytes();
        let size_bytes = content_bytes.len();

        // Check for duplicates
        let mut ops = self.ops.lock().await;
        ensure_new_content(&ops, &content_hash(content_bytes))?;

        // Build metadata
        let metadata = publish_metadata(
            &input.title,
            size_bytes,
            input.description,
            input.mime_type,
            input.tags,
        );

        // Create content and extract L1 summary
        let (hash, l1_summary) = create_and_extract(&mut ops, content_bytes, metadata)?;

        // Publish
        let price = input.price_hbar.map(hbar_to_tinybars).unwrap_or(0);

        ops.publish_content(&hash, visibility, price)
            .await
            .map_err(internal_error)?;

        let output = PublishContentOutput {
            hash: hash_to_string(&hash),
            title: input.title,
            content_type: "L0".to_string(),
            visibility: format!("{:?}", visibility),
            price_hbar: tinybars_to_hbar(price),
            size_bytes,
            mentions_extracted: l1_summary.mention_count,
            topics: l1_summary.primary_topics,
        };

        info!(
            hash = %output.hash,
            title = %output.title,
            visibility = %output.visibility,
            "Content published"
        );
        Ok(output)
    }

    /// Derive L3 content from its sources and optionally publish it.
    async fn run_synthesize_content(
        &self,
        input: SynthesizeContentInput,
    ) -> Result<SynthesizeContentOutput, ToolExit> {
        // Validate content
        check_synthesis_input(&input)?;

        // Parse source hashes
        let source_hashes = parse_source_hashes(&input.sources)?;

        let content_bytes = input.content.as_bytes();

        // Build metadata
        let metadata = publish_metadata(
            &input.title,
            content_bytes.len(),
            input.description.clone(),
            None,
            None,
        );

        let mut ops = self.ops.lock().await;

        // Derive content
        let hash = ops
            .derive_content(&source_hashes, content_bytes, metadata)
            .map_err(internal_error)?;

        // Get manifest for provenance info
        let manifest = stored_manifest(&ops, &hash, "Failed to retrieve manifest after derive")?;

        // Optionally publish
        let publication = publish_synthesis(&mut ops, &hash, &input).await?;

        let output = SynthesizeContentOutput {
            hash: hash_to_string(&hash),
            title: input.title,
            content_type: "L3".to_string(),
            sources: input.sources,
            provenance_depth: manifest.provenance.depth,
            root_source_count: manifest.provenance.root_l0l1.len(),
            published: publication.is_some(),
            visibility: publication
                .as_ref()
                .map(|(visibility, _)| visibility.clone()),
            price_hbar: publication.map(|(_, price)| price),
        };

        info!(
            hash = %output.hash,
            sources = output.sources.len(),
            provenance_depth = output.provenance_depth,
            published = output.published,
            "Content synthesized"
        );
        Ok(output)
    }

    /// Create a new version of content, inheriting metadata not overridden.
    async fn run_update_content(
        &self,
        input: UpdateContentInput,
    ) -> Result<UpdateContentOutput, ToolExit> {
        // Validate
        check_content_size(&input.content)?;

        let old_hash = string_to_hash(&input.previous_hash)
            .map_err(|e| tool_error(&NodalyncMcpError::InvalidHash(e)))?;

        let mut ops = self.ops.lock().await;

        // Load old manifest for metadata inheritance
        let old_manifest = manifest_or_error(
            &ops,
            &old_hash,
            NodalyncMcpError::NotFound(format!(
                "Previous version {} not found",
                input.previous_hash
            )),
        )?;

        let content_bytes = input.content.as_bytes();
        let size_bytes = content_bytes.len();

        // Build metadata, inheriting from previous version
        let (metadata, title) =
            inherited_metadata(&old_manifest, input.title, input.description, size_bytes);

        // Create new version
        let new_hash = ops
            .update_content(&old_hash, content_bytes, metadata)
            .map_err(internal_error)?;

        // Get new manifest for version info
        let new_manifest =
            stored_manifest(&ops, &new_hash, "Failed to retrieve manifest after update")?;

        let output = UpdateContentOutput {
            hash: hash_to_string(&new_hash),
            previous_hash: input.previous_hash,
            version_number: new_manifest.version.number,
            title,
            size_bytes,
            visibility: format!("{:?}", new_manifest.visibility),
        };

        info!(
            hash = %output.hash,
            version = output.version_number,
            "Content updated"
        );
        Ok(output)
    }

    /// Remove our content's bytes and mark its manifest Offline.
    async fn run_delete_content(
        &self,
        input: DeleteContentInput,
    ) -> Result<DeleteContentOutput, ToolExit> {
        let hash = string_to_hash(&input.hash)
            .map_err(|e| tool_error(&NodalyncMcpError::InvalidHash(e)))?;

        let mut ops = self.ops.lock().await;

        // Load manifest and verify ownership
        let mut manifest = manifest_or_error(
            &ops,
            &hash,
            NodalyncMcpError::NotFound(hash_to_string(&hash)),
        )?;

        if manifest.owner != ops.peer_id() {
            return Err(tool_error(&NodalyncMcpError::Internal(
                "Cannot delete content you don't own.".to_string(),
            ))
            .into());
        }

        // Delete content bytes
        ops.state.content.delete(&hash).map_err(internal_error)?;

        // Update manifest visibility to Offline
        manifest.visibility = Visibility::Offline;
        ops.state
            .manifests
            .update(&manifest)
            .map_err(internal_error)?;

        let output = DeleteContentOutput {
            hash: hash_to_string(&hash),
            title: manifest.metadata.title.clone(),
            content_removed: true,
            visibility: "Offline".to_string(),
        };

        info!(hash = %output.hash, "Content deleted");
        Ok(output)
    }
}

/// An internal (protocol-level) error carrying `e`'s message.
fn internal_error(e: impl std::fmt::Display) -> McpError {
    McpError::internal_error(e.to_string(), None)
}

/// Content must be non-empty and at most MAX_CONTENT_SIZE bytes.
fn check_content_size(content: &str) -> Result<(), CallToolResult> {
    if content.is_empty() {
        return Err(tool_error(&NodalyncMcpError::EmptyContent));
    }
    if content.len() > MAX_CONTENT_SIZE {
        return Err(tool_error(&NodalyncMcpError::ContentTooLarge {
            size: content.len(),
            max: MAX_CONTENT_SIZE,
        }));
    }
    Ok(())
}

/// Synthesis needs valid content and at least one source.
fn check_synthesis_input(input: &SynthesizeContentInput) -> Result<(), CallToolResult> {
    check_content_size(&input.content)?;
    if input.sources.is_empty() {
        return Err(tool_error(&NodalyncMcpError::Internal(
            "At least one source hash is required for synthesis.".to_string(),
        )));
    }
    Ok(())
}

/// Parse a visibility argument ('private', 'unlisted' or 'shared').
fn parse_visibility_input(visibility: &str) -> Result<Visibility, CallToolResult> {
    parse_visibility(visibility).ok_or_else(|| {
        tool_error(&NodalyncMcpError::Internal(format!(
            "Invalid visibility '{}'. Use 'private', 'unlisted', or 'shared'.",
            visibility
        )))
    })
}

/// Parse each source hash, naming the first invalid one.
fn parse_source_hashes(sources: &[String]) -> Result<Vec<nodalync_crypto::Hash>, CallToolResult> {
    sources
        .iter()
        .map(|s| {
            string_to_hash(s).map_err(|e| {
                tool_error(&NodalyncMcpError::InvalidHash(format!(
                    "Invalid source hash '{}': {}",
                    s, e
                )))
            })
        })
        .collect()
}

/// Refuse content that already has a manifest here.
fn ensure_new_content(
    ops: &DefaultNodeOperations,
    hash: &nodalync_crypto::Hash,
) -> Result<(), CallToolResult> {
    if let Ok(Some(_)) = ops.get_content_manifest(hash) {
        return Err(tool_error(&NodalyncMcpError::ContentAlreadyExists(
            hash_to_string(hash),
        )));
    }
    Ok(())
}

/// Metadata for new content, with the optional description, MIME type and tags.
fn publish_metadata(
    title: &str,
    size_bytes: usize,
    description: Option<String>,
    mime_type: Option<String>,
    tags: Option<Vec<String>>,
) -> nodalync_types::Metadata {
    let mut metadata = nodalync_types::Metadata::new(title.to_string(), size_bytes as u64);
    if let Some(desc) = description {
        metadata = metadata.with_description(desc);
    }
    if let Some(mime) = mime_type {
        metadata = metadata.with_mime_type(mime);
    }
    if let Some(tags) = tags {
        metadata = metadata.with_tags(tags);
    }
    metadata
}

/// Metadata for a new version: the title and description given, else the previous
/// version's; the previous MIME type and tags. Returns the metadata and its title.
fn inherited_metadata(
    old_manifest: &nodalync_types::Manifest,
    title: Option<String>,
    description: Option<String>,
    size_bytes: usize,
) -> (nodalync_types::Metadata, String) {
    let title = title.unwrap_or_else(|| old_manifest.metadata.title.clone());
    let mut metadata = nodalync_types::Metadata::new(title.clone(), size_bytes as u64);

    // Inherit or override description
    let description = description.or_else(|| old_manifest.metadata.description.clone());
    if let Some(desc) = description {
        metadata = metadata.with_description(desc);
    }

    // Inherit mime_type and tags
    if let Some(mime) = &old_manifest.metadata.mime_type {
        metadata = metadata.with_mime_type(mime.clone());
    }
    if !old_manifest.metadata.tags.is_empty() {
        metadata = metadata.with_tags(old_manifest.metadata.tags.clone());
    }
    (metadata, title)
}

/// Create content and extract its L1 summary.
fn create_and_extract(
    ops: &mut DefaultNodeOperations,
    content: &[u8],
    metadata: nodalync_types::Metadata,
) -> Result<(nodalync_crypto::Hash, nodalync_types::L1Summary), McpError> {
    let hash = ops
        .create_content(content, metadata)
        .map_err(internal_error)?;
    let l1_summary = ops.extract_l1_summary(&hash).map_err(internal_error)?;
    Ok((hash, l1_summary))
}

/// The manifest just stored for `hash`; its absence is an internal error (`missing`).
fn stored_manifest(
    ops: &DefaultNodeOperations,
    hash: &nodalync_crypto::Hash,
    missing: &'static str,
) -> Result<nodalync_types::Manifest, McpError> {
    ops.get_content_manifest(hash)
        .map_err(internal_error)?
        .ok_or_else(|| McpError::internal_error(missing, None))
}

/// The manifest for `hash`, or the tool error `not_found` when there is none.
fn manifest_or_error(
    ops: &DefaultNodeOperations,
    hash: &nodalync_crypto::Hash,
    not_found: NodalyncMcpError,
) -> Result<nodalync_types::Manifest, CallToolResult> {
    match ops.get_content_manifest(hash) {
        Ok(Some(m)) => Ok(m),
        Ok(None) => Err(tool_error(&not_found)),
        Err(e) => Err(tool_error(&NodalyncMcpError::Ops(e))),
    }
}

/// Publish synthesized content when asked. Returns its visibility (as text) and price
/// in HBAR when published.
async fn publish_synthesis(
    ops: &mut DefaultNodeOperations,
    hash: &nodalync_crypto::Hash,
    input: &SynthesizeContentInput,
) -> Result<Option<(String, f64)>, ToolExit> {
    if !input.publish.unwrap_or(false) {
        return Ok(None);
    }
    let visibility = parse_visibility_input(&input.visibility)?;
    let price = input.price_hbar.map(hbar_to_tinybars).unwrap_or(0);

    ops.publish_content(hash, visibility, price)
        .await
        .map_err(internal_error)?;

    Ok(Some((format!("{:?}", visibility), tinybars_to_hbar(price))))
}

/// Steps of the `get_earnings` tool and of reading `knowledge://` resources.
impl NodalyncMcpServer {
    /// Earnings of our content that has been queried or has earned anything.
    async fn run_get_earnings(
        &self,
        input: GetEarningsInput,
    ) -> Result<GetEarningsOutput, ToolExit> {
        let limit = input.limit.unwrap_or(20).min(100);
        let ops = self.ops.lock().await;

        let filter = earnings_filter(ops.peer_id(), limit, input.content_type.as_deref())?;
        let manifests = ops.state.manifests.list(filter).map_err(internal_error)?;

        let output = summarize_earnings(manifests);
        info!(
            content_count = output.content_count,
            total_queries = output.total_queries,
            total_revenue_hbar = output.total_revenue_hbar,
            "Earnings retrieved"
        );
        Ok(output)
    }

    /// Read a `knowledge://<hash>` resource: query the content, paying from the budget.
    async fn read_knowledge_resource(&self, uri: &str) -> Result<ReadResourceResult, McpError> {
        debug!(uri = %uri, "Reading knowledge resource");
        let hash = parse_knowledge_uri(uri)?;

        // Get content preview to check price
        let mut ops = self.ops.lock().await;
        let preview = ops
            .preview_content(&hash)
            .await
            .map_err(|e| McpError::invalid_params(format!("Content not found: {}", e), None))?;

        let price = preview.manifest.economics.price;

        // Reserve budget before query
        self.reserve_resource_budget(price)?;

        // Execute query
        let response = ops.query_content(&hash, price, None).await.map_err(|e| {
            // Refund on failure
            self.refund_paid(price);
            McpError::internal_error(format!("Query failed: {}", e), None)
        })?;

        let content_str = String::from_utf8_lossy(&response.content).to_string();

        info!(
            uri = %uri,
            cost_hbar = tinybars_to_hbar(price),
            remaining_hbar = self.budget.remaining_hbar(),
            "Resource read successfully"
        );

        Ok(ReadResourceResult {
            contents: vec![ResourceContents::text(content_str, uri.to_string())],
        })
    }

    /// Spend a paid resource's price from the budget, failing if it does not fit.
    fn reserve_resource_budget(&self, price: u64) -> Result<(), McpError> {
        if price > 0 && self.budget.spend(price).is_none() {
            return Err(McpError::invalid_request(
                format!(
                    "Insufficient budget: content costs {:.6} HBAR but only {:.6} HBAR remaining",
                    tinybars_to_hbar(price),
                    self.budget.remaining_hbar()
                ),
                None,
            ));
        }
        Ok(())
    }

    /// Give a paid query's price back to the budget.
    fn refund_paid(&self, price: u64) {
        if price > 0 {
            self.budget.refund(price);
        }
    }
}

/// The hash in a `knowledge://<hash>` URI.
fn parse_knowledge_uri(uri: &str) -> Result<nodalync_crypto::Hash, McpError> {
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

    string_to_hash(hash_str)
        .map_err(|e| McpError::invalid_params(format!("Invalid content hash: {}", e), None))
}

/// The manifest filter for our content, of one content type when given.
fn earnings_filter(
    peer_id: NodalyncPeerId,
    limit: u32,
    content_type: Option<&str>,
) -> Result<ManifestFilter, CallToolResult> {
    let filter = ManifestFilter::new().with_owner(peer_id).limit(limit);
    let Some(ct_str) = content_type else {
        return Ok(filter);
    };
    parse_content_type(ct_str)
        .map(|ct| filter.with_content_type(ct))
        .ok_or_else(|| {
            tool_error(&NodalyncMcpError::Internal(format!(
                "Invalid content type '{}'. Use L0, L1, L2, or L3.",
                ct_str
            )))
        })
}

/// Earnings of each manifest that has been queried or has earned anything, with totals.
fn summarize_earnings(manifests: Vec<nodalync_types::Manifest>) -> GetEarningsOutput {
    let mut items: Vec<ContentEarnings> = Vec::new();
    let mut total_revenue: u64 = 0;
    let mut total_queries: u64 = 0;

    for m in manifests {
        if m.economics.total_queries > 0 || m.economics.total_revenue > 0 {
            total_revenue += m.economics.total_revenue;
            total_queries += m.economics.total_queries;

            items.push(ContentEarnings {
                hash: hash_to_string(&m.hash),
                title: m.metadata.title.clone(),
                content_type: format!("{:?}", m.content_type),
                total_queries: m.economics.total_queries,
                total_revenue_hbar: tinybars_to_hbar(m.economics.total_revenue),
                price_hbar: tinybars_to_hbar(m.economics.price),
                visibility: format!("{:?}", m.visibility),
            });
        }
    }

    GetEarningsOutput {
        content_count: items.len() as u32,
        items,
        total_revenue_hbar: tinybars_to_hbar(total_revenue),
        total_queries,
    }
}

/// Validate minimum deposit (100 HBAR = 10,000,000,000 tinybars).
fn check_channel_deposit(deposit_hbar: f64) -> Result<(), CallToolResult> {
    const MIN_DEPOSIT_HBAR: f64 = 100.0;
    if deposit_hbar < MIN_DEPOSIT_HBAR {
        warn!(
            deposit = deposit_hbar,
            minimum = MIN_DEPOSIT_HBAR,
            "Deposit below minimum"
        );
        return Err(tool_error(&NodalyncMcpError::Internal(format!(
            "Deposit {} HBAR is below minimum of {} HBAR. Payment channels require at least 100 HBAR deposit.",
            deposit_hbar, MIN_DEPOSIT_HBAR
        ))));
    }
    Ok(())
}

/// Parse a base58 Nodalync peer ID (20 bytes), with the caller's error messages
/// for a string that is not base58 and for a wrong length.
fn parse_nodalync_peer_id(
    peer_id: &str,
    invalid_format: &str,
    wrong_length: impl Fn(usize) -> String,
) -> Result<NodalyncPeerId, CallToolResult> {
    let peer_bytes = bs58::decode(peer_id)
        .into_vec()
        .map_err(|_| tool_error(&NodalyncMcpError::InvalidHash(invalid_format.to_string())))?;

    let peer_arr: [u8; 20] = peer_bytes.as_slice().try_into().map_err(|_| {
        tool_error(&NodalyncMcpError::InvalidHash(wrong_length(
            peer_bytes.len(),
        )))
    })?;
    Ok(nodalync_crypto::PeerId(peer_arr))
}

/// Open a channel with a peer known by libp2p peer ID.
async fn open_channel_via_libp2p(
    ops: &mut DefaultNodeOperations,
    libp2p_peer: LibP2pPeerId,
    input: &OpenChannelInput,
    deposit_tinybars: u64,
) -> Result<OpenChannelOutput, CallToolResult> {
    info!(
        libp2p_peer = %libp2p_peer,
        deposit_hbar = input.deposit_hbar,
        "Opening channel via libp2p peer ID"
    );

    match ops
        .open_payment_channel_to_libp2p(libp2p_peer, deposit_tinybars)
        .await
    {
        Ok((channel, remote_nodalync_id)) => {
            let output = OpenChannelOutput {
                channel_id: hash_to_string(&channel.channel_id),
                transaction_id: None, // MVP: local only
                balance_tinybars: channel.my_balance,
                peer_id: peer_id_to_string(&remote_nodalync_id),
            };

            info!(
                channel_id = %output.channel_id,
                remote_nodalync_id = %output.peer_id,
                deposit_hbar = input.deposit_hbar,
                "Channel opened successfully via libp2p"
            );
            Ok(output)
        }
        Err(e) => {
            warn!(error = %e, "Failed to open channel via libp2p");
            Err(tool_error(&NodalyncMcpError::Internal(
                libp2p_open_failure_message(&e),
            )))
        }
    }
}

/// A failed libp2p channel open, with a recovery suggestion.
fn libp2p_open_failure_message(e: &nodalync_ops::OpsError) -> String {
    if e.to_string().contains("already exists") {
        "Channel already exists with this peer. This can happen when local and remote state are out of sync. \
         Try using `reset_channels` to clear local state, then query content again - channels will re-open automatically.".to_string()
    } else {
        format!(
            "Failed to open channel: {}. Check that the peer is connected and reachable.",
            e
        )
    }
}

/// Open a channel with a peer known by Nodalync peer ID.
async fn open_channel_via_nodalync(
    ops: &mut DefaultNodeOperations,
    peer_id: NodalyncPeerId,
    input: OpenChannelInput,
    deposit_tinybars: u64,
) -> Result<OpenChannelOutput, CallToolResult> {
    info!(
        nodalync_peer = %peer_id_to_string(&peer_id),
        deposit_hbar = input.deposit_hbar,
        "Opening channel via Nodalync peer ID"
    );

    match ops.open_payment_channel(&peer_id, deposit_tinybars).await {
        Ok(channel) => {
            let output = OpenChannelOutput {
                channel_id: hash_to_string(&channel.channel_id),
                transaction_id: None, // MVP: local only
                balance_tinybars: channel.my_balance,
                peer_id: input.peer_id,
            };

            info!(
                channel_id = %output.channel_id,
                deposit_hbar = input.deposit_hbar,
                "Channel opened successfully"
            );
            Ok(output)
        }
        Err(e) => {
            warn!(error = %e, "Failed to open channel via Nodalync peer ID");
            Err(tool_error(&NodalyncMcpError::Internal(format!(
                "Failed to open channel: {}. Check that the peer is connected.",
                e
            ))))
        }
    }
}

/// The tool response for a step's outcome: its output as JSON, its error result, or
/// its protocol error.
fn tool_outcome<T: serde::Serialize, E: Into<ToolExit>>(
    outcome: Result<T, E>,
) -> Result<CallToolResult, McpError> {
    match outcome.map_err(Into::into) {
        Ok(output) => json_result(&output),
        Err(ToolExit::Result(error_result)) => Ok(error_result),
        Err(ToolExit::Protocol(error)) => Err(error),
    }
}

/// A successful tool result holding `output` as pretty-printed JSON.
fn json_result<T: serde::Serialize>(output: &T) -> Result<CallToolResult, McpError> {
    let json = serde_json::to_string_pretty(output)
        .map_err(|e| McpError::internal_error(e.to_string(), None))?;
    Ok(CallToolResult::success(vec![Content::text(json)]))
}

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
        async move { self.read_knowledge_resource(&request.uri).await }
    }
}

/// Run the MCP server on stdio transport.
pub async fn run_server(
    config: McpServerConfig,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use rmcp::{transport::stdio, ServiceExt};

    info!("Starting Nodalync MCP server");

    let server = NodalyncMcpServer::new(config).await?;
    let server_clone = server.clone();

    // Serve on stdio. If the transport fails (e.g., stdin already closed),
    // treat it as a clean exit rather than an error.
    let service = match server.serve(stdio()).await {
        Ok(s) => s,
        Err(e) => {
            info!("MCP transport closed during setup: {}", e);
            server_clone.shutdown().await;
            return Ok(());
        }
    };

    // Wait for the service to complete. Connection close (e.g., client
    // disconnect, stdin EOF) is expected and not an error condition.
    if let Err(e) = service.waiting().await {
        info!("MCP transport closed: {}", e);
    }

    // Server is shutting down - close all payment channels
    info!("MCP server stopping, cleaning up...");
    server_clone.shutdown().await;

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

/// Parse a bootstrap address string into peer ID and multiaddr.
///
/// Expected format: /dns4/host/tcp/port/p2p/peer_id or /ip4/addr/tcp/port/p2p/peer_id
pub(crate) fn parse_bootstrap_address(addr_str: &str) -> Option<(LibP2pPeerId, Multiaddr)> {
    use nodalync_net::multiaddr::Protocol;

    let addr: Multiaddr = addr_str.parse().ok()?;

    // Extract peer ID from the address (the last /p2p component)
    let peer_id = addr
        .iter()
        .filter_map(|proto| match proto {
            Protocol::P2p(pid) => Some(pid),
            _ => None,
        })
        .last()?;

    // Build address without peer ID for dialing
    let dial_addr: Multiaddr = addr
        .iter()
        .filter(|proto| !matches!(proto, Protocol::P2p(_)))
        .collect();

    Some((peer_id, dial_addr))
}

/// Maximum content size (10 MB).
const MAX_CONTENT_SIZE: usize = 10 * 1024 * 1024;

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

/// Parse a visibility string to Visibility enum.
fn parse_visibility(s: &str) -> Option<Visibility> {
    match s.to_lowercase().as_str() {
        "private" => Some(Visibility::Private),
        "unlisted" => Some(Visibility::Unlisted),
        "shared" => Some(Visibility::Shared),
        "offline" => Some(Visibility::Offline),
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
            hedera: None,
        }
    }

    #[tokio::test]
    async fn test_server_creation() {
        let temp_dir = TempDir::new().unwrap();
        let config = test_config(&temp_dir);

        let server = NodalyncMcpServer::new(config).await;
        assert!(server.is_ok());
    }

    /// Create local content with `price` (tinybars) and return its hash string.
    async fn create_priced_content(server: &NodalyncMcpServer, price: u64) -> String {
        let mut ops = server.ops.lock().await;
        let content = b"Alice Smith measured the effect in Paris.";
        let hash = ops
            .create_content(
                content,
                nodalync_types::Metadata::new("Doc", content.len() as u64),
            )
            .unwrap();
        ops.set_content_price(&hash, price).unwrap();
        hash_to_string(&hash)
    }

    fn result_text(result: &CallToolResult) -> String {
        serde_json::to_string(&result.content).unwrap()
    }

    #[tokio::test]
    async fn test_query_knowledge_invalid_hash_is_error() {
        let temp_dir = TempDir::new().unwrap();
        let server = NodalyncMcpServer::new(test_config(&temp_dir))
            .await
            .unwrap();

        let input = QueryKnowledgeInput {
            query: "not-a-hash".to_string(),
            budget_hbar: None,
        };
        let result = server.query_knowledge(Parameters(input)).await.unwrap();
        assert!(result.is_error.unwrap_or(false));
    }

    #[tokio::test]
    async fn test_query_knowledge_free_local_content() {
        let temp_dir = TempDir::new().unwrap();
        let server = NodalyncMcpServer::new(test_config(&temp_dir))
            .await
            .unwrap();
        let hash = create_priced_content(&server, 0).await;

        let input = QueryKnowledgeInput {
            query: hash.clone(),
            budget_hbar: None,
        };
        let result = server.query_knowledge(Parameters(input)).await.unwrap();

        assert!(
            !result.is_error.unwrap_or(false),
            "{}",
            result_text(&result)
        );
        let text = result_text(&result);
        assert!(text.contains("Alice Smith measured the effect"));
        assert!(text.contains(&hash));
        assert!(
            !text.contains("payment_receipt_id"),
            "free query has no payment block"
        );
        assert_eq!(server.budget.spent(), 0);
    }

    #[tokio::test]
    async fn test_query_knowledge_over_budget_does_not_spend() {
        let temp_dir = TempDir::new().unwrap();
        let server = NodalyncMcpServer::new(test_config(&temp_dir))
            .await
            .unwrap();
        // 2 HBAR price against a 1 HBAR session budget
        let hash = create_priced_content(&server, hbar_to_tinybars(2.0)).await;

        let input = QueryKnowledgeInput {
            query: hash,
            budget_hbar: None,
        };
        let result = server.query_knowledge(Parameters(input)).await.unwrap();

        assert!(result.is_error.unwrap_or(false));
        assert!(result_text(&result).to_lowercase().contains("budget"));
        assert_eq!(server.budget.spent(), 0);
    }

    #[test]
    fn test_build_query_output_lists_derived_sources_once() {
        let root = content_hash(b"root");
        let derived = content_hash(b"derived");
        let mut manifest = nodalync_types::Manifest::new_l0(
            content_hash(b"content"),
            NodalyncPeerId::from_bytes([1u8; 20]),
            nodalync_types::Metadata::new("Doc", 7),
            1,
        );
        manifest.provenance.root_l0l1 = vec![nodalync_types::ProvenanceEntry::new(
            root,
            UNKNOWN_PEER_ID,
            Visibility::Shared,
        )];
        manifest.provenance.derived_from = vec![root, derived];
        let response = nodalync_ops::QueryResponse {
            content: b"content".to_vec(),
            manifest,
            receipt: nodalync_wire::PaymentReceipt {
                payment_id: content_hash(b"pay"),
                amount: 0,
                timestamp: 1,
                channel_nonce: 0,
                distributor_signature: nodalync_crypto::Signature::from_bytes([0u8; 64]),
            },
        };

        let output = build_query_output(response, 0, empty_payment_details(), 1.0);

        assert_eq!(output.sources, vec![hash_to_string(&root)]);
        assert_eq!(
            output.provenance,
            vec![hash_to_string(&root), hash_to_string(&derived)]
        );
        assert!(output.payment.is_none());
        assert_eq!(output.content, "content");
    }

    #[test]
    fn test_close_tally_counts_by_outcome() {
        let mut tally = CloseTally::default();
        tally.record(close_entry("a", "cooperative", Some("tx".into()), None));
        tally.record(close_entry("b", "cleared_unfunded", None, None));
        tally.record(close_entry(
            "c",
            "dispute_initiated",
            Some("tx2".into()),
            None,
        ));
        tally.record(close_entry("d", "failed", None, Some("boom".into())));

        assert_eq!((tally.closed, tally.disputes, tally.failed), (2, 1, 1));
        assert!(tally.results[1].success);
        assert!(!tally.results[3].success);
        assert_eq!(tally.results[3].error.as_deref(), Some("boom"));
    }

    #[test]
    fn test_parse_knowledge_uri() {
        let hash = content_hash(b"resource");
        let uri = format!("{}{}", KNOWLEDGE_URI_PREFIX, hash_to_string(&hash));
        assert_eq!(parse_knowledge_uri(&uri).unwrap(), hash);
        assert!(parse_knowledge_uri("http://example.com").is_err());
        assert!(parse_knowledge_uri("knowledge://not-a-hash").is_err());
    }

    #[test]
    fn test_content_size_and_visibility_checks() {
        assert!(check_content_size("").is_err());
        assert!(check_content_size(&"x".repeat(MAX_CONTENT_SIZE + 1)).is_err());
        assert!(check_content_size("ok").is_ok());
        assert_eq!(
            parse_visibility_input("shared").unwrap(),
            Visibility::Shared
        );
        assert!(parse_visibility_input("everyone").is_err());
        assert!(earnings_filter(NodalyncPeerId::from_bytes([1u8; 20]), 10, Some("L9")).is_err());
        assert!(earnings_filter(NodalyncPeerId::from_bytes([1u8; 20]), 10, Some("L1")).is_ok());
    }

    #[test]
    fn test_peer_id_parse_errors_keep_each_tool_message() {
        let open = parse_nodalync_peer_id("0OIl", "bad format (open)", |n| format!("{} bytes", n));
        assert!(result_text(&open.unwrap_err()).contains("bad format (open)"));
        let short = bs58::encode([1u8; 5]).into_string();
        let close = parse_nodalync_peer_id(&short, "fmt", |n| format!("got {} bytes", n));
        assert!(result_text(&close.unwrap_err()).contains("got 5 bytes"));
        let valid = bs58::encode([7u8; 20]).into_string();
        assert_eq!(
            parse_nodalync_peer_id(&valid, "fmt", |n| n.to_string()).unwrap(),
            NodalyncPeerId::from_bytes([7u8; 20])
        );
    }

    #[tokio::test]
    async fn test_publish_update_delete_flow() {
        let temp_dir = TempDir::new().unwrap();
        let server = NodalyncMcpServer::new(test_config(&temp_dir))
            .await
            .unwrap();

        let publish = PublishContentInput {
            content: "Bob Jones observed the results in Rome.".to_string(),
            title: "Original".to_string(),
            description: Some("first".to_string()),
            price_hbar: None,
            visibility: "shared".to_string(),
            mime_type: Some("text/plain".to_string()),
            tags: Some(vec!["science".to_string()]),
        };
        let result = server
            .publish_content(Parameters(publish.clone()))
            .await
            .unwrap();
        assert!(
            !result.is_error.unwrap_or(false),
            "{}",
            result_text(&result)
        );
        let hash = hash_to_string(&content_hash(publish.content.as_bytes()));

        // Publishing the same content again is refused.
        let again = server.publish_content(Parameters(publish)).await.unwrap();
        assert!(again.is_error.unwrap_or(false));

        let update = UpdateContentInput {
            previous_hash: hash.clone(),
            content: "Bob Jones observed new results in Rome.".to_string(),
            title: None,
            description: None,
        };
        let result = server.update_content(Parameters(update)).await.unwrap();
        let text = result_text(&result);
        assert!(!result.is_error.unwrap_or(false), "{}", text);
        assert!(text.contains("Original"), "title is inherited: {}", text);

        let delete = DeleteContentInput { hash: hash.clone() };
        let result = server.delete_content(Parameters(delete)).await.unwrap();
        assert!(
            !result.is_error.unwrap_or(false),
            "{}",
            result_text(&result)
        );
        let manifest = server
            .ops
            .lock()
            .await
            .get_content_manifest(&string_to_hash(&hash).unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(manifest.visibility, Visibility::Offline);
    }

    #[tokio::test]
    async fn test_status() {
        let temp_dir = TempDir::new().unwrap();
        let config = test_config(&temp_dir);

        let server = NodalyncMcpServer::new(config).await.unwrap();
        let result = server.status().await.unwrap();

        // Should return success with status info
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
    async fn test_status_without_network() {
        let temp_dir = TempDir::new().unwrap();
        let config = test_config(&temp_dir);

        let server = NodalyncMcpServer::new(config).await.unwrap();
        let result = server.status().await.unwrap();

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

    #[test]
    fn test_tool_error_format() {
        use crate::error::McpError as NodalyncMcpError;

        let error = NodalyncMcpError::NotFound("test_hash".to_string());
        let result = tool_error(&error);

        // Should be an error result
        assert!(result.is_error.unwrap_or(false));

        // Content should be JSON with expected fields
        if let Some(content) = result.content.first() {
            if let RawContent::Text(RawTextContent { text, .. }) = &content.raw {
                let json: serde_json::Value = serde_json::from_str(text).unwrap();
                assert_eq!(json["error"], "NOT_FOUND");
                assert_eq!(json["code"], 1);
                assert!(json["message"].as_str().unwrap().contains("not found"));
                assert!(json["suggestion"].is_string());
            }
        }
    }

    #[test]
    fn test_tool_error_budget_exceeded() {
        use crate::error::McpError as NodalyncMcpError;

        let error = NodalyncMcpError::BudgetExceeded {
            cost: 1_000_000,
            remaining: 500_000,
        };
        let result = tool_error(&error);

        assert!(result.is_error.unwrap_or(false));

        if let Some(content) = result.content.first() {
            if let RawContent::Text(RawTextContent { text, .. }) = &content.raw {
                let json: serde_json::Value = serde_json::from_str(text).unwrap();
                assert_eq!(json["error"], "INSUFFICIENT_BALANCE");
            }
        }
    }

    #[test]
    fn test_peer_id_to_string_roundtrip() {
        let peer_id = NodalyncPeerId([
            1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20,
        ]);
        let s = peer_id.to_string();
        assert!(!s.is_empty());
        // Nodalync peer IDs start with "ndl1"
        assert!(s.starts_with("ndl1"));
    }

    #[test]
    fn test_mcp_server_config_default() {
        let config = McpServerConfig::default();
        assert!((config.budget_hbar - 1.0).abs() < f64::EPSILON);
        assert!((config.auto_approve_hbar - 0.01).abs() < f64::EPSILON);
        assert!(!config.enable_network);
        assert!(config.hedera.is_none());
        // Default should include bootstrap nodes
        assert!(!config.bootstrap_nodes.is_empty());
    }
}
