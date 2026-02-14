//! Network event loop for Nodalync Studio.
//!
//! Spawns a background tokio task that processes inbound network events
//! (SEARCH, PREVIEW, QUERY requests from peers) and sends signed responses.
//! Without this, the desktop node is deaf — it can send requests but never
//! responds to them, breaking two-way content discovery.

use std::sync::Arc;

use nodalync_net::{Network, NetworkEvent, NetworkNode, PeerId as Libp2pPeerId};
use nodalync_wire::MessageType;
use tokio::sync::{watch, Mutex};
use tracing::{debug, error, info, warn};

use crate::protocol::ProtocolState;

/// Handle returned by [`spawn_event_loop`] to control the background task.
pub struct EventLoopHandle {
    /// Send `true` to shut down the event loop.
    shutdown_tx: watch::Sender<bool>,
    /// Join handle for the spawned task.
    join_handle: tokio::task::JoinHandle<()>,
}

impl EventLoopHandle {
    /// Signal the event loop to stop and wait for it to finish.
    pub async fn shutdown(self) {
        let _ = self.shutdown_tx.send(true);
        // Give the loop a moment to notice the signal and exit
        let _ = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            self.join_handle,
        )
        .await;
    }

    /// Signal the event loop to stop (non-blocking).
    pub fn shutdown_signal(&self) {
        let _ = self.shutdown_tx.send(true);
    }
}

/// Spawn the network event loop as a background tokio task.
///
/// The loop polls `network.next_event()`, dispatches inbound requests
/// through `ProtocolState.ops.handle_network_event()`, and sends
/// signed responses back via the network.
///
/// # Arguments
/// * `network` - Cloned `Arc<NetworkNode>` for event polling and responses.
/// * `protocol` - Shared protocol state (same `Mutex` used by Tauri commands).
///
/// # Returns
/// An [`EventLoopHandle`] that must be used to shut down the loop when the
/// network is stopped.
pub fn spawn_event_loop(
    network: Arc<NetworkNode>,
    protocol: Arc<Mutex<Option<ProtocolState>>>,
) -> EventLoopHandle {
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    let join_handle = tokio::spawn(run_event_loop(network, protocol, shutdown_rx));

    info!("Network event loop spawned");

    EventLoopHandle {
        shutdown_tx,
        join_handle,
    }
}

/// The actual event loop — runs until shutdown is signalled or the network
/// channel closes.
async fn run_event_loop(
    network: Arc<NetworkNode>,
    protocol: Arc<Mutex<Option<ProtocolState>>>,
    mut shutdown_rx: watch::Receiver<bool>,
) {
    info!("Network event loop started");

    loop {
        tokio::select! {
            // Check for shutdown signal
            _ = shutdown_rx.changed() => {
                if *shutdown_rx.borrow() {
                    info!("Network event loop shutting down (signal received)");
                    break;
                }
            }

            // Poll for the next network event
            event_result = network.next_event() => {
                match event_result {
                    Ok(event) => {
                        handle_event(&network, &protocol, event).await;
                    }
                    Err(e) => {
                        error!("Network event channel error: {} — event loop exiting", e);
                        break;
                    }
                }
            }
        }
    }

    info!("Network event loop stopped");
}

/// Handle a single network event.
///
/// For inbound requests, this acquires the protocol mutex, calls
/// `handle_network_event`, releases the mutex, then sends the response.
/// The mutex is held only during the handler call — not during the
/// network I/O — to avoid blocking Tauri commands.
async fn handle_event(
    network: &Arc<NetworkNode>,
    protocol: &Arc<Mutex<Option<ProtocolState>>>,
    event: NetworkEvent,
) {
    // Extract request_id before consuming the event
    let request_id = match &event {
        NetworkEvent::InboundRequest { request_id, .. } => Some(*request_id),
        _ => None,
    };

    // Log peer events and handle handshakes
    match &event {
        NetworkEvent::PeerConnected { peer } => {
            info!("Peer connected: {} — initiating handshake", peer);
            initiate_handshake(network, protocol, *peer).await;
            return;
        }
        NetworkEvent::PeerDisconnected { peer } => {
            info!("Peer disconnected: {}", peer);
            return; // No response needed
        }
        NetworkEvent::BroadcastReceived { topic, data } => {
            debug!("Broadcast received on topic '{}': {} bytes", topic, data.len());
            // Broadcasts need to go through handle_network_event for announcement caching
        }
        NetworkEvent::NewListenAddr { address } => {
            info!("New listen address: {}", address);
            return;
        }
        NetworkEvent::InboundRequest { peer, data, .. } => {
            debug!("Inbound request from {}: {} bytes", peer, data.len());
        }
        _ => {}
    }

    // Acquire the protocol mutex and handle the event
    let response = {
        let mut guard = protocol.lock().await;
        match guard.as_mut() {
            Some(state) => {
                match state.ops.handle_network_event(event).await {
                    Ok(resp) => resp,
                    Err(e) => {
                        warn!("Error handling network event: {}", e);
                        None
                    }
                }
            }
            None => {
                debug!("Protocol not initialized — ignoring network event");
                None
            }
        }
    };
    // Mutex released here ^

    // If there's a response and we have a request_id, send it back
    if let (Some(request_id), Some((msg_type, payload))) = (request_id, response) {
        if let Err(e) = network
            .send_signed_response(request_id, msg_type, payload)
            .await
        {
            warn!("Failed to send response for request {:?}: {}", request_id, e);
        } else {
            debug!("Sent response for request {:?}", request_id);
        }
    }
}

/// Initiate the protocol handshake with a newly connected peer.
///
/// Builds a PeerInfoPayload from our current state and sends it as
/// a request-response message. The peer should respond with their own
/// PeerInfoPayload, which gets processed by handle_network_event
/// when it arrives as a PeerInfoResponse.
///
/// This is fire-and-forget from the event loop's perspective — the
/// response is handled asynchronously when it arrives.
async fn initiate_handshake(
    network: &Arc<NetworkNode>,
    protocol: &Arc<Mutex<Option<ProtocolState>>>,
    peer: Libp2pPeerId,
) {
    // Build our PeerInfo message under the lock, then release
    let message = {
        let guard = protocol.lock().await;
        match guard.as_ref() {
            Some(state) => {
                let payload = state.ops.build_peer_info_payload();
                let payload_bytes = match nodalync_wire::encode_payload(&payload) {
                    Ok(bytes) => bytes,
                    Err(e) => {
                        warn!("Failed to encode PeerInfo payload: {}", e);
                        return;
                    }
                };

                let private_key = match state.ops.private_key() {
                    Some(pk) => pk.clone(),
                    None => {
                        debug!("No private key — skipping handshake with {}", peer);
                        return;
                    }
                };

                let timestamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;

                nodalync_wire::create_message(
                    MessageType::PeerInfo,
                    payload_bytes,
                    state.ops.peer_id(),
                    timestamp,
                    &private_key,
                )
            }
            None => {
                debug!("Protocol not initialized — skipping handshake with {}", peer);
                return;
            }
        }
    };
    // Mutex released here

    // Send the handshake as a request-response message
    // The peer's handler will respond with their PeerInfo
    info!("Sending handshake to peer {}", peer);
    match network.send(peer, message).await {
        Ok(response) => {
            debug!("Received handshake response from {}", peer);
            if response.message_type == MessageType::PeerInfoResponse {
                match nodalync_wire::decode_payload::<nodalync_wire::PeerInfoPayload>(
                    &response.payload,
                ) {
                    Ok(peer_info) => {
                        // Register the peer mapping on the network
                        network.register_peer_mapping(peer, peer_info.peer_id);

                        // Store peer info in the ops state
                        let mut guard = protocol.lock().await;
                        if let Some(state) = guard.as_mut() {
                            let _ = state.ops.handle_peer_info(&peer_info);
                        }
                        info!(
                            "Handshake complete with {} (nodalync: {}, version: {})",
                            peer, peer_info.peer_id, peer_info.protocol_version
                        );
                    }
                    Err(e) => {
                        warn!("Failed to decode handshake response from {}: {}", peer, e);
                    }
                }
            } else {
                debug!(
                    "Unexpected response type from handshake with {}: {:?}",
                    peer, response.message_type
                );
            }
        }
        Err(e) => {
            // Non-fatal: peer may not support handshake (older version)
            debug!("Handshake with {} failed (may be older node): {}", peer, e);
        }
    }
}
