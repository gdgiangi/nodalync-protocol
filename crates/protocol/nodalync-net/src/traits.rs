//! Network trait definition.
//!
//! This module defines the `Network` trait that provides the public API
//! for P2P networking operations.

use crate::error::NetworkResult;
use crate::event::NetworkEvent;
use async_trait::async_trait;
use libp2p::Multiaddr;
use nodalync_crypto::{Hash, PeerId as NodalyncPeerId};
use nodalync_wire::{
    AnnouncePayload, AnnounceUpdatePayload, ChannelClosePayload, ChannelOpenPayload, Message,
    MessageType, PreviewRequestPayload, PreviewResponsePayload, QueryRequestPayload,
    QueryResponsePayload, SearchPayload, SearchResponsePayload, SettleConfirmPayload,
};

/// The Network trait provides the public API for P2P networking.
///
/// This trait abstracts over the underlying libp2p implementation,
/// providing a clean interface for:
/// - DHT operations (announce, get, remove)
/// - Point-to-point messaging
/// - Broadcast messaging
/// - Peer management
#[async_trait]
pub trait Network: Send + Sync {
    // =========================================================================
    // DHT Operations
    // =========================================================================

    /// Announce content to the DHT.
    ///
    /// Stores `hash -> AnnouncePayload` in the Kademlia DHT,
    /// making the content discoverable by other peers.
    async fn dht_announce(&self, hash: Hash, payload: AnnouncePayload) -> NetworkResult<()>;

    /// Get an announcement from the DHT.
    ///
    /// Looks up the given hash in the DHT and returns the AnnouncePayload
    /// if found.
    async fn dht_get(&self, hash: &Hash) -> NetworkResult<Option<AnnouncePayload>>;

    /// Remove an announcement from the DHT.
    ///
    /// This is a best-effort operation; DHT records may persist on other nodes.
    async fn dht_remove(&self, hash: &Hash) -> NetworkResult<()>;

    // =========================================================================
    // Messaging
    // =========================================================================

    /// Send a message to a specific peer.
    ///
    /// Uses the request-response protocol to deliver a message and wait
    /// for a response.
    async fn send(&self, peer: libp2p::PeerId, message: Message) -> NetworkResult<Message>;

    /// Broadcast a message to all subscribers.
    ///
    /// Uses GossipSub to broadcast to all peers subscribed to the
    /// announcement topic.
    async fn broadcast(&self, message: Message) -> NetworkResult<()>;

    // =========================================================================
    // Typed Message Helpers
    // =========================================================================

    /// Send a preview request and receive the response.
    async fn send_preview_request(
        &self,
        peer: libp2p::PeerId,
        request: PreviewRequestPayload,
    ) -> NetworkResult<PreviewResponsePayload>;

    /// Send a query request and receive the response.
    async fn send_query(
        &self,
        peer: libp2p::PeerId,
        request: QueryRequestPayload,
    ) -> NetworkResult<QueryResponsePayload>;

    /// Send a search request and receive the response.
    async fn send_search(
        &self,
        peer: libp2p::PeerId,
        request: SearchPayload,
    ) -> NetworkResult<SearchResponsePayload>;

    /// Send a channel open request.
    async fn send_channel_open(
        &self,
        peer: libp2p::PeerId,
        payload: ChannelOpenPayload,
    ) -> NetworkResult<Message>;

    /// Send a channel close request.
    async fn send_channel_close(
        &self,
        peer: libp2p::PeerId,
        payload: ChannelClosePayload,
    ) -> NetworkResult<Message>;

    /// Broadcast a settlement confirmation.
    async fn broadcast_settlement_confirm(
        &self,
        payload: SettleConfirmPayload,
    ) -> NetworkResult<()>;

    /// Broadcast a content announcement.
    ///
    /// Uses GossipSub to broadcast an ANNOUNCE message to all subscribers,
    /// allowing other nodes to discover newly published content.
    async fn broadcast_announce(&self, payload: AnnouncePayload) -> NetworkResult<()>;

    /// Broadcast a content update announcement.
    ///
    /// Uses GossipSub to broadcast an ANNOUNCE_UPDATE message when
    /// existing content is updated to a new version. Peers that cached
    /// the original announcement will update their cache.
    async fn broadcast_announce_update(&self, payload: AnnounceUpdatePayload) -> NetworkResult<()>;

    // =========================================================================
    // Peer Management
    // =========================================================================

    /// Get the list of currently connected peers.
    fn connected_peers(&self) -> Vec<libp2p::PeerId>;

    /// Get the addresses this node is listening on.
    fn listen_addresses(&self) -> Vec<Multiaddr>;

    /// Dial a peer at the given address.
    async fn dial(&self, addr: Multiaddr) -> NetworkResult<()>;

    /// Dial a peer by peer ID (requires address to be known via DHT or bootstrap).
    async fn dial_peer(&self, peer: libp2p::PeerId) -> NetworkResult<()>;

    // =========================================================================
    // Events
    // =========================================================================

    /// Get the next network event.
    ///
    /// This is a polling-based interface; call this in a loop to process
    /// incoming events.
    async fn next_event(&self) -> NetworkResult<NetworkEvent>;

    /// Send a response to an inbound request (raw bytes).
    ///
    /// This sends raw bytes directly. For proper protocol compliance,
    /// use `send_signed_response` instead which wraps the payload in
    /// a signed Message.
    ///
    /// # Arguments
    /// * `request_id` - The request ID from the `InboundRequest` event
    /// * `data` - The response data to send
    async fn send_response(
        &self,
        request_id: libp2p::request_response::InboundRequestId,
        data: Vec<u8>,
    ) -> NetworkResult<()>;

    /// Send a signed response to an inbound request.
    ///
    /// This creates a signed Message with the given type and payload,
    /// encodes it in wire format, and sends it as the response.
    ///
    /// # Arguments
    /// * `request_id` - The request ID from the `InboundRequest` event
    /// * `message_type` - The response message type
    /// * `payload` - The CBOR-encoded payload
    async fn send_signed_response(
        &self,
        request_id: libp2p::request_response::InboundRequestId,
        message_type: MessageType,
        payload: Vec<u8>,
    ) -> NetworkResult<()>;

    // =========================================================================
    // Utility
    // =========================================================================

    /// Get the local libp2p peer ID.
    fn local_peer_id(&self) -> libp2p::PeerId;

    /// Get the Nodalync peer ID for a libp2p peer ID.
    fn nodalync_peer_id(&self, libp2p_peer: &libp2p::PeerId) -> Option<NodalyncPeerId>;

    /// Get the libp2p peer ID for a Nodalync peer ID.
    fn libp2p_peer_id(&self, nodalync_peer: &NodalyncPeerId) -> Option<libp2p::PeerId>;

    /// Register a peer ID mapping.
    ///
    /// This is used when we learn a peer's Nodalync ID from a message
    /// (e.g., from a ChannelAccept response).
    fn register_peer_mapping(&self, libp2p_peer: libp2p::PeerId, nodalync_peer: NodalyncPeerId);
}

#[cfg(test)]
mod tests {
    use super::*;

    // Verify the trait can be made into a trait object
    fn _assert_object_safe(_: &dyn Network) {}
}
