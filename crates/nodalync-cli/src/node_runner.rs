//! Node runner for persistent node operation.
//!
//! This module provides the event loop and PID file management for
//! running a persistent Nodalync node.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use nodalync_net::{InboundRequestId, Network, NetworkEvent, NetworkNode};
use nodalync_wire::MessageType;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::watch;
use tokio::time::interval;
use tracing::{debug, error, info, warn};

use crate::alerting::AlertManager;
use crate::config::AlertingConfig;
use crate::context::NodeContext;
use crate::error::{CliError, CliResult};
use crate::metrics::{Metrics, SharedMetrics};

// =============================================================================
// PID File Utilities
// =============================================================================

/// Default PID file name.
const PID_FILE_NAME: &str = "node.pid";

/// Default status file name.
const STATUS_FILE_NAME: &str = "node.status";

/// Get the PID file path for the given base directory.
pub fn pid_file_path(base_dir: &Path) -> PathBuf {
    base_dir.join(PID_FILE_NAME)
}

/// Write the current process PID to the PID file.
///
/// # Arguments
/// * `path` - Path to write the PID file
///
/// # Returns
/// Ok(()) on success, or an error if writing fails.
#[allow(dead_code)]
pub fn write_pid_file(path: &Path) -> CliResult<()> {
    let pid = std::process::id();
    std::fs::write(path, pid.to_string())?;
    Ok(())
}

/// Write the current process PID and start time to the PID file.
///
/// Format: `<pid> <start_time_unix_secs>`
///
/// # Arguments
/// * `path` - Path to write the PID file
///
/// # Returns
/// Ok(()) on success, or an error if writing fails.
pub fn write_pid_file_with_start_time(path: &Path) -> CliResult<()> {
    let pid = std::process::id();
    let start_time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    std::fs::write(path, format!("{} {}", pid, start_time))?;
    Ok(())
}

/// Read the PID from the PID file.
///
/// Supports both old format (just PID) and new format (PID + start time).
///
/// # Arguments
/// * `path` - Path to the PID file
///
/// # Returns
/// Some(pid) if the file exists and contains a valid PID, None otherwise.
pub fn read_pid_file(path: &Path) -> Option<u32> {
    std::fs::read_to_string(path).ok().and_then(|s| {
        let trimmed = s.trim();
        // Handle both "123" and "123 1706123456" formats
        let pid_str = trimmed.split_whitespace().next()?;
        pid_str.parse().ok()
    })
}

/// Read the start time from the PID file.
///
/// # Arguments
/// * `path` - Path to the PID file
///
/// # Returns
/// Some(start_time_secs) if the file exists and contains a start time, None otherwise.
pub fn read_start_time(path: &Path) -> Option<u64> {
    std::fs::read_to_string(path).ok().and_then(|s| {
        let parts: Vec<&str> = s.split_whitespace().collect();
        if parts.len() >= 2 {
            parts[1].parse().ok()
        } else {
            None
        }
    })
}

/// Calculate uptime in seconds from a start time.
///
/// # Arguments
/// * `start_time` - Unix timestamp in seconds when the node started
///
/// # Returns
/// Uptime in seconds, or 0 if the start time is in the future.
pub fn calculate_uptime(start_time: u64) -> u64 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    now.saturating_sub(start_time)
}

/// Remove the PID file.
///
/// # Arguments
/// * `path` - Path to the PID file
///
/// # Returns
/// Ok(()) on success (or if file doesn't exist), or an error if removal fails.
pub fn remove_pid_file(path: &Path) -> CliResult<()> {
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

/// Check if a process with the given PID is running.
///
/// Uses `kill -0` on Unix to check if the process exists.
///
/// # Arguments
/// * `pid` - Process ID to check
///
/// # Returns
/// true if the process is running, false otherwise.
#[cfg(unix)]
pub fn is_process_running(pid: u32) -> bool {
    // Try to send signal 0 to check if process exists
    // This is a standard Unix technique to check process existence
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}

#[cfg(not(unix))]
pub fn is_process_running(_pid: u32) -> bool {
    // On non-Unix platforms, we can't easily check
    // Assume the process is running if we have a PID file
    true
}

/// Check if a node is already running by examining the PID file.
///
/// # Arguments
/// * `base_dir` - The base directory containing the PID file
///
/// # Returns
/// Some(pid) if a node is running, None otherwise.
pub fn check_existing_node(base_dir: &Path) -> Option<u32> {
    let pid_path = pid_file_path(base_dir);
    if let Some(pid) = read_pid_file(&pid_path) {
        if is_process_running(pid) {
            return Some(pid);
        }
        // Stale PID file, clean it up
        let _ = remove_pid_file(&pid_path);
    }
    None
}

// =============================================================================
// Status File Utilities
// =============================================================================

/// Runtime status written by the running node.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RuntimeStatus {
    /// Number of connected peers.
    pub connected_peers: u32,
    /// Unix timestamp when status was last updated.
    pub updated_at: u64,
}

/// Get the status file path for the given base directory.
pub fn status_file_path(base_dir: &Path) -> PathBuf {
    base_dir.join(STATUS_FILE_NAME)
}

/// Write the runtime status to the status file.
pub fn write_status_file(path: &Path, status: &RuntimeStatus) -> CliResult<()> {
    let json = serde_json::to_string(status)
        .map_err(|e| CliError::User(format!("Failed to serialize status: {}", e)))?;
    std::fs::write(path, json)?;
    Ok(())
}

/// Read the runtime status from the status file.
///
/// Returns None if the file doesn't exist or can't be parsed.
pub fn read_status_file(path: &Path) -> Option<RuntimeStatus> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
}

/// Remove the status file.
pub fn remove_status_file(path: &Path) -> CliResult<()> {
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

// =============================================================================
// Health Endpoint Configuration
// =============================================================================

/// Configuration for the HTTP health endpoint.
#[derive(Debug, Clone)]
pub struct HealthConfig {
    /// Whether the health endpoint is enabled.
    pub enabled: bool,
    /// Port to listen on.
    pub port: u16,
    /// Alerting configuration.
    pub alerting: AlertingConfig,
}

impl Default for HealthConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            port: 8080,
            alerting: AlertingConfig::default(),
        }
    }
}

// =============================================================================
// Event Loop
// =============================================================================

/// Run the node event loop.
///
/// This function processes incoming network events and dispatches them to
/// the appropriate handlers. It runs until a shutdown signal is received.
///
/// # Arguments
/// * `ctx` - The node context with network and operations
/// * `shutdown_rx` - A watch receiver that triggers shutdown when changed to true
/// * `base_dir` - The base directory for status file (optional)
///
/// # Returns
/// Ok(()) on graceful shutdown, or an error if something goes wrong.
pub async fn run_event_loop(
    ctx: &mut NodeContext,
    shutdown_rx: watch::Receiver<bool>,
) -> CliResult<()> {
    run_event_loop_with_status(ctx, shutdown_rx, None).await
}

/// Run the node event loop with status file updates.
///
/// This variant writes the node's runtime status to a file periodically,
/// allowing the `status` command to read the current state.
///
/// This is a convenience wrapper around `run_event_loop_with_health` with
/// health endpoint disabled.
pub async fn run_event_loop_with_status(
    ctx: &mut NodeContext,
    shutdown_rx: watch::Receiver<bool>,
    base_dir: Option<&Path>,
) -> CliResult<()> {
    run_event_loop_with_health(ctx, shutdown_rx, base_dir, HealthConfig::default()).await
}

/// Run the node event loop with status file updates and optional health endpoint.
///
/// This variant writes the node's runtime status to a file periodically,
/// and optionally runs an HTTP health endpoint for container orchestration.
pub async fn run_event_loop_with_health(
    ctx: &mut NodeContext,
    mut shutdown_rx: watch::Receiver<bool>,
    base_dir: Option<&Path>,
    health_config: HealthConfig,
) -> CliResult<()> {
    info!("Starting node event loop");

    let network = ctx
        .network
        .as_ref()
        .ok_or(CliError::config("Network not initialized"))?;

    // Status file path (if base_dir provided)
    let status_path = base_dir.map(status_file_path);

    // Status update interval (every 5 seconds)
    let mut status_interval = interval(Duration::from_secs(5));

    // Track start time for uptime calculation
    let start_time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    // Create metrics (always created, used for instrumentation even without health endpoint)
    let metrics: SharedMetrics = Arc::new(Metrics::new());

    // Set initial node info
    let version = env!("CARGO_PKG_VERSION");
    let peer_id = network.local_peer_id().to_string();
    metrics
        .node_info
        .with_label_values(&[version, &peer_id])
        .set(1);

    // Create alert manager
    let alert_manager = Arc::new(AlertManager::new(
        health_config.alerting.clone(),
        peer_id.clone(),
    ));

    // Send startup alert
    let initial_peer_count = network.connected_peers().len() as u32;
    alert_manager.send_startup_alert(initial_peer_count).await;

    // Spawn heartbeat task if configured
    let heartbeat_shutdown_tx = if let Some(interval_duration) = alert_manager.heartbeat_interval()
    {
        let (tx, mut rx) = watch::channel(false);
        let alert_manager_clone = Arc::clone(&alert_manager);
        let network_clone = Arc::clone(network);

        tokio::spawn(async move {
            let mut heartbeat_interval = interval(interval_duration);
            // Skip first tick (immediate)
            heartbeat_interval.tick().await;

            loop {
                tokio::select! {
                    result = rx.changed() => {
                        if result.is_err() || *rx.borrow() {
                            break;
                        }
                    }
                    _ = heartbeat_interval.tick() => {
                        let peer_count = network_clone.connected_peers().len() as u32;
                        alert_manager_clone.send_heartbeat(peer_count).await;
                    }
                }
            }
        });

        info!(
            "Heartbeat alerting enabled with {:?} interval",
            interval_duration
        );
        Some(tx)
    } else {
        None
    };

    // Spawn health server if enabled
    let health_shutdown_tx = if health_config.enabled {
        let (tx, rx) = watch::channel(false);
        let network_clone = Arc::clone(network);
        let metrics_clone = Arc::clone(&metrics);
        let port = health_config.port;

        tokio::spawn(async move {
            if let Err(e) =
                run_health_server(port, network_clone, start_time, Some(metrics_clone), rx).await
            {
                warn!("Health server error: {}", e);
            }
        });

        info!("Health endpoint enabled on port {}", port);
        info!(
            "Metrics endpoint available at http://0.0.0.0:{}/metrics",
            port
        );
        Some(tx)
    } else {
        None
    };

    // Helper to write status
    let write_status = |network: &Arc<NetworkNode>, path: &Option<PathBuf>| {
        if let Some(ref path) = path {
            let status = RuntimeStatus {
                connected_peers: network.connected_peers().len() as u32,
                updated_at: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0),
            };
            if let Err(e) = write_status_file(path, &status) {
                debug!("Failed to write status file: {}", e);
            }
        }
    };

    // Write initial status
    write_status(network, &status_path);

    loop {
        tokio::select! {
            // Check for shutdown signal
            result = shutdown_rx.changed() => {
                if result.is_err() || *shutdown_rx.borrow() {
                    info!("Shutdown signal received, stopping event loop");
                    break;
                }
            }

            // Periodic status update and health check
            _ = status_interval.tick() => {
                let peer_count = network.connected_peers().len() as u32;
                write_status(network, &status_path);
                // Check health periodically even without peer events
                alert_manager.check_health(peer_count).await;
            }

            // Process network events
            event_result = network.next_event() => {
                match event_result {
                    Ok(event) => {
                        // Instrument metrics based on event type
                        match &event {
                            NetworkEvent::PeerConnected { .. } => {
                                metrics.peer_events_total.with_label_values(&["connect"]).inc();
                                metrics.connected_peers.set(network.connected_peers().len() as i64);
                            }
                            NetworkEvent::PeerDisconnected { .. } => {
                                metrics.peer_events_total.with_label_values(&["disconnect"]).inc();
                                metrics.connected_peers.set(network.connected_peers().len() as i64);
                            }
                            NetworkEvent::DhtPutComplete { success, .. } => {
                                let result = if *success { "success" } else { "failure" };
                                metrics.dht_operations_total.with_label_values(&["put", result]).inc();
                            }
                            NetworkEvent::DhtGetResult { value, .. } => {
                                let result = if value.is_some() { "success" } else { "not_found" };
                                metrics.dht_operations_total.with_label_values(&["get", result]).inc();
                            }
                            NetworkEvent::BroadcastReceived { .. } => {
                                metrics.gossipsub_messages_total.inc();
                            }
                            _ => {}
                        }

                        // Check if this is a peer connect/disconnect event
                        let peer_change = matches!(
                            &event,
                            NetworkEvent::PeerConnected { .. } | NetworkEvent::PeerDisconnected { .. }
                        );

                        if let Err(e) = handle_event(&mut ctx.ops, Arc::clone(network), event).await {
                            warn!("Error handling event: {}", e);
                        }

                        // Update status and check health on peer changes
                        if peer_change {
                            let peer_count = network.connected_peers().len() as u32;
                            write_status(network, &status_path);
                            alert_manager.check_health(peer_count).await;
                        }
                    }
                    Err(e) => {
                        error!("Network error: {}", e);
                        // Channel closed, network is shutting down
                        break;
                    }
                }
            }
        }
    }

    // Send shutdown alert
    let final_peer_count = network.connected_peers().len() as u32;
    alert_manager.send_shutdown_alert(final_peer_count).await;

    // Signal heartbeat task to shutdown
    if let Some(tx) = heartbeat_shutdown_tx {
        let _ = tx.send(true);
    }

    // Signal health server to shutdown
    if let Some(tx) = health_shutdown_tx {
        let _ = tx.send(true);
    }

    // Clean up status file on exit
    if let Some(ref path) = status_path {
        let _ = remove_status_file(path);
    }

    info!("Event loop stopped");
    Ok(())
}

/// Run a minimal HTTP health server with optional Prometheus metrics endpoint.
///
/// Routes:
/// - `GET /metrics` - Prometheus text format metrics
/// - `GET /health` or other - JSON health status
async fn run_health_server(
    port: u16,
    network: Arc<NetworkNode>,
    start_time: u64,
    metrics: Option<SharedMetrics>,
    mut shutdown_rx: watch::Receiver<bool>,
) -> CliResult<()> {
    let addr = format!("0.0.0.0:{}", port);
    let listener = TcpListener::bind(&addr).await.map_err(|e| {
        CliError::config(format!("Failed to bind health server to {}: {}", addr, e))
    })?;

    info!("Health server listening on {}", addr);

    loop {
        tokio::select! {
            // Check for shutdown
            result = shutdown_rx.changed() => {
                if result.is_err() || *shutdown_rx.borrow() {
                    info!("Health server shutting down");
                    break;
                }
            }

            // Accept connections
            accept_result = listener.accept() => {
                match accept_result {
                    Ok((mut socket, _)) => {
                        let connected_peers = network.connected_peers().len();
                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_secs())
                            .unwrap_or(0);
                        let uptime_secs = now.saturating_sub(start_time);

                        // Read and parse HTTP request
                        let mut buf = [0u8; 1024];
                        let n = socket.read(&mut buf).await.unwrap_or(0);
                        let request = String::from_utf8_lossy(&buf[..n]);

                        // Parse request path from first line (e.g., "GET /metrics HTTP/1.1")
                        let path = request
                            .lines()
                            .next()
                            .and_then(|line| line.split_whitespace().nth(1))
                            .unwrap_or("/health");

                        let (content_type, body) = if path == "/metrics" {
                            if let Some(ref m) = metrics {
                                // Update uptime before encoding
                                m.uptime_seconds.set(uptime_secs as i64);
                                m.connected_peers.set(connected_peers as i64);
                                ("text/plain; version=0.0.4; charset=utf-8", m.encode())
                            } else {
                                // Metrics not enabled, return empty
                                ("text/plain", String::from("# Metrics not enabled\n"))
                            }
                        } else {
                            // Default to health endpoint
                            let json = format!(
                                r#"{{"status":"ok","connected_peers":{},"uptime_secs":{}}}"#,
                                connected_peers, uptime_secs
                            );
                            ("application/json", json)
                        };

                        // Build HTTP response
                        let response = format!(
                            "HTTP/1.1 200 OK\r\n\
                             Content-Type: {}\r\n\
                             Content-Length: {}\r\n\
                             Connection: close\r\n\
                             \r\n\
                             {}",
                            content_type,
                            body.len(),
                            body
                        );

                        // Send response (ignore errors, client may have disconnected)
                        let _ = socket.write_all(response.as_bytes()).await;
                    }
                    Err(e) => {
                        debug!("Health server accept error: {}", e);
                    }
                }
            }
        }
    }

    Ok(())
}

/// Handle a single network event.
async fn handle_event<V, E>(
    ops: &mut nodalync_ops::NodeOperations<V, E>,
    network: Arc<NetworkNode>,
    event: NetworkEvent,
) -> CliResult<()>
where
    V: nodalync_valid::Validator,
    E: nodalync_ops::L1Extractor,
{
    // Extract request_id if this is an inbound request
    let request_id = match &event {
        NetworkEvent::InboundRequest { request_id, .. } => Some(*request_id),
        _ => None,
    };

    // Handle the event through the ops layer
    let response = ops.handle_network_event(event).await;

    // If there's a response to send and we have a request_id, send it
    if let (Some(request_id), Ok(Some((msg_type, payload)))) = (request_id, response) {
        send_response(&network, request_id, msg_type, payload).await?;
    }

    Ok(())
}

/// Send a response to an inbound request.
async fn send_response(
    network: &NetworkNode,
    request_id: InboundRequestId,
    msg_type: MessageType,
    payload: Vec<u8>,
) -> CliResult<()> {
    // Create a signed message and send as response
    network
        .send_signed_response(request_id, msg_type, payload)
        .await
        .map_err(CliError::Network)?;

    debug!("Sent response for request {:?}", request_id);
    Ok(())
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_pid_file_path() {
        let base = Path::new("/tmp/nodalync");
        let path = pid_file_path(base);
        assert_eq!(path, PathBuf::from("/tmp/nodalync/node.pid"));
    }

    #[test]
    fn test_write_read_pid_file() {
        let temp_dir = TempDir::new().unwrap();
        let pid_path = temp_dir.path().join("node.pid");

        // Write PID with start time
        write_pid_file_with_start_time(&pid_path).unwrap();

        // Read PID
        let pid = read_pid_file(&pid_path);
        assert!(pid.is_some());
        assert_eq!(pid.unwrap(), std::process::id());

        // Read start time
        let start_time = read_start_time(&pid_path);
        assert!(start_time.is_some());
        assert!(start_time.unwrap() > 0);
    }

    #[test]
    fn test_read_pid_file_old_format() {
        let temp_dir = TempDir::new().unwrap();
        let pid_path = temp_dir.path().join("node.pid");

        // Write old format (just PID)
        std::fs::write(&pid_path, "12345").unwrap();

        // Should still be able to read PID
        let pid = read_pid_file(&pid_path);
        assert_eq!(pid, Some(12345));

        // Start time should be None for old format
        let start_time = read_start_time(&pid_path);
        assert!(start_time.is_none());
    }

    #[test]
    fn test_calculate_uptime() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Started 100 seconds ago
        let uptime = calculate_uptime(now - 100);
        assert!((100..=102).contains(&uptime)); // Allow small variance

        // Future start time should give 0
        let uptime = calculate_uptime(now + 1000);
        assert_eq!(uptime, 0);
    }

    #[test]
    fn test_read_nonexistent_pid_file() {
        let pid = read_pid_file(Path::new("/nonexistent/path/node.pid"));
        assert!(pid.is_none());
    }

    #[test]
    fn test_remove_pid_file() {
        let temp_dir = TempDir::new().unwrap();
        let pid_path = temp_dir.path().join("node.pid");

        // Write and then remove
        write_pid_file(&pid_path).unwrap();
        assert!(pid_path.exists());

        remove_pid_file(&pid_path).unwrap();
        assert!(!pid_path.exists());
    }

    #[test]
    fn test_remove_nonexistent_pid_file() {
        // Should not error when removing a non-existent file
        let result = remove_pid_file(Path::new("/nonexistent/path/node.pid"));
        assert!(result.is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn test_is_process_running() {
        // Current process should be running
        let pid = std::process::id();
        assert!(is_process_running(pid));

        // PID 0 (kernel) should exist but we can't signal it
        // Use a very high PID that likely doesn't exist
        assert!(!is_process_running(999999999));
    }

    #[test]
    fn test_check_existing_node_no_file() {
        let temp_dir = TempDir::new().unwrap();
        assert!(check_existing_node(temp_dir.path()).is_none());
    }

    #[test]
    fn test_check_existing_node_stale_pid() {
        let temp_dir = TempDir::new().unwrap();
        let pid_path = pid_file_path(temp_dir.path());

        // Write a stale PID (very high number unlikely to exist)
        std::fs::write(&pid_path, "999999999").unwrap();

        // Should return None and clean up the stale file
        assert!(check_existing_node(temp_dir.path()).is_none());
        assert!(!pid_path.exists());
    }
}
