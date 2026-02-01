//! Configuration for Hedera settlement.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

use crate::error::{SettleError, SettleResult};
use crate::types::AccountId;

/// Hedera network selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum HederaNetwork {
    /// Mainnet
    Mainnet,
    /// Testnet (default for development)
    #[default]
    Testnet,
    /// Previewnet
    Previewnet,
}

impl HederaNetwork {
    /// Get the network name as a string.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Mainnet => "mainnet",
            Self::Testnet => "testnet",
            Self::Previewnet => "previewnet",
        }
    }
}

impl std::fmt::Display for HederaNetwork {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Configuration for Hedera settlement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HederaConfig {
    /// Which Hedera network to use
    pub network: HederaNetwork,

    /// Operator account ID (format: 0.0.xxxxx)
    pub account_id: String,

    /// Path to the private key file
    pub private_key_path: PathBuf,

    /// Settlement contract ID
    pub contract_id: String,

    /// Gas limits
    pub gas: GasConfig,

    /// Retry policy
    pub retry: RetryConfig,
}

impl HederaConfig {
    /// Create a new configuration for testnet.
    pub fn testnet(account_id: &str, private_key_path: PathBuf, contract_id: &str) -> Self {
        Self {
            network: HederaNetwork::Testnet,
            account_id: account_id.to_string(),
            private_key_path,
            contract_id: contract_id.to_string(),
            gas: GasConfig::default(),
            retry: RetryConfig::default(),
        }
    }

    /// Create a new configuration for mainnet.
    pub fn mainnet(account_id: &str, private_key_path: PathBuf, contract_id: &str) -> Self {
        Self {
            network: HederaNetwork::Mainnet,
            account_id: account_id.to_string(),
            private_key_path,
            contract_id: contract_id.to_string(),
            gas: GasConfig::default(),
            retry: RetryConfig::default(),
        }
    }

    /// Parse the account ID.
    pub fn parse_account_id(&self) -> SettleResult<AccountId> {
        AccountId::from_string(&self.account_id)
    }

    /// Parse the contract ID.
    pub fn parse_contract_id(&self) -> SettleResult<AccountId> {
        AccountId::from_string(&self.contract_id)
    }

    /// Validate the configuration.
    pub fn validate(&self) -> SettleResult<()> {
        // Validate account ID format
        self.parse_account_id()?;

        // Validate contract ID format
        self.parse_contract_id()?;

        // Check private key file exists
        if !self.private_key_path.exists() {
            return Err(SettleError::config(format!(
                "private key file not found: {}",
                self.private_key_path.display()
            )));
        }

        Ok(())
    }
}

impl Default for HederaConfig {
    fn default() -> Self {
        Self {
            network: HederaNetwork::Testnet,
            account_id: "0.0.0".to_string(),
            private_key_path: PathBuf::from("~/.nodalync/hedera.key"),
            contract_id: "0.0.0".to_string(),
            gas: GasConfig::default(),
            retry: RetryConfig::default(),
        }
    }
}

/// Gas limit configuration.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct GasConfig {
    /// Max gas for deposit operations
    pub max_gas_deposit: u64,
    /// Max gas for attest operations
    pub max_gas_attest: u64,
    /// Max gas for settle batch operations
    pub max_gas_settle: u64,
    /// Max gas for channel open operations
    pub max_gas_channel_open: u64,
    /// Max gas for channel close operations
    pub max_gas_channel_close: u64,
    /// Max gas for dispute operations
    pub max_gas_dispute: u64,
    /// Max gas for withdraw operations
    pub max_gas_withdraw: u64,
}

impl Default for GasConfig {
    fn default() -> Self {
        Self {
            max_gas_deposit: 100_000,
            max_gas_attest: 100_000,
            max_gas_settle: 500_000,
            max_gas_channel_open: 200_000,
            max_gas_channel_close: 200_000,
            max_gas_dispute: 300_000,
            max_gas_withdraw: 100_000,
        }
    }
}

/// Retry policy configuration.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct RetryConfig {
    /// Maximum number of retry attempts
    pub max_attempts: u32,
    /// Base delay between retries
    #[serde(with = "humantime_serde")]
    pub base_delay: Duration,
    /// Maximum delay between retries
    #[serde(with = "humantime_serde")]
    pub max_delay: Duration,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay: Duration::from_millis(500),
            max_delay: Duration::from_secs(10),
        }
    }
}

/// Serde helper for Duration (using humantime format).
mod humantime_serde {
    use serde::{Deserialize, Deserializer, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(duration.as_millis() as u64)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let millis = u64::deserialize(deserializer)?;
        Ok(Duration::from_millis(millis))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hedera_network() {
        assert_eq!(HederaNetwork::Mainnet.as_str(), "mainnet");
        assert_eq!(HederaNetwork::Testnet.as_str(), "testnet");
        assert_eq!(HederaNetwork::Previewnet.as_str(), "previewnet");
    }

    #[test]
    fn test_config_default() {
        let config = HederaConfig::default();
        assert_eq!(config.network, HederaNetwork::Testnet);
    }

    #[test]
    fn test_gas_config_default() {
        let gas = GasConfig::default();
        assert_eq!(gas.max_gas_attest, 100_000);
        assert_eq!(gas.max_gas_settle, 500_000);
    }

    #[test]
    fn test_retry_config_default() {
        let retry = RetryConfig::default();
        assert_eq!(retry.max_attempts, 3);
        assert_eq!(retry.base_delay, Duration::from_millis(500));
    }

    #[test]
    fn test_config_parse_account_id() {
        let config = HederaConfig {
            account_id: "0.0.12345".to_string(),
            ..Default::default()
        };
        let account = config.parse_account_id().unwrap();
        assert_eq!(account.num, 12345);
    }
}
