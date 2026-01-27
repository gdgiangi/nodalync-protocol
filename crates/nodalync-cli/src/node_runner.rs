//! Node runner for persistent node operation.
//!
//! This module provides the event loop and PID file management for
//! running a persistent Nodalync node.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use nodalync_net::{InboundRequestId, Network, NetworkEvent, NetworkNode};
use nodalync_wire::MessageType;
use tokio::sync::watch;
use tracing::{debug, error, info, warn};

use crate::context::NodeContext;
use crate::error::{CliError, CliResult};

// =============================================================================
// PID File Utilities
// =============================================================================

/// Default PID file name.
const PID_FILE_NAME: &str = "node.pid";

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
///
/// # Returns
/// Ok(()) on graceful shutdown, or an error if something goes wrong.
pub async fn run_event_loop(
    ctx: &mut NodeContext,
    mut shutdown_rx: watch::Receiver<bool>,
) -> CliResult<()> {
    info!("Starting node event loop");

    let network = ctx
        .network
        .as_ref()
        .ok_or(CliError::config("Network not initialized"))?;

    loop {
        tokio::select! {
            // Check for shutdown signal
            result = shutdown_rx.changed() => {
                if result.is_err() || *shutdown_rx.borrow() {
                    info!("Shutdown signal received, stopping event loop");
                    break;
                }
            }

            // Process network events
            event_result = network.next_event() => {
                match event_result {
                    Ok(event) => {
                        if let Err(e) = handle_event(&mut ctx.ops, Arc::clone(network), event).await {
                            warn!("Error handling event: {}", e);
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

    info!("Event loop stopped");
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
        assert!(uptime >= 100 && uptime <= 102); // Allow small variance

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
