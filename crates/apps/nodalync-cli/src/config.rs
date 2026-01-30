//! CLI configuration.

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::error::{CliError, CliResult};

/// Expand environment variables in a string.
/// Supports `${VAR_NAME}` syntax.
fn expand_env_vars(input: &str) -> String {
    let re = Regex::new(r"\$\{([A-Za-z_][A-Za-z0-9_]*)\}").unwrap();
    re.replace_all(input, |caps: &regex::Captures| {
        let var_name = &caps[1];
        std::env::var(var_name).unwrap_or_else(|_| caps[0].to_string())
    })
    .to_string()
}

/// CLI configuration loaded from TOML.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CliConfig {
    /// Identity configuration.
    pub identity: IdentityConfig,
    /// Storage configuration.
    pub storage: StorageConfig,
    /// Network configuration.
    pub network: NetworkConfigSection,
    /// Settlement configuration.
    pub settlement: SettlementConfig,
    /// Economics configuration.
    pub economics: EconomicsConfig,
    /// Display configuration.
    pub display: DisplayConfig,
    /// Alerting configuration.
    pub alerting: AlertingConfig,
}

impl Default for CliConfig {
    fn default() -> Self {
        let base_dir = default_base_dir();
        Self {
            identity: IdentityConfig::new(&base_dir),
            storage: StorageConfig::new(&base_dir),
            network: NetworkConfigSection::default(),
            settlement: SettlementConfig::default(),
            economics: EconomicsConfig::default(),
            display: DisplayConfig::default(),
            alerting: AlertingConfig::default(),
        }
    }
}

impl CliConfig {
    /// Load configuration from a file.
    /// Environment variables in `${VAR}` format are expanded in webhook URLs.
    pub fn load(path: &Path) -> CliResult<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let contents = std::fs::read_to_string(path)?;
        let mut config: Self = toml::from_str(&contents)?;

        // Expand environment variables in webhook URLs
        for webhook in &mut config.alerting.webhooks {
            webhook.url = expand_env_vars(&webhook.url);
        }

        Ok(config)
    }

    /// Load configuration from the default location.
    pub fn load_default() -> CliResult<Self> {
        let path = default_config_path();
        Self::load(&path)
    }

    /// Save configuration to a file.
    pub fn save(&self, path: &Path) -> CliResult<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let contents = toml::to_string_pretty(self)
            .map_err(|e| CliError::config(format!("Failed to serialize config: {}", e)))?;
        std::fs::write(path, contents)?;
        Ok(())
    }

    /// Save configuration to the default location.
    pub fn save_default(&self) -> CliResult<()> {
        let path = default_config_path();
        self.save(&path)
    }

    /// Get the base directory for all nodalync data.
    pub fn base_dir(&self) -> PathBuf {
        self.storage.base_dir()
    }
}

/// Identity configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityConfig {
    /// Path to the encrypted keypair file.
    pub keyfile: PathBuf,
}

impl IdentityConfig {
    fn new(base_dir: &Path) -> Self {
        Self {
            keyfile: base_dir.join("identity").join("keypair.key"),
        }
    }
}

impl Default for IdentityConfig {
    fn default() -> Self {
        Self::new(&default_base_dir())
    }
}

/// Storage configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Directory for content storage.
    pub content_dir: PathBuf,
    /// Path to the SQLite database.
    pub database: PathBuf,
    /// Directory for cached content.
    pub cache_dir: PathBuf,
    /// Maximum cache size in megabytes.
    #[serde(default = "default_cache_max_size")]
    pub cache_max_size_mb: u64,
}

impl StorageConfig {
    fn new(base_dir: &Path) -> Self {
        Self {
            content_dir: base_dir.join("content"),
            database: base_dir.join("nodalync.db"),
            cache_dir: base_dir.join("cache"),
            cache_max_size_mb: default_cache_max_size(),
        }
    }

    /// Get the base directory (parent of content_dir).
    pub fn base_dir(&self) -> PathBuf {
        self.content_dir
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(default_base_dir)
    }
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self::new(&default_base_dir())
    }
}

fn default_cache_max_size() -> u64 {
    1000
}

/// Network configuration section in CLI config.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NetworkConfigSection {
    /// Whether networking is enabled.
    pub enabled: bool,
    /// Addresses to listen on.
    pub listen_addresses: Vec<String>,
    /// Bootstrap nodes to connect to.
    pub bootstrap_nodes: Vec<String>,
    /// Time to wait for GossipSub propagation (seconds).
    #[serde(default = "default_gossipsub_propagation_wait")]
    pub gossipsub_propagation_wait: u64,
}

fn default_gossipsub_propagation_wait() -> u64 {
    5
}

/// Default bootstrap node addresses (US, EU, Asia).
const DEFAULT_BOOTSTRAP_NODES: &[&str] = &[
    "/dns4/nodalync-bootstrap.eastus.azurecontainer.io/tcp/9000/p2p/12D3KooWMqrUmZm4e1BJTRMWqKHCe1TSX9Vu83uJLEyCGr2dUjYm",
    "/dns4/nodalync-eu.northeurope.azurecontainer.io/tcp/9000/p2p/12D3KooWQiK8uHf877wena9MAPHHprXkmGRhAmXAYakRsMfdnk7P",
    "/dns4/nodalync-asia.southeastasia.azurecontainer.io/tcp/9000/p2p/12D3KooWFojioE6LXFs3qqBdKQeCFuMr2obsMrvXGY69jmhheLfk",
];

impl Default for NetworkConfigSection {
    fn default() -> Self {
        Self {
            enabled: true,
            listen_addresses: vec!["/ip4/0.0.0.0/tcp/9000".to_string()],
            bootstrap_nodes: DEFAULT_BOOTSTRAP_NODES
                .iter()
                .map(|s| s.to_string())
                .collect(),
            gossipsub_propagation_wait: default_gossipsub_propagation_wait(),
        }
    }
}

/// Settlement configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SettlementConfig {
    /// Network to use (e.g., "hedera-testnet", "hedera-mainnet", "mock").
    pub network: String,
    /// Account ID for Hedera.
    pub account_id: Option<String>,
    /// Path to Hedera private key.
    pub key_path: Option<PathBuf>,
    /// Settlement contract ID for Hedera.
    pub contract_id: Option<String>,
}

impl Default for SettlementConfig {
    fn default() -> Self {
        Self {
            network: "mock".to_string(),
            account_id: None,
            key_path: None,
            contract_id: None,
        }
    }
}

/// Economics configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct EconomicsConfig {
    /// Default price for published content (in HBAR).
    pub default_price: f64,
    /// Threshold for automatic settlement (in HBAR).
    pub auto_settle_threshold: f64,
}

impl Default for EconomicsConfig {
    fn default() -> Self {
        Self {
            default_price: 0.10,
            auto_settle_threshold: 100.0,
        }
    }
}

impl EconomicsConfig {
    /// Convert HBAR to tinybars (10^-8 HBAR).
    pub fn default_price_units(&self) -> u64 {
        hbar_to_tinybars(self.default_price)
    }

    /// Convert HBAR to tinybars (10^-8 HBAR).
    pub fn auto_settle_threshold_units(&self) -> u64 {
        ndl_to_units(self.auto_settle_threshold)
    }
}

/// Display configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DisplayConfig {
    /// Default output format.
    pub default_format: String,
    /// Whether to show previews in search results.
    pub show_previews: bool,
    /// Maximum search results to display.
    pub max_search_results: u32,
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            default_format: "human".to_string(),
            show_previews: true,
            max_search_results: 20,
        }
    }
}

/// Get the default base directory for nodalync data.
///
/// Priority:
/// 1. `NODALYNC_DATA_DIR` environment variable (if set)
/// 2. Platform-specific data directory (e.g., `~/.local/share/nodalync` on Linux)
/// 3. Fallback to `~/.nodalync`
pub fn default_base_dir() -> PathBuf {
    // Check environment variable first
    if let Ok(dir) = std::env::var("NODALYNC_DATA_DIR") {
        return PathBuf::from(dir);
    }

    // Use platform-specific data directory
    directories::ProjectDirs::from("io", "nodalync", "nodalync")
        .map(|dirs| dirs.data_dir().to_path_buf())
        .unwrap_or_else(|| {
            // Fallback to home directory
            std::env::var("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(".nodalync")
        })
}

/// Get the default config file path.
pub fn default_config_path() -> PathBuf {
    default_base_dir().join("config.toml")
}

/// Convert HBAR to tinybars (10^-8 HBAR).
pub fn hbar_to_tinybars(hbar: f64) -> u64 {
    (hbar * 100_000_000.0) as u64
}

/// Convert tinybars to HBAR.
pub fn tinybars_to_hbar(tinybars: u64) -> f64 {
    tinybars as f64 / 100_000_000.0
}

/// Format an amount in HBAR with proper precision.
pub fn format_hbar(tinybars: u64) -> String {
    let hbar = tinybars_to_hbar(tinybars);
    if hbar == 0.0 {
        "0 HBAR".to_string()
    } else if hbar < 0.01 {
        format!("{:.8} HBAR", hbar)
    } else if hbar < 1.0 {
        format!("{:.4} HBAR", hbar)
    } else {
        format!("{:.2} HBAR", hbar)
    }
}

// Legacy aliases for backward compatibility during transition
#[doc(hidden)]
pub fn ndl_to_units(ndl: f64) -> u64 {
    hbar_to_tinybars(ndl)
}

#[doc(hidden)]
pub fn units_to_ndl(units: u64) -> f64 {
    tinybars_to_hbar(units)
}

#[doc(hidden)]
pub fn format_ndl(units: u64) -> String {
    format_hbar(units)
}

// =============================================================================
// Alerting Configuration
// =============================================================================

/// Alerting configuration for webhook-based notifications.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AlertingConfig {
    /// Whether alerting is enabled.
    pub enabled: bool,
    /// Human-readable name for this node (used in alerts).
    pub node_name: Option<String>,
    /// Region identifier (used in alerts).
    pub region: Option<String>,
    /// Webhook configurations.
    pub webhooks: Vec<WebhookConfig>,
    /// Alert trigger conditions.
    pub conditions: AlertConditions,
    /// Rate limiting configuration.
    pub rate_limit: RateLimitConfig,
    /// Heartbeat configuration (periodic health pings).
    pub heartbeat: Option<HeartbeatConfig>,
}

/// Webhook configuration for a single endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookConfig {
    /// Webhook URL.
    pub url: String,
    /// Webhook format type.
    #[serde(default)]
    pub webhook_type: WebhookType,
    /// Optional authorization header value (e.g., "Bearer token").
    pub auth_header: Option<String>,
    /// Alert types to send to this webhook (empty = all).
    #[serde(default)]
    pub alert_types: Vec<String>,
    /// Request timeout in seconds.
    #[serde(default = "default_webhook_timeout")]
    pub timeout_secs: u64,
}

fn default_webhook_timeout() -> u64 {
    10
}

/// Webhook format types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum WebhookType {
    /// Raw JSON payload.
    #[default]
    Generic,
    /// Slack Incoming Webhook format.
    Slack,
    /// Discord Webhook format.
    Discord,
    /// PagerDuty Events API v2 format.
    Pagerduty,
}

/// Alert trigger conditions.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AlertConditions {
    /// Seconds with zero peers before triggering no_peers alert.
    pub no_peers_threshold_secs: u64,
    /// Minimum peer count before triggering low_peer_count alert.
    pub min_peer_count: Option<u32>,
    /// Whether to send an alert on node startup.
    pub alert_on_startup: bool,
    /// Whether to send an alert on graceful shutdown.
    pub alert_on_shutdown: bool,
}

impl Default for AlertConditions {
    fn default() -> Self {
        Self {
            no_peers_threshold_secs: 60,
            min_peer_count: None,
            alert_on_startup: true,
            alert_on_shutdown: true,
        }
    }
}

/// Rate limiting configuration for alerts.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RateLimitConfig {
    /// Minimum seconds between alerts of the same type.
    pub min_interval_secs: u64,
    /// Seconds after recovery before re-alerting.
    pub recovery_cooldown_secs: u64,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            min_interval_secs: 300, // 5 minutes
            recovery_cooldown_secs: 60,
        }
    }
}

/// Heartbeat configuration for periodic health pings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatConfig {
    /// Heartbeat interval in seconds.
    pub interval_secs: u64,
    /// Whether to include metrics in heartbeat.
    #[serde(default)]
    pub include_metrics: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = CliConfig::default();
        assert!(config.network.enabled);
        assert_eq!(config.economics.default_price, 0.10);
        assert_eq!(config.settlement.network, "mock");
    }

    #[test]
    fn test_hbar_conversion() {
        assert_eq!(hbar_to_tinybars(1.0), 100_000_000);
        assert_eq!(hbar_to_tinybars(0.10), 10_000_000);
        assert_eq!(tinybars_to_hbar(100_000_000), 1.0);
        assert_eq!(tinybars_to_hbar(10_000_000), 0.10);
    }

    #[test]
    fn test_format_hbar() {
        assert_eq!(format_hbar(0), "0 HBAR");
        assert_eq!(format_hbar(100_000_000), "1.00 HBAR");
        assert_eq!(format_hbar(10_000_000), "0.1000 HBAR");
        assert_eq!(format_hbar(1000), "0.00001000 HBAR");
    }

    #[test]
    fn test_config_serialization() {
        let config = CliConfig::default();
        let toml_str = toml::to_string_pretty(&config).unwrap();
        let parsed: CliConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(
            config.economics.default_price,
            parsed.economics.default_price
        );
    }

    #[test]
    fn test_expand_env_vars() {
        std::env::set_var("TEST_WEBHOOK_URL", "https://example.com/webhook");

        let input = "${TEST_WEBHOOK_URL}";
        let result = super::expand_env_vars(input);
        assert_eq!(result, "https://example.com/webhook");

        // Unset variable should remain as-is
        let input_unset = "${NONEXISTENT_VAR_12345}";
        let result_unset = super::expand_env_vars(input_unset);
        assert_eq!(result_unset, "${NONEXISTENT_VAR_12345}");

        // Mixed content
        let mixed = "prefix_${TEST_WEBHOOK_URL}_suffix";
        let result_mixed = super::expand_env_vars(mixed);
        assert_eq!(result_mixed, "prefix_https://example.com/webhook_suffix");

        std::env::remove_var("TEST_WEBHOOK_URL");
    }
}
