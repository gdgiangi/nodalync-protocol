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
    /// Enable auto-deposit on startup and when accepting channels.
    /// When enabled, the node will automatically deposit HBAR to the settlement
    /// contract to ensure it can accept payment channels from other peers.
    /// Default: false (opt-in for security).
    #[serde(default = "default_auto_deposit")]
    pub auto_deposit: bool,
    /// Minimum balance to maintain in the settlement contract (in HBAR).
    /// If the balance falls below this, auto-deposit will trigger.
    #[serde(default = "default_min_contract_balance")]
    pub min_contract_balance_hbar: f64,
    /// Amount to deposit when auto-deposit triggers (in HBAR).
    #[serde(default = "default_auto_deposit_amount")]
    pub auto_deposit_amount_hbar: f64,
    /// Maximum deposit to accept/match when a peer opens a channel (in HBAR).
    /// Caps how much you'll commit when accepting a channel request.
    /// This is a security measure to prevent unbounded commitment.
    #[serde(default = "default_max_accept_deposit")]
    pub max_accept_deposit_hbar: f64,
}

fn default_auto_deposit() -> bool {
    // SECURITY: Default to false (opt-in) to prevent automatic deposits
    // without explicit user consent. Users must explicitly enable this.
    false
}

fn default_min_contract_balance() -> f64 {
    100.0 // 100 HBAR minimum
}

fn default_auto_deposit_amount() -> f64 {
    200.0 // Deposit 200 HBAR when triggered
}

fn default_max_accept_deposit() -> f64 {
    500.0 // 500 HBAR max accept deposit per channel
}

impl Default for SettlementConfig {
    fn default() -> Self {
        Self {
            network: "hedera-testnet".to_string(),
            account_id: None,
            key_path: None,
            contract_id: None,
            auto_deposit: default_auto_deposit(),
            min_contract_balance_hbar: default_min_contract_balance(),
            auto_deposit_amount_hbar: default_auto_deposit_amount(),
            max_accept_deposit_hbar: default_max_accept_deposit(),
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
/// Delegates to [`nodalync_store::default_data_dir`] to ensure the CLI and MCP
/// server always resolve to the same storage location.
pub fn default_base_dir() -> PathBuf {
    nodalync_store::default_data_dir()
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
        assert_eq!(config.settlement.network, "hedera-testnet");
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
    fn test_hbar_to_tinybars_zero() {
        assert_eq!(hbar_to_tinybars(0.0), 0);
    }

    #[test]
    fn test_hbar_to_tinybars_fractional() {
        assert_eq!(hbar_to_tinybars(0.001), 100_000);
    }

    #[test]
    fn test_tinybars_to_hbar_roundtrip() {
        let original = 1.5;
        let tinybars = hbar_to_tinybars(original);
        let back = tinybars_to_hbar(tinybars);
        assert!((original - back).abs() < 1e-8);
    }

    #[test]
    fn test_ndl_to_units_alias() {
        let amount = 3.5;
        assert_eq!(ndl_to_units(amount), hbar_to_tinybars(amount));
    }

    #[test]
    fn test_units_to_ndl_alias() {
        let units = 500_000_000u64;
        assert_eq!(units_to_ndl(units), tinybars_to_hbar(units));
    }

    #[test]
    fn test_format_hbar_large() {
        let result = format_hbar(1_000_000_000_000);
        assert!(result.contains("HBAR"));
        // 1_000_000_000_000 tinybars = 10_000 HBAR
        assert_eq!(result, "10000.00 HBAR");
    }

    #[test]
    fn test_format_ndl_alias() {
        let units = 50_000_000u64;
        assert_eq!(format_ndl(units), format_hbar(units));
    }

    #[test]
    fn test_default_base_dir() {
        let dir = default_base_dir();
        assert!(!dir.as_os_str().is_empty());
    }

    #[test]
    fn test_default_config_path() {
        let path = default_config_path();
        assert!(path.ends_with("config.toml"));
    }

    #[test]
    fn test_config_save_and_load_roundtrip() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.toml");

        let config = CliConfig::default();
        config.save(&config_path).unwrap();

        let loaded = CliConfig::load(&config_path).unwrap();
        assert_eq!(
            config.economics.default_price,
            loaded.economics.default_price
        );
        assert_eq!(config.settlement.network, loaded.settlement.network);
        assert_eq!(config.network.enabled, loaded.network.enabled);
    }

    #[test]
    fn test_config_load_nonexistent_returns_default() {
        let path = Path::new("/tmp/nodalync_nonexistent_12345/config.toml");
        let config = CliConfig::load(path).unwrap();
        let default = CliConfig::default();
        assert_eq!(
            config.economics.default_price,
            default.economics.default_price
        );
        assert_eq!(config.settlement.network, default.settlement.network);
    }

    #[test]
    fn test_economics_default_price_units() {
        let econ = EconomicsConfig::default();
        // default_price is 0.10 HBAR = 10_000_000 tinybars
        assert_eq!(econ.default_price_units(), hbar_to_tinybars(0.10));
    }

    #[test]
    fn test_economics_auto_settle_threshold() {
        let econ = EconomicsConfig::default();
        // auto_settle_threshold is 100.0 HBAR = 10_000_000_000 tinybars
        assert_eq!(econ.auto_settle_threshold_units(), hbar_to_tinybars(100.0));
    }

    #[test]
    fn test_storage_base_dir() {
        let storage = StorageConfig::default();
        let base = storage.base_dir();
        assert!(!base.as_os_str().is_empty());
        // base_dir should be the parent of content_dir
        assert_eq!(base, storage.content_dir.parent().unwrap().to_path_buf());
    }

    #[test]
    fn test_alerting_config_defaults() {
        let alerting = AlertingConfig::default();
        assert!(!alerting.enabled);
        assert!(alerting.webhooks.is_empty());
        assert!(alerting.node_name.is_none());
        assert!(alerting.region.is_none());
        assert!(alerting.heartbeat.is_none());
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
