//! Output formatting for CLI.

use colored::Colorize;
use nodalync_types::{L1Summary, Manifest};
use serde::Serialize;

use crate::config::format_ndl;

/// Output format for CLI commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputFormat {
    /// Human-readable output.
    #[default]
    Human,
    /// JSON output.
    Json,
}

impl std::str::FromStr for OutputFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "human" | "text" => Ok(Self::Human),
            "json" => Ok(Self::Json),
            _ => Err(format!("Unknown format: {}. Use 'human' or 'json'.", s)),
        }
    }
}

impl std::fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Human => write!(f, "human"),
            Self::Json => write!(f, "json"),
        }
    }
}

/// Trait for renderable output.
pub trait Render {
    /// Render as human-readable string.
    fn render_human(&self) -> String;

    /// Render as JSON string.
    fn render_json(&self) -> String;

    /// Render in the specified format.
    fn render(&self, format: OutputFormat) -> String {
        match format {
            OutputFormat::Human => self.render_human(),
            OutputFormat::Json => self.render_json(),
        }
    }
}

// =============================================================================
// Output Types
// =============================================================================

/// Output for identity initialization.
#[derive(Debug, Serialize)]
pub struct InitOutput {
    pub peer_id: String,
    pub config_path: String,
}

impl Render for InitOutput {
    fn render_human(&self) -> String {
        format!(
            "{} {}\n{} {}",
            "Identity created:".green().bold(),
            self.peer_id,
            "Configuration saved to:".green(),
            self.config_path
        )
    }

    fn render_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
}

/// Output for whoami command.
#[derive(Debug, Serialize)]
pub struct WhoamiOutput {
    pub peer_id: String,
    pub public_key: String,
    pub addresses: Vec<String>,
}

impl Render for WhoamiOutput {
    fn render_human(&self) -> String {
        let mut lines = vec![
            format!("{} {}", "PeerId:".bold(), self.peer_id),
            format!("{} {}", "Public Key:".bold(), self.public_key),
        ];
        if !self.addresses.is_empty() {
            lines.push(format!("{}", "Addresses:".bold()));
            for addr in &self.addresses {
                lines.push(format!("  {}", addr));
            }
        }
        lines.join("\n")
    }

    fn render_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
}

/// Output for publish command.
#[derive(Debug, Serialize)]
pub struct PublishOutput {
    pub hash: String,
    pub title: String,
    pub size: u64,
    pub price: u64,
    pub visibility: String,
    pub mentions: Option<usize>,
}

impl Render for PublishOutput {
    fn render_human(&self) -> String {
        let mut lines = vec![
            format!("{} {}", "Published:".green().bold(), self.hash),
            format!("{} \"{}\"", "Title:".bold(), self.title),
            format!("{} {} bytes", "Size:".bold(), self.size),
            format!("{} {}", "Price:".bold(), format_ndl(self.price)),
            format!("{} {}", "Visibility:".bold(), self.visibility),
        ];
        if let Some(count) = self.mentions {
            lines.push(format!("{} {} found", "L1 Mentions:".bold(), count));
        }
        lines.join("\n")
    }

    fn render_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
}

/// Output for list command.
#[derive(Debug, Serialize)]
pub struct ListOutput {
    pub manifests: Vec<ManifestSummary>,
    pub total: usize,
}

#[derive(Debug, Serialize)]
pub struct ManifestSummary {
    pub hash: String,
    pub title: String,
    pub version: u32,
    pub visibility: String,
    pub price: u64,
    pub queries: u64,
    pub content_type: String,
}

impl Render for ListOutput {
    fn render_human(&self) -> String {
        if self.manifests.is_empty() {
            return "No content found.".dimmed().to_string();
        }

        let mut lines = vec![];

        // Group by visibility
        let shared: Vec<_> = self
            .manifests
            .iter()
            .filter(|m| m.visibility == "Shared")
            .collect();
        let unlisted: Vec<_> = self
            .manifests
            .iter()
            .filter(|m| m.visibility == "Unlisted")
            .collect();
        let private: Vec<_> = self
            .manifests
            .iter()
            .filter(|m| m.visibility == "Private")
            .collect();

        if !shared.is_empty() {
            lines.push(format!("{} ({})", "SHARED".green().bold(), shared.len()));
            for m in shared {
                lines.push(format_manifest_line(m));
            }
            lines.push(String::new());
        }

        if !unlisted.is_empty() {
            lines.push(format!("{} ({})", "UNLISTED".yellow().bold(), unlisted.len()));
            for m in unlisted {
                lines.push(format_manifest_line(m));
            }
            lines.push(String::new());
        }

        if !private.is_empty() {
            lines.push(format!("{} ({})", "PRIVATE".dimmed().bold(), private.len()));
            for m in private {
                lines.push(format_manifest_line(m));
            }
        }

        lines.join("\n")
    }

    fn render_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
}

fn format_manifest_line(m: &ManifestSummary) -> String {
    let hash_short = short_hash(&m.hash);
    let price_str = if m.price > 0 {
        format!(", {}", format_ndl(m.price))
    } else {
        String::new()
    };
    let queries_str = if m.queries > 0 {
        format!(", {} queries", m.queries)
    } else {
        String::new()
    };
    format!(
        "  {} \"{}\" v{}{}{}",
        hash_short.cyan(),
        m.title,
        m.version,
        price_str,
        queries_str
    )
}

/// Output for preview command.
#[derive(Debug, Serialize)]
pub struct PreviewOutput {
    pub hash: String,
    pub title: String,
    pub owner: String,
    pub price: u64,
    pub queries: u64,
    pub content_type: String,
    pub visibility: String,
    pub size: u64,
    pub mentions: Option<PreviewMentions>,
}

#[derive(Debug, Serialize)]
pub struct PreviewMentions {
    pub total: usize,
    pub preview: Vec<String>,
}

impl Render for PreviewOutput {
    fn render_human(&self) -> String {
        let mut lines = vec![
            format!("{} \"{}\"", "Title:".bold(), self.title),
            format!("{} {}", "Hash:".bold(), self.hash),
            format!("{} {}", "Owner:".bold(), short_peer_id(&self.owner)),
            format!("{} {}", "Price:".bold(), format_ndl(self.price)),
            format!("{} {}", "Queries:".bold(), self.queries),
            format!("{} {}", "Type:".bold(), self.content_type),
            format!("{} {} bytes", "Size:".bold(), self.size),
        ];

        if let Some(mentions) = &self.mentions {
            lines.push(String::new());
            lines.push(format!(
                "{} ({} of {}):",
                "L1 Mentions".bold(),
                mentions.preview.len(),
                mentions.total
            ));
            for mention in &mentions.preview {
                lines.push(format!("  - {}", mention));
            }
        }

        lines.join("\n")
    }

    fn render_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
}

/// Output for query command.
#[derive(Debug, Serialize)]
pub struct QueryOutput {
    pub hash: String,
    pub title: String,
    pub price_paid: u64,
    pub saved_to: String,
}

impl Render for QueryOutput {
    fn render_human(&self) -> String {
        format!(
            "{} {}\n{} {}\n{} {}\n{} {}",
            "Queried:".green().bold(),
            self.hash,
            "Title:".bold(),
            self.title,
            "Payment:".bold(),
            format_ndl(self.price_paid),
            "Saved to:".bold(),
            self.saved_to
        )
    }

    fn render_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
}

/// Output for balance command.
#[derive(Debug, Serialize)]
pub struct BalanceOutput {
    pub protocol_balance: u64,
    pub pending_earnings: u64,
    pub pending_payments: u32,
}

impl Render for BalanceOutput {
    fn render_human(&self) -> String {
        format!(
            "{} {}\n{} {}\n{} {} payments",
            "Protocol Balance:".bold(),
            format_ndl(self.protocol_balance).green(),
            "Pending Earnings:".bold(),
            format_ndl(self.pending_earnings),
            "Pending Settlement:".bold(),
            self.pending_payments
        )
    }

    fn render_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
}

/// Output for settle command.
#[derive(Debug, Serialize)]
pub struct SettleOutput {
    pub batch_id: Option<String>,
    pub payments_settled: u32,
    pub amount_settled: u64,
    pub recipients: u32,
}

impl Render for SettleOutput {
    fn render_human(&self) -> String {
        if self.batch_id.is_none() {
            return "No pending payments to settle.".dimmed().to_string();
        }

        format!(
            "{}\n{} {}\n{} {} to {} recipients",
            "Settlement complete!".green().bold(),
            "Batch ID:".bold(),
            self.batch_id.as_deref().unwrap_or("N/A"),
            "Settled:".bold(),
            format_ndl(self.amount_settled),
            self.recipients
        )
    }

    fn render_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
}

/// Output for deposit/withdraw commands.
#[derive(Debug, Serialize)]
pub struct TransactionOutput {
    pub operation: String,
    pub amount: u64,
    pub new_balance: u64,
    pub transaction_id: String,
}

impl Render for TransactionOutput {
    fn render_human(&self) -> String {
        format!(
            "{} {}\n{} {}\n{} {}",
            format!("{}:", self.operation.to_uppercase()).green().bold(),
            format_ndl(self.amount),
            "New Balance:".bold(),
            format_ndl(self.new_balance),
            "Transaction:".bold(),
            self.transaction_id
        )
    }

    fn render_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
}

/// Output for versions command.
#[derive(Debug, Serialize)]
pub struct VersionsOutput {
    pub version_root: String,
    pub versions: Vec<VersionInfo>,
}

#[derive(Debug, Serialize)]
pub struct VersionInfo {
    pub version: u32,
    pub hash: String,
    pub timestamp: u64,
    pub visibility: String,
    pub is_latest: bool,
}

impl Render for VersionsOutput {
    fn render_human(&self) -> String {
        let mut lines = vec![format!(
            "{} {}",
            "Version root:".bold(),
            short_hash(&self.version_root)
        )];

        for v in &self.versions {
            let latest_marker = if v.is_latest { " [latest]".green() } else { "".into() };
            let timestamp = format_timestamp(v.timestamp);
            lines.push(format!(
                "  v{}: {} ({}) - {}{}",
                v.version,
                short_hash(&v.hash).cyan(),
                timestamp,
                v.visibility.to_lowercase(),
                latest_marker
            ));
        }

        lines.join("\n")
    }

    fn render_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
}

/// Output for visibility command.
#[derive(Debug, Serialize)]
pub struct VisibilityOutput {
    pub hash: String,
    pub new_visibility: String,
}

impl Render for VisibilityOutput {
    fn render_human(&self) -> String {
        format!(
            "{} {} -> {}",
            "Visibility updated:".green().bold(),
            short_hash(&self.hash),
            self.new_visibility
        )
    }

    fn render_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
}

/// Output for update command.
#[derive(Debug, Serialize)]
pub struct UpdateOutput {
    pub previous_hash: String,
    pub previous_version: u32,
    pub new_hash: String,
    pub new_version: u32,
    pub version_root: String,
}

impl Render for UpdateOutput {
    fn render_human(&self) -> String {
        format!(
            "{} {} (v{})\n{} {} (v{})\n{} {}",
            "Previous:".bold(),
            short_hash(&self.previous_hash),
            self.previous_version,
            "New:".green().bold(),
            short_hash(&self.new_hash),
            self.new_version,
            "Version root:".bold(),
            short_hash(&self.version_root)
        )
    }

    fn render_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
}

/// Output for synthesize command.
#[derive(Debug, Serialize)]
pub struct SynthesizeOutput {
    pub hash: String,
    pub source_count: usize,
    pub provenance_roots: usize,
    pub published: bool,
    pub price: Option<u64>,
}

impl Render for SynthesizeOutput {
    fn render_human(&self) -> String {
        let mut lines = vec![
            format!("{} {}", "L3 hash:".green().bold(), self.hash),
            format!("{} {}", "Sources:".bold(), self.source_count),
            format!("{} {}", "Provenance roots:".bold(), self.provenance_roots),
        ];

        if self.published {
            if let Some(price) = self.price {
                lines.push(format!(
                    "{} {} ({})",
                    "Published:".bold(),
                    "Yes".green(),
                    format_ndl(price)
                ));
            } else {
                lines.push(format!("{} {}", "Published:".bold(), "Yes".green()));
            }
        }

        lines.join("\n")
    }

    fn render_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
}

/// Output for build-l2 command.
#[derive(Debug, Serialize)]
pub struct BuildL2Output {
    pub hash: String,
    pub entity_count: usize,
    pub relationship_count: usize,
    pub source_count: usize,
}

impl Render for BuildL2Output {
    fn render_human(&self) -> String {
        format!(
            "{} {}\n{} {}\n{} {}\n{} {}",
            "L2 Entity Graph:".green().bold(),
            self.hash,
            "Entities:".bold(),
            self.entity_count,
            "Relationships:".bold(),
            self.relationship_count,
            "Sources:".bold(),
            self.source_count
        )
    }

    fn render_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
}

/// Output for merge-l2 command.
#[derive(Debug, Serialize)]
pub struct MergeL2Output {
    pub hash: String,
    pub merged_count: usize,
    pub entity_count: usize,
    pub relationship_count: usize,
}

impl Render for MergeL2Output {
    fn render_human(&self) -> String {
        format!(
            "{} {}\n{} {}\n{} {}\n{} {}",
            "Merged L2 Graph:".green().bold(),
            self.hash,
            "Graphs merged:".bold(),
            self.merged_count,
            "Total entities:".bold(),
            self.entity_count,
            "Total relationships:".bold(),
            self.relationship_count
        )
    }

    fn render_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
}

/// Output for status command.
#[derive(Debug, Serialize)]
pub struct StatusOutput {
    pub running: bool,
    pub peer_id: String,
    pub uptime_secs: Option<u64>,
    pub connected_peers: u32,
    pub shared_content: u32,
    pub private_content: u32,
    pub pending_payments: u32,
    pub pending_amount: u64,
}

impl Render for StatusOutput {
    fn render_human(&self) -> String {
        let status = if self.running {
            "running".green()
        } else {
            "stopped".red()
        };

        let mut lines = vec![
            format!("{} {}", "Node:".bold(), status),
            format!("{} {}", "PeerId:".bold(), short_peer_id(&self.peer_id)),
        ];

        if let Some(uptime) = self.uptime_secs {
            lines.push(format!("{} {}", "Uptime:".bold(), format_duration(uptime)));
        }

        lines.push(format!(
            "{} {} connected",
            "Peers:".bold(),
            self.connected_peers
        ));
        lines.push(format!(
            "{} {} shared, {} private",
            "Content:".bold(),
            self.shared_content,
            self.private_content
        ));
        lines.push(format!(
            "{} {} payments ({})",
            "Pending:".bold(),
            self.pending_payments,
            format_ndl(self.pending_amount)
        ));

        lines.join("\n")
    }

    fn render_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
}

/// Output for start command.
#[derive(Debug, Serialize)]
pub struct StartOutput {
    pub peer_id: String,
    pub listen_addresses: Vec<String>,
    pub connected_peers: u32,
    pub daemon: bool,
}

impl Render for StartOutput {
    fn render_human(&self) -> String {
        let mut lines = vec![
            format!("{}", "Nodalync node started!".green().bold()),
            format!("{} {}", "PeerId:".bold(), short_peer_id(&self.peer_id)),
        ];

        for addr in &self.listen_addresses {
            lines.push(format!("{} {}", "Listening on:".bold(), addr));
        }

        lines.push(format!(
            "{} {} peers",
            "Connected to:".bold(),
            self.connected_peers
        ));

        if self.daemon {
            lines.push(format!("{}", "Running in background.".dimmed()));
        }

        lines.join("\n")
    }

    fn render_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
}

/// Output for stop command.
#[derive(Debug, Serialize)]
pub struct StopOutput {
    pub success: bool,
}

impl Render for StopOutput {
    fn render_human(&self) -> String {
        if self.success {
            format!(
                "{}\n{}",
                "Shutting down gracefully...".yellow(),
                "Node stopped.".green()
            )
        } else {
            "Failed to stop node.".red().to_string()
        }
    }

    fn render_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
}

/// Output for delete command.
#[derive(Debug, Serialize)]
pub struct DeleteOutput {
    pub hash: String,
}

impl Render for DeleteOutput {
    fn render_human(&self) -> String {
        format!(
            "{} {} (local copy only, provenance preserved)",
            "Deleted:".green().bold(),
            short_hash(&self.hash)
        )
    }

    fn render_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Shorten a hash for display (first 8 + last 4 characters).
pub fn short_hash(hash: &str) -> String {
    if hash.len() <= 16 {
        hash.to_string()
    } else {
        format!("{}...{}", &hash[..8], &hash[hash.len() - 4..])
    }
}

/// Shorten a peer ID for display.
pub fn short_peer_id(peer_id: &str) -> String {
    if peer_id.len() <= 20 {
        peer_id.to_string()
    } else {
        format!("{}...", &peer_id[..16])
    }
}

/// Format a timestamp as a human-readable date.
fn format_timestamp(ts: u64) -> String {
    // Simple ISO-like format: YYYY-MM-DD
    // This is a simplified version; in production you'd use chrono
    let secs = ts / 1000;
    let days = secs / 86400;
    let years_since_1970 = days / 365;
    let year = 1970 + years_since_1970;
    let day_of_year = days % 365;
    let month = (day_of_year / 30) + 1;
    let day = (day_of_year % 30) + 1;
    format!("{:04}-{:02}-{:02}", year, month.min(12), day.min(31))
}

/// Format duration in seconds as human-readable.
fn format_duration(secs: u64) -> String {
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else {
        format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
    }
}

/// Convert manifest to summary.
pub fn manifest_to_summary(m: &Manifest) -> ManifestSummary {
    ManifestSummary {
        hash: m.hash.to_string(),
        title: m.metadata.title.clone(),
        version: m.version.number,
        visibility: format!("{:?}", m.visibility),
        price: m.economics.price,
        queries: m.economics.total_queries,
        content_type: format!("{:?}", m.content_type),
    }
}

/// Convert L1Summary to preview mentions.
pub fn l1_to_preview(summary: &L1Summary) -> PreviewMentions {
    PreviewMentions {
        total: summary.mention_count as usize,
        preview: summary
            .preview_mentions
            .iter()
            .map(|m| m.content.clone())
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_format_parse() {
        assert_eq!("human".parse::<OutputFormat>().unwrap(), OutputFormat::Human);
        assert_eq!("json".parse::<OutputFormat>().unwrap(), OutputFormat::Json);
        assert!("invalid".parse::<OutputFormat>().is_err());
    }

    #[test]
    fn test_short_hash() {
        let hash = "abcdefghijklmnopqrstuvwxyz123456";
        assert_eq!(short_hash(hash), "abcdefgh...3456");
        assert_eq!(short_hash("short"), "short");
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(30), "30s");
        assert_eq!(format_duration(90), "1m 30s");
        assert_eq!(format_duration(3661), "1h 1m");
    }

    #[test]
    fn test_render_trait() {
        let output = InitOutput {
            peer_id: "ndl1abc123".to_string(),
            config_path: "~/.nodalync/config.toml".to_string(),
        };

        let human = output.render(OutputFormat::Human);
        assert!(human.contains("Identity created"));

        let json = output.render(OutputFormat::Json);
        assert!(json.contains("peer_id"));
    }
}
