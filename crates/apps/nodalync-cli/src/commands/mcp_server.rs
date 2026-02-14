//! MCP server command implementation.
//!
//! Starts an MCP server on stdio for AI assistant integration.

use std::path::PathBuf;

use nodalync_mcp::server::{run_server, HederaConfig, McpServerConfig};
use tracing::info;

use crate::config::CliConfig;
use crate::error::{CliError, CliResult};

/// Hedera configuration arguments passed from CLI.
pub struct HederaArgs {
    pub account_id: Option<String>,
    pub private_key: Option<PathBuf>,
    pub contract_id: String,
    pub network: String,
}

/// Start the MCP server.
///
/// This runs an MCP server on stdio that AI assistants like Claude
/// can use to query knowledge from the local Nodalync node.
pub async fn mcp_server(
    config: CliConfig,
    budget: f64,
    auto_approve: f64,
    enable_network: bool,
    hedera_args: HederaArgs,
) -> CliResult<String> {
    // Build Hedera config if account ID is provided
    let hedera = if let Some(account_id) = hedera_args.account_id {
        let private_key_path = hedera_args.private_key.ok_or_else(|| {
            CliError::user("--hedera-private-key is required when --hedera-account-id is set")
        })?;

        info!(
            account_id = %account_id,
            contract_id = %hedera_args.contract_id,
            network = %hedera_args.network,
            "Hedera settlement enabled"
        );

        Some(HederaConfig {
            account_id,
            private_key_path,
            contract_id: hedera_args.contract_id,
            network: hedera_args.network,
        })
    } else {
        None
    };

    info!(
        budget_hbar = budget,
        auto_approve_hbar = auto_approve,
        enable_network = enable_network,
        hedera_enabled = hedera.is_some(),
        "Starting MCP server"
    );

    // Build MCP server config using CLI config's data directory and network settings
    let mcp_config = McpServerConfig {
        budget_hbar: budget,
        auto_approve_hbar: auto_approve,
        data_dir: config.base_dir().to_path_buf(),
        enable_network,
        bootstrap_nodes: config.network.bootstrap_nodes.clone(),
        hedera,
        x402: None,
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
            enable_network: false,
            bootstrap_nodes: vec![],
            hedera: None,
            x402: None,
        };

        assert_eq!(config.budget_hbar, 1.0);
        assert_eq!(config.auto_approve_hbar, 0.01);
        assert!(!config.enable_network);
        assert!(config.hedera.is_none());
    }

    #[test]
    fn test_mcp_config_with_network() {
        let config = McpServerConfig {
            budget_hbar: 2.0,
            auto_approve_hbar: 0.1,
            data_dir: std::path::PathBuf::from("/tmp/test"),
            enable_network: true,
            bootstrap_nodes: vec![
                "/dns4/example.com/tcp/9000/p2p/12D3KooWMqrUmZm4e1BJTRMWqKHCe1TSX9Vu83uJLEyCGr2dUjYm"
                    .to_string(),
            ],
            hedera: None,
            x402: None,
        };

        assert!(config.enable_network);
        assert_eq!(config.bootstrap_nodes.len(), 1);
    }

    #[test]
    fn test_mcp_config_with_hedera() {
        let config = McpServerConfig {
            budget_hbar: 1.0,
            auto_approve_hbar: 0.01,
            data_dir: std::path::PathBuf::from("/tmp/test"),
            enable_network: false,
            bootstrap_nodes: vec![],
            hedera: Some(HederaConfig {
                account_id: "0.0.7703962".to_string(),
                private_key_path: PathBuf::from("/path/to/key"),
                contract_id: "0.0.7729011".to_string(),
                network: "testnet".to_string(),
            }),
            x402: None,
        };

        assert!(config.hedera.is_some());
        let hedera = config.hedera.unwrap();
        assert_eq!(hedera.account_id, "0.0.7703962");
        assert_eq!(hedera.network, "testnet");
    }
}
