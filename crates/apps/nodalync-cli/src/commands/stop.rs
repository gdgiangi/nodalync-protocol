//! Stop node command.

use std::time::Duration;

use crate::config::CliConfig;
use crate::error::{CliError, CliResult};
use crate::node_runner::{is_process_running, pid_file_path, read_pid_file, remove_pid_file};
use crate::output::{OutputFormat, Render, StopOutput};

use tracing::info;

/// Maximum time to wait for the node to stop gracefully.
const STOP_TIMEOUT: Duration = Duration::from_secs(5);

/// Interval to poll for process exit.
const POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Execute the stop command.
pub async fn stop(config: CliConfig, format: OutputFormat) -> CliResult<String> {
    let base_dir = config.base_dir();
    let pid_path = pid_file_path(&base_dir);

    // Read PID from file
    let pid = read_pid_file(&pid_path).ok_or(CliError::NodeNotRunning)?;

    // Check if process is actually running
    if !is_process_running(pid) {
        // Stale PID file, clean it up
        let _ = remove_pid_file(&pid_path);
        return Err(CliError::NodeNotRunning);
    }

    // Send termination signal
    #[cfg(unix)]
    {
        info!("Sending SIGTERM to node process (PID {})", pid);
        send_sigterm(pid)?;
    }

    #[cfg(not(unix))]
    {
        info!("Terminating node process (PID {})", pid);
        terminate_process(pid)?;
    }

    // Wait for graceful shutdown
    let stopped = wait_for_exit(pid, STOP_TIMEOUT).await;

    // Clean up PID file if needed
    if stopped {
        let _ = remove_pid_file(&pid_path);
    }

    let output = StopOutput { success: stopped };

    if !stopped {
        // Process didn't exit gracefully
        return Err(CliError::user(format!(
            "Node (PID {}) did not stop gracefully within {} seconds. You may need to kill it manually.",
            pid,
            STOP_TIMEOUT.as_secs()
        )));
    }

    Ok(output.render(format))
}

/// Send SIGTERM to a process.
#[cfg(unix)]
fn send_sigterm(pid: u32) -> CliResult<()> {
    use nix::sys::signal::{kill, Signal};
    use nix::unistd::Pid;

    kill(Pid::from_raw(pid as i32), Signal::SIGTERM)
        .map_err(|e| CliError::user(format!("Failed to send SIGTERM: {}", e)))?;

    Ok(())
}

/// Terminate a process on Windows.
#[cfg(not(unix))]
fn terminate_process(pid: u32) -> CliResult<()> {
    use std::process::Command;

    // Use taskkill to terminate the process
    let output = Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T"]) // /T terminates child processes too
        .output()
        .map_err(|e| CliError::user(format!("Failed to run taskkill: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(CliError::user(format!("Failed to terminate process: {}", stderr)));
    }

    Ok(())
}

/// Wait for a process to exit, with timeout.
async fn wait_for_exit(pid: u32, timeout: Duration) -> bool {
    let start = std::time::Instant::now();

    while start.elapsed() < timeout {
        if !is_process_running(pid) {
            return true;
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }

    // Final check
    !is_process_running(pid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stop_output() {
        let output = StopOutput { success: true };

        let human = output.render(OutputFormat::Human);
        assert!(human.contains("Node stopped"));

        let json = output.render(OutputFormat::Json);
        assert!(json.contains("\"success\": true"));
    }

    #[test]
    fn test_stop_output_failed() {
        let output = StopOutput { success: false };

        let human = output.render(OutputFormat::Human);
        assert!(human.contains("Failed"));
    }

    #[tokio::test]
    async fn test_wait_for_exit_already_dead() {
        // Use a PID that doesn't exist
        let result = wait_for_exit(999999999, Duration::from_millis(100)).await;
        assert!(result);
    }

    /// Regression test for Issue #47: `stop` when no node is running should
    /// return NodeNotRunning error, not "Node stopped."
    #[tokio::test]
    async fn test_stop_no_node_running() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let mut config = CliConfig::default();
        config.storage.content_dir = temp_dir.path().join("content");
        config.storage.cache_dir = temp_dir.path().join("cache");
        config.storage.database = temp_dir.path().join("nodalync.db");
        config.identity.keyfile = temp_dir.path().join("identity").join("keypair.key");

        let result = stop(config, OutputFormat::Human).await;
        assert!(result.is_err());
        assert!(
            matches!(result.as_ref().unwrap_err(), CliError::NodeNotRunning),
            "Should get NodeNotRunning, got: {:?}",
            result.unwrap_err()
        );
    }
}
