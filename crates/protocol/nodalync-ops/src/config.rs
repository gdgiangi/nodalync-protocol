//! Configuration types for the operations layer.
//!
//! This module defines configuration structures for channel management
//! and operations behavior.

use nodalync_types::Amount;

/// Configuration for payment channel behavior.
#[derive(Debug, Clone)]
pub struct ChannelConfig {
    /// Minimum deposit required to open a channel.
    pub min_deposit: Amount,
    /// Default deposit when auto-opening a channel.
    pub default_deposit: Amount,
}

impl Default for ChannelConfig {
    fn default() -> Self {
        Self {
            // 100 HBAR in tinybars
            min_deposit: 100_0000_0000,
            // 1000 HBAR in tinybars
            default_deposit: 1000_0000_0000,
        }
    }
}

impl ChannelConfig {
    /// Create a new channel configuration.
    pub fn new(min_deposit: Amount, default_deposit: Amount) -> Self {
        Self {
            min_deposit,
            default_deposit,
        }
    }
}

/// Configuration for operations behavior.
#[derive(Debug, Clone)]
pub struct OpsConfig {
    /// Channel configuration.
    pub channel: ChannelConfig,
    /// Maximum number of preview mentions to include.
    pub max_preview_mentions: usize,
    /// Settlement threshold (amount that triggers batch settlement).
    pub settlement_threshold: Amount,
    /// Settlement interval in milliseconds.
    pub settlement_interval_ms: u64,
}

impl Default for OpsConfig {
    fn default() -> Self {
        Self {
            channel: ChannelConfig::default(),
            max_preview_mentions: 5,
            // From constants
            settlement_threshold: nodalync_types::SETTLEMENT_BATCH_THRESHOLD,
            settlement_interval_ms: nodalync_types::SETTLEMENT_BATCH_INTERVAL_MS,
        }
    }
}

impl OpsConfig {
    /// Create a new operations configuration with custom channel config.
    pub fn with_channel(mut self, channel: ChannelConfig) -> Self {
        self.channel = channel;
        self
    }

    /// Set the settlement threshold.
    pub fn with_settlement_threshold(mut self, threshold: Amount) -> Self {
        self.settlement_threshold = threshold;
        self
    }

    /// Set the settlement interval.
    pub fn with_settlement_interval(mut self, interval_ms: u64) -> Self {
        self.settlement_interval_ms = interval_ms;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channel_config_default() {
        let config = ChannelConfig::default();
        assert!(config.min_deposit > 0);
        assert!(config.default_deposit >= config.min_deposit);
    }

    #[test]
    fn test_ops_config_default() {
        let config = OpsConfig::default();
        assert_eq!(config.max_preview_mentions, 5);
        assert!(config.settlement_threshold > 0);
    }

    #[test]
    fn test_ops_config_builder() {
        let config = OpsConfig::default()
            .with_channel(ChannelConfig::new(50, 500))
            .with_settlement_threshold(10000)
            .with_settlement_interval(3600000);

        assert_eq!(config.channel.min_deposit, 50);
        assert_eq!(config.settlement_threshold, 10000);
        assert_eq!(config.settlement_interval_ms, 3600000);
    }
}
