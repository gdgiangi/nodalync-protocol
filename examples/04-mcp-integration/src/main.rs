//! # MCP Integration Example
//!
//! This example demonstrates how the Nodalync MCP server works under the hood.
//! In practice, you'd use the CLI (`nodalync mcp-server`) and let your MCP client
//! (Claude Desktop, OpenClaw, etc.) handle the protocol.
//!
//! This example shows the programmatic API for:
//! - Creating an MCP server instance
//! - Budget tracking
//! - Content publishing and querying (local mode)
//!
//! Run with: `cargo run -p nodalync-mcp-example`

use std::path::PathBuf;

use nodalync_mcp::server::{McpServerConfig, NodalyncMcpServer};
use nodalync_mcp::BudgetTracker;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter("nodalync=info")
        .init();

    println!("=== Nodalync MCP Integration Example ===\n");

    // --- Budget Tracking Demo ---
    println!("1. Budget Tracking");
    println!("   ---------------");

    let budget = BudgetTracker::with_auto_approve(5.0, 0.01);
    println!("   Session budget: {} HBAR", budget.total_budget_hbar());
    println!("   Auto-approve threshold: 0.01 HBAR");
    println!("   Remaining: {} HBAR", budget.remaining_hbar());

    // Simulate spending
    let cost_tinybars = 1_000_000; // 0.01 HBAR
    if budget.can_afford(cost_tinybars) {
        budget.spend(cost_tinybars);
        println!("   Spent 0.01 HBAR → Remaining: {} HBAR", budget.remaining_hbar());
    }

    println!();

    // --- MCP Server Creation Demo (local mode, no network) ---
    println!("2. MCP Server (Local Mode)");
    println!("   -----------------------");

    let data_dir = tempfile::tempdir()?;
    let config = McpServerConfig {
        budget_hbar: 1.0,
        auto_approve_hbar: 0.01,
        data_dir: data_dir.path().to_path_buf(),
        enable_network: false,
        bootstrap_nodes: vec![],
        hedera: None,
    };

    let server = NodalyncMcpServer::new(config).await?;
    println!("   ✓ Server created (local mode, no network)");
    println!("   Budget: 1.0 HBAR");
    println!("   Network: disabled (local content only)");

    println!();

    // --- How MCP Clients Use This ---
    println!("3. How MCP Clients Connect");
    println!("   -----------------------");
    println!("   Claude Desktop config (claude_desktop_config.json):");
    println!("   {{");
    println!("     \"mcpServers\": {{");
    println!("       \"nodalync\": {{");
    println!("         \"command\": \"nodalync\",");
    println!("         \"args\": [\"mcp-server\", \"--budget\", \"1.0\", \"--enable-network\"]");
    println!("       }}");
    println!("     }}");
    println!("   }}");
    println!();
    println!("   Available tools (16):");
    println!("   - search_network: Find content across the network");
    println!("   - query_knowledge: Retrieve content by hash (auto-pays)");
    println!("   - publish_content: Add knowledge to the network");
    println!("   - synthesize_content: Create L3 cross-content summaries");
    println!("   - status: Node health, budget, channels, Hedera balance");
    println!("   - deposit_hbar: Fund settlement contract");
    println!("   - open_channel / close_channel: Payment channel management");
    println!("   - get_earnings: View creator earnings");
    println!("   - And more (preview, update, delete, visibility, versions)");

    println!();

    // Graceful shutdown
    let channels_closed = server.shutdown().await;
    println!("   Shutdown: {} channels processed", channels_closed);

    println!("\n=== Example Complete ===");

    Ok(())
}
