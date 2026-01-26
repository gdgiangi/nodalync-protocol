//! CLI configuration.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::error::{CliError, CliResult};

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
        }
    }
}

impl CliConfig {
    /// Load configuration from a file.
    pub fn load(path: &Path) -> CliResult<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let contents = std::fs::read_to_string(path)?;
        let config: Self = toml::from_str(&contents)?;
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
}

impl Default for NetworkConfigSection {
    fn default() -> Self {
        Self {
            enabled: true,
            listen_addresses: vec!["/ip4/0.0.0.0/tcp/9000".to_string()],
            bootstrap_nodes: vec![],
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
}

impl Default for SettlementConfig {
    fn default() -> Self {
        Self {
            network: "mock".to_string(),
            account_id: None,
            key_path: None,
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
pub fn default_base_dir() -> PathBuf {
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
    format_hbar(units).replace("HBAR", "HBAR")
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
}
