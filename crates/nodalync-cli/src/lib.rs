//! Command-line interface for the Nodalync protocol.
//!
//! This crate provides the `nodalync` binary for interacting with the
//! Nodalync protocol. It includes commands for:
//!
//! - **Identity Management**: Initialize and display identity
//! - **Content Management**: Publish, list, update, delete content
//! - **Discovery & Query**: Preview and query content
//! - **Synthesis**: Create L3 insights and L2 entity graphs
//! - **Economics**: Check balance, deposit, withdraw, settle
//! - **Node Management**: Start, status, stop the node
//!
//! # Quick Start
//!
//! ```bash
//! # Initialize identity
//! nodalync init
//!
//! # Publish content
//! nodalync publish document.txt --price 0.10
//!
//! # List local content
//! nodalync list
//!
//! # Check balance
//! nodalync balance
//! ```
//!
//! # Output Formats
//!
//! All commands support `--format` for output control:
//!
//! - `human` (default): Human-readable with colors
//! - `json`: Machine-readable JSON
//!
//! # Configuration
//!
//! Configuration is loaded from `~/.nodalync/config.toml`. Override with `--config`.

pub mod cli;
pub mod commands;
pub mod config;
pub mod context;
pub mod error;
pub mod output;

// Re-export main types
pub use cli::{Cli, Commands, OutputFormatArg, VisibilityArg};
pub use config::CliConfig;
pub use context::NodeContext;
pub use error::{CliError, CliResult};
pub use output::{OutputFormat, Render};
