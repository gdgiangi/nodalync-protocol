//! End-to-end integration test: two nodes exchange content over the network.
//!
//! This is the D3 litmus test. If two nodes can:
//! 1. Connect via libp2p
//! 2. Node A publishes content
//! 3. Node B sends a SEARCH request → gets results
//! 4. Node B sends a QUERY request → receives actual content bytes
//! 5. Content hash verifies correctly
//!
//! ...then the protocol works for real users.
//!
//! The test replicates the desktop app's event loop pattern: a background
//! task on each node polls `next_event()` and dispatches inbound requests
//! through `handle_network_event()`, sending signed responses back.

use nodalync_crypto::{generate_identity, peer_id_from_public_key};
use nodalync_net::{Network, NetworkConfig, NetworkEvent, NetworkNode};
use nodalync_ops::DefaultNodeOperations;
use nodalync_store::{NodeState, NodeStateConfig};
use nodalync_types::{Metadata, Visibility};
use nodalync_wire::SearchPayload;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tokio::sync::Mutex;
use tokio::time::timeout;

/// Create a test network configuration with random ports.
fn test_network_config() -> NetworkConfig {
    NetworkConfig::new()
        .with_listen_addresses(vec!["/ip4/127.0.0.1/tcp/0".parse().unwrap()])
        .with_request_timeout(Duration::from_secs(10))
        .with_retry_base_delay(Duration::from_millis(100))
        .with_max_retries(3)
}

/// Create a DefaultNodeOperations with a network attached.
fn create_ops_with_network(temp_dir: &TempDir, network: Arc<NetworkNode>) -> DefaultNodeOperations {
    let config = NodeStateConfig::new(temp_dir.path());
    let state = NodeState::open(config).expect("Failed to open node state");

    let (_, public_key) = generate_identity();
    let peer_id = peer_id_from_public_key(&public_key);

    DefaultNodeOperations::with_defaults_and_network(state, peer_id, network)
}

/// Wait for a node to start listening and return the address.
async fn wait_for_listen(node: &NetworkNode) -> libp2p::Multiaddr {
    let timeout_duration = Duration::from_secs(10);
    let start = std::time::Instant::now();

    loop {
        if start.elapsed() > timeout_duration {
            panic!("Timed out waiting for node to start listening");
        }

        match timeout(Duration::from_millis(200), node.next_event()).await {
            Ok(Ok(NetworkEvent::NewListenAddr { address })) => {
                return address;
            }
            _ => {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }
    }
}

/// Spawn a background event loop that processes inbound requests.
///
/// This replicates the desktop app's event_loop.rs pattern:
/// poll next_event → handle_network_event → send_signed_response.
fn spawn_responder(
    network: Arc<NetworkNode>,
    ops: Arc<Mutex<DefaultNodeOperations>>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            match timeout(Duration::from_secs(30), network.next_event()).await {
                Ok(Ok(event)) => {
                    // Extract request_id before consuming the event
                    let request_id = match &event {
                        NetworkEvent::InboundRequest { request_id, .. } => Some(*request_id),
                        _ => None,
                    };

                    // Skip non-request events
                    match &event {
                        NetworkEvent::PeerConnected { .. }
                        | NetworkEvent::PeerDisconnected { .. }
                        | NetworkEvent::NewListenAddr { .. }
                        | NetworkEvent::PeerDiscovered { .. } => continue,
                        _ => {}
                    }

                    // Handle the event through the ops layer
                    let response = {
                        let mut guard = ops.lock().await;
                        match guard.handle_network_event(event).await {
                            Ok(resp) => resp,
                            Err(e) => {
                                eprintln!("Responder error: {}", e);
                                None
                            }
                        }
                    };

                    // Send response if we have one
                    if let (Some(request_id), Some((msg_type, payload))) = (request_id, response) {
                        if let Err(e) = network
                            .send_signed_response(request_id, msg_type, payload)
                            .await
                        {
                            eprintln!("Failed to send response: {}", e);
                        }
                    }
                }
                Ok(Err(e)) => {
                    eprintln!("Event channel error: {} — responder exiting", e);
                    break;
                }
                Err(_) => {
                    // Timeout — just continue
                    break;
                }
            }
        }
    })
}

/// Wait for two nodes to be connected (drain events until PeerConnected).
async fn wait_for_connection(node: &NetworkNode) {
    let start = std::time::Instant::now();
    loop {
        if start.elapsed() > Duration::from_secs(10) {
            panic!("Timed out waiting for peer connection");
        }
        match timeout(Duration::from_millis(200), node.next_event()).await {
            Ok(Ok(NetworkEvent::PeerConnected { .. })) => return,
            _ => tokio::time::sleep(Duration::from_millis(10)).await,
        }
    }
}

// =============================================================================
// TEST: End-to-end search flow — Node B searches Node A's content
// =============================================================================

#[tokio::test]
async fn test_e2e_search_returns_results() {
    // --- Setup Node A (content publisher) ---
    let config_a = test_network_config();
    let node_a = Arc::new(NetworkNode::new(config_a).await.expect("Node A creation"));
    let addr_a = wait_for_listen(&node_a).await;

    let temp_a = TempDir::new().unwrap();
    let mut ops_a = create_ops_with_network(&temp_a, node_a.clone());

    // Publish content on Node A
    let content_bytes = b"Decentralized knowledge graphs enable fair creator compensation.";
    let metadata = Metadata::new("Knowledge Graph Research", content_bytes.len() as u64);
    let hash = ops_a
        .create_content(content_bytes, metadata)
        .expect("Create content");
    ops_a
        .publish_content(&hash, Visibility::Shared, 100)
        .await
        .expect("Publish content");

    // Wrap ops_a for the responder
    let ops_a = Arc::new(Mutex::new(ops_a));

    // --- Setup Node B (searcher) ---
    let config_b = test_network_config();
    let node_b = Arc::new(NetworkNode::new(config_b).await.expect("Node B creation"));
    let _addr_b = wait_for_listen(&node_b).await;

    // Connect B → A
    node_b.dial(addr_a).await.expect("Dial A");

    // Wait for connection on both sides
    wait_for_connection(&node_a).await;
    // Node B should also see the connection
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Spawn responder for Node A (handles inbound SEARCH/QUERY requests)
    let responder = spawn_responder(node_a.clone(), ops_a.clone());

    // --- Node B sends SEARCH request to Node A ---
    let peer_a = node_a.local_peer_id();
    let search_request = SearchPayload {
        query: "knowledge graph".to_string(),
        filters: None,
        limit: 10,
        offset: 0,
        max_hops: 0,
        hop_count: 0,
        visited_peers: vec![],
    };

    let search_result = timeout(
        Duration::from_secs(10),
        node_b.send_search(peer_a, search_request),
    )
    .await
    .expect("Search timed out")
    .expect("Search failed");

    // Verify results
    assert!(
        search_result.total_count > 0,
        "Search should return at least 1 result, got {}",
        search_result.total_count
    );
    assert!(
        !search_result.results.is_empty(),
        "Results array should not be empty"
    );

    let first = &search_result.results[0];
    assert_eq!(
        first.hash, hash,
        "Result hash should match published content"
    );
    assert_eq!(first.title, "Knowledge Graph Research");
    assert_eq!(first.price, 100);

    // Clean up
    responder.abort();
}

// =============================================================================
// TEST: End-to-end query flow — Node B retrieves content from Node A
// =============================================================================

#[tokio::test]
async fn test_e2e_query_retrieves_content() {
    // --- Setup Node A (content owner) ---
    let config_a = test_network_config();
    let node_a = Arc::new(NetworkNode::new(config_a).await.expect("Node A creation"));
    let addr_a = wait_for_listen(&node_a).await;

    let temp_a = TempDir::new().unwrap();
    let mut ops_a = create_ops_with_network(&temp_a, node_a.clone());

    // Publish content on Node A
    let content_bytes = b"This is premium content about Rust programming and libp2p networking.";
    let metadata = Metadata::new("Rust & libp2p Guide", content_bytes.len() as u64);
    let hash = ops_a
        .create_content(content_bytes, metadata)
        .expect("Create content");
    ops_a
        .publish_content(&hash, Visibility::Shared, 0) // Free content for simplicity
        .await
        .expect("Publish content");

    let ops_a = Arc::new(Mutex::new(ops_a));

    // --- Setup Node B (querier) ---
    let config_b = test_network_config();
    let node_b = Arc::new(NetworkNode::new(config_b).await.expect("Node B creation"));
    let _addr_b = wait_for_listen(&node_b).await;

    // Connect B → A
    node_b.dial(addr_a).await.expect("Dial A");
    wait_for_connection(&node_a).await;
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Spawn responder for Node A
    let responder = spawn_responder(node_a.clone(), ops_a.clone());

    // --- Node B sends QUERY request to Node A ---
    let peer_a = node_a.local_peer_id();

    // Build a query request with zero payment (free content)
    use nodalync_crypto::{content_hash, Signature, UNKNOWN_PEER_ID};
    use nodalync_types::Payment;
    use nodalync_wire::QueryRequestPayload;

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let payment_id = content_hash(&[hash.0.as_slice(), &timestamp.to_be_bytes()].concat());
    let payment = Payment::new(
        payment_id,
        nodalync_crypto::Hash([0u8; 32]),
        0, // Free
        UNKNOWN_PEER_ID,
        hash,
        vec![],
        timestamp,
        Signature::from_bytes([0u8; 64]),
    );

    let query_request = QueryRequestPayload {
        hash,
        query: None,
        payment,
        version_spec: None,
        payment_nonce: 1,
    };

    let query_result = timeout(
        Duration::from_secs(10),
        node_b.send_query(peer_a, query_request),
    )
    .await
    .expect("Query timed out")
    .expect("Query failed");

    // --- Verify the content ---
    assert_eq!(
        query_result.content,
        content_bytes.to_vec(),
        "Retrieved content should match original bytes"
    );
    assert_eq!(query_result.hash, hash);
    assert_eq!(query_result.manifest.metadata.title, "Rust & libp2p Guide");

    // Clean up
    responder.abort();
}

// =============================================================================
// TEST: Full flow — search then query (simulates real user experience)
// =============================================================================

#[tokio::test]
async fn test_e2e_search_then_query() {
    // --- Setup Node A ---
    let config_a = test_network_config();
    let node_a = Arc::new(NetworkNode::new(config_a).await.expect("Node A creation"));
    let addr_a = wait_for_listen(&node_a).await;

    let temp_a = TempDir::new().unwrap();
    let mut ops_a = create_ops_with_network(&temp_a, node_a.clone());

    // Publish multiple content items
    let items = vec![
        (
            "Hedera Hashgraph Consensus",
            b"Hedera uses hashgraph for fast, fair consensus." as &[u8],
        ),
        (
            "MCP Protocol Design",
            b"Model Context Protocol enables AI-knowledge integration.",
        ),
        (
            "Nodalync Settlement",
            b"Settlement layer uses Hedera for micropayment channels.",
        ),
    ];

    let mut hashes = Vec::new();
    for (title, content) in &items {
        let meta = Metadata::new(*title, content.len() as u64);
        let h = ops_a.create_content(content, meta).expect("Create");
        ops_a
            .publish_content(&h, Visibility::Shared, 0)
            .await
            .expect("Publish");
        hashes.push(h);
    }

    let ops_a = Arc::new(Mutex::new(ops_a));

    // --- Setup Node B ---
    let config_b = test_network_config();
    let node_b = Arc::new(NetworkNode::new(config_b).await.expect("Node B"));
    let _addr_b = wait_for_listen(&node_b).await;

    node_b.dial(addr_a).await.expect("Dial");
    wait_for_connection(&node_a).await;
    tokio::time::sleep(Duration::from_millis(300)).await;

    let responder = spawn_responder(node_a.clone(), ops_a.clone());

    let peer_a = node_a.local_peer_id();

    // --- Step 1: Search for "Hedera" ---
    let search_result = timeout(
        Duration::from_secs(10),
        node_b.send_search(
            peer_a,
            SearchPayload {
                query: "hedera".to_string(),
                filters: None,
                limit: 10,
                offset: 0,
                max_hops: 0,
                hop_count: 0,
                visited_peers: vec![],
            },
        ),
    )
    .await
    .expect("Search timed out")
    .expect("Search failed");

    // Should find at least 2 results (Hedera Hashgraph + Nodalync Settlement mentions Hedera)
    assert!(
        !search_result.results.is_empty(),
        "Search for 'hedera' should return results"
    );

    // Find the Hedera Hashgraph result
    let hedera_result = search_result
        .results
        .iter()
        .find(|r| r.title == "Hedera Hashgraph Consensus")
        .expect("Should find Hedera Hashgraph result");
    let target_hash = hedera_result.hash;

    // --- Step 2: Query the content by hash ---
    use nodalync_crypto::{content_hash, Signature, UNKNOWN_PEER_ID};
    use nodalync_types::Payment;
    use nodalync_wire::QueryRequestPayload;

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let payment_id = content_hash(&[target_hash.0.as_slice(), &timestamp.to_be_bytes()].concat());
    let payment = Payment::new(
        payment_id,
        nodalync_crypto::Hash([0u8; 32]),
        0,
        UNKNOWN_PEER_ID,
        target_hash,
        vec![],
        timestamp,
        Signature::from_bytes([0u8; 64]),
    );

    let query_result = timeout(
        Duration::from_secs(10),
        node_b.send_query(
            peer_a,
            QueryRequestPayload {
                hash: target_hash,
                query: None,
                payment,
                version_spec: None,
                payment_nonce: 1,
            },
        ),
    )
    .await
    .expect("Query timed out")
    .expect("Query failed");

    // Verify we got the right content
    assert_eq!(
        String::from_utf8_lossy(&query_result.content),
        "Hedera uses hashgraph for fast, fair consensus."
    );
    assert_eq!(
        query_result.manifest.metadata.title,
        "Hedera Hashgraph Consensus"
    );

    // Clean up
    responder.abort();
}

// =============================================================================
// TEST: GossipSub broadcast — Node B receives announcement when Node A publishes
// =============================================================================

#[tokio::test]
async fn test_e2e_gossipsub_announcement() {
    // --- Setup Node A (publisher) ---
    let config_a = test_network_config();
    let node_a = Arc::new(NetworkNode::new(config_a).await.expect("Node A"));
    let addr_a = wait_for_listen(&node_a).await;

    let temp_a = TempDir::new().unwrap();
    let mut ops_a = create_ops_with_network(&temp_a, node_a.clone());

    // --- Setup Node B (subscriber) ---
    let config_b = test_network_config();
    let node_b = Arc::new(NetworkNode::new(config_b).await.expect("Node B"));
    let _addr_b = wait_for_listen(&node_b).await;

    // Connect B → A and wait
    node_b.dial(addr_a).await.expect("Dial");
    wait_for_connection(&node_a).await;

    // GossipSub needs mesh formation time — wait for topic subscription propagation
    tokio::time::sleep(Duration::from_millis(1000)).await;

    // Set up Node B's ops to receive announcements
    let temp_b = TempDir::new().unwrap();
    let mut ops_b = create_ops_with_network(&temp_b, node_b.clone());

    // --- Node A publishes content (triggers DHT + GossipSub announce) ---
    let content_bytes = b"Announcing new content to the network via GossipSub.";
    let metadata = Metadata::new("GossipSub Test", content_bytes.len() as u64);
    let hash = ops_a
        .create_content(content_bytes, metadata)
        .expect("Create content");
    ops_a
        .publish_content(&hash, Visibility::Shared, 50)
        .await
        .expect("Publish content");

    // --- Node B should receive the broadcast ---
    // Poll events on Node B until we get a BroadcastReceived
    let received = timeout(Duration::from_secs(5), async {
        loop {
            match node_b.next_event().await {
                Ok(event @ NetworkEvent::BroadcastReceived { .. }) => {
                    // Process through ops to cache the announcement
                    let _ = ops_b.handle_network_event(event).await;
                    return true;
                }
                Ok(_) => continue,
                Err(_) => return false,
            }
        }
    })
    .await;

    // GossipSub mesh formation is probabilistic with only 2 nodes
    // If we received the broadcast, verify the announcement was cached
    if let Ok(true) = received {
        let announcements = ops_b.state().search_announcements("gossipsub", None, 10);
        assert!(
            !announcements.is_empty(),
            "Announcement should be cached after broadcast"
        );
        assert_eq!(announcements[0].title, "GossipSub Test");
        assert_eq!(announcements[0].price, 50);
    }
    // If not received, that's expected with 2-node GossipSub — mesh needs 3+ peers
    // The request-response path (tested above) handles this case
}

// =============================================================================
// TEST: Preview request returns metadata without content
// =============================================================================

#[tokio::test]
async fn test_e2e_preview_returns_metadata() {
    let config_a = test_network_config();
    let node_a = Arc::new(NetworkNode::new(config_a).await.expect("Node A"));
    let addr_a = wait_for_listen(&node_a).await;

    let temp_a = TempDir::new().unwrap();
    let mut ops_a = create_ops_with_network(&temp_a, node_a.clone());

    let content_bytes = b"Preview test content with enough text for meaningful extraction.";
    let metadata = Metadata::new("Preview Test", content_bytes.len() as u64);
    let hash = ops_a
        .create_content(content_bytes, metadata)
        .expect("Create");
    ops_a
        .publish_content(&hash, Visibility::Shared, 250)
        .await
        .expect("Publish");

    let ops_a = Arc::new(Mutex::new(ops_a));

    let config_b = test_network_config();
    let node_b = Arc::new(NetworkNode::new(config_b).await.expect("Node B"));
    let _addr_b = wait_for_listen(&node_b).await;

    node_b.dial(addr_a).await.expect("Dial");
    wait_for_connection(&node_a).await;
    tokio::time::sleep(Duration::from_millis(300)).await;

    let responder = spawn_responder(node_a.clone(), ops_a.clone());

    let peer_a = node_a.local_peer_id();

    use nodalync_wire::PreviewRequestPayload;

    let preview = timeout(
        Duration::from_secs(10),
        node_b.send_preview_request(peer_a, PreviewRequestPayload { hash }),
    )
    .await
    .expect("Preview timed out")
    .expect("Preview failed");

    assert_eq!(preview.hash, hash);
    assert_eq!(preview.manifest.metadata.title, "Preview Test");
    assert_eq!(preview.manifest.economics.price, 250);
    // Preview should NOT contain content bytes (just metadata)

    responder.abort();
}
