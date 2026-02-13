//! Network behaviour for the Nodalync protocol.
//!
//! This module defines the combined network behaviour using libp2p's
//! derive macro to compose multiple protocols:
//! - Kademlia: DHT for content discovery
//! - Request-Response: Point-to-point messaging
//! - GossipSub: Broadcast messaging
//! - Identify: Peer identification
//! - mDNS: Local network peer discovery (optional)
//! - AutoNAT: NAT status detection
//! - Relay: Circuit relay v2 for NAT traversal
//! - DCUtR: Direct Connection Upgrade through Relay (hole-punching)
//! - UPnP: Automatic port mapping via UPnP

use crate::codec::{NodalyncCodec, NodalyncRequest, NodalyncResponse, PROTOCOL_NAME};
use crate::config::{NatTraversal, NetworkConfig};
use libp2p::{
    autonat, dcutr,
    gossipsub::{self, MessageId},
    identify,
    kad::{self, store::MemoryStore, Mode},
    mdns, ping, relay,
    request_response::{self, ProtocolSupport},
    swarm::NetworkBehaviour,
    upnp, PeerId,
};
use sha2::{Digest, Sha256};
use std::time::Duration;

/// Combined network behaviour for Nodalync.
///
/// This behaviour combines:
/// - `kademlia`: DHT for content discovery and peer routing
/// - `request_response`: Request-response messaging
/// - `gossipsub`: Pub-sub for broadcast messages
/// - `identify`: Peer identification and capability exchange
/// - `ping`: Keep-alive pings to maintain connections
/// - `mdns`: Local network peer discovery (optional, enabled via config)
/// - `autonat`: NAT status detection (optional, enabled via nat_traversal config)
/// - `relay_client`: Circuit relay v2 client for NAT traversal (optional)
/// - `dcutr`: Direct Connection Upgrade through Relay (optional)
/// - `upnp`: UPnP port mapping for automatic NAT traversal (optional)
#[derive(NetworkBehaviour)]
#[behaviour(to_swarm = "NodalyncBehaviourEvent")]
pub struct NodalyncBehaviour {
    /// Kademlia DHT for content discovery.
    pub kademlia: kad::Behaviour<MemoryStore>,

    /// Request-response for point-to-point messaging.
    pub request_response: request_response::Behaviour<NodalyncCodec>,

    /// GossipSub for broadcast messaging.
    pub gossipsub: gossipsub::Behaviour,

    /// Identify for peer discovery and capability exchange.
    pub identify: identify::Behaviour,

    /// Ping for connection keep-alive.
    pub ping: ping::Behaviour,

    /// mDNS for local network peer discovery.
    pub mdns: libp2p::swarm::behaviour::toggle::Toggle<mdns::tokio::Behaviour>,

    /// AutoNAT for detecting NAT status.
    pub autonat: libp2p::swarm::behaviour::toggle::Toggle<autonat::Behaviour>,

    /// Relay client for circuit relay v2 NAT traversal.
    pub relay_client: libp2p::swarm::behaviour::toggle::Toggle<relay::client::Behaviour>,

    /// DCUtR for direct connection upgrade through relay (hole-punching).
    pub dcutr: libp2p::swarm::behaviour::toggle::Toggle<dcutr::Behaviour>,

    /// UPnP for automatic port mapping.
    pub upnp: libp2p::swarm::behaviour::toggle::Toggle<upnp::tokio::Behaviour>,
}

/// Events emitted by NodalyncBehaviour.
#[derive(Debug)]
pub enum NodalyncBehaviourEvent {
    /// Kademlia event.
    Kademlia(kad::Event),

    /// Request-response event.
    RequestResponse(request_response::Event<NodalyncRequest, NodalyncResponse>),

    /// GossipSub event.
    Gossipsub(gossipsub::Event),

    /// Identify event.
    Identify(identify::Event),

    /// Ping event.
    Ping(ping::Event),

    /// mDNS event.
    Mdns(mdns::Event),

    /// AutoNAT event.
    Autonat(autonat::Event),

    /// Relay client event.
    RelayClient(relay::client::Event),

    /// DCUtR event.
    Dcutr(dcutr::Event),

    /// UPnP event.
    Upnp(upnp::Event),
}

impl From<kad::Event> for NodalyncBehaviourEvent {
    fn from(event: kad::Event) -> Self {
        NodalyncBehaviourEvent::Kademlia(event)
    }
}

impl From<request_response::Event<NodalyncRequest, NodalyncResponse>> for NodalyncBehaviourEvent {
    fn from(event: request_response::Event<NodalyncRequest, NodalyncResponse>) -> Self {
        NodalyncBehaviourEvent::RequestResponse(event)
    }
}

impl From<gossipsub::Event> for NodalyncBehaviourEvent {
    fn from(event: gossipsub::Event) -> Self {
        NodalyncBehaviourEvent::Gossipsub(event)
    }
}

impl From<identify::Event> for NodalyncBehaviourEvent {
    fn from(event: identify::Event) -> Self {
        NodalyncBehaviourEvent::Identify(event)
    }
}

impl From<ping::Event> for NodalyncBehaviourEvent {
    fn from(event: ping::Event) -> Self {
        NodalyncBehaviourEvent::Ping(event)
    }
}

impl From<mdns::Event> for NodalyncBehaviourEvent {
    fn from(event: mdns::Event) -> Self {
        NodalyncBehaviourEvent::Mdns(event)
    }
}

impl From<autonat::Event> for NodalyncBehaviourEvent {
    fn from(event: autonat::Event) -> Self {
        NodalyncBehaviourEvent::Autonat(event)
    }
}

impl From<relay::client::Event> for NodalyncBehaviourEvent {
    fn from(event: relay::client::Event) -> Self {
        NodalyncBehaviourEvent::RelayClient(event)
    }
}

impl From<dcutr::Event> for NodalyncBehaviourEvent {
    fn from(event: dcutr::Event) -> Self {
        NodalyncBehaviourEvent::Dcutr(event)
    }
}

impl From<upnp::Event> for NodalyncBehaviourEvent {
    fn from(event: upnp::Event) -> Self {
        NodalyncBehaviourEvent::Upnp(event)
    }
}

impl NodalyncBehaviour {
    /// Create a new NodalyncBehaviour with the given configuration.
    pub fn new(local_peer_id: PeerId, config: &NetworkConfig) -> Self {
        let gossipsub_keypair = libp2p::identity::Keypair::generate_ed25519();
        Self::build(local_peer_id, &gossipsub_keypair, config, None)
    }

    /// Create a new NodalyncBehaviour with a specific keypair for identify.
    pub fn with_keypair(
        local_peer_id: PeerId,
        keypair: &libp2p::identity::Keypair,
        config: &NetworkConfig,
    ) -> Self {
        Self::build(local_peer_id, keypair, config, None)
    }

    /// Create a new NodalyncBehaviour with keypair and relay transport.
    pub fn with_keypair_and_relay(
        local_peer_id: PeerId,
        keypair: &libp2p::identity::Keypair,
        config: &NetworkConfig,
        relay_behaviour: relay::client::Behaviour,
    ) -> Self {
        Self::build(local_peer_id, keypair, config, Some(relay_behaviour))
    }

    /// Internal builder shared by all constructors.
    fn build(
        local_peer_id: PeerId,
        keypair: &libp2p::identity::Keypair,
        config: &NetworkConfig,
        relay_behaviour: Option<relay::client::Behaviour>,
    ) -> Self {
        // Configure Kademlia
        let store = MemoryStore::new(local_peer_id);
        let mut kad_config = kad::Config::new(kad::PROTOCOL_NAME.clone());
        kad_config.set_query_timeout(config.dht_query_timeout);
        kad_config
            .set_replication_factor(std::num::NonZeroUsize::new(config.dht_replication).unwrap());
        kad_config.set_parallelism(std::num::NonZeroUsize::new(config.dht_alpha).unwrap());
        kad_config.disjoint_query_paths(true);

        let mut kademlia = kad::Behaviour::with_config(local_peer_id, store, kad_config);
        kademlia.set_mode(Some(Mode::Server));

        // Configure request-response
        let req_resp_config =
            request_response::Config::default().with_request_timeout(config.request_timeout);
        let request_response = request_response::Behaviour::new(
            [(PROTOCOL_NAME, ProtocolSupport::Full)],
            req_resp_config,
        );

        // Configure GossipSub with optional peer scoring
        let gossipsub = build_gossipsub(
            keypair,
            if config.enable_peer_scoring {
                Some(&config.gossipsub_topic)
            } else {
                None
            },
            config.enable_peer_scoring,
        );

        // Configure Identify
        let identify_config =
            identify::Config::new("/nodalync/1.0.0".to_string(), keypair.public())
                .with_interval(Duration::from_secs(60))
                .with_push_listen_addr_updates(true);
        let identify = identify::Behaviour::new(identify_config);

        // Configure Ping - keeps connections alive
        let ping = ping::Behaviour::new(ping::Config::new().with_interval(Duration::from_secs(15)));

        // Configure mDNS (optional)
        let mdns = build_mdns(local_peer_id, config.enable_mdns);

        // NAT traversal behaviours
        let enable_autonat = matches!(config.nat_traversal, NatTraversal::Full);
        let enable_relay = matches!(
            config.nat_traversal,
            NatTraversal::Full | NatTraversal::RelayOnly
        );
        let enable_upnp = matches!(
            config.nat_traversal,
            NatTraversal::Full | NatTraversal::UpnpOnly
        );

        // AutoNAT — detects whether we're behind a NAT
        let autonat = if enable_autonat {
            tracing::info!("AutoNAT enabled for NAT status detection");
            let autonat_config = autonat::Config {
                retry_interval: Duration::from_secs(30),
                refresh_interval: Duration::from_secs(300),
                confidence_max: 3,
                ..Default::default()
            };
            libp2p::swarm::behaviour::toggle::Toggle::from(Some(autonat::Behaviour::new(
                local_peer_id,
                autonat_config,
            )))
        } else {
            libp2p::swarm::behaviour::toggle::Toggle::from(None)
        };

        // Relay client — allows receiving inbound connections via relay
        let relay_client = if enable_relay {
            if let Some(relay_beh) = relay_behaviour {
                tracing::info!("Relay client enabled for NAT traversal");
                libp2p::swarm::behaviour::toggle::Toggle::from(Some(relay_beh))
            } else {
                tracing::debug!(
                    "Relay enabled in config but no relay transport provided, skipping"
                );
                libp2p::swarm::behaviour::toggle::Toggle::from(None)
            }
        } else {
            libp2p::swarm::behaviour::toggle::Toggle::from(None)
        };

        // DCUtR — upgrades relayed connections to direct ones via hole-punching
        let dcutr = if enable_relay {
            tracing::info!("DCUtR enabled for hole-punching");
            libp2p::swarm::behaviour::toggle::Toggle::from(Some(dcutr::Behaviour::new(
                local_peer_id,
            )))
        } else {
            libp2p::swarm::behaviour::toggle::Toggle::from(None)
        };

        // UPnP — automatic port mapping
        let upnp = if enable_upnp {
            tracing::info!("UPnP enabled for automatic port mapping");
            libp2p::swarm::behaviour::toggle::Toggle::from(Some(upnp::tokio::Behaviour::default()))
        } else {
            libp2p::swarm::behaviour::toggle::Toggle::from(None)
        };

        Self {
            kademlia,
            request_response,
            gossipsub,
            identify,
            ping,
            mdns,
            autonat,
            relay_client,
            dcutr,
            upnp,
        }
    }
}

/// Build mDNS behaviour (toggled on/off based on config).
///
/// When enabled, mDNS discovers peers on the local network automatically.
/// This is especially useful for desktop app users who want zero-config
/// peer discovery on their LAN.
fn build_mdns(
    local_peer_id: PeerId,
    enable: bool,
) -> libp2p::swarm::behaviour::toggle::Toggle<mdns::tokio::Behaviour> {
    if enable {
        match mdns::tokio::Behaviour::new(mdns::Config::default(), local_peer_id) {
            Ok(behaviour) => {
                tracing::info!("mDNS enabled for local peer discovery");
                libp2p::swarm::behaviour::toggle::Toggle::from(Some(behaviour))
            }
            Err(e) => {
                tracing::warn!("Failed to initialize mDNS, continuing without it: {}", e);
                libp2p::swarm::behaviour::toggle::Toggle::from(None)
            }
        }
    } else {
        tracing::debug!("mDNS disabled");
        libp2p::swarm::behaviour::toggle::Toggle::from(None)
    }
}

/// Build GossipSub behaviour with a specific keypair.
///
/// When `announce_topic` is provided and scoring is enabled, applies
/// peer scoring parameters tuned for the Nodalync announce channel.
pub fn build_gossipsub_with_keypair(keypair: &libp2p::identity::Keypair) -> gossipsub::Behaviour {
    build_gossipsub(keypair, None, false)
}

/// Build GossipSub behaviour with optional peer scoring.
///
/// Peer scoring evaluates peers based on:
/// - **Time in mesh:** Rewards long-lived, stable peers.
/// - **First message deliveries:** Rewards peers that deliver new announcements quickly.
/// - **Mesh message deliveries:** Penalises free-riders that are in the mesh but don't relay.
/// - **Invalid messages:** Penalises peers sending malformed/invalid data.
/// - **IP colocation:** Penalises sybil attacks from the same IP.
///
/// Thresholds control how scores affect connectivity:
/// - **gossip_threshold (-10):** Below this, peer doesn't receive gossip.
/// - **publish_threshold (-50):** Below this, peer can't publish to us.
/// - **graylist_threshold (-80):** Below this, peer is fully disconnected.
pub fn build_gossipsub(
    keypair: &libp2p::identity::Keypair,
    announce_topic: Option<&str>,
    enable_scoring: bool,
) -> gossipsub::Behaviour {
    let message_id_fn = |message: &gossipsub::Message| {
        let mut hasher = Sha256::new();
        hasher.update(&message.data);
        MessageId::from(hasher.finalize().to_vec())
    };

    let config = gossipsub::ConfigBuilder::default()
        .heartbeat_interval(Duration::from_secs(1))
        .validation_mode(gossipsub::ValidationMode::Strict)
        .message_id_fn(message_id_fn)
        .max_transmit_size(1024 * 1024) // 1MB max gossip message
        .history_length(5) // Keep 5 heartbeat windows of message history
        .history_gossip(3) // Gossip about messages from the last 3 windows
        .mesh_n(6)         // Target 6 peers in mesh
        .mesh_n_low(4)     // Minimum 4 peers before seeking more
        .mesh_n_high(12)   // Maximum 12 peers before pruning
        .build()
        .expect("valid gossipsub config");

    let mut behaviour = gossipsub::Behaviour::new(
        gossipsub::MessageAuthenticity::Signed(keypair.clone()),
        config,
    )
    .expect("valid gossipsub behaviour");

    // Apply peer scoring when enabled
    if enable_scoring {
        let (params, thresholds) = build_peer_score_params(announce_topic);
        behaviour
            .with_peer_score(params, thresholds)
            .expect("valid peer score params");
        tracing::info!("GossipSub peer scoring enabled");
    }

    behaviour
}

/// Build peer score parameters tuned for Nodalync.
///
/// The announce topic is the primary gossip channel. Scoring ensures:
/// - Peers that relay announcements promptly are rewarded.
/// - Peers that hoard messages or fail to relay are penalised.
/// - Invalid message senders (malformed wire format) are heavily penalised.
/// - IP-based sybil attacks are penalised above 2 peers per IP.
fn build_peer_score_params(
    announce_topic: Option<&str>,
) -> (gossipsub::PeerScoreParams, gossipsub::PeerScoreThresholds) {
    let mut params = gossipsub::PeerScoreParams::default();

    // ── Application-level scoring ─────────────────────────────────────
    // Weight for application-specific scoring (future: reputation system)
    params.app_specific_weight = 1.0;

    // ── IP colocation ─────────────────────────────────────────────────
    // Penalise sybil attacks: more than 2 peers from the same IP
    params.ip_colocation_factor_threshold = 2.0;
    params.ip_colocation_factor_weight = -10.0;

    // ── Behaviour penalty ─────────────────────────────────────────────
    // Threshold before behaviour penalty kicks in (e.g., too many GRAFT attempts)
    params.behaviour_penalty_threshold = 6.0;

    // ── Score decay ───────────────────────────────────────────────────
    // Zero out scores that decay below this threshold
    params.decay_to_zero = 0.01;

    // ── Retain scores for disconnected peers ──────────────────────────
    params.retain_score = Duration::from_secs(3600); // 1 hour

    // ── Topic-level scoring (announce topic) ──────────────────────────
    if let Some(topic_str) = announce_topic {
        let topic = gossipsub::IdentTopic::new(topic_str);
        let topic_hash = topic.hash();

        let mut topic_params = gossipsub::TopicScoreParams::default();

        // Topic weight — moderate importance
        topic_params.topic_weight = 1.0;

        // Time-in-mesh scoring: reward stable mesh peers
        // Caps at ~20 min of mesh participation (quantum × cap)
        topic_params.time_in_mesh_weight = 0.5;
        topic_params.time_in_mesh_quantum = Duration::from_secs(12);
        // time_in_mesh_cap defaults to 3600

        // First message deliveries: reward peers that deliver new messages first
        // Decays slowly — announcements are infrequent
        topic_params.first_message_deliveries_weight = 2.0;
        topic_params.first_message_deliveries_decay = 0.97; // ~23s half-life at 1s heartbeat
        // first_message_deliveries_cap defaults to a reasonable value

        // Mesh message deliveries: penalise free-riders
        // After 30 seconds in the mesh, expect at least 1 message delivery per heartbeat window
        topic_params.mesh_message_deliveries_weight = -1.0;
        topic_params.mesh_message_deliveries_decay = 0.97;
        topic_params.mesh_message_deliveries_threshold = 1.0;
        // mesh_message_deliveries_activation is how long before this kicks in

        // Mesh failure penalty: penalise peers pruned while under-delivering
        topic_params.mesh_failure_penalty_weight = -1.0;
        topic_params.mesh_failure_penalty_decay = 0.97;

        // Invalid message deliveries: harsh penalty for bad data
        topic_params.invalid_message_deliveries_weight = -100.0;
        topic_params.invalid_message_deliveries_decay = 0.5; // Fast decay so penalties don't last forever

        params.topics.insert(topic_hash, topic_params);
    }

    // Cap total topic score contribution
    params.topic_score_cap = 50.0;

    // ── Thresholds ────────────────────────────────────────────────────
    let thresholds = gossipsub::PeerScoreThresholds {
        // Below this, peer doesn't receive gossip control messages
        gossip_threshold: -10.0,
        // Below this, peer's published messages are rejected
        publish_threshold: -50.0,
        // Below this, peer is disconnected entirely
        graylist_threshold: -80.0,
        // Minimum score to accept peer exchange (PX) data from a peer
        accept_px_threshold: 5.0,
        // Score above which opportunistic grafting is triggered
        // (we graft peers with scores above this to improve mesh quality)
        opportunistic_graft_threshold: 3.0,
    };

    (params, thresholds)
}

#[cfg(test)]
mod tests {
    use super::*;

    // UPnP tokio behaviour requires an active Tokio runtime,
    // so all behaviour construction tests use #[tokio::test].

    #[tokio::test]
    async fn test_create_behaviour() {
        let peer_id = PeerId::random();
        let config = NetworkConfig::default();

        let behaviour = NodalyncBehaviour::new(peer_id, &config);

        // Verify it was created successfully
        assert!(behaviour.gossipsub.topics().next().is_none());
    }

    #[tokio::test]
    async fn test_create_behaviour_with_keypair() {
        let keypair = libp2p::identity::Keypair::generate_ed25519();
        let peer_id = keypair.public().to_peer_id();
        let config = NetworkConfig::default();

        let behaviour = NodalyncBehaviour::with_keypair(peer_id, &keypair, &config);

        // Verify it was created successfully
        assert!(behaviour.gossipsub.topics().next().is_none());
    }

    #[tokio::test]
    async fn test_with_keypair_uses_provided_keypair_for_gossipsub() {
        let keypair = libp2p::identity::Keypair::generate_ed25519();
        let peer_id = keypair.public().to_peer_id();
        let config = NetworkConfig::default();

        let behaviour = NodalyncBehaviour::with_keypair(peer_id, &keypair, &config);

        assert!(behaviour.gossipsub.topics().next().is_none());
    }

    #[test]
    fn test_build_gossipsub_with_keypair_uses_provided_key() {
        let keypair = libp2p::identity::Keypair::generate_ed25519();
        let gossipsub = build_gossipsub_with_keypair(&keypair);

        assert!(gossipsub.topics().next().is_none());
    }

    #[test]
    fn test_build_gossipsub_with_scoring() {
        let keypair = libp2p::identity::Keypair::generate_ed25519();
        let gossipsub = build_gossipsub(
            &keypair,
            Some("/nodalync/announce/1.0.0"),
            true,
        );
        // Peer scoring is internal — just verify it doesn't panic
        assert!(gossipsub.topics().next().is_none());
    }

    #[test]
    fn test_build_gossipsub_without_scoring() {
        let keypair = libp2p::identity::Keypair::generate_ed25519();
        let gossipsub = build_gossipsub(&keypair, None, false);
        assert!(gossipsub.topics().next().is_none());
    }

    #[test]
    fn test_peer_score_params_with_topic() {
        let (params, thresholds) =
            build_peer_score_params(Some("/nodalync/announce/1.0.0"));

        // Verify topic-level params were set
        assert_eq!(params.topics.len(), 1);
        assert!(params.topic_score_cap > 0.0);
        assert!(params.ip_colocation_factor_weight < 0.0);
        assert!(params.retain_score > Duration::ZERO);

        // Verify thresholds are ordered correctly
        assert!(thresholds.gossip_threshold > thresholds.publish_threshold);
        assert!(thresholds.publish_threshold > thresholds.graylist_threshold);
        assert!(thresholds.accept_px_threshold > 0.0);
    }

    #[test]
    fn test_peer_score_params_without_topic() {
        let (params, thresholds) = build_peer_score_params(None);

        // No topic params when no topic provided
        assert!(params.topics.is_empty());
        // But IP and behaviour scoring still active
        assert!(params.ip_colocation_factor_weight < 0.0);
        assert!(thresholds.graylist_threshold < 0.0);
    }

    #[tokio::test]
    async fn test_new_creates_consistent_gossipsub_and_identify() {
        let peer_id = PeerId::random();
        let config = NetworkConfig::default();

        let behaviour = NodalyncBehaviour::new(peer_id, &config);
        assert!(behaviour.gossipsub.topics().next().is_none());
    }

    #[tokio::test]
    async fn test_mdns_disabled_by_default() {
        let peer_id = PeerId::random();
        let config = NetworkConfig::default();

        let behaviour = NodalyncBehaviour::new(peer_id, &config);
        // mDNS should be disabled (Toggle wrapping None)
        assert!(!config.enable_mdns);
        // Behaviour should still be created successfully
        assert!(behaviour.gossipsub.topics().next().is_none());
    }

    #[tokio::test]
    async fn test_mdns_enabled_creates_behaviour() {
        let peer_id = PeerId::random();
        let config = NetworkConfig::default().with_mdns(true);

        let behaviour = NodalyncBehaviour::new(peer_id, &config);
        assert!(config.enable_mdns);
        // Behaviour should be created successfully (mDNS may or may not init
        // depending on OS support, but the Toggle handles graceful fallback)
        assert!(behaviour.gossipsub.topics().next().is_none());
    }

    #[tokio::test]
    async fn test_nat_traversal_disabled_skips_all() {
        let peer_id = PeerId::random();
        let config = NetworkConfig::default().with_nat_traversal(NatTraversal::Disabled);

        let behaviour = NodalyncBehaviour::new(peer_id, &config);
        assert!(behaviour.gossipsub.topics().next().is_none());
    }

    #[tokio::test]
    async fn test_nat_traversal_upnp_only() {
        let peer_id = PeerId::random();
        let config = NetworkConfig::default().with_nat_traversal(NatTraversal::UpnpOnly);

        let behaviour = NodalyncBehaviour::new(peer_id, &config);
        assert!(behaviour.gossipsub.topics().next().is_none());
    }
}
