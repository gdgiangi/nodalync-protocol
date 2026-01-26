//! CLI argument definitions using clap.

use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

use crate::output::OutputFormat;

/// Nodalync Protocol CLI.
#[derive(Parser, Debug)]
#[command(name = "nodalync")]
#[command(author = "Nodalync Contributors")]
#[command(version)]
#[command(about = "Command-line interface for the Nodalync protocol")]
#[command(
    long_about = "Nodalync is a protocol for knowledge ownership, synthesis, and monetization.\n\nRun 'nodalync init' to get started."
)]
pub struct Cli {
    /// Subcommand to execute.
    #[command(subcommand)]
    pub command: Commands,

    /// Path to configuration file.
    #[arg(short, long, global = true)]
    pub config: Option<PathBuf>,

    /// Output format (human or json).
    #[arg(short, long, global = true, default_value = "human")]
    pub format: OutputFormatArg,

    /// Enable verbose logging.
    #[arg(short, long, global = true)]
    pub verbose: bool,
}

/// Output format argument for clap.
#[derive(Debug, Clone, Copy, ValueEnum, Default)]
pub enum OutputFormatArg {
    /// Human-readable output.
    #[default]
    Human,
    /// JSON output.
    Json,
}

impl From<OutputFormatArg> for OutputFormat {
    fn from(arg: OutputFormatArg) -> Self {
        match arg {
            OutputFormatArg::Human => OutputFormat::Human,
            OutputFormatArg::Json => OutputFormat::Json,
        }
    }
}

/// CLI commands.
#[derive(Subcommand, Debug)]
pub enum Commands {
    // =========================================================================
    // Identity Commands
    // =========================================================================
    /// Initialize a new identity.
    ///
    /// Creates a new Ed25519 keypair and stores it encrypted.
    /// Also creates a default configuration file.
    Init,

    /// Show identity information.
    ///
    /// Displays the PeerId, public key, and listening addresses.
    Whoami,

    // =========================================================================
    // Content Management Commands
    // =========================================================================
    /// Publish content to the network.
    ///
    /// Hashes the file, extracts L1 mentions, and announces to the DHT.
    Publish {
        /// Path to the file to publish.
        file: PathBuf,

        /// Price per query in HBAR (default from config).
        #[arg(short, long)]
        price: Option<f64>,

        /// Visibility level.
        #[arg(short = 'V', long, default_value = "shared")]
        visibility: VisibilityArg,

        /// Title for the content (defaults to filename).
        #[arg(short, long)]
        title: Option<String>,

        /// Description for the content.
        #[arg(short, long)]
        description: Option<String>,
    },

    /// List local content.
    ///
    /// Shows all content stored locally, grouped by visibility.
    List {
        /// Filter by visibility level.
        #[arg(short = 'V', long)]
        visibility: Option<VisibilityArg>,

        /// Filter by content type.
        #[arg(short = 't', long)]
        content_type: Option<ContentTypeArg>,

        /// Maximum results to show.
        #[arg(short, long, default_value = "50")]
        limit: u32,
    },

    /// Update content (create a new version).
    ///
    /// Creates a new version linked to the previous content.
    Update {
        /// Hash of the content to update.
        hash: String,

        /// Path to the new file.
        file: PathBuf,

        /// New title (optional).
        #[arg(short, long)]
        title: Option<String>,
    },

    /// Change content visibility.
    ///
    /// Updates how content is discovered and served.
    Visibility {
        /// Hash of the content.
        hash: String,

        /// New visibility level.
        level: VisibilityArg,
    },

    /// Show all versions of content.
    ///
    /// Lists the complete version history.
    Versions {
        /// Hash of any version in the chain.
        hash: String,
    },

    /// Delete local content.
    ///
    /// Removes the local copy but preserves provenance records.
    Delete {
        /// Hash of the content to delete.
        hash: String,

        /// Skip confirmation prompt.
        #[arg(short = 'F', long)]
        force: bool,
    },

    // =========================================================================
    // Discovery & Query Commands
    // =========================================================================
    /// Preview content metadata (free).
    ///
    /// Shows title, price, L1 summary without paying.
    Preview {
        /// Hash of the content to preview.
        hash: String,
    },

    /// Query content (paid).
    ///
    /// Retrieves full content and pays the owner.
    Query {
        /// Hash of the content to query.
        hash: String,

        /// Output path for the content (optional).
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    // =========================================================================
    // Synthesis Commands
    // =========================================================================
    /// Create L3 synthesis from sources.
    ///
    /// Combines insights from multiple sources with proper provenance.
    Synthesize {
        /// Source content hashes (comma-separated).
        #[arg(short, long, value_delimiter = ',', required = true)]
        sources: Vec<String>,

        /// Path to the synthesis output file.
        #[arg(short, long)]
        output: PathBuf,

        /// Title for the synthesis.
        #[arg(short, long)]
        title: Option<String>,

        /// Price if publishing (optional).
        #[arg(short, long)]
        price: Option<f64>,

        /// Publish immediately after creation.
        #[arg(long)]
        publish: bool,
    },

    /// Build L2 Entity Graph from L1 sources.
    ///
    /// Creates a personal knowledge graph from extracted mentions.
    BuildL2 {
        /// L1 content hashes to include.
        #[arg(required = true)]
        sources: Vec<String>,

        /// Title for the entity graph.
        #[arg(short, long)]
        title: Option<String>,
    },

    /// Merge multiple L2 Entity Graphs.
    ///
    /// Combines entity graphs with conflict resolution.
    MergeL2 {
        /// L2 graph hashes to merge.
        #[arg(required = true)]
        graphs: Vec<String>,

        /// Title for the merged graph.
        #[arg(short, long)]
        title: Option<String>,
    },

    /// Reference external L3 as L0 for future derivations.
    ///
    /// Promotes an L3 synthesis to a primary source, allowing it
    /// to be used as a foundation for new content.
    Reference {
        /// Hash of the L3 content to reference.
        hash: String,
    },

    // =========================================================================
    // Economics Commands
    // =========================================================================
    /// Show balance and pending earnings.
    Balance,

    /// Show earnings breakdown by content.
    ///
    /// Lists content sorted by total revenue earned.
    Earnings {
        /// Filter by content hash prefix.
        #[arg(long)]
        content: Option<String>,

        /// Maximum results to show.
        #[arg(short, long, default_value = "10")]
        limit: u32,
    },

    /// Deposit tokens to protocol balance.
    Deposit {
        /// Amount in HBAR to deposit.
        amount: f64,
    },

    /// Withdraw tokens from protocol balance.
    Withdraw {
        /// Amount in HBAR to withdraw.
        amount: f64,
    },

    /// Force settlement of pending payments.
    ///
    /// Creates a batch and settles on-chain.
    Settle,

    // =========================================================================
    // Node Management Commands
    // =========================================================================
    /// Start the Nodalync node.
    ///
    /// Begins listening for connections and serving content.
    Start {
        /// Run in daemon mode (background).
        #[arg(short, long)]
        daemon: bool,
    },

    /// Show node status.
    ///
    /// Displays uptime, peers, content counts, pending payments.
    Status,

    /// Stop the running node.
    ///
    /// Gracefully shuts down the node.
    Stop,

    // =========================================================================
    // MCP Server Commands
    // =========================================================================
    /// Start the MCP server for AI assistant integration.
    ///
    /// Runs an MCP server on stdio that allows AI assistants like Claude
    /// to query knowledge from your local node.
    McpServer {
        /// Session budget in HBAR (default: 1.0).
        #[arg(short, long, default_value = "1.0")]
        budget: f64,

        /// Auto-approve threshold in HBAR (default: 0.01).
        /// Queries below this amount are approved automatically.
        #[arg(short, long, default_value = "0.01")]
        auto_approve: f64,
    },
}

/// Visibility level argument for clap.
#[derive(Debug, Clone, Copy, ValueEnum, Default)]
pub enum VisibilityArg {
    /// Local only, not served.
    Private,
    /// Served if hash known, not in DHT.
    Unlisted,
    /// Announced to DHT, publicly queryable.
    #[default]
    Shared,
}

impl From<VisibilityArg> for nodalync_types::Visibility {
    fn from(arg: VisibilityArg) -> Self {
        match arg {
            VisibilityArg::Private => nodalync_types::Visibility::Private,
            VisibilityArg::Unlisted => nodalync_types::Visibility::Unlisted,
            VisibilityArg::Shared => nodalync_types::Visibility::Shared,
        }
    }
}

impl std::fmt::Display for VisibilityArg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Private => write!(f, "private"),
            Self::Unlisted => write!(f, "unlisted"),
            Self::Shared => write!(f, "shared"),
        }
    }
}

/// Content type argument for clap.
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum ContentTypeArg {
    /// Raw input documents.
    L0,
    /// Extracted mentions.
    L1,
    /// Entity graphs.
    L2,
    /// Insights/synthesis.
    L3,
}

impl From<ContentTypeArg> for nodalync_types::ContentType {
    fn from(arg: ContentTypeArg) -> Self {
        match arg {
            ContentTypeArg::L0 => nodalync_types::ContentType::L0,
            ContentTypeArg::L1 => nodalync_types::ContentType::L1,
            ContentTypeArg::L2 => nodalync_types::ContentType::L2,
            ContentTypeArg::L3 => nodalync_types::ContentType::L3,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn test_cli_parse() {
        // Test that CLI can be constructed
        Cli::command().debug_assert();
    }

    #[test]
    fn test_visibility_conversion() {
        let vis: nodalync_types::Visibility = VisibilityArg::Shared.into();
        assert_eq!(vis, nodalync_types::Visibility::Shared);
    }

    #[test]
    fn test_content_type_conversion() {
        let ct: nodalync_types::ContentType = ContentTypeArg::L3.into();
        assert_eq!(ct, nodalync_types::ContentType::L3);
    }
}
