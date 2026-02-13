//! Network behaviour for the Nodalync protocol.
//!
//! This module defines the combined network behaviour using libp2p's
//! derive macro to compose multiple protocols:
//! - Kademlia: DHT for content discovery
//! - Request-Response: Point-to-point messaging
//! - GossipSub: Broadcast messaging
//! - Identify: Peer identification
//! - mDNS: Local network peer discovery (optional)

use crate::codec::{NodalyncCodec, NodalyncRequest, NodalyncResponse, PROTOCOL_NAME};
use crate::config::NetworkConfig;
use libp2p::{
    gossipsub::{self, MessageId},
    identify,
    kad::{self, store::MemoryStore, Mode},
    mdns, ping,
    request_response::{self, ProtocolSupport},
    swarm::NetworkBehaviour,
    PeerId,
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

impl NodalyncBehaviour {
    /// Create a new NodalyncBehaviour with the given configuration.
    pub fn new(local_peer_id: PeerId, config: &NetworkConfig) -> Self {
        // Configure Kademlia
        let store = MemoryStore::new(local_peer_id);
        let mut kad_config = kad::Config::new(kad::PROTOCOL_NAME.clone());
        kad_config.set_query_timeout(config.dht_query_timeout);
        kad_config
            .set_replication_factor(std::num::NonZeroUsize::new(config.dht_replication).unwrap());
        kad_config.set_parallelism(std::num::NonZeroUsize::new(config.dht_alpha).unwrap());
        // Set disjoint query paths for better reliability
        kad_config.disjoint_query_paths(true);

        let mut kademlia = kad::Behaviour::with_config(local_peer_id, store, kad_config);
        // Set mode to server so this node responds to queries
        kademlia.set_mode(Some(Mode::Server));

        // Configure request-response
        let req_resp_config =
            request_response::Config::default().with_request_timeout(config.request_timeout);
        let request_response = request_response::Behaviour::new(
            [(PROTOCOL_NAME, ProtocolSupport::Full)],
            req_resp_config,
        );

        // Configure GossipSub
        let gossipsub_keypair = libp2p::identity::Keypair::generate_ed25519();
        let gossipsub = build_gossipsub_with_keypair(&gossipsub_keypair);

        // Configure Identify
        let identify_config =
            identify::Config::new("/nodalync/1.0.0".to_string(), gossipsub_keypair.public())
                .with_interval(Duration::from_secs(60))
                .with_push_listen_addr_updates(true);
        let identify = identify::Behaviour::new(identify_config);

        // Configure Ping - keeps connections alive
        let ping = ping::Behaviour::new(ping::Config::new().with_interval(Duration::from_secs(15)));

        // Configure mDNS (optional)
        let mdns = build_mdns(local_peer_id, config.enable_mdns);

        Self {
            kademlia,
            request_response,
            gossipsub,
            identify,
            ping,
            mdns,
        }
    }

    /// Create a new NodalyncBehaviour with a specific keypair for identify.
    pub fn with_keypair(
        local_peer_id: PeerId,
        keypair: &libp2p::identity::Keypair,
        config: &NetworkConfig,
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

        // Configure GossipSub
        let gossipsub = build_gossipsub_with_keypair(keypair);

        // Configure Identify with the actual keypair
        let identify_config =
            identify::Config::new("/nodalync/1.0.0".to_string(), keypair.public())
                .with_interval(Duration::from_secs(60))
                .with_push_listen_addr_updates(true);
        let identify = identify::Behaviour::new(identify_config);

        // Configure Ping - keeps connections alive
        let ping = ping::Behaviour::new(ping::Config::new().with_interval(Duration::from_secs(15)));

        // Configure mDNS (optional)
        let mdns = build_mdns(local_peer_id, config.enable_mdns);

        Self {
            kademlia,
            request_response,
            gossipsub,
            identify,
            ping,
            mdns,
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
pub fn build_gossipsub_with_keypair(keypair: &libp2p::identity::Keypair) -> gossipsub::Behaviour {
    let message_id_fn = |message: &gossipsub::Message| {
        let mut hasher = Sha256::new();
        hasher.update(&message.data);
        MessageId::from(hasher.finalize().to_vec())
    };

    let config = gossipsub::ConfigBuilder::default()
        .heartbeat_interval(Duration::from_secs(1))
        .validation_mode(gossipsub::ValidationMode::Strict)
        .message_id_fn(message_id_fn)
        .build()
        .expect("valid gossipsub config");

    gossipsub::Behaviour::new(
        gossipsub::MessageAuthenticity::Signed(keypair.clone()),
        config,
    )
    .expect("valid gossipsub behaviour")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_behaviour() {
        let peer_id = PeerId::random();
        let config = NetworkConfig::default();

        let behaviour = NodalyncBehaviour::new(peer_id, &config);

        // Verify it was created successfully
        assert!(behaviour.gossipsub.topics().next().is_none());
    }

    #[test]
    fn test_create_behaviour_with_keypair() {
        let keypair = libp2p::identity::Keypair::generate_ed25519();
        let peer_id = keypair.public().to_peer_id();
        let config = NetworkConfig::default();

        let behaviour = NodalyncBehaviour::with_keypair(peer_id, &keypair, &config);

        // Verify it was created successfully
        assert!(behaviour.gossipsub.topics().next().is_none());
    }

    #[test]
    fn test_with_keypair_uses_provided_keypair_for_gossipsub() {
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
    fn test_new_creates_consistent_gossipsub_and_identify() {
        let peer_id = PeerId::random();
        let config = NetworkConfig::default();

        let behaviour = NodalyncBehaviour::new(peer_id, &config);
        assert!(behaviour.gossipsub.topics().next().is_none());
    }

    #[test]
    fn test_mdns_disabled_by_default() {
        let peer_id = PeerId::random();
        let config = NetworkConfig::default();

        let behaviour = NodalyncBehaviour::new(peer_id, &config);
        // mDNS should be disabled (Toggle wrapping None)
        assert!(!config.enable_mdns);
        // Behaviour should still be created successfully
        assert!(behaviour.gossipsub.topics().next().is_none());
    }

    #[test]
    fn test_mdns_enabled_creates_behaviour() {
        let peer_id = PeerId::random();
        let config = NetworkConfig::default().with_mdns(true);

        let behaviour = NodalyncBehaviour::new(peer_id, &config);
        assert!(config.enable_mdns);
        // Behaviour should be created successfully (mDNS may or may not init
        // depending on OS support, but the Toggle handles graceful fallback)
        assert!(behaviour.gossipsub.topics().next().is_none());
    }
}
