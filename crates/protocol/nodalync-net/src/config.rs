//! Network configuration.
//!
//! This module defines configuration options for the network layer.

use libp2p::Multiaddr;
use nodalync_types::constants::{MAX_RETRY_ATTEMPTS, MESSAGE_TIMEOUT_MS};
use std::time::Duration;

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
    /// Default: 30 seconds.
    pub idle_connection_timeout: Duration,

    /// Ed25519 secret key bytes (32 bytes) for stable identity.
    ///
    /// When provided, the network node derives its libp2p keypair from this
    /// seed, giving a stable PeerId across restarts. When `None`, a random
    /// keypair is generated (useful for tests).
    pub identity_secret: Option<[u8; 32]>,
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
            idle_connection_timeout: Duration::from_secs(30),
            identity_secret: None,
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

    /// Set the Ed25519 identity secret for stable peer identity.
    ///
    /// When set, the node derives its libp2p keypair from this 32-byte seed,
    /// giving a deterministic PeerId that persists across restarts.
    pub fn with_identity_secret(mut self, secret: [u8; 32]) -> Self {
        self.identity_secret = Some(secret);
        self
    }

    /// Preview the libp2p PeerId that would result from the configured identity.
    ///
    /// Returns `None` if no identity secret is set.
    /// Useful for displaying the stable PeerId before starting the network.
    pub fn preview_peer_id(&self) -> Option<libp2p::PeerId> {
        self.identity_secret.as_ref().and_then(|secret| {
            libp2p::identity::Keypair::ed25519_from_bytes(*secret)
                .ok()
                .map(|kp| kp.public().to_peer_id())
        })
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

        // Idle connection timeout
        assert_eq!(config.idle_connection_timeout, Duration::from_secs(30));
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
    fn test_identity_secret_default_none() {
        let config = NetworkConfig::default();
        assert!(config.identity_secret.is_none());
        assert!(config.preview_peer_id().is_none());
    }

    #[test]
    fn test_identity_secret_deterministic_peer_id() {
        let secret = [42u8; 32];

        let config1 = NetworkConfig::new().with_identity_secret(secret);
        let config2 = NetworkConfig::new().with_identity_secret(secret);

        let pid1 = config1.preview_peer_id().expect("should have PeerId");
        let pid2 = config2.preview_peer_id().expect("should have PeerId");

        // Same secret → same PeerId
        assert_eq!(pid1, pid2);
    }

    #[test]
    fn test_different_secrets_different_peer_ids() {
        let secret_a = [1u8; 32];
        let secret_b = [2u8; 32];

        let pid_a = NetworkConfig::new()
            .with_identity_secret(secret_a)
            .preview_peer_id()
            .unwrap();
        let pid_b = NetworkConfig::new()
            .with_identity_secret(secret_b)
            .preview_peer_id()
            .unwrap();

        assert_ne!(pid_a, pid_b);
    }
}
