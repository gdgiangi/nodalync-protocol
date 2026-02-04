//! Mock implementation of the `Network` trait for testing.
//!
//! Provides a configurable mock network that records sent messages
//! and returns pre-configured responses.

use async_trait::async_trait;
use libp2p::Multiaddr;
use nodalync_crypto::{Hash, PeerId as NodalyncPeerId};
use nodalync_net::{Network, NetworkError, NetworkEvent, NetworkResult};
use nodalync_wire::{
    AnnouncePayload, ChannelClosePayload, ChannelOpenPayload, Message, MessageType,
    PreviewRequestPayload, PreviewResponsePayload, QueryRequestPayload, QueryResponsePayload,
    SearchPayload, SearchResponsePayload, SettleConfirmPayload,
};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

struct MockNetworkInner {
    /// DHT storage: hash -> AnnouncePayload.
    dht: HashMap<Hash, AnnouncePayload>,
    /// Sent messages for assertion (spy pattern).
    sent_messages: Vec<(libp2p::PeerId, Message)>,
    /// Broadcast messages for assertion.
    broadcast_messages: Vec<Message>,
    /// Configurable preview responses keyed by content hash.
    preview_responses: HashMap<Hash, PreviewResponsePayload>,
    /// Configurable query responses keyed by content hash.
    query_responses: HashMap<Hash, QueryResponsePayload>,
    /// Configurable search responses keyed by query string.
    search_responses: HashMap<String, SearchResponsePayload>,
    /// Configurable channel open responses keyed by channel ID hash.
    channel_open_responses: HashMap<Hash, Message>,
    /// Configurable channel close responses keyed by channel ID hash.
    channel_close_responses: HashMap<Hash, Message>,
    /// Peer ID mappings: Nodalync -> libp2p.
    nodalync_to_libp2p: HashMap<NodalyncPeerId, libp2p::PeerId>,
    /// Peer ID mappings: libp2p -> Nodalync.
    libp2p_to_nodalync: HashMap<libp2p::PeerId, NodalyncPeerId>,
    /// Connected peers.
    connected_peers: Vec<libp2p::PeerId>,
    /// Event queue for next_event().
    events: VecDeque<NetworkEvent>,
    /// Local peer ID.
    local_peer_id: libp2p::PeerId,
    /// Listen addresses.
    listen_addresses: Vec<Multiaddr>,
    /// Recorded raw responses sent via send_response.
    raw_responses: Vec<Vec<u8>>,
    /// Recorded signed responses sent via send_signed_response.
    signed_responses: Vec<(MessageType, Vec<u8>)>,
}

impl MockNetworkInner {
    fn new(local_peer_id: libp2p::PeerId) -> Self {
        Self {
            dht: HashMap::new(),
            sent_messages: Vec::new(),
            broadcast_messages: Vec::new(),
            preview_responses: HashMap::new(),
            query_responses: HashMap::new(),
            search_responses: HashMap::new(),
            channel_open_responses: HashMap::new(),
            channel_close_responses: HashMap::new(),
            nodalync_to_libp2p: HashMap::new(),
            libp2p_to_nodalync: HashMap::new(),
            connected_peers: Vec::new(),
            events: VecDeque::new(),
            local_peer_id,
            listen_addresses: Vec::new(),
            raw_responses: Vec::new(),
            signed_responses: Vec::new(),
        }
    }
}

/// A mock implementation of the `Network` trait for testing.
///
/// Records all sent messages and returns pre-configured responses.
/// Uses `Arc<Mutex<...>>` internally, so it is cheap to clone and
/// all clones share the same state.
#[derive(Clone)]
pub struct MockNetwork {
    inner: Arc<Mutex<MockNetworkInner>>,
}

impl Default for MockNetwork {
    fn default() -> Self {
        Self::new()
    }
}

impl MockNetwork {
    /// Create a new MockNetwork with a random local peer ID.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(MockNetworkInner::new(libp2p::PeerId::random()))),
        }
    }

    /// Create a new MockNetwork with a specific local peer ID.
    pub fn with_local_peer_id(peer_id: libp2p::PeerId) -> Self {
        Self {
            inner: Arc::new(Mutex::new(MockNetworkInner::new(peer_id))),
        }
    }

    // =========================================================================
    // Builder Methods
    // =========================================================================

    /// Add a pre-configured preview response for a given content hash.
    pub fn with_preview_response(self, hash: Hash, response: PreviewResponsePayload) -> Self {
        self.inner
            .lock()
            .unwrap()
            .preview_responses
            .insert(hash, response);
        self
    }

    /// Add a pre-configured query response for a given content hash.
    pub fn with_query_response(self, hash: Hash, response: QueryResponsePayload) -> Self {
        self.inner
            .lock()
            .unwrap()
            .query_responses
            .insert(hash, response);
        self
    }

    /// Add a pre-configured search response for a given query string.
    pub fn with_search_response(self, query: String, response: SearchResponsePayload) -> Self {
        self.inner
            .lock()
            .unwrap()
            .search_responses
            .insert(query, response);
        self
    }

    /// Add a pre-configured channel open response for a given channel ID.
    pub fn with_channel_open_response(self, channel_id: Hash, response: Message) -> Self {
        self.inner
            .lock()
            .unwrap()
            .channel_open_responses
            .insert(channel_id, response);
        self
    }

    /// Add a pre-configured channel close response for a given channel ID.
    pub fn with_channel_close_response(self, channel_id: Hash, response: Message) -> Self {
        self.inner
            .lock()
            .unwrap()
            .channel_close_responses
            .insert(channel_id, response);
        self
    }

    /// Add a connected peer.
    pub fn with_connected_peer(self, peer: libp2p::PeerId) -> Self {
        self.inner.lock().unwrap().connected_peers.push(peer);
        self
    }

    /// Add a listen address.
    pub fn with_listen_address(self, addr: Multiaddr) -> Self {
        self.inner.lock().unwrap().listen_addresses.push(addr);
        self
    }

    /// Enqueue a network event to be returned by `next_event`.
    pub fn enqueue_event(&self, event: NetworkEvent) {
        self.inner.lock().unwrap().events.push_back(event);
    }

    /// Add a DHT entry directly.
    pub fn with_dht_entry(self, hash: Hash, payload: AnnouncePayload) -> Self {
        self.inner.lock().unwrap().dht.insert(hash, payload);
        self
    }

    /// Add a peer ID mapping.
    pub fn with_peer_mapping(
        self,
        libp2p_peer: libp2p::PeerId,
        nodalync_peer: NodalyncPeerId,
    ) -> Self {
        {
            let mut inner = self.inner.lock().unwrap();
            inner.nodalync_to_libp2p.insert(nodalync_peer, libp2p_peer);
            inner.libp2p_to_nodalync.insert(libp2p_peer, nodalync_peer);
        }
        self
    }

    // =========================================================================
    // Assertion Helpers
    // =========================================================================

    /// Get all sent point-to-point messages.
    pub fn sent_messages(&self) -> Vec<(libp2p::PeerId, Message)> {
        self.inner.lock().unwrap().sent_messages.clone()
    }

    /// Get all broadcast messages.
    pub fn broadcast_messages(&self) -> Vec<Message> {
        self.inner.lock().unwrap().broadcast_messages.clone()
    }

    /// Get the number of sent messages.
    pub fn sent_message_count(&self) -> usize {
        self.inner.lock().unwrap().sent_messages.len()
    }

    /// Get the number of broadcast messages.
    pub fn broadcast_message_count(&self) -> usize {
        self.inner.lock().unwrap().broadcast_messages.len()
    }

    /// Get all raw responses sent via `send_response`.
    pub fn raw_responses(&self) -> Vec<Vec<u8>> {
        self.inner.lock().unwrap().raw_responses.clone()
    }

    /// Get all signed responses sent via `send_signed_response`.
    pub fn signed_responses(&self) -> Vec<(MessageType, Vec<u8>)> {
        self.inner.lock().unwrap().signed_responses.clone()
    }

    /// Get the current DHT entries.
    pub fn dht_entries(&self) -> HashMap<Hash, AnnouncePayload> {
        self.inner.lock().unwrap().dht.clone()
    }

    /// Clear all recorded messages.
    pub fn clear_messages(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.sent_messages.clear();
        inner.broadcast_messages.clear();
        inner.raw_responses.clear();
        inner.signed_responses.clear();
    }
}

#[async_trait]
impl Network for MockNetwork {
    // =========================================================================
    // DHT Operations
    // =========================================================================

    async fn dht_announce(&self, hash: Hash, payload: AnnouncePayload) -> NetworkResult<()> {
        self.inner.lock().unwrap().dht.insert(hash, payload);
        Ok(())
    }

    async fn dht_get(&self, hash: &Hash) -> NetworkResult<Option<AnnouncePayload>> {
        Ok(self.inner.lock().unwrap().dht.get(hash).cloned())
    }

    async fn dht_remove(&self, hash: &Hash) -> NetworkResult<()> {
        self.inner.lock().unwrap().dht.remove(hash);
        Ok(())
    }

    // =========================================================================
    // Messaging
    // =========================================================================

    async fn send(&self, peer: libp2p::PeerId, message: Message) -> NetworkResult<Message> {
        let response = message.clone();
        self.inner
            .lock()
            .unwrap()
            .sent_messages
            .push((peer, message));
        // Return the same message as a default response
        Ok(response)
    }

    async fn broadcast(&self, message: Message) -> NetworkResult<()> {
        self.inner.lock().unwrap().broadcast_messages.push(message);
        Ok(())
    }

    // =========================================================================
    // Typed Message Helpers
    // =========================================================================

    async fn send_preview_request(
        &self,
        _peer: libp2p::PeerId,
        request: PreviewRequestPayload,
    ) -> NetworkResult<PreviewResponsePayload> {
        let inner = self.inner.lock().unwrap();
        inner
            .preview_responses
            .get(&request.hash)
            .cloned()
            .ok_or_else(|| {
                NetworkError::Timeout(format!(
                    "no mock preview response configured for hash {}",
                    request.hash
                ))
            })
    }

    async fn send_query(
        &self,
        _peer: libp2p::PeerId,
        request: QueryRequestPayload,
    ) -> NetworkResult<QueryResponsePayload> {
        let inner = self.inner.lock().unwrap();
        inner
            .query_responses
            .get(&request.hash)
            .cloned()
            .ok_or_else(|| {
                NetworkError::Timeout(format!(
                    "no mock query response configured for hash {}",
                    request.hash
                ))
            })
    }

    async fn send_search(
        &self,
        _peer: libp2p::PeerId,
        request: SearchPayload,
    ) -> NetworkResult<SearchResponsePayload> {
        let inner = self.inner.lock().unwrap();
        inner
            .search_responses
            .get(&request.query)
            .cloned()
            .ok_or_else(|| {
                NetworkError::Timeout(format!(
                    "no mock search response configured for query '{}'",
                    request.query
                ))
            })
    }

    async fn send_channel_open(
        &self,
        _peer: libp2p::PeerId,
        payload: ChannelOpenPayload,
    ) -> NetworkResult<Message> {
        let inner = self.inner.lock().unwrap();
        inner
            .channel_open_responses
            .get(&payload.channel_id)
            .cloned()
            .ok_or_else(|| {
                NetworkError::Timeout(format!(
                    "no mock channel open response configured for channel {}",
                    payload.channel_id
                ))
            })
    }

    async fn send_channel_close(
        &self,
        _peer: libp2p::PeerId,
        payload: ChannelClosePayload,
    ) -> NetworkResult<Message> {
        let inner = self.inner.lock().unwrap();
        inner
            .channel_close_responses
            .get(&payload.channel_id)
            .cloned()
            .ok_or_else(|| {
                NetworkError::Timeout(format!(
                    "no mock channel close response configured for channel {}",
                    payload.channel_id
                ))
            })
    }

    async fn broadcast_settlement_confirm(
        &self,
        _payload: SettleConfirmPayload,
    ) -> NetworkResult<()> {
        Ok(())
    }

    async fn broadcast_announce(&self, _payload: AnnouncePayload) -> NetworkResult<()> {
        Ok(())
    }

    // =========================================================================
    // Peer Management
    // =========================================================================

    fn connected_peers(&self) -> Vec<libp2p::PeerId> {
        self.inner.lock().unwrap().connected_peers.clone()
    }

    fn listen_addresses(&self) -> Vec<Multiaddr> {
        self.inner.lock().unwrap().listen_addresses.clone()
    }

    async fn dial(&self, _addr: Multiaddr) -> NetworkResult<()> {
        Ok(())
    }

    async fn dial_peer(&self, _peer: libp2p::PeerId) -> NetworkResult<()> {
        Ok(())
    }

    // =========================================================================
    // Events
    // =========================================================================

    async fn next_event(&self) -> NetworkResult<NetworkEvent> {
        self.inner
            .lock()
            .unwrap()
            .events
            .pop_front()
            .ok_or(NetworkError::ChannelClosed)
    }

    async fn send_response(
        &self,
        _request_id: libp2p::request_response::InboundRequestId,
        data: Vec<u8>,
    ) -> NetworkResult<()> {
        self.inner.lock().unwrap().raw_responses.push(data);
        Ok(())
    }

    async fn send_signed_response(
        &self,
        _request_id: libp2p::request_response::InboundRequestId,
        message_type: MessageType,
        payload: Vec<u8>,
    ) -> NetworkResult<()> {
        self.inner
            .lock()
            .unwrap()
            .signed_responses
            .push((message_type, payload));
        Ok(())
    }

    // =========================================================================
    // Utility
    // =========================================================================

    fn local_peer_id(&self) -> libp2p::PeerId {
        self.inner.lock().unwrap().local_peer_id
    }

    fn nodalync_peer_id(&self, libp2p_peer: &libp2p::PeerId) -> Option<NodalyncPeerId> {
        self.inner
            .lock()
            .unwrap()
            .libp2p_to_nodalync
            .get(libp2p_peer)
            .copied()
    }

    fn libp2p_peer_id(&self, nodalync_peer: &NodalyncPeerId) -> Option<libp2p::PeerId> {
        self.inner
            .lock()
            .unwrap()
            .nodalync_to_libp2p
            .get(nodalync_peer)
            .copied()
    }

    fn register_peer_mapping(&self, libp2p_peer: libp2p::PeerId, nodalync_peer: NodalyncPeerId) {
        let mut inner = self.inner.lock().unwrap();
        inner.nodalync_to_libp2p.insert(nodalync_peer, libp2p_peer);
        inner.libp2p_to_nodalync.insert(libp2p_peer, nodalync_peer);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nodalync_crypto::content_hash;

    #[tokio::test]
    async fn test_dht_roundtrip() {
        let net = MockNetwork::new();
        let hash = content_hash(b"test content");
        let payload = AnnouncePayload {
            hash,
            content_type: nodalync_types::ContentType::L0,
            title: "Test".to_string(),
            l1_summary: nodalync_types::L1Summary::empty(hash),
            price: 100,
            addresses: vec![],
            publisher_peer_id: None,
        };

        net.dht_announce(hash, payload.clone()).await.unwrap();
        let retrieved = net.dht_get(&hash).await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().title, "Test");
    }

    #[tokio::test]
    async fn test_dht_remove() {
        let net = MockNetwork::new();
        let hash = content_hash(b"test");
        let payload = AnnouncePayload {
            hash,
            content_type: nodalync_types::ContentType::L0,
            title: "Test".to_string(),
            l1_summary: nodalync_types::L1Summary::empty(hash),
            price: 0,
            addresses: vec![],
            publisher_peer_id: None,
        };

        net.dht_announce(hash, payload).await.unwrap();
        net.dht_remove(&hash).await.unwrap();
        let retrieved = net.dht_get(&hash).await.unwrap();
        assert!(retrieved.is_none());
    }

    #[tokio::test]
    async fn test_send_records_messages() {
        let net = MockNetwork::new();
        let peer = libp2p::PeerId::random();
        let msg = Message::new(
            1,
            MessageType::Ping,
            Hash([0u8; 32]),
            0,
            NodalyncPeerId([0u8; 20]),
            vec![],
            nodalync_crypto::Signature::from_bytes([0u8; 64]),
        );

        let _ = net.send(peer, msg).await;
        assert_eq!(net.sent_message_count(), 1);
    }

    #[tokio::test]
    async fn test_broadcast_records_messages() {
        let net = MockNetwork::new();
        let msg = Message::new(
            1,
            MessageType::Announce,
            Hash([0u8; 32]),
            0,
            NodalyncPeerId([0u8; 20]),
            vec![],
            nodalync_crypto::Signature::from_bytes([0u8; 64]),
        );

        net.broadcast(msg).await.unwrap();
        assert_eq!(net.broadcast_message_count(), 1);
    }

    #[tokio::test]
    async fn test_next_event_empty() {
        let net = MockNetwork::new();
        let result = net.next_event().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_next_event_queued() {
        let net = MockNetwork::new();
        let peer = libp2p::PeerId::random();
        net.enqueue_event(NetworkEvent::PeerConnected { peer });

        let event = net.next_event().await.unwrap();
        assert!(matches!(event, NetworkEvent::PeerConnected { .. }));
    }

    #[test]
    fn test_peer_mapping() {
        let net = MockNetwork::new();
        let libp2p_peer = libp2p::PeerId::random();
        let nodalync_peer = NodalyncPeerId([1u8; 20]);

        net.register_peer_mapping(libp2p_peer, nodalync_peer);

        assert_eq!(net.nodalync_peer_id(&libp2p_peer), Some(nodalync_peer));
        assert_eq!(net.libp2p_peer_id(&nodalync_peer), Some(libp2p_peer));
    }

    #[test]
    fn test_connected_peers() {
        let peer = libp2p::PeerId::random();
        let net = MockNetwork::new().with_connected_peer(peer);
        assert_eq!(net.connected_peers(), vec![peer]);
    }

    #[test]
    fn test_clone_shares_state() {
        let net = MockNetwork::new();
        let net2 = net.clone();

        let peer = libp2p::PeerId::random();
        let nodalync_peer = NodalyncPeerId([2u8; 20]);
        net.register_peer_mapping(peer, nodalync_peer);

        // The clone should see the mapping
        assert_eq!(net2.nodalync_peer_id(&peer), Some(nodalync_peer));
    }

    #[test]
    fn test_clear_messages() {
        let net = MockNetwork::new();
        // Add some state
        net.inner
            .lock()
            .unwrap()
            .broadcast_messages
            .push(Message::new(
                1,
                MessageType::Announce,
                Hash([0u8; 32]),
                0,
                NodalyncPeerId([0u8; 20]),
                vec![],
                nodalync_crypto::Signature::from_bytes([0u8; 64]),
            ));
        assert_eq!(net.broadcast_message_count(), 1);

        net.clear_messages();
        assert_eq!(net.broadcast_message_count(), 0);
    }
}
