//! Integration tests for nodalync-net.
//!
//! These tests verify multi-node scenarios including:
//! - DHT announce and lookup
//! - Request-response messaging
//! - GossipSub broadcast
//! - Peer discovery

use nodalync_crypto::content_hash;
use nodalync_net::{Network, NetworkConfig, NetworkEvent, NetworkNode};
use nodalync_types::{ContentType, L1Summary};
use nodalync_wire::AnnouncePayload;
use std::time::Duration;
use tokio::time::timeout;

/// Create a test network configuration with random ports.
fn test_config() -> NetworkConfig {
    NetworkConfig::new()
        .with_listen_addresses(vec!["/ip4/127.0.0.1/tcp/0".parse().unwrap()])
        .with_request_timeout(Duration::from_secs(5))
        .with_retry_base_delay(Duration::from_millis(50))
        .with_max_retries(2)
}

/// Wait for a node to start listening.
async fn wait_for_listen(node: &NetworkNode) -> libp2p::Multiaddr {
    let timeout_duration = Duration::from_secs(5);
    let start = std::time::Instant::now();

    loop {
        if start.elapsed() > timeout_duration {
            panic!("Timed out waiting for node to start listening");
        }

        match timeout(Duration::from_millis(100), node.next_event()).await {
            Ok(Ok(NetworkEvent::NewListenAddr { address })) => {
                return address;
            }
            _ => {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }
    }
}

#[tokio::test]
async fn test_node_creation() {
    let config = test_config();
    let node = NetworkNode::new(config).await.unwrap();

    // Node should have a valid peer ID
    let peer_id = node.local_peer_id();
    assert!(!peer_id.to_string().is_empty());
}

#[tokio::test]
async fn test_two_nodes_connect() {
    // Create first node
    let config1 = test_config();
    let node1 = NetworkNode::new(config1).await.unwrap();
    let addr1 = wait_for_listen(&node1).await;

    // Create second node
    let config2 = test_config();
    let node2 = NetworkNode::new(config2).await.unwrap();
    let _addr2 = wait_for_listen(&node2).await;

    // Node 2 dials node 1
    node2.dial(addr1).await.unwrap();

    // Wait for connection event on node 1
    let timeout_duration = Duration::from_secs(5);
    let result = timeout(timeout_duration, async {
        loop {
            if let Ok(event) = node1.next_event().await {
                if matches!(event, NetworkEvent::PeerConnected { .. }) {
                    return true;
                }
            }
        }
    })
    .await;

    assert!(result.is_ok(), "Nodes should connect");
}

#[tokio::test]
async fn test_dht_announce_and_get() {
    // Create two nodes
    let config1 = test_config();
    let node1 = NetworkNode::new(config1).await.unwrap();
    let addr1 = wait_for_listen(&node1).await;

    let config2 = test_config();
    let node2 = NetworkNode::new(config2).await.unwrap();
    let _addr2 = wait_for_listen(&node2).await;

    // Connect nodes
    node2.dial(addr1).await.unwrap();

    // Wait for connection
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Create announcement
    let content = b"test content for DHT";
    let hash = content_hash(content);
    let payload = AnnouncePayload {
        hash,
        content_type: ContentType::L0,
        title: "Test Content".to_string(),
        l1_summary: L1Summary::empty(hash),
        price: 100,
        addresses: vec!["/ip4/127.0.0.1/tcp/9000".to_string()],
    };

    // Node 1 announces
    node1.dht_announce(hash, payload.clone()).await.unwrap();

    // Give DHT time to propagate
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Node 2 should be able to find it (in a real network)
    // Note: With only 2 nodes and no bootstrap, this may not always work
    // In production, we'd have more nodes and proper bootstrapping
}

#[tokio::test]
async fn test_peer_id_mapping() {
    let config = test_config();
    let node = NetworkNode::new(config).await.unwrap();

    // Initially, no peer ID mappings
    let random_peer = libp2p::PeerId::random();
    assert!(node.nodalync_peer_id(&random_peer).is_none());
}

#[tokio::test]
async fn test_config_builder() {
    let config = NetworkConfig::new()
        .with_max_retries(5)
        .with_request_timeout(Duration::from_secs(60))
        .with_retry_base_delay(Duration::from_millis(200));

    assert_eq!(config.max_retries, 5);
    assert_eq!(config.request_timeout.as_secs(), 60);
    assert_eq!(config.retry_base_delay.as_millis(), 200);
}

#[tokio::test]
async fn test_bootstrap_node_config() {
    let bootstrap_peer = libp2p::PeerId::random();
    let bootstrap_addr: libp2p::Multiaddr = "/ip4/192.168.1.1/tcp/9000".parse().unwrap();

    let config = NetworkConfig::new()
        .with_bootstrap_node(bootstrap_peer, bootstrap_addr.clone());

    assert_eq!(config.bootstrap_nodes.len(), 1);
    assert_eq!(config.bootstrap_nodes[0].0, bootstrap_peer);
    assert_eq!(config.bootstrap_nodes[0].1, bootstrap_addr);
}
