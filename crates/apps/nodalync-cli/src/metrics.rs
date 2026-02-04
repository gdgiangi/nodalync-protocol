//! Prometheus metrics for the Nodalync node.
//!
//! This module provides instrumentation for monitoring the node's health,
//! network activity, settlement operations, and content management.

use prometheus::{
    Encoder, Histogram, HistogramOpts, IntCounter, IntCounterVec, IntGauge, IntGaugeVec, Opts,
    Registry, TextEncoder,
};
use std::sync::Arc;

/// Metrics registry and definitions for the Nodalync node.
pub struct Metrics {
    /// The Prometheus registry containing all metrics.
    pub registry: Registry,

    // =========================================================================
    // Network Metrics
    // =========================================================================
    /// Current number of connected peers.
    pub connected_peers: IntGauge,

    /// Total peer connection events by type (connect/disconnect).
    pub peer_events_total: IntCounterVec,

    /// Total DHT operations by operation and result.
    pub dht_operations_total: IntCounterVec,

    /// Total GossipSub messages received.
    pub gossipsub_messages_total: IntCounter,

    // =========================================================================
    // Settlement Metrics
    // =========================================================================
    /// Current contract balance in tinybars.
    pub contract_balance_tinybars: IntGauge,

    /// Total settlement batches by status.
    pub settlement_batches_total: IntCounterVec,

    /// Total settlement errors by error type.
    pub settlement_errors_total: IntCounterVec,

    /// Settlement latency in seconds.
    pub settlement_latency_seconds: Histogram,

    // =========================================================================
    // Content Metrics
    // =========================================================================
    /// Total content published by type.
    pub content_published_total: IntCounterVec,

    /// Total queries processed.
    pub queries_total: IntCounter,

    /// Query latency in seconds.
    pub query_latency_seconds: Histogram,

    // =========================================================================
    // Node Metrics
    // =========================================================================
    /// Node uptime in seconds.
    pub uptime_seconds: IntGauge,

    /// Node information (version, peer_id).
    pub node_info: IntGaugeVec,
}

impl Metrics {
    /// Create a new Metrics instance with all metrics registered.
    pub fn new() -> Self {
        let registry = Registry::new();

        // Network metrics
        let connected_peers = IntGauge::with_opts(Opts::new(
            "nodalync_connected_peers",
            "Current number of connected peers",
        ))
        .expect("metric creation should not fail");

        let peer_events_total = IntCounterVec::new(
            Opts::new("nodalync_peer_events_total", "Total peer connection events"),
            &["event"],
        )
        .expect("metric creation should not fail");

        let dht_operations_total = IntCounterVec::new(
            Opts::new("nodalync_dht_operations_total", "Total DHT operations"),
            &["op", "result"],
        )
        .expect("metric creation should not fail");

        let gossipsub_messages_total = IntCounter::with_opts(Opts::new(
            "nodalync_gossipsub_messages_total",
            "Total GossipSub messages received",
        ))
        .expect("metric creation should not fail");

        // Settlement metrics
        let contract_balance_tinybars = IntGauge::with_opts(Opts::new(
            "nodalync_contract_balance_tinybars",
            "Settlement contract balance in tinybars",
        ))
        .expect("metric creation should not fail");

        let settlement_batches_total = IntCounterVec::new(
            Opts::new(
                "nodalync_settlement_batches_total",
                "Total settlement batches",
            ),
            &["status"],
        )
        .expect("metric creation should not fail");

        let settlement_errors_total = IntCounterVec::new(
            Opts::new(
                "nodalync_settlement_errors_total",
                "Total settlement errors",
            ),
            &["error_type"],
        )
        .expect("metric creation should not fail");

        let settlement_latency_seconds = Histogram::with_opts(
            HistogramOpts::new(
                "nodalync_settlement_latency_seconds",
                "Settlement operation latency in seconds",
            )
            .buckets(vec![0.1, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0, 60.0]),
        )
        .expect("metric creation should not fail");

        // Content metrics
        let content_published_total = IntCounterVec::new(
            Opts::new(
                "nodalync_content_published_total",
                "Total content published",
            ),
            &["type"],
        )
        .expect("metric creation should not fail");

        let queries_total = IntCounter::with_opts(Opts::new(
            "nodalync_queries_total",
            "Total queries processed",
        ))
        .expect("metric creation should not fail");

        let query_latency_seconds = Histogram::with_opts(
            HistogramOpts::new("nodalync_query_latency_seconds", "Query latency in seconds")
                .buckets(vec![0.01, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0]),
        )
        .expect("metric creation should not fail");

        // Node metrics
        let uptime_seconds = IntGauge::with_opts(Opts::new(
            "nodalync_uptime_seconds",
            "Node uptime in seconds",
        ))
        .expect("metric creation should not fail");

        let node_info = IntGaugeVec::new(
            Opts::new("nodalync_node_info", "Node information"),
            &["version", "peer_id"],
        )
        .expect("metric creation should not fail");

        // Register all metrics
        registry
            .register(Box::new(connected_peers.clone()))
            .expect("registration should not fail");
        registry
            .register(Box::new(peer_events_total.clone()))
            .expect("registration should not fail");
        registry
            .register(Box::new(dht_operations_total.clone()))
            .expect("registration should not fail");
        registry
            .register(Box::new(gossipsub_messages_total.clone()))
            .expect("registration should not fail");
        registry
            .register(Box::new(contract_balance_tinybars.clone()))
            .expect("registration should not fail");
        registry
            .register(Box::new(settlement_batches_total.clone()))
            .expect("registration should not fail");
        registry
            .register(Box::new(settlement_errors_total.clone()))
            .expect("registration should not fail");
        registry
            .register(Box::new(settlement_latency_seconds.clone()))
            .expect("registration should not fail");
        registry
            .register(Box::new(content_published_total.clone()))
            .expect("registration should not fail");
        registry
            .register(Box::new(queries_total.clone()))
            .expect("registration should not fail");
        registry
            .register(Box::new(query_latency_seconds.clone()))
            .expect("registration should not fail");
        registry
            .register(Box::new(uptime_seconds.clone()))
            .expect("registration should not fail");
        registry
            .register(Box::new(node_info.clone()))
            .expect("registration should not fail");

        Self {
            registry,
            connected_peers,
            peer_events_total,
            dht_operations_total,
            gossipsub_messages_total,
            contract_balance_tinybars,
            settlement_batches_total,
            settlement_errors_total,
            settlement_latency_seconds,
            content_published_total,
            queries_total,
            query_latency_seconds,
            uptime_seconds,
            node_info,
        }
    }

    /// Encode all metrics in Prometheus text format.
    pub fn encode(&self) -> String {
        let encoder = TextEncoder::new();
        let metric_families = self.registry.gather();
        let mut buffer = Vec::new();
        encoder
            .encode(&metric_families, &mut buffer)
            .expect("encoding should not fail");
        String::from_utf8(buffer).expect("metrics are valid utf8")
    }

    /// Record a settlement error by its type label.
    pub fn record_settlement_error(&self, error: &nodalync_settle::SettleError) {
        let label = Self::error_to_label(error);
        self.settlement_errors_total
            .with_label_values(&[label])
            .inc();
    }

    /// Convert a SettleError to a metric label.
    fn error_to_label(error: &nodalync_settle::SettleError) -> &'static str {
        use nodalync_settle::SettleError;
        match error {
            SettleError::InsufficientBalance { .. } => "insufficient_balance",
            SettleError::AccountNotFound(_) => "account_not_found",
            SettleError::TransactionFailed(_) => "transaction_failed",
            SettleError::ChannelNotFound(_) => "channel_not_found",
            SettleError::EmptyBatch => "empty_batch",
            SettleError::NoHederaAccount => "no_hedera_account",
            SettleError::HederaSdk(_) => "hedera_sdk",
            SettleError::Config(_) => "config",
            SettleError::Network(_) => "network",
            SettleError::Timeout(_) => "timeout",
            SettleError::InvalidAccountId(_) => "invalid_account_id",
            SettleError::InvalidTransactionId(_) => "invalid_transaction_id",
            SettleError::ChannelAlreadyExists(_) => "channel_already_exists",
            SettleError::ChannelNotOpen(_) => "channel_not_open",
            SettleError::DisputePeriodNotElapsed => "dispute_period_not_elapsed",
            SettleError::InvalidNonce { .. } => "invalid_nonce",
            SettleError::Io(_) => "io",
            SettleError::Internal(_) => "internal",
        }
    }
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Shared metrics handle for use across async tasks.
pub type SharedMetrics = Arc<Metrics>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_creation() {
        let metrics = Metrics::new();
        assert!(metrics.encode().contains("nodalync_connected_peers"));
    }

    #[test]
    fn test_metrics_encode() {
        let metrics = Metrics::new();
        metrics.connected_peers.set(5);
        metrics
            .peer_events_total
            .with_label_values(&["connect"])
            .inc();

        let output = metrics.encode();
        assert!(output.contains("nodalync_connected_peers 5"));
        assert!(output.contains("nodalync_peer_events_total"));
    }

    #[test]
    fn test_dht_operations_counter() {
        let metrics = Metrics::new();
        metrics
            .dht_operations_total
            .with_label_values(&["put", "success"])
            .inc();
        metrics
            .dht_operations_total
            .with_label_values(&["get", "not_found"])
            .inc();

        let output = metrics.encode();
        assert!(output.contains("nodalync_dht_operations_total"));
    }

    #[test]
    fn test_settlement_error_labels() {
        use nodalync_settle::SettleError;

        assert_eq!(
            Metrics::error_to_label(&SettleError::insufficient_balance(0, 100)),
            "insufficient_balance"
        );
        assert_eq!(
            Metrics::error_to_label(&SettleError::network("timeout")),
            "network"
        );
        assert_eq!(
            Metrics::error_to_label(&SettleError::channel_not_found("ch1")),
            "channel_not_found"
        );
    }

    #[test]
    fn test_histogram_recording() {
        let metrics = Metrics::new();
        metrics.settlement_latency_seconds.observe(1.5);
        metrics.query_latency_seconds.observe(0.05);

        let output = metrics.encode();
        assert!(output.contains("nodalync_settlement_latency_seconds"));
        assert!(output.contains("nodalync_query_latency_seconds"));
    }
}
