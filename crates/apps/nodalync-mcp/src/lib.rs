//! MCP (Model Context Protocol) server for Nodalync.
//!
//! This crate provides an MCP server that allows AI assistants like Claude
//! to query knowledge from the Nodalync network and pay automatically.
//!
//! # Overview
//!
//! The MCP server exposes two main tools:
//!
//! - **query_knowledge**: Query content from the network with payment
//! - **list_sources**: Browse available content by topic
//!
//! And one resource:
//!
//! - **knowledge://{hash}**: Direct content access by hash
//!
//! # Usage
//!
//! The server is typically started via the CLI:
//!
//! ```bash
//! nodalync mcp-server --budget 1.0
//! ```
//!
//! Or configured in Claude Desktop's MCP config:
//!
//! ```json
//! {
//!   "mcpServers": {
//!     "nodalync": {
//!       "command": "nodalync",
//!       "args": ["mcp-server", "--budget", "1.0"]
//!     }
//!   }
//! }
//! ```
//!
//! # Budget Tracking
//!
//! The server tracks spending against a session budget:
//!
//! - Session budget is set at startup (in HBAR)
//! - Use `preview_content` to inspect pricing before execution
//! - Queries without an explicit allowance and resource reads use the
//!   auto-approve threshold (default 0.01 HBAR, inclusive)
//! - Queries are rejected if they would exceed remaining budget
//! - Query budgets exclude deposits, channel funding, and network fees;
//!   explicit allowances require authorization from the caller's host

pub mod budget;
pub mod error;
pub mod server;
pub mod tools;

pub use budget::{BudgetStatus, BudgetTracker};
pub use error::{McpError, McpResult};
pub use server::NodalyncMcpServer;
pub use tools::{
    ContentEarnings, DeleteContentInput, DeleteContentOutput, GetEarningsInput, GetEarningsOutput,
    ListSourcesInput, ListSourcesOutput, ListVersionsInput, ListVersionsOutput,
    PreviewContentInput, PreviewContentOutput, PublishContentInput, PublishContentOutput,
    QueryKnowledgeInput, QueryKnowledgeOutput, SetVisibilityInput, SetVisibilityOutput,
    SynthesizeContentInput, SynthesizeContentOutput, UpdateContentInput, UpdateContentOutput,
    VersionEntry,
};
