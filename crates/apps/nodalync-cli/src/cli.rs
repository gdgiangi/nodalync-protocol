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
    Init {
        /// Run interactive setup wizard.
        #[arg(short, long)]
        wizard: bool,
    },

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
        #[arg(short, long, allow_hyphen_values = true, value_parser = parse_non_negative_price)]
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

        /// Include content available from network peers.
        #[arg(short = 'n', long)]
        network: bool,
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

        /// Price per query in HBAR (defaults to previous version's price).
        #[arg(short, long, allow_hyphen_values = true, value_parser = parse_non_negative_price)]
        price: Option<f64>,
    },

    /// Change content visibility.
    ///
    /// Updates how content is discovered and served.
    Visibility {
        /// Hash of the content.
        hash: String,

        /// New visibility level (private, unlisted, or shared).
        #[arg(short, long, alias = "visibility")]
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
        #[arg(short, long, allow_hyphen_values = true, value_parser = parse_non_negative_price)]
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
    // Channel Commands
    // =========================================================================
    /// Open a payment channel with a peer.
    ///
    /// Creates a new payment channel for off-chain micropayments.
    /// The deposit is locked until the channel is closed.
    ///
    /// Minimum deposit: 100 HBAR
    OpenChannel {
        /// Peer ID to open channel with (ndl1..., 12D3KooW..., or 40 hex chars).
        peer_id: String,

        /// Deposit amount in HBAR (minimum: 100).
        #[arg(short, long)]
        deposit: f64,
    },

    /// Close a payment channel with a peer.
    ///
    /// Attempts cooperative close first (requires peer to be online).
    /// If the peer doesn't respond, suggests using dispute-channel.
    CloseChannel {
        /// Peer ID of the channel to close.
        peer_id: String,
    },

    /// Initiate a dispute-based channel close.
    ///
    /// Use this when the peer is offline or unresponsive.
    /// Starts a 24-hour dispute period before funds can be released.
    DisputeChannel {
        /// Peer ID of the channel to dispute.
        peer_id: String,
    },

    /// Resolve a channel dispute after the waiting period.
    ///
    /// Can only be called after the 24-hour dispute period has elapsed.
    /// Finalizes the channel close and distributes funds.
    ResolveDispute {
        /// Peer ID of the disputed channel.
        peer_id: String,
    },

    /// List all payment channels.
    ///
    /// Shows all channels with their states and balances.
    ListChannels,

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

        /// Enable HTTP health endpoint.
        #[arg(long)]
        health: bool,

        /// Port for the HTTP health endpoint.
        #[arg(long, default_value = "8080")]
        health_port: u16,
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

        /// Enable network connectivity for live peer search.
        ///
        /// When enabled, the MCP server can search connected peers
        /// in real-time using the search_network tool.
        #[arg(long)]
        enable_network: bool,

        /// Hedera account ID for settlement (e.g., 0.0.7703962).
        #[arg(long, env = "NODALYNC_HEDERA_ACCOUNT_ID")]
        hedera_account_id: Option<String>,

        /// Path to Hedera private key file.
        #[arg(long, env = "NODALYNC_HEDERA_KEY_PATH")]
        hedera_private_key: Option<PathBuf>,

        /// Hedera settlement contract ID (default: 0.0.7729011).
        #[arg(
            long,
            env = "NODALYNC_HEDERA_CONTRACT_ID",
            default_value = "0.0.7729011"
        )]
        hedera_contract_id: String,

        /// Hedera network (testnet, mainnet, previewnet).
        #[arg(long, env = "NODALYNC_HEDERA_NETWORK", default_value = "testnet")]
        hedera_network: String,
    },

    // =========================================================================
    // Discovery Commands
    // =========================================================================
    /// Search for content by keyword.
    ///
    /// Searches title, description, and tags of local content.
    Search {
        /// Search query (matches title, description, and tags).
        query: String,

        /// Filter by content type.
        #[arg(short = 't', long)]
        content_type: Option<ContentTypeArg>,

        /// Maximum results to show.
        #[arg(short, long, default_value = "20")]
        limit: u32,

        /// Search across network (not just local).
        #[arg(short, long)]
        all: bool,
    },

    // =========================================================================
    // Shell Completion Commands
    // =========================================================================
    /// Generate shell completions.
    ///
    /// Outputs shell completion scripts for various shells.
    Completions {
        /// Shell to generate completions for.
        shell: CompletionShell,
    },
}

/// Shell types for completion generation.
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum CompletionShell {
    /// Bash shell.
    Bash,
    /// Zsh shell.
    Zsh,
    /// Fish shell.
    Fish,
    /// PowerShell.
    PowerShell,
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

/// Minimum non-zero price in HBAR (1 tinybar = 0.00000001 HBAR).
const MIN_NONZERO_PRICE_HBAR: f64 = 0.00000001;

/// Parse a price value, rejecting negative numbers and sub-tinybar amounts.
fn parse_non_negative_price(s: &str) -> Result<f64, String> {
    let value: f64 = s
        .parse()
        .map_err(|_| format!("'{}' is not a valid price", s))?;
    if value < 0.0 {
        return Err("Price cannot be negative".to_string());
    }
    // Issue #48: reject positive prices that round to 0 tinybars
    if value > 0.0 && value < MIN_NONZERO_PRICE_HBAR {
        return Err(format!(
            "Price {} HBAR is too small — minimum non-zero price is {} HBAR (1 tinybar)",
            value, MIN_NONZERO_PRICE_HBAR
        ));
    }
    Ok(value)
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

    /// Regression test for Issue #21: negative price should show clear error.
    #[test]
    fn test_negative_price_rejected() {
        let result = parse_non_negative_price("-100");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Price cannot be negative");
    }

    #[test]
    fn test_valid_price_accepted() {
        assert_eq!(parse_non_negative_price("0").unwrap(), 0.0);
        assert_eq!(parse_non_negative_price("0.10").unwrap(), 0.10);
        assert_eq!(parse_non_negative_price("100").unwrap(), 100.0);
    }

    #[test]
    fn test_invalid_price_format() {
        let result = parse_non_negative_price("abc");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not a valid price"));
    }

    /// Regression test for Issue #21: verify clap passes negative price to
    /// value_parser instead of interpreting it as a flag.
    #[test]
    fn test_clap_negative_price_error_message() {
        let result = Cli::try_parse_from(["nodalync", "publish", "file.txt", "--price", "-100"]);
        assert!(result.is_err());
        let err_str = result.unwrap_err().to_string();
        assert!(
            err_str.contains("Price cannot be negative"),
            "Clap should report 'Price cannot be negative', got: {}",
            err_str
        );
    }

    /// Regression test for Issue #48: price that rounds to 0 tinybars should be rejected.
    #[test]
    fn test_sub_tinybar_price_rejected() {
        let result = parse_non_negative_price("0.000000001");
        assert!(result.is_err());
        let err_msg = result.unwrap_err();
        assert!(
            err_msg.contains("too small"),
            "Error should say price is too small, got: {}",
            err_msg
        );
    }

    #[test]
    fn test_minimum_valid_nonzero_price() {
        // 0.00000001 HBAR = 1 tinybar, should be accepted
        let result = parse_non_negative_price("0.00000001");
        assert!(result.is_ok(), "1 tinybar price should be accepted");
        assert_eq!(result.unwrap(), 0.00000001);
    }

    #[test]
    fn test_zero_price_still_allowed() {
        // Price of exactly 0 is free content and should be allowed
        let result = parse_non_negative_price("0");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0.0);
    }
}
