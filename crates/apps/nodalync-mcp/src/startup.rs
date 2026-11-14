//! Start-up of the MCP server's node: network, settlement, operations and
//! the background tasks that keep them running.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;
use tracing::{debug, info, warn};

use nodalync_crypto::{PeerId as NodalyncPeerId, PrivateKey, PublicKey};
use nodalync_net::{Network, NetworkConfig, NetworkNode};
use nodalync_ops::DefaultNodeOperations;
use nodalync_store::{ChannelStore, NodeState};

use crate::server::{parse_bootstrap_address, HederaConfig, McpServerConfig};

/// Boxed error returned by start-up steps.
pub(crate) type StartupError = Box<dyn std::error::Error + Send + Sync>;

/// Shared node operations.
pub(crate) type SharedOps = Arc<Mutex<DefaultNodeOperations>>;

/// When the network is enabled, create the network node, subscribe to announcements
/// and bootstrap (with a timeout).
///
/// Subscription and bootstrap failures are logged: the server continues with
/// limited connectivity.
pub(crate) async fn start_network(
    config: &McpServerConfig,
    private_key: &PrivateKey,
    public_key: PublicKey,
) -> Result<Option<Arc<NetworkNode>>, StartupError> {
    if !config.enable_network {
        return Ok(None);
    }
    info!("Initializing network for MCP server...");

    let net_config = build_network_config(config, private_key, &public_key)?;
    let libp2p_keypair = to_libp2p_keypair(private_key)?;
    let node =
        NetworkNode::with_keypair(private_key.clone(), public_key, libp2p_keypair, net_config)
            .await?;

    // Subscribe to announcements
    if let Err(e) = node.subscribe_announcements().await {
        warn!("Failed to subscribe to announcements: {}", e);
    }

    bootstrap_with_timeout(&node).await;
    Ok(Some(Arc::new(node)))
}

/// Bootstrap to the network, giving up after 15 seconds.
async fn bootstrap_with_timeout(node: &NetworkNode) {
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
}

/// Create Hedera settlement if configured.
///
/// Without the `hedera-sdk` feature a Hedera configuration is an error.
pub(crate) async fn create_settlement(
    hedera: Option<&HederaConfig>,
) -> Result<Option<Arc<dyn nodalync_settle::Settlement>>, StartupError> {
    let Some(hedera) = hedera else {
        return Ok(None);
    };
    info!(
        account_id = %hedera.account_id,
        contract_id = %hedera.contract_id,
        network = %hedera.network,
        "Initializing Hedera settlement..."
    );
    hedera_settlement(hedera).await.map(Some)
}

#[cfg(feature = "hedera-sdk")]
async fn hedera_settlement(
    hedera: &HederaConfig,
) -> Result<Arc<dyn nodalync_settle::Settlement>, StartupError> {
    // Create Hedera config
    let hedera_config = nodalync_settle::HederaConfig {
        network: hedera_network(&hedera.network),
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
            std::io::Error::other(format!("Failed to initialize Hedera settlement: {}", e))
        })?;

    info!("Hedera settlement initialized successfully");
    Ok(Arc::new(settlement) as Arc<dyn nodalync_settle::Settlement>)
}

#[cfg(not(feature = "hedera-sdk"))]
async fn hedera_settlement(
    _hedera: &HederaConfig,
) -> Result<Arc<dyn nodalync_settle::Settlement>, StartupError> {
    Err(Box::new(std::io::Error::other(
        "Hedera settlement requires the 'hedera-sdk' feature. \
         Build with: cargo build --features hedera-sdk",
    )))
}

/// The Hedera network named in the configuration; anything unrecognized is testnet.
#[cfg(feature = "hedera-sdk")]
fn hedera_network(name: &str) -> nodalync_settle::HederaNetwork {
    match name.to_lowercase().as_str() {
        "mainnet" => nodalync_settle::HederaNetwork::Mainnet,
        "previewnet" => nodalync_settle::HederaNetwork::Previewnet,
        _ => nodalync_settle::HederaNetwork::Testnet,
    }
}

/// Create operations with network and/or settlement.
pub(crate) fn build_ops(
    state: NodeState,
    peer_id: NodalyncPeerId,
    network: Option<&Arc<NetworkNode>>,
    settlement: Option<&Arc<dyn nodalync_settle::Settlement>>,
) -> DefaultNodeOperations {
    match (network, settlement) {
        (Some(net), Some(settle)) => DefaultNodeOperations::with_defaults_network_and_settlement(
            state,
            peer_id,
            Arc::clone(net) as Arc<dyn Network>,
            Arc::clone(settle),
        ),
        (Some(net), None) => DefaultNodeOperations::with_defaults_and_network(
            state,
            peer_id,
            Arc::clone(net) as Arc<dyn Network>,
        ),
        (None, Some(settle)) => {
            DefaultNodeOperations::with_defaults_and_settlement(state, peer_id, Arc::clone(settle))
        }
        (None, None) => DefaultNodeOperations::with_defaults(state, peer_id),
    }
}

/// Spawn the background tasks a networked server runs: the network event
/// processor, announcement cleanup and periodic settlement.
pub(crate) fn spawn_background_tasks(ops: &SharedOps, network: &Arc<NetworkNode>) {
    tokio::spawn(process_network_events(Arc::clone(ops), Arc::clone(network)));
    tokio::spawn(clean_up_announcements(Arc::clone(ops)));
    tokio::spawn(settle_periodically(Arc::clone(ops)));
}

/// Hand each network event to the operations layer until the network stops.
async fn process_network_events(ops: SharedOps, network: Arc<NetworkNode>) {
    info!("MCP event processor started");
    loop {
        match network.next_event().await {
            Ok(event) => {
                let mut ops_guard = ops.lock().await;
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
}

/// Every hour, remove announcements older than 7 days.
async fn clean_up_announcements(ops: SharedOps) {
    // Cleanup announcements older than 7 days
    const ANNOUNCEMENT_TTL_SECONDS: i64 = 7 * 24 * 60 * 60;
    // Run cleanup every hour
    const CLEANUP_INTERVAL_SECONDS: u64 = 60 * 60;

    loop {
        tokio::time::sleep(Duration::from_secs(CLEANUP_INTERVAL_SECONDS)).await;

        let ops_guard = ops.lock().await;
        let deleted = ops_guard
            .state
            .cleanup_old_announcements(ANNOUNCEMENT_TTL_SECONDS);
        if deleted > 0 {
            info!(deleted = deleted, "Cleaned up old announcements");
        }
        drop(ops_guard);
    }
}

/// Every 5 minutes, trigger a settlement batch while any channel is open.
async fn settle_periodically(ops: SharedOps) {
    // Settlement interval: 5 minutes
    const SETTLEMENT_INTERVAL_SECONDS: u64 = 5 * 60;

    loop {
        tokio::time::sleep(Duration::from_secs(SETTLEMENT_INTERVAL_SECONDS)).await;
        settle_once(&ops).await;
    }
}

/// Trigger one settlement batch if any channel is open.
///
/// This settles channels that have exceeded the threshold (100 HBAR) or time limit (1 hour).
async fn settle_once(ops: &SharedOps) {
    let mut ops_guard = ops.lock().await;

    // Check if there are any channels that need settlement
    let channels = ops_guard.state.channels.list_open().unwrap_or_default();
    if channels.is_empty() {
        return;
    }

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
