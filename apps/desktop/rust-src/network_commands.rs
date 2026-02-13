//! Tauri IPC commands for network management and peering.
//!
//! These commands expose peer discovery, connection management,
//! and network configuration to the React frontend.
//! Includes peer persistence: known peers are saved to disk and
//! used as bootstrap nodes on next startup.

use nodalync_net::{Network, NetworkConfig, NetworkNode};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::State;
use tokio::sync::Mutex;
use tracing::{info, warn};

use crate::peer_store::PeerStore;
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
    protocol: State<'_, Arc<Mutex<Option<ProtocolState>>>>,
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
    protocol: State<'_, Arc<Mutex<Option<ProtocolState>>>>,
    event_loop: State<'_, Mutex<Option<crate::event_loop::EventLoopHandle>>>,
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

    // Spawn the network event loop for inbound request handling
    let protocol_arc = Arc::clone(&*protocol);
    let handle = crate::event_loop::spawn_event_loop(node.clone(), protocol_arc);
    {
        let mut el_guard = event_loop.lock().await;
        *el_guard = Some(handle);
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
    protocol: State<'_, Arc<Mutex<Option<ProtocolState>>>>,
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

// ─── Peer Persistence Commands ───────────────────────────────────────────────

/// Save currently connected peers to disk for reconnection on restart.
///
/// Called automatically on network stop and on app shutdown.
/// Can also be called periodically by the frontend.
#[tauri::command]
pub async fn save_known_peers(
    protocol: State<'_, Mutex<Option<ProtocolState>>>,
) -> Result<usize, String> {
    let guard = protocol.lock().await;
    let state = guard
        .as_ref()
        .ok_or("Node not initialized — unlock first")?;

    let network = match &state.network {
        Some(n) => n,
        None => return Ok(0), // No network running, nothing to save
    };

    let mut store = PeerStore::load(&state.data_dir);

    // Record all currently connected peers
    let peers = network.connected_peers();
    let listen_addrs = network.listen_addresses();

    for peer in &peers {
        let peer_str = peer.to_string();
        let nodalync_id = network.nodalync_peer_id(peer).map(|id| id.to_string());

        // We don't have per-peer addresses from the Network trait,
        // so record with empty addresses for now (existing addresses are preserved)
        store.record_peer(&peer_str, vec![], nodalync_id, false);
    }

    // Prune peers not seen in 30 days
    store.prune_stale(30);

    store.save(&state.data_dir)?;
    Ok(store.peers.len())
}

/// Get the list of known peers from the persistent store.
#[tauri::command]
pub async fn get_known_peers(
    protocol: State<'_, Mutex<Option<ProtocolState>>>,
) -> Result<Vec<KnownPeerInfo>, String> {
    let guard = protocol.lock().await;
    let state = guard
        .as_ref()
        .ok_or("Node not initialized — unlock first")?;

    let store = PeerStore::load(&state.data_dir);

    let peers: Vec<KnownPeerInfo> = store
        .peers
        .values()
        .map(|p| KnownPeerInfo {
            peer_id: p.peer_id.clone(),
            addresses: p.addresses.clone(),
            nodalync_id: p.nodalync_id.clone(),
            last_seen: p.last_seen.to_rfc3339(),
            connection_count: p.connection_count,
            manual: p.manual,
        })
        .collect();

    Ok(peers)
}

/// Add a peer manually to the known peers store.
///
/// Used when a user enters a peer address from the UI.
/// The peer will be used as a bootstrap node on next network start.
#[tauri::command]
pub async fn add_known_peer(
    peer_id: String,
    address: String,
    protocol: State<'_, Mutex<Option<ProtocolState>>>,
) -> Result<(), String> {
    let guard = protocol.lock().await;
    let state = guard
        .as_ref()
        .ok_or("Node not initialized — unlock first")?;

    // Validate the peer ID and address
    let _: nodalync_net::PeerId = peer_id
        .parse()
        .map_err(|e| format!("Invalid peer ID: {}", e))?;
    let _: nodalync_net::Multiaddr = address
        .parse()
        .map_err(|e| format!("Invalid multiaddr: {}", e))?;

    let mut store = PeerStore::load(&state.data_dir);
    store.record_peer(&peer_id, vec![address], None, true);
    store.save(&state.data_dir)?;

    info!("Manually added known peer: {}", peer_id);
    Ok(())
}

/// Start the network with auto-discovery enabled.
///
/// This is the recommended way to start the network for the desktop app:
/// 1. Loads known peers from disk as bootstrap nodes
/// 2. Enables mDNS for LAN discovery
/// 3. Starts the P2P network
/// 4. Returns network info
///
/// Hephaestus should call this after unlock for seamless networking.
#[tauri::command]
pub async fn auto_start_network(
    listen_port: Option<u16>,
    protocol: State<'_, Mutex<Option<ProtocolState>>>,
) -> Result<NetworkInfo, String> {
    // Check node is initialized and network isn't already running
    let data_dir = {
        let guard = protocol.lock().await;
        let state = guard
            .as_ref()
            .ok_or("Node not initialized — unlock first")?;
        if state.network.is_some() {
            return Err("Network already running. Stop it first.".into());
        }
        state.data_dir.clone()
    };

    // Load known peers
    let store = PeerStore::load(&data_dir);
    let bootstrap_entries = store.bootstrap_entries(20);

    // Build config with mDNS enabled and known peers as bootstrap
    let mut config = NetworkConfig::default().with_mdns(true);

    if let Some(port) = listen_port {
        let addr: nodalync_net::Multiaddr = format!("/ip4/0.0.0.0/tcp/{}", port)
            .parse()
            .map_err(|e| format!("Invalid listen address: {}", e))?;
        config.listen_addresses = vec![addr];
    }

    // Add known peers as bootstrap nodes
    let mut bootstrap_count = 0;
    for (peer_id_str, addr_str) in &bootstrap_entries {
        match (peer_id_str.parse(), addr_str.parse()) {
            (Ok(pid), Ok(addr)) => {
                config.bootstrap_nodes.push((pid, addr));
                bootstrap_count += 1;
            }
            (Err(e), _) => warn!("Skipping saved peer (bad peer ID): {}", e),
            (_, Err(e)) => warn!("Skipping saved peer (bad address): {}", e),
        }
    }

    info!(
        "Auto-starting network: port={:?}, mDNS=true, bootstrap_peers={}",
        listen_port, bootstrap_count
    );

    // Create network node
    let node = NetworkNode::new(config)
        .await
        .map_err(|e| format!("Failed to create network node: {}", e))?;

    let node = Arc::new(node);

    // Bootstrap if we have bootstrap nodes
    if bootstrap_count > 0 {
        if let Err(e) = node.bootstrap().await {
            info!("Bootstrap note: {} (may be first node in network)", e);
        }
    }

    // Subscribe to announcements
    if let Err(e) = node.subscribe_announcements().await {
        info!("Announcement subscription note: {}", e);
    }

    // Collect info before storing (avoid holding lock across await)
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

    // Store in protocol state
    {
        let mut guard = protocol.lock().await;
        let state = guard
            .as_mut()
            .ok_or("Node not initialized — unlock first")?;
        state.ops.set_network(node.clone());
        state.network = Some(node);
    }

    info!(
        "Network auto-started: {} listen addresses, {} initial peers, mDNS enabled",
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

/// Known peer info returned to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnownPeerInfo {
    pub peer_id: String,
    pub addresses: Vec<String>,
    pub nodalync_id: Option<String>,
    pub last_seen: String,
    pub connection_count: u32,
    pub manual: bool,
}
