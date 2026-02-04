//! Network events.
//!
//! This module defines the events emitted by the network layer.

use libp2p::Multiaddr;
use nodalync_crypto::Hash;
use nodalync_wire::Message;

/// Events emitted by the network layer.
///
/// These events are returned by `Network::next_event()` and represent
/// significant occurrences in the P2P network.
#[derive(Debug)]
#[non_exhaustive]
pub enum NetworkEvent {
    /// A message was received from a peer.
    MessageReceived {
        /// The libp2p peer ID of the sender.
        peer: libp2p::PeerId,
        /// The decoded message.
        message: Message,
    },

    /// A new peer connected.
    PeerConnected {
        /// The libp2p peer ID of the new peer.
        peer: libp2p::PeerId,
    },

    /// A peer disconnected.
    PeerDisconnected {
        /// The libp2p peer ID of the disconnected peer.
        peer: libp2p::PeerId,
    },

    /// DHT put operation completed.
    DhtPutComplete {
        /// The key that was stored.
        key: Hash,
        /// Whether the operation succeeded.
        success: bool,
    },

    /// DHT get operation returned a result.
    DhtGetResult {
        /// The key that was queried.
        key: Hash,
        /// The value if found, None if not found.
        value: Option<Vec<u8>>,
    },

    /// Started listening on a new address.
    NewListenAddr {
        /// The new listen address.
        address: Multiaddr,
    },

    /// Bootstrap process completed.
    BootstrapComplete {
        /// Number of peers discovered during bootstrap.
        peers_discovered: usize,
    },

    /// Received a broadcast message via GossipSub.
    BroadcastReceived {
        /// The topic the message was received on.
        topic: String,
        /// The raw message data.
        data: Vec<u8>,
    },

    /// Request-response inbound request received.
    InboundRequest {
        /// The libp2p peer ID of the requester.
        peer: libp2p::PeerId,
        /// The request ID for responding.
        request_id: libp2p::request_response::InboundRequestId,
        /// The raw request data.
        data: Vec<u8>,
    },
}

impl NetworkEvent {
    /// Returns the peer ID if this event is associated with a specific peer.
    pub fn peer(&self) -> Option<&libp2p::PeerId> {
        match self {
            NetworkEvent::MessageReceived { peer, .. } => Some(peer),
            NetworkEvent::PeerConnected { peer } => Some(peer),
            NetworkEvent::PeerDisconnected { peer } => Some(peer),
            NetworkEvent::InboundRequest { peer, .. } => Some(peer),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libp2p::PeerId;

    #[test]
    fn test_event_peer() {
        let peer = PeerId::random();

        let event = NetworkEvent::PeerConnected { peer };
        assert_eq!(event.peer(), Some(&peer));

        let event = NetworkEvent::BootstrapComplete {
            peers_discovered: 5,
        };
        assert!(event.peer().is_none());
    }

    #[test]
    fn test_network_event_variants_debug() {
        let peer = PeerId::random();

        // PeerConnected
        let event = NetworkEvent::PeerConnected { peer };
        let debug = format!("{:?}", event);
        assert!(
            debug.contains("PeerConnected"),
            "Debug should contain variant name"
        );

        // PeerDisconnected
        let event = NetworkEvent::PeerDisconnected { peer };
        let debug = format!("{:?}", event);
        assert!(debug.contains("PeerDisconnected"));

        // DhtPutComplete
        let event = NetworkEvent::DhtPutComplete {
            key: Hash([0u8; 32]),
            success: true,
        };
        let debug = format!("{:?}", event);
        assert!(debug.contains("DhtPutComplete"));
        assert!(debug.contains("true"));

        // DhtGetResult
        let event = NetworkEvent::DhtGetResult {
            key: Hash([1u8; 32]),
            value: Some(vec![1, 2, 3]),
        };
        let debug = format!("{:?}", event);
        assert!(debug.contains("DhtGetResult"));

        // NewListenAddr
        let addr: Multiaddr = "/ip4/127.0.0.1/tcp/9000".parse().unwrap();
        let event = NetworkEvent::NewListenAddr { address: addr };
        let debug = format!("{:?}", event);
        assert!(debug.contains("NewListenAddr"));

        // BootstrapComplete
        let event = NetworkEvent::BootstrapComplete {
            peers_discovered: 42,
        };
        let debug = format!("{:?}", event);
        assert!(debug.contains("BootstrapComplete"));
        assert!(debug.contains("42"));

        // BroadcastReceived
        let event = NetworkEvent::BroadcastReceived {
            topic: "/nodalync/announce/1.0.0".to_string(),
            data: vec![10, 20, 30],
        };
        let debug = format!("{:?}", event);
        assert!(debug.contains("BroadcastReceived"));
        assert!(debug.contains("nodalync"));
    }

    #[test]
    fn test_network_event_peer_connected_has_peer_id() {
        let peer = PeerId::random();
        let event = NetworkEvent::PeerConnected { peer };

        let extracted = event.peer();
        assert!(extracted.is_some());
        assert_eq!(*extracted.unwrap(), peer);
    }

    #[test]
    fn test_network_event_dht_get_no_peer() {
        let event = NetworkEvent::DhtGetResult {
            key: Hash([5u8; 32]),
            value: None,
        };

        assert!(
            event.peer().is_none(),
            "DhtGetResult should not have an associated peer"
        );

        // Also check DhtPutComplete
        let event = NetworkEvent::DhtPutComplete {
            key: Hash([6u8; 32]),
            success: false,
        };
        assert!(
            event.peer().is_none(),
            "DhtPutComplete should not have an associated peer"
        );

        // Also check NewListenAddr
        let addr: Multiaddr = "/ip4/0.0.0.0/tcp/0".parse().unwrap();
        let event = NetworkEvent::NewListenAddr { address: addr };
        assert!(
            event.peer().is_none(),
            "NewListenAddr should not have an associated peer"
        );

        // Also check BroadcastReceived
        let event = NetworkEvent::BroadcastReceived {
            topic: "test".to_string(),
            data: vec![],
        };
        assert!(
            event.peer().is_none(),
            "BroadcastReceived should not have an associated peer"
        );
    }

    #[test]
    fn test_network_event_message_received_has_peer() {
        let peer = PeerId::random();
        let msg = Message::new(
            1,
            nodalync_wire::MessageType::Ping,
            Hash([0u8; 32]),
            0,
            nodalync_crypto::PeerId::from_bytes([0u8; 20]),
            vec![],
            nodalync_crypto::Signature::from_bytes([0u8; 64]),
        );

        let event = NetworkEvent::MessageReceived { peer, message: msg };

        let extracted = event.peer();
        assert!(extracted.is_some());
        assert_eq!(*extracted.unwrap(), peer);
    }
}
