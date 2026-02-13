//! P2P networking for the Nodalync protocol.
//!
//! This crate provides libp2p-based P2P networking for the Nodalync protocol,
//! as specified in Protocol Specification §11. It includes:
//!
//! - **DHT Discovery**: Kademlia-based content discovery via hash lookup
//! - **Request-Response**: Point-to-point messaging for queries and channels
//! - **GossipSub**: Broadcast messaging for announcements
//! - **Peer Management**: Connection handling and peer discovery
//!
//! # Overview
//!
//! The networking layer uses libp2p with the following stack:
//!
//! - **Transport**: TCP + Noise (encryption) + Yamux (multiplexing)
//! - **DHT**: Kademlia with bucket_size=20, alpha=3, replication=20
//! - **Messaging**: Request-response with 30s timeout, 3 retries
//! - **Broadcast**: GossipSub with strict validation
//!
//! # Example
//!
//! ```no_run
//! use nodalync_net::{NetworkNode, NetworkConfig, Network};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Create a network node with default configuration
//!     let config = NetworkConfig::default();
//!     let node = NetworkNode::new(config).await?;
//!
//!     // Bootstrap the node
//!     node.bootstrap().await?;
//!
//!     // Subscribe to announcements
//!     node.subscribe_announcements().await?;
//!
//!     // Process events
//!     loop {
//!         let event = node.next_event().await?;
//!         println!("Received event: {:?}", event);
//!     }
//! }
//! ```
//!
//! # DHT Operations
//!
//! Content discovery uses hash-based DHT lookup:
//!
//! ```no_run
//! use nodalync_net::{NetworkNode, NetworkConfig, Network};
//! use nodalync_crypto::content_hash;
//! use nodalync_wire::AnnouncePayload;
//!
//! async fn example(node: &NetworkNode, payload: AnnouncePayload) {
//!     let content = b"hello world";
//!     let hash = content_hash(content);
//!
//!     // Announce content to DHT
//!     node.dht_announce(hash, payload).await.unwrap();
//!
//!     // Look up content by hash
//!     if let Some(announcement) = node.dht_get(&hash).await.unwrap() {
//!         println!("Found: {:?}", announcement);
//!     }
//! }
//! ```
//!
//! # Request-Response
//!
//! Point-to-point messaging uses the request-response protocol:
//!
//! ```no_run
//! use nodalync_net::{NetworkNode, NetworkConfig, Network};
//! use nodalync_wire::PreviewRequestPayload;
//! use nodalync_crypto::content_hash;
//!
//! async fn example(node: &NetworkNode, peer: libp2p::PeerId) {
//!     let hash = content_hash(b"content");
//!     let request = PreviewRequestPayload { hash };
//!
//!     // Send preview request and get response
//!     let response = node.send_preview_request(peer, request).await.unwrap();
//!     println!("Preview: {:?}", response);
//! }
//! ```
//!
//! # Peer ID Mapping
//!
//! The network layer maintains a bidirectional mapping between libp2p PeerIds
//! and Nodalync PeerIds. This mapping is populated when peers exchange
//! PeerInfoPayload messages.
//!
//! ```no_run
//! use nodalync_net::{NetworkNode, Network};
//!
//! fn example(node: &NetworkNode, libp2p_peer: libp2p::PeerId) {
//!     // Get Nodalync peer ID for a libp2p peer
//!     if let Some(nodalync_id) = node.nodalync_peer_id(&libp2p_peer) {
//!         println!("Nodalync ID: {:?}", nodalync_id);
//!     }
//! }
//! ```

pub mod behaviour;
pub mod codec;
pub mod config;
pub mod error;
pub mod event;
pub mod node;
pub mod peer_id;
pub mod traits;
pub mod transport;

// Re-export main types at crate root

// Configuration
pub use config::{NatTraversal, NetworkConfig};

// Error types
pub use error::{NetworkError, NetworkResult};

// Event types
pub use event::{NatStatus, NetworkEvent};

// Node
pub use node::NetworkNode;

// Peer ID mapping
pub use peer_id::PeerIdMapper;

// The Network trait
pub use traits::Network;

// Re-export libp2p types commonly needed
pub use libp2p::request_response::InboundRequestId;
pub use libp2p::{identity, multiaddr, Multiaddr, PeerId};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exports() {
        // Verify key types are exported
        let _: NetworkConfig = NetworkConfig::default();
    }

    #[tokio::test]
    async fn test_create_node() {
        let config = NetworkConfig::default();
        let result = NetworkNode::new(config).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_stable_peer_id_with_identity_secret() {
        let secret = [7u8; 32];

        // Create two nodes with the same identity secret
        let config1 = NetworkConfig::new().with_identity_secret(secret);
        let config2 = NetworkConfig::new().with_identity_secret(secret);

        let node1 = NetworkNode::new(config1).await.unwrap();
        let node2 = NetworkNode::new(config2).await.unwrap();

        // They should have the same libp2p PeerId
        assert_eq!(node1.local_peer_id(), node2.local_peer_id());
    }

    #[tokio::test]
    async fn test_random_peer_id_without_secret() {
        let config1 = NetworkConfig::default();
        let config2 = NetworkConfig::default();

        let node1 = NetworkNode::new(config1).await.unwrap();
        let node2 = NetworkNode::new(config2).await.unwrap();

        // Random identities should differ (overwhelmingly likely)
        assert_ne!(node1.local_peer_id(), node2.local_peer_id());
    }
}
