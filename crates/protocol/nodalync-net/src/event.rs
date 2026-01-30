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
}
