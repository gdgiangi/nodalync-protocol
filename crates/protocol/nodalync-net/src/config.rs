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

    /// Ed25519 secret key bytes (32 bytes) for stable identity.
    ///
    /// When provided, the network node derives its libp2p keypair from this
    /// seed, giving a stable PeerId across restarts. When `None`, a random
    /// keypair is generated (useful for tests).
    pub identity_secret: Option<[u8; 32]>,

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

    // ─── Resource Management ────────────────────────────────────────────
    /// Maximum total established connections (inbound + outbound).
    ///
    /// Default: 100. Prevents resource exhaustion on desktop machines.
    pub max_established_connections: u32,

    /// Maximum established connections per peer.
    ///
    /// Default: 2. Prevents a single peer from consuming too many connections.
    pub max_established_per_peer: u32,

    /// Maximum pending incoming connections (not yet established).
    ///
    /// Default: 64. Limits SYN-flood style attacks.
    pub max_pending_incoming: u32,

    /// Maximum pending outgoing connections.
    ///
    /// Default: 64. Prevents excessive outbound connection attempts.
    pub max_pending_outgoing: u32,

    /// Maximum inbound request-response requests per peer per window.
    ///
    /// Default: 30. Prevents query floods from a single peer.
    pub request_rate_limit: u32,

    /// Window duration for request rate limiting.
    ///
    /// Default: 60 seconds.
    pub request_rate_window: Duration,

    /// Maximum concurrent inbound requests being processed.
    ///
    /// Default: 128. Prevents memory exhaustion from large payloads.
    pub max_concurrent_inbound_requests: usize,

    /// Maximum message size in bytes for request-response.
    ///
    /// Default: 10 MB (10_485_760 bytes). Prevents oversized payloads.
    pub max_message_size: usize,

    /// Whether to enable GossipSub peer scoring.
    ///
    /// When enabled, peers are scored based on message delivery,
    /// mesh participation, and protocol compliance. Low-scoring
    /// peers are pruned from the mesh and eventually graylist-blocked.
    ///
    /// Default: true. Disable only for testing.
    pub enable_peer_scoring: bool,
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
            identity_secret: None,
            nat_traversal: NatTraversal::Full,
            relay_nodes: Vec::new(),
            max_relay_reservations: 3,
            // Resource management defaults
            max_established_connections: 100,
            max_established_per_peer: 2,
            max_pending_incoming: 64,
            max_pending_outgoing: 64,
            request_rate_limit: 30,
            request_rate_window: Duration::from_secs(60),
            max_concurrent_inbound_requests: 128,
            max_message_size: 10 * 1024 * 1024, // 10 MB
            enable_peer_scoring: true,
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

    /// Set maximum total established connections.
    pub fn with_max_established_connections(mut self, max: u32) -> Self {
        self.max_established_connections = max;
        self
    }

    /// Set maximum established connections per peer.
    pub fn with_max_established_per_peer(mut self, max: u32) -> Self {
        self.max_established_per_peer = max;
        self
    }

    /// Set maximum pending incoming connections.
    pub fn with_max_pending_incoming(mut self, max: u32) -> Self {
        self.max_pending_incoming = max;
        self
    }

    /// Set maximum pending outgoing connections.
    pub fn with_max_pending_outgoing(mut self, max: u32) -> Self {
        self.max_pending_outgoing = max;
        self
    }

    /// Set request rate limit (max requests per peer per window).
    pub fn with_request_rate_limit(mut self, limit: u32, window: Duration) -> Self {
        self.request_rate_limit = limit;
        self.request_rate_window = window;
        self
    }

    /// Set maximum concurrent inbound requests.
    pub fn with_max_concurrent_inbound_requests(mut self, max: usize) -> Self {
        self.max_concurrent_inbound_requests = max;
        self
    }

    /// Set maximum message size in bytes.
    pub fn with_max_message_size(mut self, max: usize) -> Self {
        self.max_message_size = max;
        self
    }

    /// Enable or disable GossipSub peer scoring.
    pub fn with_peer_scoring(mut self, enable: bool) -> Self {
        self.enable_peer_scoring = enable;
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
        // Resource management defaults
        assert_eq!(config.max_established_connections, 100);
        assert_eq!(config.max_established_per_peer, 2);
        assert_eq!(config.max_pending_incoming, 64);
        assert_eq!(config.max_pending_outgoing, 64);
        assert_eq!(config.request_rate_limit, 30);
        assert_eq!(config.request_rate_window, Duration::from_secs(60));
        assert_eq!(config.max_concurrent_inbound_requests, 128);
        assert_eq!(config.max_message_size, 10 * 1024 * 1024);
        assert!(config.enable_peer_scoring);
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

        // Resource management defaults
        assert_eq!(config.max_established_connections, 100);
        assert_eq!(config.max_established_per_peer, 2);
        assert_eq!(config.max_pending_incoming, 64);
        assert_eq!(config.max_pending_outgoing, 64);
        assert_eq!(config.request_rate_limit, 30);
        assert_eq!(config.request_rate_window, Duration::from_secs(60));
        assert_eq!(config.max_concurrent_inbound_requests, 128);
        assert_eq!(config.max_message_size, 10 * 1024 * 1024);
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

    #[test]
    fn test_resource_management_builders() {
        let config = NetworkConfig::new()
            .with_max_established_connections(50)
            .with_max_established_per_peer(3)
            .with_max_pending_incoming(32)
            .with_max_pending_outgoing(32)
            .with_request_rate_limit(10, Duration::from_secs(30))
            .with_max_concurrent_inbound_requests(64)
            .with_max_message_size(5 * 1024 * 1024);

        assert_eq!(config.max_established_connections, 50);
        assert_eq!(config.max_established_per_peer, 3);
        assert_eq!(config.max_pending_incoming, 32);
        assert_eq!(config.max_pending_outgoing, 32);
        assert_eq!(config.request_rate_limit, 10);
        assert_eq!(config.request_rate_window, Duration::from_secs(30));
        assert_eq!(config.max_concurrent_inbound_requests, 64);
        assert_eq!(config.max_message_size, 5 * 1024 * 1024);
    }
}
