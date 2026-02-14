//! Tauri IPC commands for network management and peering.
//!
//! These commands expose peer discovery, connection management,
//! and network configuration to the React frontend.
//! Includes peer persistence: known peers are saved to disk and
//! used as bootstrap nodes on next startup.
//! Includes seed node management for first-run network discovery.

use nodalync_net::{Network, NetworkConfig, NetworkNode};
use nodalync_store::ManifestStore;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::State;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

use crate::health_monitor::{self, NetworkHealth, SharedHealth};
use crate::invite;
use crate::peer_store::PeerStore;
use crate::protocol::ProtocolState;
use crate::seed_store::{SeedNode, SeedSource, SeedStore};

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
    /// Protocol version from handshake (None if handshake not yet complete)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol_version: Option<String>,
    /// Number of content items the peer hosts
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_count: Option<u64>,
    /// Node display name from handshake
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_name: Option<String>,
    /// Whether we have the peer's public key (handshake complete)
    pub handshake_complete: bool,
}

impl PeerInfo {
    /// Build PeerInfo from a libp2p peer, enriching with handshake data from the store.
    fn from_peer(
        p: &nodalync_net::PeerId,
        network: &dyn Network,
        ops: Option<&crate::protocol::ProtocolState>,
    ) -> Self {
        let nodalync_id = network.nodalync_peer_id(p).map(|id| id.to_string());

        // Try to look up stored peer info from handshake
        let stored = nodalync_id.as_ref().and_then(|_| {
            let nid = network.nodalync_peer_id(p)?;
            ops.and_then(|s| {
                use nodalync_store::PeerStore;
                s.ops.state().peers.get(&nid).ok().flatten()
            })
        });

        let handshake_complete = stored.as_ref().map_or(false, |s| s.public_key.0 != [0u8; 32]);

        PeerInfo {
            libp2p_id: p.to_string(),
            nodalync_id,
            protocol_version: None, // Not stored in PeerInfo struct yet
            content_count: None,    // Not stored in PeerInfo struct yet
            node_name: None,        // Not stored in PeerInfo struct yet
            handshake_complete,
        }
    }
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
                .map(|p| PeerInfo::from_peer(p, network.as_ref(), Some(state)))
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
    // Check node is initialized and get identity secret
    let identity_secret = {
        let guard = protocol.lock().await;
        if guard.is_none() {
            return Err("Node not initialized — unlock first".into());
        }
        let state = guard.as_ref().unwrap();
        if state.network.is_some() {
            return Err("Network already running. Stop it first.".into());
        }
        state.ops.private_key().map(|k| *k.as_bytes())
    };

    // Build config with stable identity
    let mut config = NetworkConfig::default();

    if let Some(secret) = identity_secret {
        config = config.with_identity_secret(secret);
    }

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
                protocol_version: None,
                content_count: None,
                node_name: None,
                handshake_complete: false,
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
    protocol: State<'_, Arc<Mutex<Option<ProtocolState>>>>,
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
    protocol: State<'_, Arc<Mutex<Option<ProtocolState>>>>,
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
    protocol: State<'_, Arc<Mutex<Option<ProtocolState>>>>,
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
/// 4. Spawns the event loop for inbound request handling
/// 5. Returns network info
///
/// Hephaestus should call this after unlock for seamless networking.
#[tauri::command]
pub async fn auto_start_network(
    listen_port: Option<u16>,
    protocol: State<'_, Arc<Mutex<Option<ProtocolState>>>>,
    event_loop: State<'_, Mutex<Option<crate::event_loop::EventLoopHandle>>>,
    health_monitor: State<'_, Mutex<Option<health_monitor::HealthMonitorHandle>>>,
    shared_health: State<'_, SharedHealth>,
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

    // Get identity secret for stable PeerId
    let identity_secret = {
        let guard = protocol.lock().await;
        let state = guard.as_ref().ok_or("Node not initialized — unlock first")?;
        state.ops.private_key().map(|k| *k.as_bytes())
    };

    // Load seed nodes (builtin testnet seeds + user-configured)
    let seed_store = SeedStore::load(&data_dir);
    let seed_entries = seed_store.bootstrap_entries();

    // Load known peers (previously connected peers)
    let peer_store = PeerStore::load(&data_dir);
    let peer_entries = peer_store.bootstrap_entries(20);

    // Build config with mDNS enabled, known peers as bootstrap, and stable identity
    let mut config = NetworkConfig::default().with_mdns(true);

    // Use the node's identity for a stable PeerId across restarts
    if let Some(secret) = identity_secret {
        config = config.with_identity_secret(secret);
        info!("Using persistent identity for stable PeerId");
    }

    if let Some(port) = listen_port {
        let addr: nodalync_net::Multiaddr = format!("/ip4/0.0.0.0/tcp/{}", port)
            .parse()
            .map_err(|e| format!("Invalid listen address: {}", e))?;
        config.listen_addresses = vec![addr];
    }

    // Priority 1: Add seed nodes as bootstrap (highest priority — enable first-run discovery)
    let mut seed_count = 0;
    for (peer_id_str, addr_str) in &seed_entries {
        match (peer_id_str.parse(), addr_str.parse()) {
            (Ok(pid), Ok(addr)) => {
                config.bootstrap_nodes.push((pid, addr));
                seed_count += 1;
            }
            (Err(e), _) => warn!("Skipping seed (bad peer ID): {}", e),
            (_, Err(e)) => warn!("Skipping seed (bad address): {}", e),
        }
    }

    // Priority 2: Add known peers as bootstrap (avoid duplicates with seeds)
    let mut peer_count_bootstrap = 0;
    let seed_peer_ids: std::collections::HashSet<&str> =
        seed_entries.iter().map(|(pid, _)| *pid).collect();
    for (peer_id_str, addr_str) in &peer_entries {
        if seed_peer_ids.contains(peer_id_str) {
            debug!("Skipping known peer {} (already in seeds)", peer_id_str);
            continue;
        }
        match (peer_id_str.parse(), addr_str.parse()) {
            (Ok(pid), Ok(addr)) => {
                config.bootstrap_nodes.push((pid, addr));
                peer_count_bootstrap += 1;
            }
            (Err(e), _) => warn!("Skipping saved peer (bad peer ID): {}", e),
            (_, Err(e)) => warn!("Skipping saved peer (bad address): {}", e),
        }
    }

    let bootstrap_count = seed_count + peer_count_bootstrap;

    info!(
        "Auto-starting network: port={:?}, mDNS=true, seeds={}, known_peers={}, total_bootstrap={}",
        listen_port, seed_count, peer_count_bootstrap, bootstrap_count
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
                protocol_version: None,
                content_count: None,
                node_name: None,
                handshake_complete: false,
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
        state.network = Some(node.clone());
    }

    // Spawn the network event loop for inbound request handling
    // Without this, the node can send but never respond to peer requests.
    let protocol_arc = Arc::clone(&*protocol);
    let handle = crate::event_loop::spawn_event_loop(node.clone(), protocol_arc);
    {
        let mut el_guard = event_loop.lock().await;
        *el_guard = Some(handle);
    }

    // Spawn the background health monitor (30s health checks + auto-reconnect)
    {
        let protocol_arc = Arc::clone(&*protocol);
        let health_clone = Arc::clone(&*shared_health);
        let hm_handle = health_monitor::spawn_health_monitor(
            node.clone(),
            protocol_arc,
            health_clone,
        );
        let mut hm_guard = health_monitor.lock().await;
        *hm_guard = Some(hm_handle);
    }

    // Re-announce all Shared content so peers can discover us
    {
        let mut guard = protocol.lock().await;
        if let Some(state) = guard.as_mut() {
            match state.ops.reannounce_all_content().await {
                Ok(count) if count > 0 => {
                    info!("Re-announced {} content items to network", count);
                }
                Ok(_) => {} // No content to re-announce
                Err(e) => {
                    warn!("Content re-announcement failed (network still active): {}", e);
                }
            }
        }
    }

    info!(
        "Network auto-started: {} listen addresses, {} initial peers, mDNS enabled, event loop active",
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

/// Re-announce all published content to the network.
///
/// Use after network start or when peers may have lost track of our content.
/// Returns the number of content items re-announced.
#[tauri::command]
pub async fn reannounce_content(
    protocol: State<'_, Arc<Mutex<Option<ProtocolState>>>>,
) -> Result<u32, String> {
    let mut guard = protocol.lock().await;
    let state = guard
        .as_mut()
        .ok_or("Node not initialized — unlock first")?;

    let count = state
        .ops
        .reannounce_all_content()
        .await
        .map_err(|e| format!("Re-announcement failed: {}", e))?;

    info!("Manually re-announced {} content items", count);
    Ok(count)
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

// ─── NAT Status Command ─────────────────────────────────────────────────────

/// NAT status information returned to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NatStatusInfo {
    /// "unknown", "public", or "private"
    pub status: String,
    /// True if NAT traversal (relay/UPnP/DCUtR) is enabled
    pub nat_traversal_enabled: bool,
    /// Number of relay reservations active
    pub relay_reservations: usize,
}

/// Get the current NAT status as detected by AutoNAT.
///
/// This tells the frontend whether the node is:
/// - **public**: directly reachable from the internet
/// - **private**: behind NAT, uses relay/hole-punching
/// - **unknown**: probing in progress
///
/// Use this for the network status display in the graph view.
#[tauri::command]
pub async fn get_nat_status(
    protocol: State<'_, Arc<Mutex<Option<ProtocolState>>>>,
) -> Result<NatStatusInfo, String> {
    let guard = protocol.lock().await;
    let state = guard
        .as_ref()
        .ok_or("Node not initialized — unlock first")?;

    match &state.network {
        Some(network) => {
            let status = network.nat_status();
            Ok(NatStatusInfo {
                status: status.to_string(),
                nat_traversal_enabled: true,
                relay_reservations: 0, // TODO: track active reservations
            })
        }
        None => Ok(NatStatusInfo {
            status: "unknown".to_string(),
            nat_traversal_enabled: false,
            relay_reservations: 0,
        }),
    }
}

// ─── Network Health Command ──────────────────────────────────────────────────

/// Get the current network health status.
///
/// Returns a snapshot from the background health monitor, including:
/// - Connection count and known peers
/// - Uptime, reconnect stats
/// - Health classification ("healthy", "degraded", "disconnected", "offline")
///
/// Hephaestus: poll this from the frontend every 10-30s for the network
/// status indicator. It's cheap — just reads a pre-computed snapshot.
#[tauri::command]
pub async fn get_network_health(
    shared_health: State<'_, SharedHealth>,
) -> Result<NetworkHealth, String> {
    let health = shared_health.lock().await;
    Ok(health.clone())
}

// ─── Seed Node Commands ──────────────────────────────────────────────────────

/// Seed node info returned to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeedNodeInfo {
    pub peer_id: String,
    pub address: String,
    pub label: Option<String>,
    pub source: String,
    pub enabled: bool,
    pub added_at: String,
}

impl From<&SeedNode> for SeedNodeInfo {
    fn from(seed: &SeedNode) -> Self {
        Self {
            peer_id: seed.peer_id.clone(),
            address: seed.address.clone(),
            label: seed.label.clone(),
            source: match seed.source {
                SeedSource::Builtin => "builtin".to_string(),
                SeedSource::User => "user".to_string(),
                SeedSource::Dns => "dns".to_string(),
                SeedSource::PeerExchange => "peer_exchange".to_string(),
            },
            enabled: seed.enabled,
            added_at: seed.added_at.to_rfc3339(),
        }
    }
}

/// Get all configured seed nodes.
///
/// Returns builtin testnet seeds + any user-added seeds.
/// Use this to display seed configuration in the network settings UI.
#[tauri::command]
pub async fn get_seed_nodes(
    protocol: State<'_, Arc<Mutex<Option<ProtocolState>>>>,
) -> Result<Vec<SeedNodeInfo>, String> {
    let guard = protocol.lock().await;
    let state = guard
        .as_ref()
        .ok_or("Node not initialized — unlock first")?;

    let store = SeedStore::load(&state.data_dir);
    Ok(store.seeds.iter().map(SeedNodeInfo::from).collect())
}

/// Add a seed node for network discovery.
///
/// The seed will be used as a bootstrap node on the next network start.
/// If the network is currently running, the seed is saved but not used
/// until the next restart.
///
/// Peer ID: libp2p PeerId string (e.g. "12D3KooW...")
/// Address: Multiaddr (e.g. "/ip4/x.x.x.x/tcp/9000" or "/dns4/seed.nodalync.io/tcp/9000")
#[tauri::command]
pub async fn add_seed_node(
    peer_id: String,
    address: String,
    label: Option<String>,
    protocol: State<'_, Arc<Mutex<Option<ProtocolState>>>>,
) -> Result<Vec<SeedNodeInfo>, String> {
    let guard = protocol.lock().await;
    let state = guard
        .as_ref()
        .ok_or("Node not initialized — unlock first")?;

    let mut store = SeedStore::load(&state.data_dir);
    store.add_seed(peer_id, address, label)?;
    store
        .save(&state.data_dir)
        .map_err(|e| format!("Failed to save seeds: {}", e))?;

    Ok(store.seeds.iter().map(SeedNodeInfo::from).collect())
}

/// Remove a seed node. Builtin seeds are disabled instead of removed.
#[tauri::command]
pub async fn remove_seed_node(
    peer_id: String,
    protocol: State<'_, Arc<Mutex<Option<ProtocolState>>>>,
) -> Result<Vec<SeedNodeInfo>, String> {
    let guard = protocol.lock().await;
    let state = guard
        .as_ref()
        .ok_or("Node not initialized — unlock first")?;

    let mut store = SeedStore::load(&state.data_dir);
    store.remove_seed(&peer_id)?;
    store
        .save(&state.data_dir)
        .map_err(|e| format!("Failed to save seeds: {}", e))?;

    Ok(store.seeds.iter().map(SeedNodeInfo::from).collect())
}

/// Network diagnostics — analyze why the node can't find peers.
///
/// Checks seed nodes, known peers, NAT status, and network health
/// to produce actionable diagnostic info.
#[tauri::command]
pub async fn diagnose_network(
    protocol: State<'_, Arc<Mutex<Option<ProtocolState>>>>,
    shared_health: State<'_, SharedHealth>,
) -> Result<NetworkDiagnostics, String> {
    let guard = protocol.lock().await;
    let state = guard
        .as_ref()
        .ok_or("Node not initialized — unlock first")?;

    let seed_store = SeedStore::load(&state.data_dir);
    let peer_store = PeerStore::load(&state.data_dir);
    let health = shared_health.lock().await;

    let mut issues: Vec<String> = Vec::new();
    let mut suggestions: Vec<String> = Vec::new();

    // Check seeds
    let enabled_seeds = seed_store.enabled_count();
    if enabled_seeds == 0 {
        issues.push("No seed nodes configured. Cannot discover the network.".to_string());
        suggestions.push(
            "Add a seed node: invoke('add_seed_node', { peer_id: '...', address: '...' })"
                .to_string(),
        );
    }

    // Check known peers
    let known_peer_count = peer_store.peers.len();
    if known_peer_count == 0 && enabled_seeds == 0 {
        issues.push("No known peers and no seeds. Node is isolated.".to_string());
        suggestions.push(
            "If another node is on your LAN, mDNS will find it. Otherwise, add a seed node."
                .to_string(),
        );
    }

    // Check network state
    let network_active = state.network.is_some();
    if !network_active {
        issues.push("Network is not running.".to_string());
        suggestions.push("Start the network: invoke('auto_start_network')".to_string());
    }

    // Check NAT
    let nat_status = if let Some(network) = &state.network {
        network.nat_status().to_string()
    } else {
        "offline".to_string()
    };

    if nat_status == "private" {
        suggestions.push(
            "Node is behind NAT. Relay and hole-punching are active but connections may be slower."
                .to_string(),
        );
    }

    // Check health
    let connected = health.connected_peers;
    if network_active && connected == 0 {
        issues.push("Network is running but no peers connected.".to_string());
        if enabled_seeds > 0 {
            suggestions.push("Seeds are configured but unreachable. Check your internet connection or verify seed addresses.".to_string());
        }
    }

    let overall = if issues.is_empty() {
        "healthy".to_string()
    } else if connected > 0 {
        "degraded".to_string()
    } else {
        "disconnected".to_string()
    };

    Ok(NetworkDiagnostics {
        overall_status: overall,
        network_active,
        connected_peers: connected,
        known_peers: known_peer_count,
        seed_nodes: enabled_seeds,
        nat_status,
        issues,
        suggestions,
    })
}

/// Network diagnostics result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkDiagnostics {
    pub overall_status: String,
    pub network_active: bool,
    pub connected_peers: usize,
    pub known_peers: usize,
    pub seed_nodes: usize,
    pub nat_status: String,
    pub issues: Vec<String>,
    pub suggestions: Vec<String>,
}

// ─── Connection Invite Commands ──────────────────────────────────────────────

/// Invite info returned to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InviteInfo {
    /// Compact invite string (single best address).
    pub compact: String,
    /// Full invite string (all addresses, metadata).
    pub full: String,
    /// This node's peer ID.
    pub peer_id: String,
    /// Listen addresses included in the invite.
    pub addresses: Vec<String>,
}

/// Generate a connection invite that another user can paste to connect.
///
/// The invite encodes this node's peer ID and listen addresses.
/// Two formats are returned:
/// - **compact**: Short, single-address. Good for messaging.
/// - **full**: All addresses + metadata. More robust for NAT scenarios.
///
/// Requires the network to be running (needs listen addresses).
#[tauri::command]
pub async fn generate_invite(
    protocol: State<'_, Arc<Mutex<Option<ProtocolState>>>>,
) -> Result<InviteInfo, String> {
    let guard = protocol.lock().await;
    let state = guard
        .as_ref()
        .ok_or("Node not initialized — unlock first")?;

    let network = state
        .network
        .as_ref()
        .ok_or("Network not running. Start the network first.")?;

    let peer_id = network.local_peer_id().to_string();
    let listen_addrs: Vec<String> = network
        .listen_addresses()
        .iter()
        .map(|a| a.to_string())
        .collect();

    if listen_addrs.is_empty() {
        return Err("No listen addresses available. Wait for network to fully start.".to_string());
    }

    // Pick the best address for compact invite:
    // Prefer public IP > private IP > localhost
    let best_addr = pick_best_address(&listen_addrs);

    let compact = invite::generate_compact_invite(&peer_id, &best_addr);
    let full = invite::generate_full_invite(
        &peer_id,
        listen_addrs.clone(),
        None, // Could read node name from identity
    )?;

    Ok(InviteInfo {
        compact,
        full,
        peer_id,
        addresses: listen_addrs,
    })
}

/// Accept a connection invite from another user.
///
/// Parses the invite string, adds the peer to known peers, and
/// attempts to dial them immediately if the network is running.
///
/// Returns the parsed invite data + connection result.
#[tauri::command]
pub async fn accept_invite(
    invite_string: String,
    protocol: State<'_, Arc<Mutex<Option<ProtocolState>>>>,
) -> Result<AcceptInviteResult, String> {
    let data = invite::parse_invite(&invite_string)?;

    // Add to known peers
    {
        let guard = protocol.lock().await;
        let state = guard
            .as_ref()
            .ok_or("Node not initialized — unlock first")?;

        let mut store = PeerStore::load(&state.data_dir);
        store.record_peer(&data.pid, data.addrs.clone(), None, true);
        store
            .save(&state.data_dir)
            .map_err(|e| format!("Failed to save peer: {}", e))?;
    }

    // Try to connect immediately if network is running
    let connected = {
        let guard = protocol.lock().await;
        let state = guard.as_ref().ok_or("Node not initialized")?;

        if let Some(network) = &state.network {
            let mut connected = false;
            for addr_str in &data.addrs {
                if let Ok(addr) = addr_str.parse::<nodalync_net::Multiaddr>() {
                    match network.dial(addr).await {
                        Ok(_) => {
                            info!("Connected to invited peer {} via {}", data.pid, addr_str);
                            connected = true;
                            break;
                        }
                        Err(e) => {
                            warn!("Failed to dial {} via {}: {}", data.pid, addr_str, e);
                        }
                    }
                }
            }
            connected
        } else {
            false // Network not running — peer saved for next start
        }
    };

    Ok(AcceptInviteResult {
        peer_id: data.pid,
        name: data.name,
        addresses: data.addrs,
        connected,
        saved: true,
    })
}

/// Result of accepting a connection invite.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcceptInviteResult {
    /// Peer ID from the invite.
    pub peer_id: String,
    /// Node name from the invite (if provided).
    pub name: Option<String>,
    /// Addresses from the invite.
    pub addresses: Vec<String>,
    /// Whether we successfully connected right now.
    pub connected: bool,
    /// Whether the peer was saved for future reconnection.
    pub saved: bool,
}

/// Pick the best listen address for a compact invite.
///
/// Priority: public IP > private IP > localhost.
/// Avoids including localhost addresses which are useless to external users.
fn pick_best_address(addresses: &[String]) -> String {
    // Classify addresses
    let mut public = Vec::new();
    let mut private = Vec::new();
    let mut other = Vec::new();

    for addr in addresses {
        if addr.contains("/ip4/127.") || addr.contains("/ip6/::1") {
            // Skip localhost
            continue;
        } else if addr.contains("/ip4/10.")
            || addr.contains("/ip4/172.")
            || addr.contains("/ip4/192.168.")
        {
            private.push(addr.as_str());
        } else if addr.contains("/ip4/") || addr.contains("/ip6/") || addr.contains("/dns") {
            public.push(addr.as_str());
        } else {
            other.push(addr.as_str());
        }
    }

    // Return best available
    if let Some(addr) = public.first() {
        addr.to_string()
    } else if let Some(addr) = private.first() {
        addr.to_string()
    } else if let Some(addr) = other.first() {
        addr.to_string()
    } else {
        // Fallback: use first address even if localhost
        addresses
            .first()
            .cloned()
            .unwrap_or_else(|| "/ip4/0.0.0.0/tcp/0".to_string())
    }
}

// ─── Resource Stats Command ──────────────────────────────────────────────────

/// Resource usage stats returned to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceStats {
    /// Total connected peers.
    pub connected_peers: usize,
    /// Current listen addresses.
    pub listen_addresses: usize,
    /// Total content items stored locally.
    pub content_count: usize,
    /// Network active.
    pub network_active: bool,
    /// Known peers in persistent store.
    pub known_peers: usize,
    /// Enabled seed nodes.
    pub seed_nodes: usize,
}

/// Get resource usage stats for the node.
///
/// Provides a quick overview of the node's resource utilisation.
/// Useful for the network view and dashboard widgets.
#[tauri::command]
pub async fn get_resource_stats(
    protocol: State<'_, Arc<Mutex<Option<ProtocolState>>>>,
) -> Result<ResourceStats, String> {
    let guard = protocol.lock().await;
    let state = guard
        .as_ref()
        .ok_or("Node not initialized — unlock first")?;

    let connected_peers = state
        .network
        .as_ref()
        .map(|n| n.connected_peers().len())
        .unwrap_or(0);

    let listen_addresses = state
        .network
        .as_ref()
        .map(|n| n.listen_addresses().len())
        .unwrap_or(0);

    let content_count = {
        let filter = nodalync_store::ManifestFilter::new();
        state
            .ops
            .state()
            .manifests
            .list(filter)
            .map(|v| v.len())
            .unwrap_or(0)
    };

    let peer_store = PeerStore::load(&state.data_dir);
    let seed_store = SeedStore::load(&state.data_dir);

    Ok(ResourceStats {
        connected_peers,
        listen_addresses,
        content_count,
        network_active: state.network.is_some(),
        known_peers: peer_store.peers.len(),
        seed_nodes: seed_store.enabled_count(),
    })
}
