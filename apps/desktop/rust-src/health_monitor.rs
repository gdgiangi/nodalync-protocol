//! Background network health monitor for Nodalync Studio.
//!
//! Runs a periodic check (default 30s) that:
//! 1. Monitors peer connections and reconnects if they drop
//! 2. Saves known peers to disk periodically (every 5 minutes)
//! 3. Tracks network health metrics (uptime, reconnect count, peer stability)
//! 4. Provides `get_network_health` IPC command for the frontend
//!
//! The desktop app spec requires: "Background health check every 30s,
//! reconnect if needed." This module implements that contract.

use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use nodalync_net::{Network, NetworkNode};
use serde::{Deserialize, Serialize};
use tokio::sync::{watch, Mutex};
use tracing::{debug, info, warn};

use crate::peer_store::PeerStore;
use crate::protocol::ProtocolState;

// ─── Configuration ───────────────────────────────────────────────────────────

/// How often the health monitor runs (seconds).
const HEALTH_CHECK_INTERVAL_SECS: u64 = 30;

/// How often known peers are saved to disk (seconds).
const PEER_SAVE_INTERVAL_SECS: u64 = 300; // 5 minutes

/// Minimum peers before we attempt reconnection.
const MIN_PEER_THRESHOLD: usize = 1;

/// Maximum bootstrap peers to try reconnecting to at once.
const MAX_RECONNECT_ATTEMPTS: usize = 10;

// ─── Health Status ───────────────────────────────────────────────────────────

/// Snapshot of network health, returned by `get_network_health`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkHealth {
    /// Whether the network is currently active.
    pub active: bool,
    /// Number of currently connected peers.
    pub connected_peers: usize,
    /// Number of known peers in the persistent store.
    pub known_peers: usize,
    /// Network uptime since last start (seconds).
    pub uptime_secs: u64,
    /// Total reconnection attempts since network start.
    pub reconnect_attempts: u32,
    /// Successful reconnections since network start.
    pub reconnect_successes: u32,
    /// Last health check timestamp.
    pub last_check: Option<String>,
    /// Last peer save timestamp.
    pub last_peer_save: Option<String>,
    /// Health status: "healthy", "degraded", "disconnected", "offline".
    pub status: String,
    /// Human-readable status message.
    pub message: String,
}

impl Default for NetworkHealth {
    fn default() -> Self {
        Self {
            active: false,
            connected_peers: 0,
            known_peers: 0,
            uptime_secs: 0,
            reconnect_attempts: 0,
            reconnect_successes: 0,
            last_check: None,
            last_peer_save: None,
            status: "offline".to_string(),
            message: "Network not started".to_string(),
        }
    }
}

/// Internal mutable state for the health monitor.
struct MonitorState {
    network_start: Instant,
    reconnect_attempts: u32,
    reconnect_successes: u32,
    last_check: Option<DateTime<Utc>>,
    last_peer_save: Instant,
    last_peer_count: usize,
}

impl MonitorState {
    fn new() -> Self {
        Self {
            network_start: Instant::now(),
            reconnect_attempts: 0,
            reconnect_successes: 0,
            last_check: None,
            last_peer_save: Instant::now(),
            last_peer_count: 0,
        }
    }
}

// ─── Shared Health State ─────────────────────────────────────────────────────

/// Thread-safe shared health state, readable by the IPC command.
pub type SharedHealth = Arc<Mutex<NetworkHealth>>;

/// Create a new shared health state.
pub fn new_shared_health() -> SharedHealth {
    Arc::new(Mutex::new(NetworkHealth::default()))
}

// ─── Health Monitor Handle ───────────────────────────────────────────────────

/// Handle to control the health monitor background task.
pub struct HealthMonitorHandle {
    shutdown_tx: watch::Sender<bool>,
    join_handle: tokio::task::JoinHandle<()>,
}

impl HealthMonitorHandle {
    /// Signal the health monitor to stop and wait for it to finish.
    pub async fn shutdown(self) {
        let _ = self.shutdown_tx.send(true);
        let _ = tokio::time::timeout(Duration::from_secs(5), self.join_handle).await;
    }

    /// Signal the health monitor to stop (non-blocking).
    pub fn shutdown_signal(&self) {
        let _ = self.shutdown_tx.send(true);
    }
}

// ─── Spawn ───────────────────────────────────────────────────────────────────

/// Spawn the health monitor as a background tokio task.
///
/// # Arguments
/// * `network` - The active network node.
/// * `protocol` - Shared protocol state (for peer store data dir).
/// * `health` - Shared health state that the IPC command reads.
///
/// # Returns
/// A [`HealthMonitorHandle`] to shut down the monitor.
pub fn spawn_health_monitor(
    network: Arc<NetworkNode>,
    protocol: Arc<Mutex<Option<ProtocolState>>>,
    health: SharedHealth,
) -> HealthMonitorHandle {
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    let join_handle = tokio::spawn(run_health_monitor(
        network,
        protocol,
        health,
        shutdown_rx,
    ));

    info!("Network health monitor spawned (interval={}s)", HEALTH_CHECK_INTERVAL_SECS);

    HealthMonitorHandle {
        shutdown_tx,
        join_handle,
    }
}

// ─── Monitor Loop ────────────────────────────────────────────────────────────

async fn run_health_monitor(
    network: Arc<NetworkNode>,
    protocol: Arc<Mutex<Option<ProtocolState>>>,
    health: SharedHealth,
    mut shutdown_rx: watch::Receiver<bool>,
) {
    let mut state = MonitorState::new();
    let mut interval = tokio::time::interval(Duration::from_secs(HEALTH_CHECK_INTERVAL_SECS));

    // Don't burst-fire missed ticks if the system was asleep
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    info!("Health monitor loop started");

    loop {
        tokio::select! {
            _ = shutdown_rx.changed() => {
                if *shutdown_rx.borrow() {
                    info!("Health monitor shutting down (signal received)");
                    break;
                }
            }
            _ = interval.tick() => {
                run_health_check(&network, &protocol, &health, &mut state).await;
            }
        }
    }

    // Mark health as offline on shutdown
    {
        let mut h = health.lock().await;
        h.active = false;
        h.status = "offline".to_string();
        h.message = "Network stopped".to_string();
    }

    info!("Health monitor stopped");
}

/// Execute a single health check cycle.
async fn run_health_check(
    network: &Arc<NetworkNode>,
    protocol: &Arc<Mutex<Option<ProtocolState>>>,
    health: &SharedHealth,
    state: &mut MonitorState,
) {
    let now = Utc::now();
    state.last_check = Some(now);

    let peer_count = network.connected_peers().len();
    let listen_addrs = network.listen_addresses().len();

    debug!(
        "Health check: peers={}, listen_addrs={}, prev_peers={}",
        peer_count, listen_addrs, state.last_peer_count
    );

    // Track peer count changes
    if peer_count < state.last_peer_count && state.last_peer_count > 0 {
        info!(
            "Peer count dropped: {} → {} (lost {} peers)",
            state.last_peer_count,
            peer_count,
            state.last_peer_count - peer_count
        );
    } else if peer_count > state.last_peer_count {
        debug!(
            "Peer count increased: {} → {}",
            state.last_peer_count, peer_count
        );
    }
    state.last_peer_count = peer_count;

    // Reconnect if below threshold
    if peer_count < MIN_PEER_THRESHOLD {
        attempt_reconnect(network, protocol, state).await;
    }

    // Periodic peer save
    if state.last_peer_save.elapsed() >= Duration::from_secs(PEER_SAVE_INTERVAL_SECS) {
        save_peers(network, protocol, state).await;
    }

    // Update shared health snapshot
    let (known_count, last_save_str) = {
        let guard = protocol.lock().await;
        let known = guard
            .as_ref()
            .map(|s| PeerStore::load(&s.data_dir).peers.len())
            .unwrap_or(0);
        let save_ts = if state.last_peer_save.elapsed() < Duration::from_secs(PEER_SAVE_INTERVAL_SECS) {
            Some(now.to_rfc3339())
        } else {
            None
        };
        (known, save_ts)
    };

    let uptime = state.network_start.elapsed().as_secs();
    let (status, message) = classify_health(peer_count, listen_addrs, uptime);

    {
        let mut h = health.lock().await;
        h.active = true;
        h.connected_peers = peer_count;
        h.known_peers = known_count;
        h.uptime_secs = uptime;
        h.reconnect_attempts = state.reconnect_attempts;
        h.reconnect_successes = state.reconnect_successes;
        h.last_check = Some(now.to_rfc3339());
        h.last_peer_save = last_save_str.or_else(|| h.last_peer_save.clone());
        h.status = status;
        h.message = message;
    }
}

/// Classify network health into a status string.
fn classify_health(peers: usize, listen_addrs: usize, uptime_secs: u64) -> (String, String) {
    if listen_addrs == 0 {
        return (
            "degraded".to_string(),
            "No listen addresses — network may not be reachable".to_string(),
        );
    }

    if peers == 0 {
        if uptime_secs < 60 {
            // Just started — give it time
            return (
                "connecting".to_string(),
                "Searching for peers...".to_string(),
            );
        }
        return (
            "disconnected".to_string(),
            "No peers connected — attempting reconnection".to_string(),
        );
    }

    if peers < 3 {
        return (
            "degraded".to_string(),
            format!("{} peer(s) connected — network is sparse", peers),
        );
    }

    (
        "healthy".to_string(),
        format!("{} peers connected", peers),
    )
}

/// Attempt to reconnect to known peers from the persistent store.
async fn attempt_reconnect(
    network: &Arc<NetworkNode>,
    protocol: &Arc<Mutex<Option<ProtocolState>>>,
    state: &mut MonitorState,
) {
    let entries: Vec<(String, String)> = {
        let guard = protocol.lock().await;
        match guard.as_ref() {
            Some(s) => {
                let store = PeerStore::load(&s.data_dir);
                store
                    .bootstrap_entries(MAX_RECONNECT_ATTEMPTS)
                    .into_iter()
                    .map(|(pid, addr)| (pid.to_string(), addr.to_string()))
                    .collect()
            }
            None => return,
        }
    };

    if entries.is_empty() {
        debug!("No known peers to reconnect to");
        return;
    }

    info!(
        "Attempting reconnection to {} known peers",
        entries.len()
    );

    for (_peer_id_str, addr_str) in &entries {
        state.reconnect_attempts += 1;

        match addr_str.parse() {
            Ok(addr) => {
                match network.dial(addr).await {
                    Ok(()) => {
                        state.reconnect_successes += 1;
                        info!("Reconnected to peer via {}", addr_str);
                    }
                    Err(e) => {
                        debug!("Reconnect failed for {}: {}", addr_str, e);
                    }
                }
            }
            Err(e) => {
                debug!("Skipping invalid address {}: {}", addr_str, e);
            }
        }
    }

    // Re-bootstrap the DHT after reconnections
    if let Err(e) = network.bootstrap().await {
        debug!("Post-reconnect bootstrap note: {}", e);
    }
}

/// Save current peers to the persistent store.
async fn save_peers(
    network: &Arc<NetworkNode>,
    protocol: &Arc<Mutex<Option<ProtocolState>>>,
    state: &mut MonitorState,
) {
    let guard = protocol.lock().await;
    let data_dir = match guard.as_ref() {
        Some(s) => s.data_dir.clone(),
        None => return,
    };
    drop(guard); // Release lock before I/O

    let mut store = PeerStore::load(&data_dir);
    let peers = network.connected_peers();

    for peer in &peers {
        let peer_str = peer.to_string();
        let nodalync_id = network.nodalync_peer_id(peer).map(|id| id.to_string());
        store.record_peer(&peer_str, vec![], nodalync_id, false);
    }

    store.prune_stale(30);

    match store.save(&data_dir) {
        Ok(()) => {
            info!("Saved {} known peers to disk", store.peers.len());
            state.last_peer_save = Instant::now();
        }
        Err(e) => {
            warn!("Failed to save peers: {}", e);
        }
    }
}
