//! MCP server command implementation.
//!
//! Starts an MCP server on stdio for AI assistant integration.

use nodalync_mcp::server::{run_server, McpServerConfig};
use tracing::info;

use crate::config::CliConfig;
use crate::error::{CliError, CliResult};

/// Start the MCP server.
///
/// This runs an MCP server on stdio that AI assistants like Claude
/// can use to query knowledge from the local Nodalync node.
pub async fn mcp_server(config: CliConfig, budget: f64, auto_approve: f64) -> CliResult<String> {
    info!(
        budget_hbar = budget,
        auto_approve_hbar = auto_approve,
        "Starting MCP server"
    );

    // Build MCP server config using CLI config's data directory
    let mcp_config = McpServerConfig {
        budget_hbar: budget,
        auto_approve_hbar: auto_approve,
        data_dir: config.base_dir().to_path_buf(),
    };

    // Run the MCP server (this blocks until the server exits)
    run_server(mcp_config)
        .await
        .map_err(|e| CliError::user(format!("MCP server error: {}", e)))?;

    // This line is only reached if the server exits cleanly
    Ok("MCP server stopped.".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_config_creation() {
        // Just verify we can create the config
        let config = McpServerConfig {
            budget_hbar: 1.0,
            auto_approve_hbar: 0.01,
            data_dir: std::path::PathBuf::from("/tmp/test"),
        };

        assert_eq!(config.budget_hbar, 1.0);
        assert_eq!(config.auto_approve_hbar, 0.01);
    }
}
