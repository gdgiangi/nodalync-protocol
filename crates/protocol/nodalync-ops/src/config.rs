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
    /// Maximum deposit to accept/match when a peer opens a channel.
    /// Caps how much we'll commit when accepting a channel request.
    pub max_accept_deposit: Amount,
    /// Whether to auto-deposit when handling channel open requests.
    /// When true, the handler will deposit to the contract if balance is low.
    /// Default: false (opt-in for security).
    pub auto_deposit_on_channel_open: bool,
    /// Fixed amount to deposit when auto-deposit triggers (in tinybars).
    /// This is never derived from the peer's request.
    pub auto_deposit_amount: Amount,
    /// Balance threshold that triggers auto-deposit (in tinybars).
    pub auto_deposit_min_balance: Amount,
    /// Cooldown between auto-deposits in seconds.
    /// Prevents rapid deposits from malicious channel open spam.
    pub auto_deposit_cooldown_secs: u64,
}

impl Default for ChannelConfig {
    fn default() -> Self {
        Self {
            // 100 HBAR in tinybars
            min_deposit: 100_0000_0000,
            // 1000 HBAR in tinybars
            default_deposit: 1000_0000_0000,
            // 500 HBAR max accept deposit
            max_accept_deposit: 500_0000_0000,
            // Disabled by default for security
            auto_deposit_on_channel_open: false,
            // 200 HBAR auto-deposit amount
            auto_deposit_amount: 200_0000_0000,
            // 100 HBAR minimum balance threshold
            auto_deposit_min_balance: 100_0000_0000,
            // 5 minute cooldown
            auto_deposit_cooldown_secs: 300,
        }
    }
}

impl ChannelConfig {
    /// Create a new channel configuration with basic settings.
    pub fn new(min_deposit: Amount, default_deposit: Amount) -> Self {
        Self {
            min_deposit,
            default_deposit,
            ..Default::default()
        }
    }

    /// Set the maximum deposit to accept when a peer opens a channel.
    pub fn with_max_accept_deposit(mut self, amount: Amount) -> Self {
        self.max_accept_deposit = amount;
        self
    }

    /// Enable or disable auto-deposit on channel open.
    pub fn with_auto_deposit(mut self, enabled: bool) -> Self {
        self.auto_deposit_on_channel_open = enabled;
        self
    }

    /// Set the fixed auto-deposit amount.
    pub fn with_auto_deposit_amount(mut self, amount: Amount) -> Self {
        self.auto_deposit_amount = amount;
        self
    }

    /// Set the minimum balance threshold for auto-deposit.
    pub fn with_auto_deposit_min_balance(mut self, amount: Amount) -> Self {
        self.auto_deposit_min_balance = amount;
        self
    }

    /// Set the cooldown between auto-deposits in seconds.
    pub fn with_auto_deposit_cooldown(mut self, secs: u64) -> Self {
        self.auto_deposit_cooldown_secs = secs;
        self
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
    /// Settlement timeout in milliseconds (for query handler).
    pub settlement_timeout_ms: u64,
}

impl Default for OpsConfig {
    fn default() -> Self {
        Self {
            channel: ChannelConfig::default(),
            max_preview_mentions: 5,
            // From constants
            settlement_threshold: nodalync_types::SETTLEMENT_BATCH_THRESHOLD,
            settlement_interval_ms: nodalync_types::SETTLEMENT_BATCH_INTERVAL_MS,
            settlement_timeout_ms: 30_000,
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

    /// Set the settlement timeout in milliseconds.
    pub fn with_settlement_timeout(mut self, timeout_ms: u64) -> Self {
        self.settlement_timeout_ms = timeout_ms;
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
        // New security defaults
        assert_eq!(config.max_accept_deposit, 500_0000_0000);
        assert!(!config.auto_deposit_on_channel_open);
        assert_eq!(config.auto_deposit_amount, 200_0000_0000);
        assert_eq!(config.auto_deposit_min_balance, 100_0000_0000);
        assert_eq!(config.auto_deposit_cooldown_secs, 300);
    }

    #[test]
    fn test_channel_config_builder() {
        let config = ChannelConfig::default()
            .with_max_accept_deposit(1000_0000_0000)
            .with_auto_deposit(true)
            .with_auto_deposit_amount(500_0000_0000)
            .with_auto_deposit_min_balance(200_0000_0000)
            .with_auto_deposit_cooldown(600);

        assert_eq!(config.max_accept_deposit, 1000_0000_0000);
        assert!(config.auto_deposit_on_channel_open);
        assert_eq!(config.auto_deposit_amount, 500_0000_0000);
        assert_eq!(config.auto_deposit_min_balance, 200_0000_0000);
        assert_eq!(config.auto_deposit_cooldown_secs, 600);
    }

    #[test]
    fn test_channel_config_new_preserves_defaults() {
        // Verify that ChannelConfig::new() preserves the new security defaults
        let config = ChannelConfig::new(50, 500);
        assert_eq!(config.min_deposit, 50);
        assert_eq!(config.default_deposit, 500);
        // Security fields should have default values
        assert_eq!(config.max_accept_deposit, 500_0000_0000);
        assert!(!config.auto_deposit_on_channel_open);
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
