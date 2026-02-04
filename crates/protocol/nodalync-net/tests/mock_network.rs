//! Integration tests for the Network trait contract using MockNetwork.
//!
//! These tests verify the Network trait API via the MockNetwork implementation
//! from nodalync-test-utils.

use nodalync_crypto::{content_hash, Hash, PeerId as NodalyncPeerId, Signature};
use nodalync_net::Network;
use nodalync_test_utils::MockNetwork;
use nodalync_types::{ContentType, L1Summary};
use nodalync_wire::{AnnouncePayload, Message, MessageType};

/// Helper to create a test AnnouncePayload with a given title and hash.
fn make_announce_payload(hash: Hash, title: &str) -> AnnouncePayload {
    AnnouncePayload {
        hash,
        content_type: ContentType::L0,
        title: title.to_string(),
        l1_summary: L1Summary::empty(hash),
        price: 100,
        addresses: vec![],
        publisher_peer_id: None,
    }
}

/// Helper to create a dummy Message for testing.
fn make_test_message(msg_type: MessageType) -> Message {
    Message::new(
        1,
        msg_type,
        Hash([0u8; 32]),
        0,
        NodalyncPeerId([0u8; 20]),
        vec![],
        Signature::from_bytes([0u8; 64]),
    )
}

#[tokio::test]
async fn test_dht_announce_and_get_roundtrip() {
    let net = MockNetwork::new();
    let hash = content_hash(b"roundtrip content");
    let payload = make_announce_payload(hash, "Roundtrip Test");

    net.dht_announce(hash, payload.clone()).await.unwrap();

    let retrieved = net.dht_get(&hash).await.unwrap();
    assert!(
        retrieved.is_some(),
        "DHT get should return the announced payload"
    );

    let retrieved = retrieved.unwrap();
    assert_eq!(retrieved.title, "Roundtrip Test");
    assert_eq!(retrieved.hash, hash);
    assert_eq!(retrieved.price, 100);
    assert_eq!(retrieved.content_type, ContentType::L0);
}

#[tokio::test]
async fn test_dht_get_nonexistent_returns_none() {
    let net = MockNetwork::new();
    let hash = content_hash(b"nonexistent content");

    let result = net.dht_get(&hash).await.unwrap();
    assert!(
        result.is_none(),
        "DHT get for unknown hash should return None"
    );
}

#[tokio::test]
async fn test_dht_remove_deletes_entry() {
    let net = MockNetwork::new();
    let hash = content_hash(b"content to remove");
    let payload = make_announce_payload(hash, "Will Be Removed");

    // Announce and verify it exists
    net.dht_announce(hash, payload).await.unwrap();
    assert!(net.dht_get(&hash).await.unwrap().is_some());

    // Remove and verify it is gone
    net.dht_remove(&hash).await.unwrap();
    let result = net.dht_get(&hash).await.unwrap();
    assert!(result.is_none(), "DHT get after remove should return None");
}

#[tokio::test]
async fn test_send_records_messages() {
    let net = MockNetwork::new();
    let peer = libp2p::PeerId::random();
    let msg = make_test_message(MessageType::Ping);

    let _ = net.send(peer, msg).await;

    let sent = net.sent_messages();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].0, peer);
    assert_eq!(sent[0].1.message_type, MessageType::Ping);
    assert_eq!(net.sent_message_count(), 1);
}

#[tokio::test]
async fn test_broadcast_announce_records_message() {
    let net = MockNetwork::new();
    let msg = make_test_message(MessageType::Announce);

    net.broadcast(msg).await.unwrap();

    let broadcasts = net.broadcast_messages();
    assert_eq!(broadcasts.len(), 1);
    assert_eq!(broadcasts[0].message_type, MessageType::Announce);
    assert_eq!(net.broadcast_message_count(), 1);
}

#[tokio::test]
async fn test_peer_id_mapping_bidirectional() {
    let net = MockNetwork::new();
    let libp2p_peer = libp2p::PeerId::random();
    let nodalync_peer = NodalyncPeerId([42u8; 20]);

    net.register_peer_mapping(libp2p_peer, nodalync_peer);

    // libp2p -> nodalync direction
    let resolved_nodalync = net.nodalync_peer_id(&libp2p_peer);
    assert_eq!(resolved_nodalync, Some(nodalync_peer));

    // nodalync -> libp2p direction
    let resolved_libp2p = net.libp2p_peer_id(&nodalync_peer);
    assert_eq!(resolved_libp2p, Some(libp2p_peer));
}

#[tokio::test]
async fn test_register_peer_mapping_overwrite() {
    let net = MockNetwork::new();
    let nodalync_peer = NodalyncPeerId([99u8; 20]);

    // Register with first libp2p peer
    let libp2p_peer_1 = libp2p::PeerId::random();
    net.register_peer_mapping(libp2p_peer_1, nodalync_peer);
    assert_eq!(net.libp2p_peer_id(&nodalync_peer), Some(libp2p_peer_1));

    // Overwrite with second libp2p peer
    let libp2p_peer_2 = libp2p::PeerId::random();
    net.register_peer_mapping(libp2p_peer_2, nodalync_peer);

    // nodalync -> libp2p should now resolve to the second peer
    assert_eq!(net.libp2p_peer_id(&nodalync_peer), Some(libp2p_peer_2));

    // The second libp2p peer should resolve to the nodalync peer
    assert_eq!(net.nodalync_peer_id(&libp2p_peer_2), Some(nodalync_peer));
}

#[tokio::test]
async fn test_connected_peers_tracking() {
    let peer1 = libp2p::PeerId::random();
    let peer2 = libp2p::PeerId::random();
    let peer3 = libp2p::PeerId::random();

    let net = MockNetwork::new()
        .with_connected_peer(peer1)
        .with_connected_peer(peer2)
        .with_connected_peer(peer3);

    let peers = net.connected_peers();
    assert_eq!(peers.len(), 3);
    assert!(peers.contains(&peer1));
    assert!(peers.contains(&peer2));
    assert!(peers.contains(&peer3));
}

#[tokio::test]
async fn test_clone_shares_state() {
    let net = MockNetwork::new();
    let net_clone = net.clone();

    // Mutate via the original
    let hash = content_hash(b"shared state content");
    let payload = make_announce_payload(hash, "Shared");
    net.dht_announce(hash, payload).await.unwrap();

    // The clone should see the change
    let retrieved = net_clone.dht_get(&hash).await.unwrap();
    assert!(
        retrieved.is_some(),
        "Clone should share state with original"
    );
    assert_eq!(retrieved.unwrap().title, "Shared");

    // Mutate via the clone and verify the original sees it
    let peer = libp2p::PeerId::random();
    let nodalync_peer = NodalyncPeerId([7u8; 20]);
    net_clone.register_peer_mapping(peer, nodalync_peer);
    assert_eq!(net.nodalync_peer_id(&peer), Some(nodalync_peer));
}

#[tokio::test]
async fn test_clear_messages() {
    let net = MockNetwork::new();
    let peer = libp2p::PeerId::random();

    // Send some messages
    let _ = net.send(peer, make_test_message(MessageType::Ping)).await;
    let _ = net.send(peer, make_test_message(MessageType::Pong)).await;
    net.broadcast(make_test_message(MessageType::Announce))
        .await
        .unwrap();

    assert_eq!(net.sent_message_count(), 2);
    assert_eq!(net.broadcast_message_count(), 1);

    // Clear and verify
    net.clear_messages();
    assert_eq!(net.sent_message_count(), 0);
    assert_eq!(net.broadcast_message_count(), 0);
    assert!(net.sent_messages().is_empty());
    assert!(net.broadcast_messages().is_empty());
    assert!(net.raw_responses().is_empty());
    assert!(net.signed_responses().is_empty());
}
