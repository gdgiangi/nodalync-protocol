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

use nodalync_crypto::{peer_id_from_public_key, PeerId as NodalyncPeerId, UNKNOWN_PEER_ID};
use nodalync_net::{Multiaddr, Network, NetworkConfig, NetworkNode, PeerId as LibP2pPeerId};
use nodalync_ops::DefaultNodeOperations;
use nodalync_store::{ChannelStore, ManifestFilter, ManifestStore, NodeState, NodeStateConfig};
use nodalync_types::{ContentType, Visibility};

use crate::budget::{hbar_to_tinybars, tinybars_to_hbar, BudgetTracker};
use crate::error::McpError as NodalyncMcpError;
use crate::tools::{
    hash_to_string, string_to_hash, CloseChannelInput, DepositHbarInput, DepositHbarOutput,
    ListSourcesInput, ListSourcesOutput, OpenChannelInput, OpenChannelOutput, PaymentDetails,
    QueryKnowledgeInput, QueryKnowledgeOutput, SearchNetworkInput, SearchNetworkOutput,
    SearchResultInfo, SourceInfo, StatusOutput,
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
            data_dir: directories::ProjectDirs::from("", "", "nodalync")
                .map(|d| d.data_dir().to_path_buf())
                .unwrap_or_else(|| std::path::PathBuf::from("~/.nodalync")),
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

        // Create Hedera settlement if configured
        let settlement: Option<Arc<dyn nodalync_settle::Settlement>> =
            if let Some(ref hedera) = config.hedera {
                info!(
                    account_id = %hedera.account_id,
                    contract_id = %hedera.contract_id,
                    network = %hedera.network,
                    "Initializing Hedera settlement..."
                );

                #[cfg(feature = "hedera-sdk")]
                {
                    // Parse network
                    let network = match hedera.network.to_lowercase().as_str() {
                        "mainnet" => nodalync_settle::HederaNetwork::Mainnet,
                        "previewnet" => nodalync_settle::HederaNetwork::Previewnet,
                        _ => nodalync_settle::HederaNetwork::Testnet,
                    };

                    // Create Hedera config
                    let hedera_config = nodalync_settle::HederaConfig {
                        network,
                        account_id: hedera.account_id.clone(),
                        private_key_path: hedera.private_key_path.clone(),
                        contract_id: hedera.contract_id.clone(),
                        gas: nodalync_settle::GasConfig::default(),
                        retry: nodalync_settle::RetryConfig::default(),
                    };

                    // Initialize real Hedera settlement
                    let settlement = nodalync_settle::HederaSettlement::new(hedera_config)
                        .await
                        .map_err(|e| {
                            std::io::Error::other(format!(
                                "Failed to initialize Hedera settlement: {}",
                                e
                            ))
                        })?;

                    info!("Hedera settlement initialized successfully");
                    Some(Arc::new(settlement) as Arc<dyn nodalync_settle::Settlement>)
                }

                #[cfg(not(feature = "hedera-sdk"))]
                {
                    return Err(Box::new(std::io::Error::other(
                        "Hedera settlement requires the 'hedera-sdk' feature. \
                         Build with: cargo build --features hedera-sdk",
                    )));
                }
            } else {
                None
            };

        // Create operations with network and/or settlement
        let mut ops = match (&network, &settlement) {
            (Some(net), Some(settle)) => {
                DefaultNodeOperations::with_defaults_network_and_settlement(
                    state,
                    peer_id,
                    Arc::clone(net) as Arc<dyn nodalync_net::Network>,
                    Arc::clone(settle),
                )
            }
            (Some(net), None) => DefaultNodeOperations::with_defaults_and_network(
                state,
                peer_id,
                Arc::clone(net) as Arc<dyn nodalync_net::Network>,
            ),
            (None, Some(settle)) => DefaultNodeOperations::with_defaults_and_settlement(
                state,
                peer_id,
                Arc::clone(settle),
            ),
            (None, None) => DefaultNodeOperations::with_defaults(state, peer_id),
        };

        // Set the private key for signing payments
        ops.set_private_key(private_key);

        // Wrap ops in Arc<Mutex> for sharing
        let ops = Arc::new(Mutex::new(ops));

        // Spawn background event processor if network is enabled
        if let Some(ref net) = network {
            let ops_clone = Arc::clone(&ops);
            let network_clone = Arc::clone(net);

            tokio::spawn(async move {
                info!("MCP event processor started");
                loop {
                    match network_clone.next_event().await {
                        Ok(event) => {
                            let mut ops_guard = ops_clone.lock().await;
                            if let Err(e) = ops_guard.handle_network_event(event).await {
                                warn!("MCP event handler error: {}", e);
                            }
                        }
                        Err(e) => {
                            warn!("MCP network event error: {} - stopping event processor", e);
                            break;
                        }
                    }
                }
            });
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

        // Parse query as hash
        let hash = match string_to_hash(&input.query) {
            Ok(h) => h,
            Err(e) => {
                return Ok(tool_error(&NodalyncMcpError::InvalidHash(e)));
            }
        };

        // Track all payment operations for the response
        let mut payment_details = PaymentDetails {
            channel_opened: false,
            channel_id: None,
            channel_tx_id: None,
            deposit_tx_id: None,
            deposit_amount_hbar: None,
            provider_peer_id: None,
            payment_receipt_id: None,
            hedera_balance_hbar: None,
        };

        // Get preview to check price and find provider
        let mut ops = self.ops.lock().await;
        let preview = match ops.preview_content(&hash).await {
            Ok(p) => p,
            Err(e) => {
                warn!(hash = %hash_to_string(&hash), error = %e, "Content not found");
                return Ok(tool_error(&NodalyncMcpError::Ops(e)));
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
            return Ok(tool_error(&NodalyncMcpError::BudgetExceeded {
                cost: price,
                remaining: max_budget,
            }));
        }

        // Check session budget
        if !self.budget.can_afford(price) {
            return Ok(tool_error(&NodalyncMcpError::BudgetExceeded {
                cost: price,
                remaining: self.budget.remaining(),
            }));
        }

        // === AUTO-DEPOSIT IF NEEDED ===
        // For paid content, ensure we have enough in settlement contract
        if price > 0 {
            if let Some(ref settlement) = self.settlement {
                // Check current balance
                if let Ok(balance) = settlement.get_balance().await {
                    // Minimum deposit: max of (price * 10, 1 HBAR) to avoid frequent deposits
                    let min_required = (price * 10).max(hbar_to_tinybars(1.0));
                    if balance < min_required {
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
                }
            }
        }

        // === AUTO-OPEN PAYMENT CHANNEL IF NEEDED ===
        if price > 0 {
            let libp2p_peer_opt = preview
                .provider_peer_id
                .as_ref()
                .and_then(|s| s.parse::<LibP2pPeerId>().ok());

            if let Some(libp2p_peer) = libp2p_peer_opt {
                // Check if we have an existing channel
                let existing_nodalync_id = self
                    .network
                    .as_ref()
                    .and_then(|n| n.nodalync_peer_id(&libp2p_peer));

                let has_channel = existing_nodalync_id
                    .map(|id| ops.has_open_channel(&id).unwrap_or(false))
                    .unwrap_or(false);

                if !has_channel {
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
                            payment_details.channel_opened = true;
                            payment_details.channel_id = Some(hash_to_string(&channel.channel_id));
                            payment_details.provider_peer_id =
                                Some(peer_id_to_string(&remote_nodalync_id));

                            // Get on-chain tx ID if available
                            if let Some(tx_id) = channel.funding_tx_id {
                                payment_details.channel_tx_id = Some(tx_id);
                            }
                        }
                        Err(e) => {
                            warn!(error = %e, "Failed to open payment channel");
                        }
                    }
                } else {
                    payment_details.provider_peer_id =
                        existing_nodalync_id.map(|p| peer_id_to_string(&p));
                }
            } else if preview.manifest.owner != UNKNOWN_PEER_ID {
                // Fallback to Nodalync peer ID
                let peer = preview.manifest.owner;
                payment_details.provider_peer_id = Some(peer_id_to_string(&peer));

                if !ops.has_open_channel(&peer).unwrap_or(false) {
                    let channel_deposit = hbar_to_tinybars(1.0);

                    match ops.open_payment_channel(&peer, channel_deposit).await {
                        Ok(channel) => {
                            payment_details.channel_opened = true;
                            payment_details.channel_id = Some(hash_to_string(&channel.channel_id));
                            if let Some(tx_id) = channel.funding_tx_id {
                                payment_details.channel_tx_id = Some(tx_id);
                            }
                            tokio::time::sleep(Duration::from_millis(500)).await;
                        }
                        Err(e) => {
                            warn!(error = %e, "Failed to open payment channel");
                        }
                    }
                }
            }
        }

        // === RESERVE BUDGET AND EXECUTE QUERY ===
        if price > 0 && self.budget.spend(price).is_none() {
            return Ok(tool_error(&NodalyncMcpError::BudgetExceeded {
                cost: price,
                remaining: self.budget.remaining(),
            }));
        }

        // Execute query with automatic retry on channel requirement
        let response = match ops.query_content(&hash, price, None).await {
            Ok(r) => r,
            Err(nodalync_ops::OpsError::ChannelRequiredWithPeerInfo {
                nodalync_peer_id,
                libp2p_peer_id,
            }) => {
                info!("Server requires payment channel - auto-opening and retrying");
                let channel_deposit = hbar_to_tinybars(1.0);

                // Try libp2p peer ID first
                if let Some(ref libp2p_str) = libp2p_peer_id {
                    if let Ok(libp2p_peer) = libp2p_str.parse::<LibP2pPeerId>() {
                        if let Ok((channel, remote_id)) = ops
                            .open_payment_channel_to_libp2p(libp2p_peer, channel_deposit)
                            .await
                        {
                            payment_details.channel_opened = true;
                            payment_details.channel_id = Some(hash_to_string(&channel.channel_id));
                            payment_details.provider_peer_id = Some(peer_id_to_string(&remote_id));
                            if let Some(tx_id) = channel.funding_tx_id {
                                payment_details.channel_tx_id = Some(tx_id);
                            }
                        }
                    }
                } else if let Some(ref nodalync_id) = nodalync_peer_id {
                    if let Ok(channel) =
                        ops.open_payment_channel(nodalync_id, channel_deposit).await
                    {
                        payment_details.channel_opened = true;
                        payment_details.channel_id = Some(hash_to_string(&channel.channel_id));
                        payment_details.provider_peer_id = Some(peer_id_to_string(nodalync_id));
                        if let Some(tx_id) = channel.funding_tx_id {
                            payment_details.channel_tx_id = Some(tx_id);
                        }
                        tokio::time::sleep(Duration::from_millis(500)).await;
                    }
                }

                // Retry query
                match ops.query_content(&hash, price, None).await {
                    Ok(r) => r,
                    Err(e) => {
                        if price > 0 {
                            self.budget.refund(price);
                        }
                        return Ok(tool_error(&NodalyncMcpError::Ops(e)));
                    }
                }
            }
            Err(e) => {
                if price > 0 {
                    self.budget.refund(price);
                }
                return Ok(tool_error(&NodalyncMcpError::Ops(e)));
            }
        };

        // Record payment receipt
        payment_details.payment_receipt_id = Some(hash_to_string(&response.receipt.payment_id));

        // Get updated Hedera balance
        if let Some(ref settlement) = self.settlement {
            if let Ok(balance) = settlement.get_balance().await {
                payment_details.hedera_balance_hbar = Some(tinybars_to_hbar(balance));
            }
        }

        // Build output
        let content_str = String::from_utf8_lossy(&response.content).to_string();
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

        // Only include payment details if there was a cost
        let payment = if price > 0 {
            Some(payment_details)
        } else {
            None
        };

        let output = QueryKnowledgeOutput {
            content: content_str,
            hash: hash_to_string(&response.manifest.hash),
            sources,
            provenance,
            cost_hbar: price_hbar,
            remaining_budget_hbar: self.budget.remaining_hbar(),
            payment,
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
                            peer_id: r.publisher_peer_id.clone(),
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

                            // For local manifests, use the owner as peer_id
                            let owner_peer_id = peer_id_to_string(&m.owner);
                            sources.push(SourceInfo {
                                hash: hash_to_string(&m.hash),
                                title: m.metadata.title.clone(),
                                price_hbar: tinybars_to_hbar(m.economics.price),
                                preview,
                                topics: m.metadata.tags.clone(),
                                peer_id: Some(owner_peer_id),
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
                            peer_id: announce.publisher_peer_id.clone(),
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
        let ops = self.ops.lock().await;

        // Network status
        let (connected_peers, is_bootstrapped) = if let Some(ref network) = self.network {
            let peers = network.connected_peers().len() as u32;
            (peers, peers > 0)
        } else {
            (0, false)
        };
        let peer_id = ops.peer_id().to_string();

        // Local content count
        let filter = ManifestFilter::new();
        let local_content_count = match ops.state.manifests.list(filter) {
            Ok(manifests) => manifests.len() as u32,
            Err(_) => 0,
        };

        // Channel status
        let channels = ops.state.channels.list_open().unwrap_or_default();
        let open_channels = channels.len() as u32;
        let channel_balance_tinybars: u64 = channels
            .iter()
            .map(|(_, c)| c.my_balance + c.their_balance)
            .sum();

        // Hedera status
        let (hedera_account_id, hedera_network, hedera_balance_hbar) =
            if let (Some(config), Some(settlement)) = (&self.hedera_config, &self.settlement) {
                let balance = settlement.get_balance().await.ok().map(tinybars_to_hbar);
                (
                    Some(config.account_id.clone()),
                    Some(config.network.clone()),
                    balance,
                )
            } else {
                (None, None, None)
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
            // Hedera
            hedera_configured: self.settlement.is_some(),
            hedera_account_id,
            hedera_network,
            hedera_balance_hbar,
        };

        info!(
            connected_peers = connected_peers,
            open_channels = open_channels,
            budget_remaining = output.budget_remaining_hbar,
            hedera_balance = ?output.hedera_balance_hbar,
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
        // Validate minimum deposit (100 HBAR = 10,000,000,000 tinybars)
        const MIN_DEPOSIT_HBAR: f64 = 100.0;
        if input.deposit_hbar < MIN_DEPOSIT_HBAR {
            warn!(
                deposit = input.deposit_hbar,
                minimum = MIN_DEPOSIT_HBAR,
                "Deposit below minimum"
            );
            return Ok(tool_error(&NodalyncMcpError::Internal(format!(
                "Deposit {} HBAR is below minimum of {} HBAR. Payment channels require at least 100 HBAR deposit.",
                input.deposit_hbar, MIN_DEPOSIT_HBAR
            ))));
        }

        let deposit_tinybars = hbar_to_tinybars(input.deposit_hbar);
        let mut ops = self.ops.lock().await;

        // Check if network is available
        if !ops.has_network() {
            warn!("Cannot open channel: network not available");
            return Ok(tool_error(&NodalyncMcpError::Internal(
                "Network not available. Ensure MCP server is started with --enable-network"
                    .to_string(),
            )));
        }

        // Try to parse as libp2p peer ID first (starts with "12D3Koo" or similar)
        if let Ok(libp2p_peer) = input.peer_id.parse::<LibP2pPeerId>() {
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

                    let json = serde_json::to_string_pretty(&output)
                        .map_err(|e| McpError::internal_error(e.to_string(), None))?;

                    return Ok(CallToolResult::success(vec![Content::text(json)]));
                }
                Err(e) => {
                    let error_str = e.to_string();
                    warn!(error = %e, "Failed to open channel via libp2p");

                    // Provide helpful recovery suggestions
                    let message = if error_str.contains("already exists") {
                        "Channel already exists with this peer. This can happen when local and remote state are out of sync. \
                         Try using `reset_channels` to clear local state, then query content again - channels will re-open automatically.".to_string()
                    } else {
                        format!("Failed to open channel: {}. Check that the peer is connected and reachable.", e)
                    };

                    return Ok(tool_error(&NodalyncMcpError::Internal(message)));
                }
            }
        }

        // Try to parse as Nodalync peer ID (20 bytes base58, starts with "ndl")
        let peer_bytes = match bs58::decode(&input.peer_id).into_vec() {
            Ok(bytes) => bytes,
            Err(_) => {
                return Ok(tool_error(&NodalyncMcpError::InvalidHash(
                    "Invalid peer ID format. Use the peer_id from list_sources (starts with 12D3Koo).".to_string()
                )));
            }
        };

        if peer_bytes.len() != 20 {
            return Ok(tool_error(&NodalyncMcpError::InvalidHash(
                format!("Invalid peer ID. Expected libp2p peer ID (from list_sources, starts with 12D3Koo) or Nodalync peer ID (20 bytes). Got {} bytes.", peer_bytes.len())
            )));
        }

        let mut peer_arr = [0u8; 20];
        peer_arr.copy_from_slice(&peer_bytes);
        let peer_id = nodalync_crypto::PeerId(peer_arr);

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

                let json = serde_json::to_string_pretty(&output)
                    .map_err(|e| McpError::internal_error(e.to_string(), None))?;

                Ok(CallToolResult::success(vec![Content::text(json)]))
            }
            Err(e) => {
                warn!(error = %e, "Failed to open channel via Nodalync peer ID");
                Ok(tool_error(&NodalyncMcpError::Internal(format!(
                    "Failed to open channel: {}. Check that the peer is connected.",
                    e
                ))))
            }
        }
    }

    /// Close a payment channel and settle on-chain.
    ///
    /// Closes an open payment channel with a peer, settling the final balance
    /// on Hedera. Any remaining funds are returned to your account.
    #[tool(
        description = "Close a payment channel and settle the final balance on Hedera. Closes the channel with the specified peer and returns any remaining funds. Use status to see open channels first."
    )]
    async fn close_channel(
        &self,
        Parameters(input): Parameters<CloseChannelInput>,
    ) -> Result<CallToolResult, McpError> {
        use crate::tools::CloseChannelOutput;

        // Parse the Nodalync peer ID
        let peer_bytes = match bs58::decode(&input.peer_id).into_vec() {
            Ok(bytes) => bytes,
            Err(_) => {
                return Ok(tool_error(&NodalyncMcpError::InvalidHash(
                    "Invalid peer ID format. Use the peer_id from status or a previous open_channel response.".to_string()
                )));
            }
        };

        if peer_bytes.len() != 20 {
            return Ok(tool_error(&NodalyncMcpError::InvalidHash(
                format!("Invalid peer ID. Expected Nodalync peer ID (20 bytes base58, starts with 'ndl'). Got {} bytes.", peer_bytes.len())
            )));
        }

        let mut peer_arr = [0u8; 20];
        peer_arr.copy_from_slice(&peer_bytes);
        let peer_id = nodalync_crypto::PeerId(peer_arr);

        let mut ops = self.ops.lock().await;

        // Get channel info before closing
        let channel_info =
            ops.state.channels.get(&peer_id).map_err(|e| {
                McpError::internal_error(format!("Failed to get channel: {}", e), None)
            })?;

        let final_balance = channel_info.as_ref().map(|c| c.my_balance).unwrap_or(0);

        info!(
            peer_id = %peer_id_to_string(&peer_id),
            balance_tinybars = final_balance,
            "Closing payment channel"
        );

        // Use the simple close (without signature exchange) for MCP server
        // since we don't have easy access to the private key here
        match ops.close_payment_channel_simple(&peer_id).await {
            Ok(tx_id_opt) => {
                // Get updated Hedera balance after settlement
                let hedera_balance = if let Some(ref settlement) = self.settlement {
                    settlement.get_balance().await.ok().map(tinybars_to_hbar)
                } else {
                    None
                };

                let output = CloseChannelOutput {
                    success: true,
                    transaction_id: tx_id_opt,
                    final_balance_tinybars: final_balance,
                    peer_id: input.peer_id.clone(),
                    hedera_balance_hbar: hedera_balance,
                };

                info!(
                    peer_id = %input.peer_id,
                    tx_id = ?output.transaction_id,
                    final_balance_tinybars = final_balance,
                    hedera_balance_hbar = ?hedera_balance,
                    "Channel closed successfully"
                );

                let json = serde_json::to_string_pretty(&output)
                    .map_err(|e| McpError::internal_error(e.to_string(), None))?;

                Ok(CallToolResult::success(vec![Content::text(json)]))
            }
            Err(e) => {
                warn!(error = %e, "Failed to close channel");
                Ok(tool_error(&NodalyncMcpError::Internal(format!(
                    "Failed to close channel: {}. Make sure the channel exists and is open.",
                    e
                ))))
            }
        }
    }

    /// Reset all payment channels to recover from inconsistent states.
    ///
    /// Use this when you encounter "channel already exists" errors or other
    /// channel-related issues. This clears local channel state without
    /// affecting on-chain balances.
    #[tool(
        description = "Reset all local payment channel state. Use this to recover from 'channel already exists' errors or other channel synchronization issues. This clears local channel records but does not affect your on-chain Hedera balance. After reset, channels will be re-opened automatically when needed."
    )]
    async fn reset_channels(&self) -> Result<CallToolResult, McpError> {
        use crate::tools::ResetChannelsOutput;

        let mut ops = self.ops.lock().await;

        // Count existing channels before clearing
        let channels = ops.state.channels.list_open().unwrap_or_default();
        let count = channels.len() as u32;

        // Clear all channels
        if let Err(e) = ops.state.channels.clear_all() {
            warn!(error = %e, "Failed to clear channels");
            return Ok(tool_error(&NodalyncMcpError::Internal(format!(
                "Failed to reset channels: {}",
                e
            ))));
        }

        info!(channels_cleared = count, "Payment channels reset");

        let output = ResetChannelsOutput {
            channels_cleared: count,
            message: if count > 0 {
                format!("Cleared {} channel(s). Channels will be re-opened automatically when you query content.", count)
            } else {
                "No local channels to clear. If you're seeing 'channel already exists' errors, the remote peer may have stale state - try querying content again and channels will sync.".to_string()
            },
        };

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
}
