//! Alerting system for webhook-based notifications.
//!
//! This module provides the `AlertManager` which tracks node health state
//! and sends alerts via configured webhooks when state changes occur.
//!
//! # Alert Types
//!
//! - `node_started` - Node startup notification
//! - `node_shutdown` - Graceful shutdown notification
//! - `no_peers` - Zero peers for configured threshold
//! - `low_peer_count` - Below minimum peer threshold
//! - `recovered` - Back to healthy state
//! - `heartbeat` - Periodic health ping
//!
//! # Webhook Formats
//!
//! - `generic` - Raw JSON payload
//! - `slack` - Rich attachments with colors
//! - `discord` - Embeds with fields
//! - `pagerduty` - Events API v2 (trigger/resolve)

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tracing::{debug, error, info};

use crate::config::{AlertingConfig, WebhookConfig, WebhookType};

// =============================================================================
// Alert Types
// =============================================================================

/// Types of alerts that can be sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertType {
    /// Node has started.
    NodeStarted,
    /// Node is shutting down gracefully.
    NodeShutdown,
    /// Zero connected peers for threshold duration.
    NoPeers,
    /// Peer count below minimum threshold.
    LowPeerCount,
    /// Node has recovered from unhealthy state.
    Recovered,
    /// Periodic heartbeat.
    Heartbeat,
}

impl AlertType {
    /// Get the string identifier for this alert type.
    pub fn as_str(&self) -> &'static str {
        match self {
            AlertType::NodeStarted => "node_started",
            AlertType::NodeShutdown => "node_shutdown",
            AlertType::NoPeers => "no_peers",
            AlertType::LowPeerCount => "low_peer_count",
            AlertType::Recovered => "recovered",
            AlertType::Heartbeat => "heartbeat",
        }
    }
}

/// Severity levels for alerts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AlertSeverity {
    /// Informational alert.
    Info,
    /// Warning alert.
    Warning,
    /// Critical alert requiring immediate attention.
    Critical,
}

impl AlertSeverity {
    /// Get the string identifier for this severity.
    pub fn as_str(&self) -> &'static str {
        match self {
            AlertSeverity::Info => "info",
            AlertSeverity::Warning => "warning",
            AlertSeverity::Critical => "critical",
        }
    }

    /// Get the color code for this severity (for Slack/Discord).
    pub fn color(&self) -> &'static str {
        match self {
            AlertSeverity::Info => "#36a64f",     // Green
            AlertSeverity::Warning => "#ffcc00",  // Yellow
            AlertSeverity::Critical => "#ff0000", // Red
        }
    }
}

impl AlertType {
    /// Get the default severity for this alert type.
    pub fn default_severity(&self) -> AlertSeverity {
        match self {
            AlertType::NodeStarted => AlertSeverity::Info,
            AlertType::NodeShutdown => AlertSeverity::Warning,
            AlertType::NoPeers => AlertSeverity::Critical,
            AlertType::LowPeerCount => AlertSeverity::Warning,
            AlertType::Recovered => AlertSeverity::Info,
            AlertType::Heartbeat => AlertSeverity::Info,
        }
    }
}

// =============================================================================
// Alert Payload
// =============================================================================

/// Metrics included in alert payloads.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertMetrics {
    /// Number of currently connected peers.
    pub connected_peers: u32,
    /// Node uptime in seconds.
    pub uptime_secs: u64,
}

/// Full alert payload sent to webhooks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertPayload {
    /// Type of alert.
    pub alert_type: AlertType,
    /// Severity level.
    pub severity: AlertSeverity,
    /// Human-readable message.
    pub message: String,
    /// Node's libp2p peer ID.
    pub peer_id: String,
    /// Human-readable node name (if configured).
    pub node_name: Option<String>,
    /// Region identifier (if configured).
    pub region: Option<String>,
    /// Unix timestamp when alert was generated.
    pub timestamp: u64,
    /// Current metrics.
    pub metrics: AlertMetrics,
    /// CLI version (e.g., "0.9.1").
    pub cli_version: String,
    /// Protocol version (e.g., "0.5.0").
    pub protocol_version: String,
}

impl AlertPayload {
    /// Create a new alert payload.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        alert_type: AlertType,
        message: String,
        peer_id: String,
        node_name: Option<String>,
        region: Option<String>,
        metrics: AlertMetrics,
        cli_version: String,
        protocol_version: String,
    ) -> Self {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        Self {
            alert_type,
            severity: alert_type.default_severity(),
            message,
            peer_id,
            node_name,
            region,
            timestamp,
            metrics,
            cli_version,
            protocol_version,
        }
    }

    /// Get the display name for this node.
    fn display_name(&self) -> String {
        self.node_name
            .clone()
            .unwrap_or_else(|| self.peer_id[..12.min(self.peer_id.len())].to_string())
    }
}

// =============================================================================
// Health State
// =============================================================================

/// Tracked health state of the node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthState {
    /// Node is healthy with sufficient peers.
    Healthy,
    /// Node has zero peers.
    NoPeers,
    /// Node has peers but below minimum threshold.
    LowPeers,
}

// =============================================================================
// Alert Manager
// =============================================================================

/// Internal state for the alert manager.
struct AlertManagerState {
    /// Current health state.
    health_state: HealthState,
    /// When the node entered the current unhealthy state.
    unhealthy_since: Option<Instant>,
    /// When the node last recovered.
    last_recovery: Option<Instant>,
    /// Last time each alert type was sent.
    last_alert_times: HashMap<AlertType, Instant>,
    /// HTTP client for sending webhooks.
    client: reqwest::Client,
}

/// Manager for sending alerts based on health state changes.
pub struct AlertManager {
    /// Configuration.
    config: AlertingConfig,
    /// Peer ID for this node.
    peer_id: String,
    /// CLI version string.
    cli_version: String,
    /// Protocol version string.
    protocol_version: String,
    /// Node start time.
    start_time: Instant,
    /// Internal state (protected by mutex for async access).
    state: Arc<Mutex<AlertManagerState>>,
}

impl AlertManager {
    /// Create a new alert manager.
    pub fn new(config: AlertingConfig, peer_id: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_default();

        Self {
            config,
            peer_id,
            cli_version: env!("CARGO_PKG_VERSION").to_string(),
            protocol_version: nodalync_types::VERSION.to_string(),
            start_time: Instant::now(),
            state: Arc::new(Mutex::new(AlertManagerState {
                health_state: HealthState::Healthy,
                unhealthy_since: None,
                last_recovery: None,
                last_alert_times: HashMap::new(),
                client,
            })),
        }
    }

    /// Check if alerting is enabled.
    pub fn is_enabled(&self) -> bool {
        self.config.enabled && !self.config.webhooks.is_empty()
    }

    /// Get the configured heartbeat interval, if any.
    pub fn heartbeat_interval(&self) -> Option<Duration> {
        self.config
            .heartbeat
            .as_ref()
            .map(|h| Duration::from_secs(h.interval_secs))
    }

    /// Get the current uptime in seconds.
    fn uptime_secs(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }

    /// Create an alert payload with current metrics.
    fn create_payload(
        &self,
        alert_type: AlertType,
        message: String,
        peer_count: u32,
    ) -> AlertPayload {
        AlertPayload::new(
            alert_type,
            message,
            self.peer_id.clone(),
            self.config.node_name.clone(),
            self.config.region.clone(),
            AlertMetrics {
                connected_peers: peer_count,
                uptime_secs: self.uptime_secs(),
            },
            self.cli_version.clone(),
            self.protocol_version.clone(),
        )
    }

    /// Send a startup alert.
    pub async fn send_startup_alert(&self, peer_count: u32) {
        if !self.is_enabled() || !self.config.conditions.alert_on_startup {
            return;
        }

        let message = format!(
            "Node {} started",
            self.config.node_name.as_deref().unwrap_or("nodalync")
        );
        let payload = self.create_payload(AlertType::NodeStarted, message, peer_count);
        self.send_alert(payload).await;
    }

    /// Send a shutdown alert.
    pub async fn send_shutdown_alert(&self, peer_count: u32) {
        if !self.is_enabled() || !self.config.conditions.alert_on_shutdown {
            return;
        }

        let message = format!(
            "Node {} shutting down",
            self.config.node_name.as_deref().unwrap_or("nodalync")
        );
        let payload = self.create_payload(AlertType::NodeShutdown, message, peer_count);
        self.send_alert(payload).await;
    }

    /// Send a heartbeat alert.
    pub async fn send_heartbeat(&self, peer_count: u32) {
        if !self.is_enabled() {
            return;
        }

        let message = format!(
            "Heartbeat from {}",
            self.config.node_name.as_deref().unwrap_or("nodalync")
        );
        let payload = self.create_payload(AlertType::Heartbeat, message, peer_count);
        self.send_alert(payload).await;
    }

    /// Check health and potentially send alerts based on peer count changes.
    ///
    /// This should be called whenever the peer count changes or periodically.
    pub async fn check_health(&self, peer_count: u32) {
        if !self.is_enabled() {
            return;
        }

        // Determine what alert to send (if any) while holding the lock
        let pending_alert = {
            let mut state = self.state.lock().await;
            let now = Instant::now();

            // Determine new health state
            let new_state = if peer_count == 0 {
                HealthState::NoPeers
            } else if let Some(min) = self.config.conditions.min_peer_count {
                if peer_count < min {
                    HealthState::LowPeers
                } else {
                    HealthState::Healthy
                }
            } else {
                HealthState::Healthy
            };

            // Handle state transitions and determine if we need to send an alert
            match (state.health_state, new_state) {
                // Transition to unhealthy
                (HealthState::Healthy, HealthState::NoPeers | HealthState::LowPeers) => {
                    state.health_state = new_state;
                    state.unhealthy_since = Some(now);
                    debug!("Health state changed to {:?}", new_state);
                    None
                }

                // Already unhealthy with no peers, check if threshold exceeded
                (HealthState::NoPeers, HealthState::NoPeers) => {
                    if let Some(since) = state.unhealthy_since {
                        let duration = now.duration_since(since).as_secs();
                        if duration >= self.config.conditions.no_peers_threshold_secs
                            && self.should_send_alert(&state, AlertType::NoPeers, now)
                        {
                            state.last_alert_times.insert(AlertType::NoPeers, now);
                            let message = format!(
                                "Node {} has had zero peers for {} seconds",
                                self.config.node_name.as_deref().unwrap_or("nodalync"),
                                duration
                            );
                            Some((AlertType::NoPeers, message))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }

                // Low peers threshold check
                (HealthState::LowPeers, HealthState::LowPeers) => {
                    if self.should_send_alert(&state, AlertType::LowPeerCount, now) {
                        state.last_alert_times.insert(AlertType::LowPeerCount, now);
                        let message = format!(
                            "Node {} has only {} peers (minimum: {})",
                            self.config.node_name.as_deref().unwrap_or("nodalync"),
                            peer_count,
                            self.config.conditions.min_peer_count.unwrap_or(0)
                        );
                        Some((AlertType::LowPeerCount, message))
                    } else {
                        None
                    }
                }

                // Transition to healthy (recovery)
                (HealthState::NoPeers | HealthState::LowPeers, HealthState::Healthy) => {
                    info!("Node recovered to healthy state with {} peers", peer_count);
                    state.health_state = HealthState::Healthy;
                    state.unhealthy_since = None;
                    state.last_recovery = Some(now);
                    let message = format!(
                        "Node {} recovered with {} peers",
                        self.config.node_name.as_deref().unwrap_or("nodalync"),
                        peer_count
                    );
                    Some((AlertType::Recovered, message))
                }

                // State change between unhealthy states
                (HealthState::NoPeers, HealthState::LowPeers)
                | (HealthState::LowPeers, HealthState::NoPeers) => {
                    state.health_state = new_state;
                    state.unhealthy_since = Some(now);
                    None
                }

                // No change
                (HealthState::Healthy, HealthState::Healthy) => None,
            }
        }; // Lock is released here

        // Send alert outside the lock if needed
        if let Some((alert_type, message)) = pending_alert {
            let payload = self.create_payload(alert_type, message, peer_count);
            self.send_alert(payload).await;
        }
    }

    /// Check if we should send an alert based on rate limits and cooldowns.
    fn should_send_alert(
        &self,
        state: &AlertManagerState,
        alert_type: AlertType,
        now: Instant,
    ) -> bool {
        // Check rate limit
        if let Some(last) = state.last_alert_times.get(&alert_type) {
            let elapsed = now.duration_since(*last).as_secs();
            if elapsed < self.config.rate_limit.min_interval_secs {
                debug!(
                    "Rate limited: {} seconds since last {:?} alert",
                    elapsed, alert_type
                );
                return false;
            }
        }

        // Check recovery cooldown
        if let Some(recovery) = state.last_recovery {
            let since_recovery = now.duration_since(recovery).as_secs();
            if since_recovery < self.config.rate_limit.recovery_cooldown_secs {
                debug!(
                    "In recovery cooldown: {} seconds since recovery",
                    since_recovery
                );
                return false;
            }
        }

        true
    }

    /// Send an alert to all configured webhooks.
    async fn send_alert(&self, payload: AlertPayload) {
        let state = self.state.lock().await;
        let client = state.client.clone();
        drop(state);

        for webhook in &self.config.webhooks {
            // Check if this webhook should receive this alert type
            if !webhook.alert_types.is_empty()
                && !webhook
                    .alert_types
                    .contains(&payload.alert_type.as_str().to_string())
            {
                continue;
            }

            let result = self.send_to_webhook(&client, webhook, &payload).await;

            match result {
                Ok(()) => {
                    info!(
                        "Sent {:?} alert to {} webhook",
                        payload.alert_type,
                        webhook.webhook_type.as_str()
                    );
                }
                Err(e) => {
                    error!(
                        "Failed to send {:?} alert to {}: {}",
                        payload.alert_type, webhook.url, e
                    );
                }
            }
        }
    }

    /// Send an alert to a single webhook.
    async fn send_to_webhook(
        &self,
        client: &reqwest::Client,
        webhook: &WebhookConfig,
        payload: &AlertPayload,
    ) -> Result<(), String> {
        let body = match webhook.webhook_type {
            WebhookType::Generic => format_generic(payload),
            WebhookType::Slack => format_slack(payload),
            WebhookType::Discord => format_discord(payload),
            WebhookType::Pagerduty => format_pagerduty(payload),
        };

        let mut request = client
            .post(&webhook.url)
            .header("Content-Type", "application/json")
            .timeout(Duration::from_secs(webhook.timeout_secs))
            .body(body);

        if let Some(ref auth) = webhook.auth_header {
            request = request.header("Authorization", auth);
        }

        let response = request.send().await.map_err(|e| e.to_string())?;

        if !response.status().is_success() {
            return Err(format!(
                "Webhook returned status {}: {}",
                response.status(),
                response.text().await.unwrap_or_default()
            ));
        }

        Ok(())
    }
}

impl WebhookType {
    /// Get the string identifier for this webhook type.
    pub fn as_str(&self) -> &'static str {
        match self {
            WebhookType::Generic => "generic",
            WebhookType::Slack => "slack",
            WebhookType::Discord => "discord",
            WebhookType::Pagerduty => "pagerduty",
        }
    }
}

// =============================================================================
// Webhook Formatters
// =============================================================================

/// Format payload for generic JSON webhook.
fn format_generic(payload: &AlertPayload) -> String {
    serde_json::to_string(payload).unwrap_or_else(|_| "{}".to_string())
}

/// Format payload for Slack Incoming Webhook.
fn format_slack(payload: &AlertPayload) -> String {
    let color = payload.severity.color();
    let title = format!(
        "[{}] {}",
        payload.severity.as_str().to_uppercase(),
        payload.alert_type.as_str()
    );

    let mut fields = vec![
        serde_json::json!({
            "title": "Node",
            "value": payload.display_name(),
            "short": true
        }),
        serde_json::json!({
            "title": "Peers",
            "value": payload.metrics.connected_peers.to_string(),
            "short": true
        }),
    ];

    if let Some(ref region) = payload.region {
        fields.push(serde_json::json!({
            "title": "Region",
            "value": region,
            "short": true
        }));
    }

    fields.push(serde_json::json!({
        "title": "Uptime",
        "value": format_duration(payload.metrics.uptime_secs),
        "short": true
    }));

    // Add version information
    fields.push(serde_json::json!({
        "title": "CLI Version",
        "value": &payload.cli_version,
        "short": true
    }));

    fields.push(serde_json::json!({
        "title": "Protocol",
        "value": &payload.protocol_version,
        "short": true
    }));

    let slack_payload = serde_json::json!({
        "attachments": [{
            "color": color,
            "title": title,
            "text": payload.message,
            "fields": fields,
            "ts": payload.timestamp
        }]
    });

    serde_json::to_string(&slack_payload).unwrap_or_else(|_| "{}".to_string())
}

/// Format payload for Discord Webhook.
fn format_discord(payload: &AlertPayload) -> String {
    let color = match payload.severity {
        AlertSeverity::Info => 0x36a64f,     // Green
        AlertSeverity::Warning => 0xffcc00,  // Yellow
        AlertSeverity::Critical => 0xff0000, // Red
    };

    let mut fields = vec![
        serde_json::json!({
            "name": "Node",
            "value": payload.display_name(),
            "inline": true
        }),
        serde_json::json!({
            "name": "Peers",
            "value": payload.metrics.connected_peers.to_string(),
            "inline": true
        }),
    ];

    if let Some(ref region) = payload.region {
        fields.push(serde_json::json!({
            "name": "Region",
            "value": region,
            "inline": true
        }));
    }

    fields.push(serde_json::json!({
        "name": "Uptime",
        "value": format_duration(payload.metrics.uptime_secs),
        "inline": true
    }));

    // Add version information
    fields.push(serde_json::json!({
        "name": "CLI Version",
        "value": &payload.cli_version,
        "inline": true
    }));

    fields.push(serde_json::json!({
        "name": "Protocol",
        "value": &payload.protocol_version,
        "inline": true
    }));

    let discord_payload = serde_json::json!({
        "embeds": [{
            "title": format!("{} - {}", payload.alert_type.as_str(), payload.severity.as_str()),
            "description": payload.message,
            "color": color,
            "fields": fields,
            "timestamp": format_iso8601(payload.timestamp)
        }]
    });

    serde_json::to_string(&discord_payload).unwrap_or_else(|_| "{}".to_string())
}

/// Format payload for PagerDuty Events API v2.
fn format_pagerduty(payload: &AlertPayload) -> String {
    let (event_action, severity) = match payload.alert_type {
        AlertType::Recovered => ("resolve", "info"),
        AlertType::NodeStarted | AlertType::Heartbeat => ("trigger", "info"),
        AlertType::NodeShutdown => ("trigger", "warning"),
        AlertType::NoPeers => ("trigger", "critical"),
        AlertType::LowPeerCount => ("trigger", "warning"),
    };

    let dedup_key = format!(
        "nodalync-{}-{}",
        payload.peer_id,
        match payload.alert_type {
            AlertType::NoPeers | AlertType::Recovered => "connectivity",
            AlertType::LowPeerCount => "low-peers",
            _ => payload.alert_type.as_str(),
        }
    );

    let pagerduty_payload = serde_json::json!({
        "routing_key": "", // Will be set via auth header or URL path
        "event_action": event_action,
        "dedup_key": dedup_key,
        "payload": {
            "summary": payload.message,
            "severity": severity,
            "source": payload.display_name(),
            "custom_details": {
                "peer_id": payload.peer_id,
                "node_name": payload.node_name,
                "region": payload.region,
                "connected_peers": payload.metrics.connected_peers,
                "uptime_secs": payload.metrics.uptime_secs,
                "cli_version": payload.cli_version,
                "protocol_version": payload.protocol_version
            }
        }
    });

    serde_json::to_string(&pagerduty_payload).unwrap_or_else(|_| "{}".to_string())
}

/// Format seconds as human-readable duration.
fn format_duration(secs: u64) -> String {
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else if secs < 86400 {
        format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
    } else {
        format!("{}d {}h", secs / 86400, (secs % 86400) / 3600)
    }
}

/// Format Unix timestamp as ISO 8601 string.
fn format_iso8601(timestamp: u64) -> String {
    use std::time::{Duration, UNIX_EPOCH};
    let datetime = UNIX_EPOCH + Duration::from_secs(timestamp);
    // Simple ISO 8601 format without external dependencies
    let secs_since_epoch = datetime
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    // Calculate date components (simplified, doesn't handle leap seconds)
    let days = secs_since_epoch / 86400;
    let time_secs = secs_since_epoch % 86400;
    let hours = time_secs / 3600;
    let mins = (time_secs % 3600) / 60;
    let secs = time_secs % 60;

    // Days since Unix epoch to year/month/day (simplified)
    let mut year = 1970;
    let mut remaining_days = days;

    loop {
        let days_in_year = if is_leap_year(year) { 366 } else { 365 };
        if remaining_days < days_in_year {
            break;
        }
        remaining_days -= days_in_year;
        year += 1;
    }

    let days_in_months: [u64; 12] = if is_leap_year(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month = 1;
    for days_in_month in days_in_months.iter() {
        if remaining_days < *days_in_month {
            break;
        }
        remaining_days -= days_in_month;
        month += 1;
    }

    let day = remaining_days + 1;

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hours, mins, secs
    )
}

#[allow(clippy::manual_is_multiple_of)]
fn is_leap_year(year: u64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AlertConditions, RateLimitConfig};

    fn test_config() -> AlertingConfig {
        AlertingConfig {
            enabled: true,
            node_name: Some("test-node".to_string()),
            region: Some("us-east".to_string()),
            webhooks: vec![WebhookConfig {
                url: "https://example.com/webhook".to_string(),
                webhook_type: WebhookType::Generic,
                auth_header: None,
                alert_types: vec![],
                timeout_secs: 10,
            }],
            conditions: AlertConditions {
                no_peers_threshold_secs: 60,
                min_peer_count: Some(2),
                alert_on_startup: true,
                alert_on_shutdown: true,
            },
            rate_limit: RateLimitConfig {
                min_interval_secs: 300,
                recovery_cooldown_secs: 60,
            },
            heartbeat: None,
        }
    }

    #[test]
    fn test_alert_type_as_str() {
        assert_eq!(AlertType::NodeStarted.as_str(), "node_started");
        assert_eq!(AlertType::NoPeers.as_str(), "no_peers");
        assert_eq!(AlertType::Recovered.as_str(), "recovered");
    }

    #[test]
    fn test_alert_severity() {
        assert_eq!(
            AlertType::NodeStarted.default_severity(),
            AlertSeverity::Info
        );
        assert_eq!(
            AlertType::NoPeers.default_severity(),
            AlertSeverity::Critical
        );
        assert_eq!(
            AlertType::LowPeerCount.default_severity(),
            AlertSeverity::Warning
        );
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(30), "30s");
        assert_eq!(format_duration(90), "1m 30s");
        assert_eq!(format_duration(3700), "1h 1m");
        assert_eq!(format_duration(90000), "1d 1h");
    }

    #[test]
    fn test_alert_payload_creation() {
        let payload = AlertPayload::new(
            AlertType::NodeStarted,
            "Test message".to_string(),
            "12D3KooWTest".to_string(),
            Some("test-node".to_string()),
            Some("us-east".to_string()),
            AlertMetrics {
                connected_peers: 5,
                uptime_secs: 100,
            },
            "0.9.1".to_string(),
            "0.5.0".to_string(),
        );

        assert_eq!(payload.alert_type, AlertType::NodeStarted);
        assert_eq!(payload.severity, AlertSeverity::Info);
        assert_eq!(payload.message, "Test message");
        assert_eq!(payload.metrics.connected_peers, 5);
        assert_eq!(payload.cli_version, "0.9.1");
        assert_eq!(payload.protocol_version, "0.5.0");
    }

    #[test]
    fn test_format_generic() {
        let payload = AlertPayload::new(
            AlertType::NoPeers,
            "No peers".to_string(),
            "peer123".to_string(),
            None,
            None,
            AlertMetrics {
                connected_peers: 0,
                uptime_secs: 60,
            },
            "0.9.1".to_string(),
            "0.5.0".to_string(),
        );

        let json = format_generic(&payload);
        assert!(json.contains("no_peers"));
        assert!(json.contains("critical"));
        assert!(json.contains("cli_version"));
        assert!(json.contains("protocol_version"));
    }

    #[test]
    fn test_format_slack() {
        let payload = AlertPayload::new(
            AlertType::NoPeers,
            "No peers connected".to_string(),
            "peer123".to_string(),
            Some("test-node".to_string()),
            Some("us-east".to_string()),
            AlertMetrics {
                connected_peers: 0,
                uptime_secs: 60,
            },
            "0.9.1".to_string(),
            "0.5.0".to_string(),
        );

        let json = format_slack(&payload);
        assert!(json.contains("attachments"));
        assert!(json.contains("#ff0000")); // Critical color
        assert!(json.contains("test-node"));
        assert!(json.contains("CLI Version"));
        assert!(json.contains("0.9.1"));
    }

    #[test]
    fn test_format_discord() {
        let payload = AlertPayload::new(
            AlertType::Recovered,
            "Node recovered".to_string(),
            "peer123".to_string(),
            None,
            None,
            AlertMetrics {
                connected_peers: 3,
                uptime_secs: 120,
            },
            "0.9.1".to_string(),
            "0.5.0".to_string(),
        );

        let json = format_discord(&payload);
        assert!(json.contains("embeds"));
        assert!(json.contains("3581519")); // Green color (0x36a64f) as decimal
        assert!(json.contains("CLI Version"));
        assert!(json.contains("Protocol"));
        assert!(json.contains("0.9.1"));
        assert!(json.contains("0.5.0"));
    }

    #[test]
    fn test_format_pagerduty() {
        let payload = AlertPayload::new(
            AlertType::NoPeers,
            "No peers".to_string(),
            "peer123".to_string(),
            None,
            None,
            AlertMetrics {
                connected_peers: 0,
                uptime_secs: 60,
            },
            "0.9.1".to_string(),
            "0.5.0".to_string(),
        );

        let json = format_pagerduty(&payload);
        assert!(json.contains("trigger"));
        assert!(json.contains("critical"));
        assert!(json.contains("dedup_key"));
        assert!(json.contains("cli_version"));
    }

    #[test]
    fn test_pagerduty_resolve_on_recovery() {
        let payload = AlertPayload::new(
            AlertType::Recovered,
            "Recovered".to_string(),
            "peer123".to_string(),
            None,
            None,
            AlertMetrics {
                connected_peers: 3,
                uptime_secs: 120,
            },
            "0.9.1".to_string(),
            "0.5.0".to_string(),
        );

        let json = format_pagerduty(&payload);
        assert!(json.contains("resolve"));
    }

    #[test]
    fn test_webhook_filtering() {
        let config = AlertingConfig {
            enabled: true,
            node_name: None,
            region: None,
            webhooks: vec![WebhookConfig {
                url: "https://example.com/webhook".to_string(),
                webhook_type: WebhookType::Generic,
                auth_header: None,
                alert_types: vec!["no_peers".to_string(), "recovered".to_string()],
                timeout_secs: 10,
            }],
            conditions: AlertConditions::default(),
            rate_limit: RateLimitConfig::default(),
            heartbeat: None,
        };

        // This webhook should only receive no_peers and recovered alerts
        let webhook = &config.webhooks[0];
        assert!(webhook.alert_types.contains(&"no_peers".to_string()));
        assert!(webhook.alert_types.contains(&"recovered".to_string()));
        assert!(!webhook.alert_types.contains(&"heartbeat".to_string()));
    }

    #[tokio::test]
    async fn test_alert_manager_is_enabled() {
        let config = test_config();
        let manager = AlertManager::new(config, "peer123".to_string());
        assert!(manager.is_enabled());

        let disabled_config = AlertingConfig {
            enabled: false,
            ..test_config()
        };
        let disabled_manager = AlertManager::new(disabled_config, "peer123".to_string());
        assert!(!disabled_manager.is_enabled());

        let no_webhooks_config = AlertingConfig {
            webhooks: vec![],
            ..test_config()
        };
        let no_webhooks_manager = AlertManager::new(no_webhooks_config, "peer123".to_string());
        assert!(!no_webhooks_manager.is_enabled());
    }

    #[tokio::test]
    async fn test_health_state_transitions() {
        // Use a dummy webhook URL that won't be contacted (tests don't actually send)
        let config = AlertingConfig {
            enabled: true,
            node_name: Some("test".to_string()),
            region: None,
            webhooks: vec![WebhookConfig {
                url: "http://localhost:0/test".to_string(), // Won't be contacted
                webhook_type: WebhookType::Generic,
                auth_header: None,
                alert_types: vec![],
                timeout_secs: 1,
            }],
            conditions: AlertConditions {
                no_peers_threshold_secs: 1,
                min_peer_count: Some(2),
                alert_on_startup: false,
                alert_on_shutdown: false,
            },
            rate_limit: RateLimitConfig {
                min_interval_secs: 0,
                recovery_cooldown_secs: 0,
            },
            heartbeat: None,
        };

        let manager = AlertManager::new(config, "peer123".to_string());

        // Start healthy
        {
            let state = manager.state.lock().await;
            assert_eq!(state.health_state, HealthState::Healthy);
        }

        // Transition to no peers
        manager.check_health(0).await;
        {
            let state = manager.state.lock().await;
            assert_eq!(state.health_state, HealthState::NoPeers);
            assert!(state.unhealthy_since.is_some());
        }

        // Recover
        manager.check_health(5).await;
        {
            let state = manager.state.lock().await;
            assert_eq!(state.health_state, HealthState::Healthy);
            assert!(state.unhealthy_since.is_none());
            assert!(state.last_recovery.is_some());
        }

        // Transition to low peers
        manager.check_health(1).await;
        {
            let state = manager.state.lock().await;
            assert_eq!(state.health_state, HealthState::LowPeers);
        }
    }

    #[test]
    fn test_format_iso8601() {
        // Test Unix epoch
        assert_eq!(format_iso8601(0), "1970-01-01T00:00:00Z");

        // Test a known timestamp (2024-01-15 12:30:45 UTC)
        let ts = 1705321845;
        let result = format_iso8601(ts);
        assert!(result.starts_with("2024-01-15"));
        assert!(result.ends_with("Z"));
    }
}
