//! MCP server implementation for Nodalync.
//!
//! Uses the RMCP SDK to expose Nodalync knowledge querying to AI assistants.

use std::collections::HashMap;
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
use nodalync_net::{Multiaddr, Network, NetworkConfig, NetworkNode, PeerId as LibP2pPeerId};
use nodalync_ops::DefaultNodeOperations;
use nodalync_store::{
    ChannelStore, ContentStore, ManifestFilter, ManifestStore, NodeState, NodeStateConfig,
};
use nodalync_types::{ContentType, Visibility};

use crate::budget::{hbar_to_tinybars, tinybars_to_hbar, BudgetTracker};
use crate::error::McpError as NodalyncMcpError;
use crate::tools::{
    hash_to_string, string_to_hash, ChannelCloseResult, ChannelInfo, CloseAllChannelsOutput,
    CloseChannelInput, ContentEarnings, DeleteContentInput, DeleteContentOutput, DepositHbarInput,
    DepositHbarOutput, GetEarningsInput, GetEarningsOutput, ListSourcesInput, ListSourcesOutput,
    ListVersionsInput, ListVersionsOutput, OpenChannelInput, OpenChannelOutput, PaymentDetails,
    PreviewContentInput, PreviewContentOutput, PublishContentInput, PublishContentOutput,
    QueryKnowledgeInput, QueryKnowledgeOutput, SearchNetworkInput, SearchNetworkOutput,
    SearchResultInfo, SetVisibilityInput, SetVisibilityOutput, SourceInfo, StatusOutput,
    SynthesizeContentInput, SynthesizeContentOutput, UpdateContentInput, UpdateContentOutput,
    VersionEntry, X402PaymentRequiredOutput, X402PaymentRequirement, X402StatusOutput,
};

use nodalync_x402::{PaymentGate, X402Config};

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
    /// Optional x402 payment protocol configuration.
    /// When enabled, paid content queries return 402 Payment Required responses
    /// that x402-compatible clients can fulfill via the Blocky402 facilitator.
    pub x402: Option<X402Config>,
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
            x402: None,
        }
    }
}

/// Maximum number of cached query results per session.
const MAX_CACHE_ENTRIES: usize = 100;

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
    /// Session-scoped cache for query results (content is immutable by hash).
    query_cache: Arc<Mutex<HashMap<nodalync_crypto::Hash, String>>>,
    /// x402 payment gate for HTTP 402 payment flow.
    x402_gate: Arc<PaymentGate>,
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

            // Spawn background cleanup task for old announcements
            let ops_cleanup = Arc::clone(&ops);
            tokio::spawn(async move {
                // Cleanup announcements older than 7 days
                const ANNOUNCEMENT_TTL_SECONDS: i64 = 7 * 24 * 60 * 60;
                // Run cleanup every hour
                const CLEANUP_INTERVAL_SECONDS: u64 = 60 * 60;

                loop {
                    tokio::time::sleep(Duration::from_secs(CLEANUP_INTERVAL_SECONDS)).await;

                    let ops_guard = ops_cleanup.lock().await;
                    let deleted = ops_guard
                        .state
                        .cleanup_old_announcements(ANNOUNCEMENT_TTL_SECONDS);
                    if deleted > 0 {
                        info!(deleted = deleted, "Cleaned up old announcements");
                    }
                    drop(ops_guard);
                }
            });

            // Spawn background settlement task to periodically settle channels
            let ops_settlement = Arc::clone(&ops);
            tokio::spawn(async move {
                // Settlement interval: 5 minutes
                const SETTLEMENT_INTERVAL_SECONDS: u64 = 5 * 60;

                loop {
                    tokio::time::sleep(Duration::from_secs(SETTLEMENT_INTERVAL_SECONDS)).await;

                    let mut ops_guard = ops_settlement.lock().await;

                    // Check if there are any channels that need settlement
                    let channels = ops_guard.state.channels.list_open().unwrap_or_default();
                    if channels.is_empty() {
                        continue;
                    }

                    // Trigger settlement batch - this settles channels that have
                    // exceeded the threshold (100 HBAR) or time limit (1 hour)
                    match ops_guard.trigger_settlement_batch().await {
                        Ok(Some(batch_id)) => {
                            info!(
                                batch_id = %batch_id,
                                "Background settlement batch submitted"
                            );
                        }
                        Ok(None) => {
                            // No settlement needed (threshold not reached)
                            debug!("Background settlement check: no settlement needed");
                        }
                        Err(e) => {
                            warn!(error = %e, "Background settlement batch failed");
                        }
                    }

                    drop(ops_guard);
                }
            });
        }

        // Create budget tracker
        let budget = BudgetTracker::with_auto_approve(config.budget_hbar, config.auto_approve_hbar);

        // Initialize x402 payment gate
        let x402_gate = if let Some(ref x402_config) = config.x402 {
            info!(
                account_id = %x402_config.account_id,
                network = %x402_config.network,
                app_fee_percent = x402_config.app_fee_percent,
                "x402 payment gate enabled"
            );
            Arc::new(PaymentGate::new(x402_config.clone()).map_err(|e| {
                std::io::Error::other(format!("Failed to initialize x402 gate: {}", e))
            })?)
        } else {
            Arc::new(PaymentGate::disabled())
        };

        info!(
            budget_hbar = config.budget_hbar,
            auto_approve_hbar = config.auto_approve_hbar,
            network_enabled = config.enable_network,
            hedera_enabled = config.hedera.is_some(),
            x402_enabled = config.x402.is_some(),
            "MCP server initialized"
        );

        Ok(Self {
            ops,
            budget: Arc::new(budget),
            tool_router: Self::tool_router(),
            network,
            settlement,
            hedera_config: config.hedera.clone(),
            query_cache: Arc::new(Mutex::new(HashMap::new())),
            x402_gate,
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

        // Parse query as hash
        let hash = match string_to_hash(&input.query) {
            Ok(h) => h,
            Err(e) => {
                return Ok(tool_error(&NodalyncMcpError::InvalidHash(e)));
            }
        };

        // Check session cache (content is immutable by hash, so cache is always valid)
        {
            let cache = self.query_cache.lock().await;
            if let Some(cached_json) = cache.get(&hash) {
                debug!(hash = %input.query, "Returning cached query result");
                return Ok(CallToolResult::success(vec![Content::text(
                    cached_json.clone(),
                )]));
            }
        }

        // Track all payment operations for the response
        let mut payment_details = PaymentDetails {
            channel_opened: false,
            channel_id: None,
            channel_tx_id: None,
            deposit_tx_id: None,
            deposit_amount_hbar: None,
            provider_peer_id: None,
            payment_receipt_id: None,
            hedera_account_balance_hbar: None,
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

        // === x402 PAYMENT FLOW ===
        // When x402 is enabled and content has a price, handle the x402 payment protocol:
        // 1. If no payment provided: return 402 Payment Required with payment requirements
        // 2. If payment provided: validate via facilitator, settle, then deliver content
        if price > 0 && self.x402_gate.is_enabled() {
            if let Some(ref payment_header) = input.x402_payment {
                // Client provided x402 payment — validate and settle
                info!(
                    hash = %hash_to_string(&hash),
                    "Processing x402 payment for content query"
                );

                let content_hash_str = hash_to_string(&hash);
                match self.x402_gate.process_payment(payment_header, &content_hash_str, price).await {
                    Ok(payment_response) => {
                        // Payment succeeded — deliver the content
                        info!(
                            hash = %content_hash_str,
                            tx_hash = ?payment_response.tx_hash,
                            "x402 payment verified and settled"
                        );

                        // Execute the query (no budget deduction needed — paid via x402)
                        let response = match ops.query_content(&hash, price, None).await {
                            Ok(r) => r,
                            Err(e) => {
                                return Ok(tool_error(&NodalyncMcpError::Ops(e)));
                            }
                        };

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
                            let h_str = hash_to_string(h);
                            if !provenance.contains(&h_str) {
                                provenance.push(h_str);
                            }
                        }

                        // Build x402 payment details for the response
                        let x402_payment_details = PaymentDetails {
                            channel_opened: false,
                            channel_id: None,
                            channel_tx_id: None,
                            deposit_tx_id: None,
                            deposit_amount_hbar: None,
                            provider_peer_id: preview.provider_peer_id.clone(),
                            payment_receipt_id: payment_response.tx_hash.clone(),
                            hedera_account_balance_hbar: None,
                        };

                        let output = QueryKnowledgeOutput {
                            content: content_str,
                            hash: hash_to_string(&response.manifest.hash),
                            sources,
                            provenance,
                            cost_hbar: price_hbar,
                            remaining_budget_hbar: self.budget.remaining_hbar(),
                            payment: Some(x402_payment_details),
                        };

                        let json = serde_json::to_string_pretty(&output)
                            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

                        // Cache the result
                        {
                            let mut cache = self.query_cache.lock().await;
                            if cache.len() >= MAX_CACHE_ENTRIES {
                                if let Some(key) = cache.keys().next().cloned() {
                                    cache.remove(&key);
                                }
                            }
                            cache.insert(hash, json.clone());
                        }

                        return Ok(CallToolResult::success(vec![Content::text(json)]));
                    }
                    Err(e) => {
                        warn!(error = %e, "x402 payment validation failed");
                        return Ok(tool_error(&NodalyncMcpError::X402PaymentFailed {
                            reason: e.to_string(),
                        }));
                    }
                }
            } else {
                // No payment provided — return 402 Payment Required
                let x402_config = self.x402_gate.config();
                let total_tinybars = price + (price * x402_config.app_fee_percent as u64 / 100);
                let total_hbar = tinybars_to_hbar(total_tinybars);

                let content_hash_str = hash_to_string(&hash);
                let title = preview.manifest.metadata.title.clone();
                let description = preview.l1_summary.summary.clone();

                let output = X402PaymentRequiredOutput {
                    status: "payment_required".to_string(),
                    x402_version: 1,
                    content_hash: content_hash_str.clone(),
                    title,
                    description,
                    price_hbar,
                    total_required_hbar: total_hbar,
                    app_fee_percent: x402_config.app_fee_percent,
                    accepts: vec![X402PaymentRequirement {
                        scheme: "exact".to_string(),
                        network: x402_config.network.clone(),
                        amount: total_tinybars.to_string(),
                        asset: x402_config.asset.clone(),
                        pay_to: x402_config.account_id.clone(),
                        max_timeout_seconds: x402_config.max_timeout_seconds,
                    }],
                    instruction: format!(
                        "Content requires payment. Retry query_knowledge with hash '{}' and \
                         x402_payment field containing a base64-encoded x402 payment header.",
                        content_hash_str
                    ),
                };

                info!(
                    hash = %content_hash_str,
                    price_hbar = price_hbar,
                    total_hbar = total_hbar,
                    "Returning x402 payment required"
                );

                let json = serde_json::to_string_pretty(&output)
                    .map_err(|e| McpError::internal_error(e.to_string(), None))?;

                // Return as a non-error result (the client needs to read and act on this)
                return Ok(CallToolResult::success(vec![Content::text(json)]));
            }
        }

        // === NATIVE PAYMENT CHANNEL FLOW (when x402 is disabled) ===

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

        // Get updated Hedera account balance (live on-chain balance)
        if let Some(ref settlement) = self.settlement {
            if let Ok(balance) = settlement.get_account_balance().await {
                payment_details.hedera_account_balance_hbar = Some(tinybars_to_hbar(balance));
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

        // Store in session cache (evict oldest if full)
        {
            let mut cache = self.query_cache.lock().await;
            if cache.len() >= MAX_CACHE_ENTRIES {
                // Remove an arbitrary entry to make room
                if let Some(key) = cache.keys().next().cloned() {
                    cache.remove(&key);
                }
            }
            cache.insert(hash, json.clone());
        }

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
    /// If the peer is unresponsive, initiates a dispute (24-hour settlement period).
    #[tool(
        description = "Close a payment channel and settle the final balance on Hedera. Closes the channel with the specified peer and returns any remaining funds. Use status to see open channels first. Accepts both libp2p peer IDs (12D3Koo...) and Nodalync peer IDs (ndl...)."
    )]
    async fn close_channel(
        &self,
        Parameters(input): Parameters<CloseChannelInput>,
    ) -> Result<CallToolResult, McpError> {
        use crate::tools::CloseChannelOutput;
        use nodalync_ops::CloseResult;

        // Parse peer ID - accept both libp2p (12D3KooW...) and Nodalync (ndl...) formats
        let peer_id = if input.peer_id.starts_with("12D3KooW") {
            // libp2p peer ID - look up Nodalync peer ID from network
            let Some(ref network) = self.network else {
                return Ok(tool_error(&NodalyncMcpError::Internal(
                    "Network not available. Cannot look up Nodalync peer ID from libp2p ID."
                        .to_string(),
                )));
            };

            let libp2p_peer: LibP2pPeerId = input
                .peer_id
                .parse()
                .map_err(|_| McpError::internal_error("Invalid libp2p peer ID format", None))?;

            // Look up the Nodalync peer ID
            match network.nodalync_peer_id(&libp2p_peer) {
                Some(nodalync_id) => nodalync_id,
                None => {
                    return Ok(tool_error(&NodalyncMcpError::InvalidHash(
                        "No Nodalync peer ID mapping found for this libp2p peer. \
                         The peer may not have an open channel with you."
                            .to_string(),
                    )));
                }
            }
        } else {
            // Nodalync peer ID (ndl...)
            let peer_bytes = match bs58::decode(&input.peer_id).into_vec() {
                Ok(bytes) => bytes,
                Err(_) => {
                    return Ok(tool_error(&NodalyncMcpError::InvalidHash(
                        "Invalid peer ID format. Use the peer_id from status or channel details."
                            .to_string(),
                    )));
                }
            };

            if peer_bytes.len() != 20 {
                return Ok(tool_error(&NodalyncMcpError::InvalidHash(format!(
                    "Invalid peer ID. Expected Nodalync peer ID (20 bytes base58, starts with 'ndl') \
                     or libp2p peer ID (starts with '12D3KooW'). Got {} bytes.",
                    peer_bytes.len()
                ))));
            }

            let mut peer_arr = [0u8; 20];
            peer_arr.copy_from_slice(&peer_bytes);
            nodalync_crypto::PeerId(peer_arr)
        };

        let mut ops = self.ops.lock().await;

        // Get channel info before closing
        let channel_info =
            ops.state.channels.get(&peer_id).map_err(|e| {
                McpError::internal_error(format!("Failed to get channel: {}", e), None)
            })?;

        let Some(channel) = channel_info else {
            return Ok(tool_error(&NodalyncMcpError::Internal(
                "No channel exists with this peer.".to_string(),
            )));
        };

        let final_balance = channel.my_balance;

        info!(
            peer_id = %peer_id_to_string(&peer_id),
            balance_tinybars = final_balance,
            "Closing payment channel"
        );

        // Get the private key for signing
        let Some(private_key) = ops.private_key().cloned() else {
            return Ok(tool_error(&NodalyncMcpError::Internal(
                "Private key not available. Cannot sign channel close.".to_string(),
            )));
        };

        // Attempt cooperative close with proper signature
        let result = ops.close_payment_channel(&peer_id, &private_key).await;
        drop(ops); // Release lock before async Hedera calls

        // Get updated Hedera account balance
        let hedera_balance = if let Some(ref settlement) = self.settlement {
            settlement
                .get_account_balance()
                .await
                .ok()
                .map(tinybars_to_hbar)
        } else {
            None
        };

        match result {
            Ok(CloseResult::Success {
                transaction_id,
                final_balances,
            }) => {
                let output = CloseChannelOutput {
                    success: true,
                    close_method: "cooperative".to_string(),
                    transaction_id: Some(transaction_id),
                    final_balance_tinybars: final_balances.0,
                    peer_id: input.peer_id.clone(),
                    hedera_account_balance_hbar: hedera_balance,
                };

                info!(
                    peer_id = %input.peer_id,
                    close_method = "cooperative",
                    tx_id = ?output.transaction_id,
                    "Channel closed successfully"
                );

                let json = serde_json::to_string_pretty(&output)
                    .map_err(|e| McpError::internal_error(e.to_string(), None))?;

                Ok(CallToolResult::success(vec![Content::text(json)]))
            }
            Ok(CloseResult::SuccessOffChain { final_balances }) => {
                let output = CloseChannelOutput {
                    success: true,
                    close_method: "off_chain".to_string(),
                    transaction_id: None,
                    final_balance_tinybars: final_balances.0,
                    peer_id: input.peer_id.clone(),
                    hedera_account_balance_hbar: hedera_balance,
                };

                info!(
                    peer_id = %input.peer_id,
                    close_method = "off_chain",
                    "Channel closed off-chain"
                );

                let json = serde_json::to_string_pretty(&output)
                    .map_err(|e| McpError::internal_error(e.to_string(), None))?;

                Ok(CallToolResult::success(vec![Content::text(json)]))
            }
            Ok(CloseResult::PeerUnresponsive { .. }) => {
                // Peer didn't respond - initiate dispute
                info!(
                    peer_id = %input.peer_id,
                    "Peer unresponsive, initiating dispute"
                );

                let mut ops = self.ops.lock().await;
                match ops.dispute_payment_channel(&peer_id, &private_key).await {
                    Ok(tx_id) => {
                        let output = CloseChannelOutput {
                            success: true,
                            close_method: "dispute_initiated".to_string(),
                            transaction_id: Some(tx_id),
                            final_balance_tinybars: final_balance,
                            peer_id: input.peer_id.clone(),
                            hedera_account_balance_hbar: hedera_balance,
                        };

                        info!(
                            peer_id = %input.peer_id,
                            close_method = "dispute_initiated",
                            tx_id = ?output.transaction_id,
                            "Dispute initiated - settlement in 24 hours"
                        );

                        let json = serde_json::to_string_pretty(&output)
                            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

                        Ok(CallToolResult::success(vec![Content::text(json)]))
                    }
                    Err(e) => {
                        warn!(error = %e, "Failed to initiate dispute");
                        Ok(tool_error(&NodalyncMcpError::Internal(format!(
                            "Peer unresponsive and dispute failed: {}",
                            e
                        ))))
                    }
                }
            }
            Ok(CloseResult::OnChainFailed { error }) => {
                warn!(error = %error, "On-chain close failed");
                Ok(tool_error(&NodalyncMcpError::Internal(format!(
                    "On-chain settlement failed: {}",
                    error
                ))))
            }
            Err(e) => {
                warn!(error = %e, "Failed to close channel");
                Ok(tool_error(&NodalyncMcpError::Internal(format!(
                    "Failed to close channel: {}",
                    e
                ))))
            }
        }
    }

    /// Close all open payment channels.
    ///
    /// Attempts to cooperatively close all open channels. For any unresponsive
    /// peers, initiates a dispute to ensure settlement (24-hour waiting period).
    #[tool(
        description = "Close all open payment channels and settle balances on Hedera. Attempts cooperative close first; if a peer is unresponsive, automatically initiates a dispute to ensure payment. Use this before ending a session to settle all pending payments."
    )]
    async fn close_all_channels(&self) -> Result<CallToolResult, McpError> {
        let mut results: Vec<ChannelCloseResult> = Vec::new();
        let mut channels_closed = 0u32;
        let mut disputes_initiated = 0u32;
        let mut channels_failed = 0u32;

        // Get list of open channels and private key
        let (channels, private_key) = {
            let ops = self.ops.lock().await;
            let channels = ops.state.channels.list_open().unwrap_or_default();
            let private_key = ops.private_key().cloned();
            (channels, private_key)
        };

        let Some(private_key) = private_key else {
            return Ok(tool_error(&NodalyncMcpError::Internal(
                "Private key not available. Cannot sign channel closes.".to_string(),
            )));
        };

        let channels_processed = channels.len() as u32;

        info!(
            channels_count = channels_processed,
            "Closing all payment channels"
        );

        // Close each channel
        for (peer_id, channel) in channels {
            let peer_id_str = peer_id_to_string(&peer_id);
            let _final_balance = channel.my_balance;
            let was_funded_on_chain = channel.funding_tx_id.is_some();

            // Try cooperative close with timeout
            let close_result = {
                let mut ops = self.ops.lock().await;
                tokio::time::timeout(
                    Duration::from_secs(5),
                    ops.close_payment_channel(&peer_id, &private_key),
                )
                .await
            };

            match close_result {
                Ok(Ok(nodalync_ops::CloseResult::Success { transaction_id, .. })) => {
                    results.push(ChannelCloseResult {
                        peer_id: peer_id_str.clone(),
                        success: true,
                        close_method: "cooperative".to_string(),
                        transaction_id: Some(transaction_id),
                        error: None,
                    });
                    channels_closed += 1;
                    info!(peer_id = %peer_id_str, "Channel closed cooperatively");
                }
                Ok(Ok(nodalync_ops::CloseResult::SuccessOffChain { .. })) => {
                    results.push(ChannelCloseResult {
                        peer_id: peer_id_str.clone(),
                        success: true,
                        close_method: "off_chain".to_string(),
                        transaction_id: None,
                        error: None,
                    });
                    channels_closed += 1;
                    info!(peer_id = %peer_id_str, "Channel closed off-chain");
                }
                Ok(Ok(nodalync_ops::CloseResult::PeerUnresponsive { .. }))
                | Ok(Err(_))
                | Err(_) => {
                    // Peer unresponsive or error - initiate dispute
                    let dispute_result = {
                        let mut ops = self.ops.lock().await;
                        ops.dispute_payment_channel(&peer_id, &private_key).await
                    };

                    match dispute_result {
                        Ok(tx_id) => {
                            results.push(ChannelCloseResult {
                                peer_id: peer_id_str.clone(),
                                success: true,
                                close_method: "dispute_initiated".to_string(),
                                transaction_id: Some(tx_id),
                                error: None,
                            });
                            disputes_initiated += 1;
                            info!(
                                peer_id = %peer_id_str,
                                "Dispute initiated for unresponsive peer"
                            );
                        }
                        Err(e) => {
                            // If the channel was never funded on-chain, just remove it
                            if !was_funded_on_chain {
                                let mut ops = self.ops.lock().await;
                                let _ = ops.state.channels.delete(&peer_id);
                                results.push(ChannelCloseResult {
                                    peer_id: peer_id_str.clone(),
                                    success: true,
                                    close_method: "cleared_unfunded".to_string(),
                                    transaction_id: None,
                                    error: None,
                                });
                                channels_closed += 1;
                                info!(
                                    peer_id = %peer_id_str,
                                    "Cleared unfunded channel from local state"
                                );
                            } else {
                                results.push(ChannelCloseResult {
                                    peer_id: peer_id_str.clone(),
                                    success: false,
                                    close_method: "failed".to_string(),
                                    transaction_id: None,
                                    error: Some(e.to_string()),
                                });
                                channels_failed += 1;
                                warn!(
                                    peer_id = %peer_id_str,
                                    error = %e,
                                    "Failed to close or dispute channel"
                                );
                            }
                        }
                    }
                }
                Ok(Ok(nodalync_ops::CloseResult::OnChainFailed { error })) => {
                    // On-chain failed - try dispute
                    let dispute_result = {
                        let mut ops = self.ops.lock().await;
                        ops.dispute_payment_channel(&peer_id, &private_key).await
                    };

                    match dispute_result {
                        Ok(tx_id) => {
                            results.push(ChannelCloseResult {
                                peer_id: peer_id_str.clone(),
                                success: true,
                                close_method: "dispute_initiated".to_string(),
                                transaction_id: Some(tx_id),
                                error: None,
                            });
                            disputes_initiated += 1;
                        }
                        Err(e) => {
                            // If the channel was never funded on-chain, just remove it
                            if !was_funded_on_chain {
                                let mut ops = self.ops.lock().await;
                                let _ = ops.state.channels.delete(&peer_id);
                                results.push(ChannelCloseResult {
                                    peer_id: peer_id_str.clone(),
                                    success: true,
                                    close_method: "cleared_unfunded".to_string(),
                                    transaction_id: None,
                                    error: None,
                                });
                                channels_closed += 1;
                                info!(
                                    peer_id = %peer_id_str,
                                    "Cleared unfunded channel from local state"
                                );
                            } else {
                                results.push(ChannelCloseResult {
                                    peer_id: peer_id_str.clone(),
                                    success: false,
                                    close_method: "failed".to_string(),
                                    transaction_id: None,
                                    error: Some(format!(
                                        "On-chain close failed: {}. Dispute also failed: {}",
                                        error, e
                                    )),
                                });
                                channels_failed += 1;
                            }
                        }
                    }
                }
            }
        }

        // Get updated Hedera balance
        let hedera_balance = if let Some(ref settlement) = self.settlement {
            settlement
                .get_account_balance()
                .await
                .ok()
                .map(tinybars_to_hbar)
        } else {
            None
        };

        let output = CloseAllChannelsOutput {
            channels_processed,
            channels_closed,
            disputes_initiated,
            channels_failed,
            results,
            hedera_account_balance_hbar: hedera_balance,
        };

        info!(
            processed = channels_processed,
            closed = channels_closed,
            disputes = disputes_initiated,
            failed = channels_failed,
            "Close all channels complete"
        );

        let json = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
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

        // Validate content
        if input.content.is_empty() {
            return Ok(tool_error(&NodalyncMcpError::EmptyContent));
        }
        if input.content.len() > MAX_CONTENT_SIZE {
            return Ok(tool_error(&NodalyncMcpError::ContentTooLarge {
                size: input.content.len(),
                max: MAX_CONTENT_SIZE,
            }));
        }

        // Parse visibility
        let visibility = match parse_visibility(&input.visibility) {
            Some(v) => v,
            None => {
                return Ok(tool_error(&NodalyncMcpError::Internal(format!(
                    "Invalid visibility '{}'. Use 'private', 'unlisted', or 'shared'.",
                    input.visibility
                ))));
            }
        };

        let content_bytes = input.content.as_bytes();
        let size_bytes = content_bytes.len();

        // Check for duplicates
        let computed_hash = content_hash(content_bytes);
        let mut ops = self.ops.lock().await;

        if let Ok(Some(_)) = ops.get_content_manifest(&computed_hash) {
            return Ok(tool_error(&NodalyncMcpError::ContentAlreadyExists(
                hash_to_string(&computed_hash),
            )));
        }

        // Build metadata
        let mut metadata = nodalync_types::Metadata::new(input.title.clone(), size_bytes as u64);
        if let Some(desc) = input.description {
            metadata = metadata.with_description(desc);
        }
        if let Some(mime) = input.mime_type {
            metadata = metadata.with_mime_type(mime);
        }
        if let Some(tags) = input.tags {
            metadata = metadata.with_tags(tags);
        }

        // Create content
        let hash = ops
            .create_content(content_bytes, metadata)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        // Extract L1 summary
        let l1_summary = ops
            .extract_l1_summary(&hash)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        // Publish
        let price = input.price_hbar.map(hbar_to_tinybars).unwrap_or(0);

        ops.publish_content(&hash, visibility, price)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

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

        let json = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
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

        // Validate content
        if input.content.is_empty() {
            return Ok(tool_error(&NodalyncMcpError::EmptyContent));
        }
        if input.content.len() > MAX_CONTENT_SIZE {
            return Ok(tool_error(&NodalyncMcpError::ContentTooLarge {
                size: input.content.len(),
                max: MAX_CONTENT_SIZE,
            }));
        }
        if input.sources.is_empty() {
            return Ok(tool_error(&NodalyncMcpError::Internal(
                "At least one source hash is required for synthesis.".to_string(),
            )));
        }

        // Parse source hashes
        let mut source_hashes = Vec::new();
        for s in &input.sources {
            match string_to_hash(s) {
                Ok(h) => source_hashes.push(h),
                Err(e) => {
                    return Ok(tool_error(&NodalyncMcpError::InvalidHash(format!(
                        "Invalid source hash '{}': {}",
                        s, e
                    ))));
                }
            }
        }

        let content_bytes = input.content.as_bytes();
        let size_bytes = content_bytes.len();

        // Build metadata
        let mut metadata = nodalync_types::Metadata::new(input.title.clone(), size_bytes as u64);
        if let Some(desc) = input.description {
            metadata = metadata.with_description(desc);
        }

        let mut ops = self.ops.lock().await;

        // Derive content
        let hash = ops
            .derive_content(&source_hashes, content_bytes, metadata)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        // Get manifest for provenance info
        let manifest = ops
            .get_content_manifest(&hash)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?
            .ok_or_else(|| {
                McpError::internal_error("Failed to retrieve manifest after derive", None)
            })?;

        let provenance_depth = manifest.provenance.depth;
        let root_source_count = manifest.provenance.root_l0l1.len();

        // Optionally publish
        let published = input.publish.unwrap_or(false);
        let mut vis_str = None;
        let mut price_out = None;

        if published {
            let visibility = match parse_visibility(&input.visibility) {
                Some(v) => v,
                None => {
                    return Ok(tool_error(&NodalyncMcpError::Internal(format!(
                        "Invalid visibility '{}'. Use 'private', 'unlisted', or 'shared'.",
                        input.visibility
                    ))));
                }
            };

            let price = input.price_hbar.map(hbar_to_tinybars).unwrap_or(0);

            ops.publish_content(&hash, visibility, price)
                .await
                .map_err(|e| McpError::internal_error(e.to_string(), None))?;

            vis_str = Some(format!("{:?}", visibility));
            price_out = Some(tinybars_to_hbar(price));
        }

        let output = SynthesizeContentOutput {
            hash: hash_to_string(&hash),
            title: input.title,
            content_type: "L3".to_string(),
            sources: input.sources,
            provenance_depth,
            root_source_count,
            published,
            visibility: vis_str,
            price_hbar: price_out,
        };

        info!(
            hash = %output.hash,
            sources = output.sources.len(),
            provenance_depth = provenance_depth,
            published = published,
            "Content synthesized"
        );

        let json = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
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

        // Validate
        if input.content.is_empty() {
            return Ok(tool_error(&NodalyncMcpError::EmptyContent));
        }
        if input.content.len() > MAX_CONTENT_SIZE {
            return Ok(tool_error(&NodalyncMcpError::ContentTooLarge {
                size: input.content.len(),
                max: MAX_CONTENT_SIZE,
            }));
        }

        let old_hash = match string_to_hash(&input.previous_hash) {
            Ok(h) => h,
            Err(e) => return Ok(tool_error(&NodalyncMcpError::InvalidHash(e))),
        };

        let mut ops = self.ops.lock().await;

        // Load old manifest for metadata inheritance
        let old_manifest = match ops.get_content_manifest(&old_hash) {
            Ok(Some(m)) => m,
            Ok(None) => {
                return Ok(tool_error(&NodalyncMcpError::NotFound(format!(
                    "Previous version {} not found",
                    input.previous_hash
                ))));
            }
            Err(e) => return Ok(tool_error(&NodalyncMcpError::Ops(e))),
        };

        let content_bytes = input.content.as_bytes();
        let size_bytes = content_bytes.len();

        // Build metadata, inheriting from previous version
        let title = input
            .title
            .unwrap_or_else(|| old_manifest.metadata.title.clone());
        let mut metadata = nodalync_types::Metadata::new(title.clone(), size_bytes as u64);

        // Inherit or override description
        let description = input
            .description
            .or_else(|| old_manifest.metadata.description.clone());
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

        // Create new version
        let new_hash = ops
            .update_content(&old_hash, content_bytes, metadata)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        // Get new manifest for version info
        let new_manifest = ops
            .get_content_manifest(&new_hash)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?
            .ok_or_else(|| {
                McpError::internal_error("Failed to retrieve manifest after update", None)
            })?;

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

        let json = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
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

        let hash = match string_to_hash(&input.hash) {
            Ok(h) => h,
            Err(e) => return Ok(tool_error(&NodalyncMcpError::InvalidHash(e))),
        };

        let mut ops = self.ops.lock().await;

        // Load manifest and verify ownership
        let mut manifest = match ops.get_content_manifest(&hash) {
            Ok(Some(m)) => m,
            Ok(None) => {
                return Ok(tool_error(&NodalyncMcpError::NotFound(hash_to_string(
                    &hash,
                ))));
            }
            Err(e) => return Ok(tool_error(&NodalyncMcpError::Ops(e))),
        };

        let our_peer_id = ops.peer_id();
        if manifest.owner != our_peer_id {
            return Ok(tool_error(&NodalyncMcpError::Internal(
                "Cannot delete content you don't own.".to_string(),
            )));
        }

        let title = manifest.metadata.title.clone();

        // Delete content bytes
        ops.state
            .content
            .delete(&hash)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        // Update manifest visibility to Offline
        manifest.visibility = Visibility::Offline;
        ops.state
            .manifests
            .update(&manifest)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let output = DeleteContentOutput {
            hash: hash_to_string(&hash),
            title,
            content_removed: true,
            visibility: "Offline".to_string(),
        };

        info!(hash = %output.hash, "Content deleted");

        let json = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
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

        let limit = input.limit.unwrap_or(20).min(100);
        let ops = self.ops.lock().await;

        let peer_id = ops.peer_id();
        let mut filter = ManifestFilter::new().with_owner(peer_id).limit(limit);

        if let Some(ref ct_str) = input.content_type {
            if let Some(ct) = parse_content_type(ct_str) {
                filter = filter.with_content_type(ct);
            } else {
                return Ok(tool_error(&NodalyncMcpError::Internal(format!(
                    "Invalid content type '{}'. Use L0, L1, L2, or L3.",
                    ct_str
                ))));
            }
        }

        let manifests = ops
            .state
            .manifests
            .list(filter)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

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

        let content_count = items.len() as u32;

        let output = GetEarningsOutput {
            items,
            total_revenue_hbar: tinybars_to_hbar(total_revenue),
            total_queries,
            content_count,
        };

        info!(
            content_count = content_count,
            total_queries = total_queries,
            total_revenue_hbar = output.total_revenue_hbar,
            "Earnings retrieved"
        );

        let json = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    /// Get x402 payment protocol status.
    ///
    /// Shows whether x402 is enabled, configuration details, and transaction statistics.
    #[tool(
        description = "Get x402 payment protocol status. Shows whether HTTP 402 micropayments \
                       are enabled, the settlement network, facilitator endpoint, fee configuration, \
                       and transaction statistics. x402 enables AI agents to pay for knowledge \
                       access via the standard HTTP 402 Payment Required flow."
    )]
    async fn x402_status(&self) -> Result<CallToolResult, McpError> {
        let gate_status = self.x402_gate.status().await;

        let output = X402StatusOutput {
            enabled: gate_status.enabled,
            network: gate_status.network.clone(),
            facilitator_url: gate_status.facilitator_url.clone(),
            account_id: gate_status.account_id.clone(),
            app_fee_percent: gate_status.app_fee_percent,
            total_transactions: gate_status.total_transactions,
            total_volume: gate_status.total_volume,
            total_app_fees: gate_status.total_app_fees,
            total_volume_hbar: tinybars_to_hbar(gate_status.total_volume),
            total_app_fees_hbar: tinybars_to_hbar(gate_status.total_app_fees),
        };

        info!(
            enabled = output.enabled,
            transactions = output.total_transactions,
            volume_hbar = output.total_volume_hbar,
            "x402 status requested"
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
                 Use `search_network` or `list_sources` to discover content, then `query_knowledge` \
                 to retrieve it. Supports two payment modes: native payment channels (automatic) \
                 and x402 HTTP micropayments (for external agents). Check `x402_status` for \
                 payment protocol details. Access content directly via `knowledge://{hash}` resources."
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

            // Auto-open payment channel if needed (aligned with query_knowledge)
            if price > 0 {
                let libp2p_peer_opt = preview
                    .provider_peer_id
                    .as_ref()
                    .and_then(|s| s.parse::<LibP2pPeerId>().ok());

                if let Some(libp2p_peer) = libp2p_peer_opt {
                    let existing_nodalync_id = self
                        .network
                        .as_ref()
                        .and_then(|n| n.nodalync_peer_id(&libp2p_peer));

                    let has_channel = existing_nodalync_id
                        .map(|id| ops.has_open_channel(&id).unwrap_or(false))
                        .unwrap_or(false);

                    if !has_channel {
                        let channel_deposit = hbar_to_tinybars(1.0);
                        info!(
                            provider_libp2p = %libp2p_peer,
                            deposit_hbar = 1.0,
                            "Auto-opening payment channel for resource read"
                        );

                        if let Err(e) = ops
                            .open_payment_channel_to_libp2p(libp2p_peer, channel_deposit)
                            .await
                        {
                            warn!(error = %e, "Failed to auto-open payment channel");
                        }
                    }
                } else if preview.manifest.owner != UNKNOWN_PEER_ID {
                    let peer = preview.manifest.owner;
                    if !ops.has_open_channel(&peer).unwrap_or(false) {
                        let channel_deposit = hbar_to_tinybars(1.0);
                        if let Err(e) = ops.open_payment_channel(&peer, channel_deposit).await {
                            warn!(error = %e, "Failed to auto-open payment channel");
                        } else {
                            tokio::time::sleep(Duration::from_millis(500)).await;
                        }
                    }
                }
            }

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

            // Execute query with retry on channel requirement (aligned with query_knowledge)
            let response = match ops.query_content(&hash, price, None).await {
                Ok(r) => r,
                Err(nodalync_ops::OpsError::ChannelRequiredWithPeerInfo {
                    nodalync_peer_id,
                    libp2p_peer_id,
                }) => {
                    info!("Server requires payment channel - auto-opening and retrying");
                    let channel_deposit = hbar_to_tinybars(1.0);

                    if let Some(ref libp2p_str) = libp2p_peer_id {
                        if let Ok(libp2p_peer) = libp2p_str.parse::<LibP2pPeerId>() {
                            let _ = ops
                                .open_payment_channel_to_libp2p(libp2p_peer, channel_deposit)
                                .await;
                        }
                    } else if let Some(ref nodalync_id) = nodalync_peer_id {
                        if let Ok(_channel) =
                            ops.open_payment_channel(nodalync_id, channel_deposit).await
                        {
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
                            return Err(McpError::internal_error(
                                format!("Query failed after channel retry: {}", e),
                                None,
                            ));
                        }
                    }
                }
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

    // Listen on all interfaces with random port (required for incoming peer connections)
    // The request/response protocol requires bidirectional connectivity - peers need
    // to be able to send responses back to us, which requires listening on 0.0.0.0
    Ok(NetworkConfig {
        listen_addresses: vec!["/ip4/0.0.0.0/tcp/0".parse()?],
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
            x402: None,
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

    #[tokio::test]
    async fn test_query_cache_returns_cached_result() {
        let temp_dir = TempDir::new().unwrap();
        let config = test_config(&temp_dir);
        let server = NodalyncMcpServer::new(config).await.unwrap();

        // Publish free content so we can query it
        let input: PublishContentInput = serde_json::from_str(
            r#"{"title": "Cache Test", "content": "Cached content for testing", "price_hbar": 0.0}"#,
        )
        .unwrap();
        let pub_result = server.publish_content(Parameters(input)).await.unwrap();
        assert!(!pub_result.is_error.unwrap_or(false));

        // Extract the hash from the publish output
        let pub_text = match &pub_result.content[0].raw {
            RawContent::Text(t) => t.text.clone(),
            _ => panic!("Expected text content"),
        };
        let pub_json: serde_json::Value = serde_json::from_str(&pub_text).unwrap();
        let hash_str = pub_json["hash"].as_str().unwrap().to_string();

        // Query the content - should cache the result
        let query_input: QueryKnowledgeInput =
            serde_json::from_str(&format!(r#"{{"query": "{}"}}"#, hash_str)).unwrap();
        let result1 = server
            .query_knowledge(Parameters(query_input))
            .await
            .unwrap();
        assert!(!result1.is_error.unwrap_or(false));

        // Verify cache has an entry
        let cache = server.query_cache.lock().await;
        assert_eq!(
            cache.len(),
            1,
            "Cache should have one entry after first query"
        );
        drop(cache);

        // Query again - should return cached result
        let query_input2: QueryKnowledgeInput =
            serde_json::from_str(&format!(r#"{{"query": "{}"}}"#, hash_str)).unwrap();
        let result2 = server
            .query_knowledge(Parameters(query_input2))
            .await
            .unwrap();
        assert!(!result2.is_error.unwrap_or(false));

        // Cache should still have exactly one entry (not two)
        let cache = server.query_cache.lock().await;
        assert_eq!(
            cache.len(),
            1,
            "Cache should still have one entry after second query (cache hit)"
        );

        // Both results should have the same content
        let text1 = match &result1.content[0].raw {
            RawContent::Text(t) => &t.text,
            _ => panic!("Expected text"),
        };
        let text2 = match &result2.content[0].raw {
            RawContent::Text(t) => &t.text,
            _ => panic!("Expected text"),
        };
        assert_eq!(text1, text2, "Cached result should match original");
    }

    fn test_config_with_x402(temp_dir: &TempDir) -> McpServerConfig {
        McpServerConfig {
            budget_hbar: 1.0,
            auto_approve_hbar: 0.01,
            data_dir: temp_dir.path().to_path_buf(),
            enable_network: false,
            bootstrap_nodes: vec![],
            hedera: None,
            x402: Some(X402Config::testnet("0.0.7703962", 5)),
        }
    }

    #[tokio::test]
    async fn test_x402_status_disabled() {
        let temp_dir = TempDir::new().unwrap();
        let config = test_config(&temp_dir);

        let server = NodalyncMcpServer::new(config).await.unwrap();
        let result = server.x402_status().await.unwrap();

        assert!(!result.is_error.unwrap_or(false));

        if let Some(content) = result.content.first() {
            if let RawContent::Text(RawTextContent { text, .. }) = &content.raw {
                let json: serde_json::Value = serde_json::from_str(text).unwrap();
                assert_eq!(json["enabled"], false);
                assert_eq!(json["total_transactions"], 0);
            }
        }
    }

    #[tokio::test]
    async fn test_x402_status_enabled() {
        let temp_dir = TempDir::new().unwrap();
        let config = test_config_with_x402(&temp_dir);

        let server = NodalyncMcpServer::new(config).await.unwrap();
        let result = server.x402_status().await.unwrap();

        assert!(!result.is_error.unwrap_or(false));

        if let Some(content) = result.content.first() {
            if let RawContent::Text(RawTextContent { text, .. }) = &content.raw {
                let json: serde_json::Value = serde_json::from_str(text).unwrap();
                assert_eq!(json["enabled"], true);
                assert_eq!(json["network"], "hedera:testnet");
                assert_eq!(json["account_id"], "0.0.7703962");
                assert_eq!(json["app_fee_percent"], 5);
                assert_eq!(json["total_transactions"], 0);
                assert_eq!(json["total_volume"], 0);
            }
        }
    }

    #[tokio::test]
    async fn test_x402_payment_required_for_paid_content() {
        let temp_dir = TempDir::new().unwrap();
        let config = test_config_with_x402(&temp_dir);

        let server = NodalyncMcpServer::new(config).await.unwrap();

        // Publish paid content (0.01 HBAR per query)
        let input: PublishContentInput = serde_json::from_str(
            r#"{"title": "Paid Knowledge", "content": "This is premium knowledge content", "price_hbar": 0.01}"#,
        ).unwrap();
        let pub_result = server.publish_content(Parameters(input)).await.unwrap();
        assert!(!pub_result.is_error.unwrap_or(false));

        // Extract hash
        let pub_text = match &pub_result.content[0].raw {
            RawContent::Text(t) => t.text.clone(),
            _ => panic!("Expected text content"),
        };
        let pub_json: serde_json::Value = serde_json::from_str(&pub_text).unwrap();
        let hash_str = pub_json["hash"].as_str().unwrap().to_string();

        // Query without x402_payment — should get payment_required response
        let query_input: QueryKnowledgeInput = serde_json::from_str(
            &format!(r#"{{"query": "{}"}}"#, hash_str),
        ).unwrap();
        let result = server.query_knowledge(Parameters(query_input)).await.unwrap();

        // Should NOT be an error — it's a successful response with payment requirements
        assert!(!result.is_error.unwrap_or(false));

        if let Some(content) = result.content.first() {
            if let RawContent::Text(RawTextContent { text, .. }) = &content.raw {
                let json: serde_json::Value = serde_json::from_str(text).unwrap();

                // Verify it's a payment required response
                assert_eq!(json["status"], "payment_required");
                assert_eq!(json["x402_version"], 1);
                assert_eq!(json["content_hash"], hash_str);
                assert_eq!(json["title"], "Paid Knowledge");
                assert_eq!(json["app_fee_percent"], 5);

                // Verify accepts array
                let accepts = json["accepts"].as_array().unwrap();
                assert_eq!(accepts.len(), 1);
                assert_eq!(accepts[0]["scheme"], "exact");
                assert_eq!(accepts[0]["network"], "hedera:testnet");
                assert_eq!(accepts[0]["pay_to"], "0.0.7703962");
                assert_eq!(accepts[0]["asset"], "HBAR");

                // Verify amount: 0.01 HBAR = 1,000,000 tinybars + 5% = 1,050,000
                let amount: u64 = accepts[0]["amount"].as_str().unwrap().parse().unwrap();
                assert_eq!(amount, 1_050_000);

                // Verify instruction mentions retry
                assert!(json["instruction"].as_str().unwrap().contains("x402_payment"));
            } else {
                panic!("Expected text content");
            }
        } else {
            panic!("Expected content in result");
        }
    }

    #[tokio::test]
    async fn test_x402_free_content_bypasses_x402() {
        let temp_dir = TempDir::new().unwrap();
        let config = test_config_with_x402(&temp_dir);

        let server = NodalyncMcpServer::new(config).await.unwrap();

        // Publish FREE content
        let input: PublishContentInput = serde_json::from_str(
            r#"{"title": "Free Knowledge", "content": "This is free knowledge for everyone", "price_hbar": 0.0}"#,
        ).unwrap();
        let pub_result = server.publish_content(Parameters(input)).await.unwrap();
        assert!(!pub_result.is_error.unwrap_or(false));

        let pub_text = match &pub_result.content[0].raw {
            RawContent::Text(t) => t.text.clone(),
            _ => panic!("Expected text content"),
        };
        let pub_json: serde_json::Value = serde_json::from_str(&pub_text).unwrap();
        let hash_str = pub_json["hash"].as_str().unwrap().to_string();

        // Query free content — should bypass x402 and return content directly
        let query_input: QueryKnowledgeInput = serde_json::from_str(
            &format!(r#"{{"query": "{}"}}"#, hash_str),
        ).unwrap();
        let result = server.query_knowledge(Parameters(query_input)).await.unwrap();

        assert!(!result.is_error.unwrap_or(false));

        if let Some(content) = result.content.first() {
            if let RawContent::Text(RawTextContent { text, .. }) = &content.raw {
                let json: serde_json::Value = serde_json::from_str(text).unwrap();

                // Should be actual content, NOT a payment_required response
                assert!(json.get("status").is_none(), "Free content should not return payment_required");
                assert_eq!(json["content"], "This is free knowledge for everyone");
                assert_eq!(json["cost_hbar"], 0.0);
            }
        }
    }

    #[tokio::test]
    async fn test_x402_invalid_payment_rejected() {
        let temp_dir = TempDir::new().unwrap();
        let config = test_config_with_x402(&temp_dir);

        let server = NodalyncMcpServer::new(config).await.unwrap();

        // Publish paid content
        let input: PublishContentInput = serde_json::from_str(
            r#"{"title": "Premium", "content": "Premium content", "price_hbar": 0.01}"#,
        ).unwrap();
        let pub_result = server.publish_content(Parameters(input)).await.unwrap();
        let pub_text = match &pub_result.content[0].raw {
            RawContent::Text(t) => t.text.clone(),
            _ => panic!("Expected text content"),
        };
        let pub_json: serde_json::Value = serde_json::from_str(&pub_text).unwrap();
        let hash_str = pub_json["hash"].as_str().unwrap().to_string();

        // Query with garbage x402_payment — should fail with payment error
        let query_input: QueryKnowledgeInput = serde_json::from_str(
            &format!(r#"{{"query": "{}", "x402_payment": "not-valid-base64-!!!"}}"#, hash_str),
        ).unwrap();
        let result = server.query_knowledge(Parameters(query_input)).await.unwrap();

        // Should be an error
        assert!(result.is_error.unwrap_or(false));

        if let Some(content) = result.content.first() {
            if let RawContent::Text(RawTextContent { text, .. }) = &content.raw {
                let json: serde_json::Value = serde_json::from_str(text).unwrap();
                assert_eq!(json["error"], "PAYMENT_INVALID");
            }
        }
    }

    #[tokio::test]
    async fn test_x402_disabled_paid_content_uses_native_flow() {
        let temp_dir = TempDir::new().unwrap();
        let config = test_config(&temp_dir); // No x402

        let server = NodalyncMcpServer::new(config).await.unwrap();

        // Publish paid content
        let input: PublishContentInput = serde_json::from_str(
            r#"{"title": "Native Paid", "content": "Content via native payment channels", "price_hbar": 0.01}"#,
        ).unwrap();
        let pub_result = server.publish_content(Parameters(input)).await.unwrap();
        let pub_text = match &pub_result.content[0].raw {
            RawContent::Text(t) => t.text.clone(),
            _ => panic!("Expected text content"),
        };
        let pub_json: serde_json::Value = serde_json::from_str(&pub_text).unwrap();
        let hash_str = pub_json["hash"].as_str().unwrap().to_string();

        // Query paid content without x402 — should use native flow (budget check)
        let query_input: QueryKnowledgeInput = serde_json::from_str(
            &format!(r#"{{"query": "{}"}}"#, hash_str),
        ).unwrap();
        let result = server.query_knowledge(Parameters(query_input)).await.unwrap();

        // Should return content (budget check passes, native payment channel flow)
        assert!(!result.is_error.unwrap_or(false));

        if let Some(content) = result.content.first() {
            if let RawContent::Text(RawTextContent { text, .. }) = &content.raw {
                let json: serde_json::Value = serde_json::from_str(text).unwrap();
                // Native flow delivers content directly
                assert_eq!(json["content"], "Content via native payment channels");
                assert!(json.get("status").is_none(), "Should not be payment_required");
            }
        }
    }
}
