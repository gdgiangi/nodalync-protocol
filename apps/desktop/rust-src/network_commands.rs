//! Tauri IPC commands for network management and peering.
//!
//! These commands expose peer discovery, connection management,
//! and network configuration to the React frontend.

use nodalync_net::{Network, NetworkConfig, NetworkNode};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::State;
use tokio::sync::Mutex;
use tracing::info;

use crate::protocol::ProtocolState;

// ─── Response Types ──────────────────────────────────────────────────────────

/// Detailed network info returned to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkInfo {
    pub active: bool,
    pub listen_addresses: Vec<String>,
    pub connected_peers: Vec<PeerInfo>,
    pub peer_count: usize,
}

/// Info about a connected peer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    pub libp2p_id: String,
    pub nodalync_id: Option<String>,
}

/// Result of dialing a peer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DialResult {
    pub success: bool,
    pub address: String,
    pub error: Option<String>,
}

// ─── Network Info Command ────────────────────────────────────────────────────

/// Get detailed network information including listen addresses.
///
/// This is what a user needs to share with others for peering:
/// their listen addresses can be used as bootstrap nodes.
#[tauri::command]
pub async fn get_network_info(
    protocol: State<'_, Mutex<Option<ProtocolState>>>,
) -> Result<NetworkInfo, String> {
    let guard = protocol.lock().await;
    let state = guard
        .as_ref()
        .ok_or("Node not initialized — unlock first")?;

    match &state.network {
        Some(network) => {
            let listen_addrs: Vec<String> = network
                .listen_addresses()
                .iter()
                .map(|a| a.to_string())
                .collect();

            let peers: Vec<PeerInfo> = network
                .connected_peers()
                .iter()
                .map(|p| {
                    let nodalync_id = network.nodalync_peer_id(p).map(|id| id.to_string());
                    PeerInfo {
                        libp2p_id: p.to_string(),
                        nodalync_id,
                    }
                })
                .collect();

            let peer_count = peers.len();

            Ok(NetworkInfo {
                active: true,
                listen_addresses: listen_addrs,
                connected_peers: peers,
                peer_count,
            })
        }
        None => Ok(NetworkInfo {
            active: false,
            listen_addresses: vec![],
            connected_peers: vec![],
            peer_count: 0,
        }),
    }
}

// ─── Start Network with Config ───────────────────────────────────────────────

/// Start the P2P network with optional configuration.
///
/// Allows specifying a listen port and bootstrap nodes.
/// If no port is given, a random port is used.
/// Bootstrap nodes should be in the format: "peer_id@/ip4/x.x.x.x/tcp/port"
#[tauri::command]
pub async fn start_network_configured(
    listen_port: Option<u16>,
    bootstrap_nodes: Option<Vec<String>>,
    protocol: State<'_, Mutex<Option<ProtocolState>>>,
) -> Result<NetworkInfo, String> {
    // Check node is initialized
    {
        let guard = protocol.lock().await;
        if guard.is_none() {
            return Err("Node not initialized — unlock first".into());
        }
        if guard.as_ref().unwrap().network.is_some() {
            return Err("Network already running. Stop it first.".into());
        }
    }

    // Build config
    let mut config = NetworkConfig::default();

    if let Some(port) = listen_port {
        let addr: nodalync_net::Multiaddr = format!("/ip4/0.0.0.0/tcp/{}", port)
            .parse()
            .map_err(|e| format!("Invalid listen address: {}", e))?;
        config.listen_addresses = vec![addr];
    }

    if let Some(nodes) = bootstrap_nodes {
        for node_str in nodes {
            let (peer_id, addr) = parse_bootstrap_node(&node_str)?;
            config.bootstrap_nodes.push((peer_id, addr));
        }
    }

    info!(
        "Starting network: port={:?}, bootstrap_count={}",
        listen_port,
        config.bootstrap_nodes.len()
    );

    // Create network node (async — outside lock)
    let node = NetworkNode::new(config)
        .await
        .map_err(|e| format!("Failed to create network node: {}", e))?;

    // Bootstrap if we have bootstrap nodes
    let has_bootstrap = !node.listen_addresses().is_empty();

    let node = Arc::new(node);

    // Store in protocol state
    {
        let mut guard = protocol.lock().await;
        let state = guard
            .as_mut()
            .ok_or("Node not initialized — unlock first")?;
        state.ops.set_network(node.clone());
        state.network = Some(node.clone());
    }

    // Bootstrap after storing (so peer connections work)
    if has_bootstrap {
        if let Err(e) = node.bootstrap().await {
            info!("Bootstrap note: {} (may be first node in network)", e);
        }
    }

    // Subscribe to announcements
    if let Err(e) = node.subscribe_announcements().await {
        info!("Announcement subscription note: {}", e);
    }

    // Return network info
    let listen_addrs: Vec<String> = node
        .listen_addresses()
        .iter()
        .map(|a| a.to_string())
        .collect();
    let peers: Vec<PeerInfo> = node
        .connected_peers()
        .iter()
        .map(|p| {
            let nodalync_id = node.nodalync_peer_id(p).map(|id| id.to_string());
            PeerInfo {
                libp2p_id: p.to_string(),
                nodalync_id,
            }
        })
        .collect();
    let peer_count = peers.len();

    info!(
        "Network started: {} listen addresses, {} peers",
        listen_addrs.len(),
        peer_count
    );

    Ok(NetworkInfo {
        active: true,
        listen_addresses: listen_addrs,
        connected_peers: peers,
        peer_count,
    })
}

// ─── Dial Peer Command ───────────────────────────────────────────────────────

/// Dial a specific multiaddress to connect to a peer.
///
/// Used for manual peering when the user has a peer's address.
#[tauri::command]
pub async fn dial_peer(
    address: String,
    protocol: State<'_, Mutex<Option<ProtocolState>>>,
) -> Result<DialResult, String> {
    let guard = protocol.lock().await;
    let state = guard
        .as_ref()
        .ok_or("Node not initialized — unlock first")?;

    let network = state
        .network
        .as_ref()
        .ok_or("Network not running — start it first")?;

    let addr: nodalync_net::Multiaddr = address
        .parse()
        .map_err(|e| format!("Invalid multiaddress: {}", e))?;

    match network.dial(addr).await {
        Ok(()) => {
            info!("Dial succeeded: {}", address);
            Ok(DialResult {
                success: true,
                address,
                error: None,
            })
        }
        Err(e) => {
            info!("Dial failed: {} — {}", address, e);
            Ok(DialResult {
                success: false,
                address,
                error: Some(e.to_string()),
            })
        }
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Parse a bootstrap node string in format "peer_id@multiaddr"
/// e.g. "12D3KooW...@/ip4/192.168.1.5/tcp/9000"
fn parse_bootstrap_node(
    s: &str,
) -> Result<(nodalync_net::PeerId, nodalync_net::Multiaddr), String> {
    let parts: Vec<&str> = s.splitn(2, '@').collect();
    if parts.len() != 2 {
        return Err(format!(
            "Invalid bootstrap node format. Expected 'peer_id@/ip4/x.x.x.x/tcp/port', got: {}",
            s
        ));
    }

    let peer_id: nodalync_net::PeerId = parts[0]
        .parse()
        .map_err(|e| format!("Invalid peer ID '{}': {}", parts[0], e))?;

    let addr: nodalync_net::Multiaddr = parts[1]
        .parse()
        .map_err(|e| format!("Invalid multiaddr '{}': {}", parts[1], e))?;

    Ok((peer_id, addr))
}
