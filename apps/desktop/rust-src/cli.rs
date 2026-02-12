use std::process::Command;
use serde_json::Value;
use tracing::{debug, error};
use crate::types::*;

/// Execute a nodalync-cli command and return the output
pub async fn execute_cli_command(args: &[&str]) -> Result<String, String> {
    debug!("Executing CLI command: nodalync-cli {}", args.join(" "));
    
    let output = Command::new("nodalync-cli")
        .args(args)
        .output()
        .map_err(|e| {
            error!("Failed to execute CLI command: {}", e);
            format!("Failed to execute command: {}", e)
        })?;
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        error!("CLI command failed: {}", stderr);
        return Err(format!("Command failed: {}", stderr));
    }
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.to_string())
}

/// Execute a CLI command and parse JSON output
pub async fn execute_cli_json<T>(args: &[&str]) -> Result<T, String>
where
    T: for<'de> serde::Deserialize<'de>,
{
    let output = execute_cli_command(args).await?;
    serde_json::from_str(&output)
        .map_err(|e| format!("Failed to parse JSON output: {}", e))
}

/// Check if the CLI is available and working
pub async fn check_cli_available() -> bool {
    match execute_cli_command(&["--version"]).await {
        Ok(_) => true,
        Err(e) => {
            error!("CLI not available: {}", e);
            false
        }
    }
}

/// Parse version string from CLI output
pub fn parse_version(version_output: &str) -> Option<String> {
    // Expected format: "nodalync-cli 0.10.1"
    version_output
        .split_whitespace()
        .nth(1)
        .map(|v| v.to_string())
}

/// Validate JSON structure for specific command outputs
pub fn validate_node_status(json: &Value) -> bool {
    json.get("running").is_some() && 
    json.get("version").is_some()
}

pub fn validate_earnings_info(json: &Value) -> bool {
    json.get("total_earned").is_some() &&
    json.get("active_channels").is_some()
}

pub fn validate_network_stats(json: &Value) -> bool {
    json.get("connected_peers").is_some() &&
    json.get("dht_entries").is_some()
}

/// Format error messages for UI display
pub fn format_cli_error(error: &str) -> String {
    if error.contains("command not found") || error.contains("not recognized") {
        "Nodalync CLI is not installed or not in PATH. Please ensure nodalync-cli is installed and accessible.".to_string()
    } else if error.contains("No such file or directory") {
        "Node data directory not found. Please initialize a node first.".to_string()
    } else if error.contains("Permission denied") {
        "Permission denied. Try running as administrator or check file permissions.".to_string()
    } else if error.contains("Connection refused") {
        "Could not connect to node. Make sure the node is running.".to_string()
    } else {
        format!("CLI Error: {}", error)
    }
}