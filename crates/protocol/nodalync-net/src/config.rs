//! Network configuration.
//!
//! This module defines configuration options for the network layer.

use libp2p::Multiaddr;
use nodalync_types::constants::{MAX_RETRY_ATTEMPTS, MESSAGE_TIMEOUT_MS};
use std::time::Duration;

/// NAT traversal strategy.
///
/// Controls how the node handles Network Address Translation (NAT),
/// which is critical for desktop users behind home routers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NatTraversal {
    /// Disable all NAT traversal. Only useful for nodes with public IPs.
    Disabled,
    /// Enable UPnP port mapping only (simplest, works with most routers).
    UpnpOnly,
    /// Enable relay + DCUtR hole-punching (works even when UPnP fails).
    RelayOnly,
    /// Enable all strategies: UPnP, AutoNAT detection, relay, DCUtR.
    /// This is the recommended default for desktop apps.
    Full,
}

/// Configuration for the network layer.
#[derive(Debug, Clone)]
pub struct NetworkConfig {
    /// Addresses to listen on.
    ///
    /// Default: `["/ip4/0.0.0.0/tcp/0"]` (random port).
    pub listen_addresses: Vec<Multiaddr>,

    /// Bootstrap nodes to connect to on startup.
    ///
    /// These should be well-known nodes that help with initial peer discovery.
    pub bootstrap_nodes: Vec<(libp2p::PeerId, Multiaddr)>,

    /// Timeout for request-response operations.
    ///
    /// Default: 30 seconds (from spec).
    pub request_timeout: Duration,

    /// Maximum retry attempts for failed operations.
    ///
    /// Default: 3 (from spec).
    pub max_retries: u32,

    /// Base delay for exponential backoff retry.
    ///
    /// Actual delay is `base_delay * 2^attempt`.
    /// Default: 100ms.
    pub retry_base_delay: Duration,

    /// Whether to enable mDNS for local peer discovery.
    ///
    /// Default: false (not in MVP).
    pub enable_mdns: bool,

    /// Kademlia bucket size (k).
    ///
    /// Default: 20 (from spec).
    pub dht_bucket_size: usize,

    /// Kademlia concurrency parameter (alpha).
    ///
    /// Default: 3 (from spec).
    pub dht_alpha: usize,

    /// Kademlia replication factor.
    ///
    /// Default: 20 (from spec).
    pub dht_replication: usize,

    /// DHT query timeout.
    ///
    /// Default: 60 seconds.
    pub dht_query_timeout: Duration,

    /// GossipSub topic for announcements.
    ///
    /// Default: "/nodalync/announce/1.0.0".
    pub gossipsub_topic: String,

    /// Idle connection timeout.
    ///
    /// Default: 60 seconds.
    pub idle_connection_timeout: Duration,

    /// NAT traversal strategy.
    ///
    /// Default: `Full` (UPnP + AutoNAT + Relay + DCUtR).
    pub nat_traversal: NatTraversal,

    /// Known relay nodes for NAT traversal.
    ///
    /// These are well-known public nodes that can relay traffic
    /// for nodes behind NATs that can't use UPnP.
    /// Format: `(PeerId, Multiaddr)`.
    pub relay_nodes: Vec<(libp2p::PeerId, Multiaddr)>,

    /// Maximum number of relay reservations to maintain.
    ///
    /// Default: 3. More reservations = better reachability but more overhead.
    pub max_relay_reservations: usize,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            listen_addresses: vec!["/ip4/0.0.0.0/tcp/0".parse().unwrap()],
            bootstrap_nodes: Vec::new(),
            request_timeout: Duration::from_millis(MESSAGE_TIMEOUT_MS),
            max_retries: MAX_RETRY_ATTEMPTS,
            retry_base_delay: Duration::from_millis(100),
            enable_mdns: false,
            dht_bucket_size: nodalync_types::constants::DHT_BUCKET_SIZE,
            dht_alpha: nodalync_types::constants::DHT_ALPHA,
            dht_replication: nodalync_types::constants::DHT_REPLICATION,
            dht_query_timeout: Duration::from_secs(60),
            gossipsub_topic: "/nodalync/announce/1.0.0".to_string(),
            idle_connection_timeout: Duration::from_secs(60),
            nat_traversal: NatTraversal::Full,
            relay_nodes: Vec::new(),
            max_relay_reservations: 3,
        }
    }
}

impl NetworkConfig {
    /// Create a new configuration with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set listen addresses.
    pub fn with_listen_addresses(mut self, addresses: Vec<Multiaddr>) -> Self {
        self.listen_addresses = addresses;
        self
    }

    /// Add a bootstrap node.
    pub fn with_bootstrap_node(mut self, peer_id: libp2p::PeerId, addr: Multiaddr) -> Self {
        self.bootstrap_nodes.push((peer_id, addr));
        self
    }

    /// Set request timeout.
    pub fn with_request_timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = timeout;
        self
    }

    /// Set max retries.
    pub fn with_max_retries(mut self, retries: u32) -> Self {
        self.max_retries = retries;
        self
    }

    /// Set retry base delay.
    pub fn with_retry_base_delay(mut self, delay: Duration) -> Self {
        self.retry_base_delay = delay;
        self
    }

    /// Enable or disable mDNS.
    pub fn with_mdns(mut self, enable: bool) -> Self {
        self.enable_mdns = enable;
        self
    }

    /// Set NAT traversal strategy.
    pub fn with_nat_traversal(mut self, strategy: NatTraversal) -> Self {
        self.nat_traversal = strategy;
        self
    }

    /// Add a relay node for NAT traversal.
    pub fn with_relay_node(mut self, peer_id: libp2p::PeerId, addr: Multiaddr) -> Self {
        self.relay_nodes.push((peer_id, addr));
        self
    }

    /// Set maximum relay reservations.
    pub fn with_max_relay_reservations(mut self, max: usize) -> Self {
        self.max_relay_reservations = max;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = NetworkConfig::default();
        assert_eq!(config.listen_addresses.len(), 1);
        assert!(config.bootstrap_nodes.is_empty());
        assert_eq!(config.request_timeout.as_millis(), 30_000);
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.dht_bucket_size, 20);
        assert_eq!(config.dht_alpha, 3);
        assert_eq!(config.dht_replication, 20);
        assert_eq!(config.nat_traversal, NatTraversal::Full);
        assert!(config.relay_nodes.is_empty());
        assert_eq!(config.max_relay_reservations, 3);
        assert_eq!(config.idle_connection_timeout, Duration::from_secs(60));
    }

    #[test]
    fn test_config_builder() {
        let config = NetworkConfig::new()
            .with_max_retries(5)
            .with_request_timeout(Duration::from_secs(60))
            .with_mdns(true);

        assert_eq!(config.max_retries, 5);
        assert_eq!(config.request_timeout.as_secs(), 60);
        assert!(config.enable_mdns);
    }

    #[test]
    fn test_add_bootstrap_node() {
        let peer_id = libp2p::PeerId::random();
        let addr: Multiaddr = "/ip4/192.168.1.1/tcp/9000".parse().unwrap();

        let config = NetworkConfig::new().with_bootstrap_node(peer_id, addr.clone());

        assert_eq!(config.bootstrap_nodes.len(), 1);
        assert_eq!(config.bootstrap_nodes[0].0, peer_id);
        assert_eq!(config.bootstrap_nodes[0].1, addr);
    }

    #[test]
    fn test_network_config_defaults() {
        let config = NetworkConfig::default();

        // Listen address: /ip4/0.0.0.0/tcp/0
        assert_eq!(config.listen_addresses.len(), 1);
        let expected_addr: Multiaddr = "/ip4/0.0.0.0/tcp/0".parse().unwrap();
        assert_eq!(config.listen_addresses[0], expected_addr);

        // Timeout: 30s (MESSAGE_TIMEOUT_MS = 30_000)
        assert_eq!(config.request_timeout, Duration::from_millis(30_000));

        // DHT parameters from spec
        assert_eq!(config.dht_bucket_size, 20);
        assert_eq!(config.dht_alpha, 3);
        assert_eq!(config.dht_replication, 20);

        // GossipSub topic
        assert_eq!(config.gossipsub_topic, "/nodalync/announce/1.0.0");

        // Retry settings
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.retry_base_delay, Duration::from_millis(100));

        // mDNS disabled by default
        assert!(!config.enable_mdns);

        // Bootstrap nodes empty by default
        assert!(config.bootstrap_nodes.is_empty());

        // DHT query timeout
        assert_eq!(config.dht_query_timeout, Duration::from_secs(60));

        // Idle connection timeout (60s for desktop app peer stability)
        assert_eq!(config.idle_connection_timeout, Duration::from_secs(60));

        // NAT traversal defaults
        assert_eq!(config.nat_traversal, NatTraversal::Full);
        assert!(config.relay_nodes.is_empty());
        assert_eq!(config.max_relay_reservations, 3);
    }

    #[test]
    fn test_network_config_with_custom_timeout() {
        let config = NetworkConfig::new().with_request_timeout(Duration::from_secs(120));

        assert_eq!(config.request_timeout, Duration::from_secs(120));

        // Other defaults should remain unchanged
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.dht_bucket_size, 20);
    }

    #[test]
    fn test_network_config_with_retry_delay() {
        let config = NetworkConfig::new().with_retry_base_delay(Duration::from_millis(500));

        assert_eq!(config.retry_base_delay, Duration::from_millis(500));

        // Other defaults should remain unchanged
        assert_eq!(config.request_timeout, Duration::from_millis(30_000));
        assert_eq!(config.max_retries, 3);
    }

    #[test]
    fn test_nat_traversal_config() {
        let config = NetworkConfig::new().with_nat_traversal(NatTraversal::Disabled);
        assert_eq!(config.nat_traversal, NatTraversal::Disabled);

        let config = NetworkConfig::new().with_nat_traversal(NatTraversal::UpnpOnly);
        assert_eq!(config.nat_traversal, NatTraversal::UpnpOnly);

        let config = NetworkConfig::new().with_nat_traversal(NatTraversal::RelayOnly);
        assert_eq!(config.nat_traversal, NatTraversal::RelayOnly);
    }

    #[test]
    fn test_relay_node_config() {
        let peer_id = libp2p::PeerId::random();
        let addr: Multiaddr = "/ip4/1.2.3.4/tcp/9000".parse().unwrap();

        let config = NetworkConfig::new()
            .with_relay_node(peer_id, addr.clone())
            .with_max_relay_reservations(5);

        assert_eq!(config.relay_nodes.len(), 1);
        assert_eq!(config.relay_nodes[0].0, peer_id);
        assert_eq!(config.relay_nodes[0].1, addr);
        assert_eq!(config.max_relay_reservations, 5);
    }
}
