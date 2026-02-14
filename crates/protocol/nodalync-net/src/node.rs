//! Network node implementation.
//!
//! This module implements the `NetworkNode` struct which provides
//! the concrete implementation of the `Network` trait.

use crate::behaviour::{NodalyncBehaviour, NodalyncBehaviourEvent};
use crate::codec::{NodalyncRequest, NodalyncResponse};
use crate::config::{NatTraversal, NetworkConfig};
use crate::error::{NetworkError, NetworkResult};
use crate::event::{NatStatus, NetworkEvent};
use crate::peer_id::PeerIdMapper;
use crate::traits::Network;
use crate::transport::{build_transport, build_transport_with_relay};

use async_trait::async_trait;
use futures::StreamExt;
use libp2p::{
    gossipsub::IdentTopic,
    kad::{self, QueryResult, RecordKey},
    mdns,
    request_response::{self, OutboundRequestId, ResponseChannel},
    swarm::{dial_opts::DialOpts, SwarmEvent},
    Multiaddr, PeerId, Swarm,
};
use nodalync_crypto::{
    generate_identity, peer_id_from_public_key, Hash, PeerId as NodalyncPeerId, PrivateKey,
};
use nodalync_wire::{
    create_message, decode_message, decode_payload, encode_message, encode_payload,
    AnnouncePayload, AnnounceUpdatePayload, ChannelClosePayload, ChannelOpenPayload, Message,
    MessageType, PreviewRequestPayload, PreviewResponsePayload, QueryErrorPayload,
    QueryRequestPayload, QueryResponsePayload, SearchPayload, SearchResponsePayload,
    SettleConfirmPayload,
};
use std::collections::HashMap;
use std::sync::{Arc, RwLock as StdRwLock};
use tokio::sync::{mpsc, oneshot, Mutex, RwLock};
use tracing::{debug, info, warn};

/// Shared state passed to the swarm event loop.
///
/// Groups related parameters to avoid too many function arguments.
struct SwarmContext {
    pending_requests: PendingRequests,
    peer_mapper: PeerIdMapper,
    connected_peers: Arc<StdRwLock<std::collections::HashSet<PeerId>>>,
    listen_addrs: Arc<StdRwLock<Vec<Multiaddr>>>,
    gossip_topic: String,
    nat_status: Arc<StdRwLock<NatStatus>>,
}

/// Commands sent to the swarm task.
#[allow(dead_code)]
enum SwarmCommand {
    /// Dial a multiaddress.
    Dial {
        addr: Multiaddr,
        response: oneshot::Sender<NetworkResult<()>>,
    },

    /// Dial a peer by ID.
    DialPeer {
        peer: PeerId,
        response: oneshot::Sender<NetworkResult<()>>,
    },

    /// Send a request-response message.
    SendRequest {
        peer: PeerId,
        data: Vec<u8>,
        response: oneshot::Sender<NetworkResult<Vec<u8>>>,
    },

    /// Put a record in the DHT.
    DhtPut {
        key: Vec<u8>,
        value: Vec<u8>,
        response: oneshot::Sender<NetworkResult<()>>,
    },

    /// Get a record from the DHT.
    DhtGet {
        key: Vec<u8>,
        response: oneshot::Sender<NetworkResult<Option<Vec<u8>>>>,
    },

    /// Remove a record from the DHT.
    DhtRemove {
        key: Vec<u8>,
        response: oneshot::Sender<NetworkResult<()>>,
    },

    /// Publish a GossipSub message.
    GossipPublish {
        topic: String,
        data: Vec<u8>,
        response: oneshot::Sender<NetworkResult<()>>,
    },

    /// Subscribe to a GossipSub topic.
    GossipSubscribe {
        topic: String,
        response: oneshot::Sender<NetworkResult<()>>,
    },

    /// Get connected peers.
    GetConnectedPeers {
        response: oneshot::Sender<Vec<PeerId>>,
    },

    /// Get listen addresses.
    GetListenAddresses {
        response: oneshot::Sender<Vec<Multiaddr>>,
    },

    /// Add an address for a peer to the DHT routing table.
    AddAddress { peer: PeerId, addr: Multiaddr },

    /// Bootstrap the DHT.
    Bootstrap {
        response: oneshot::Sender<NetworkResult<()>>,
    },

    /// Respond to an inbound request.
    SendResponse {
        request_id: libp2p::request_response::InboundRequestId,
        data: Vec<u8>,
    },
}

/// Type alias for pending request map to reduce type complexity.
type PendingRequests =
    Arc<RwLock<HashMap<OutboundRequestId, oneshot::Sender<NetworkResult<Vec<u8>>>>>>;

/// A P2P network node.
///
/// This struct manages the libp2p swarm and provides the `Network` trait
/// implementation for interacting with the P2P network.
pub struct NetworkNode {
    /// The local libp2p peer ID.
    local_peer_id: PeerId,

    /// The local Nodalync peer ID.
    nodalync_peer_id: NodalyncPeerId,

    /// The private key for signing messages.
    private_key: PrivateKey,

    /// Peer ID mapper for libp2p <-> nodalync conversion.
    peer_mapper: PeerIdMapper,

    /// Set of currently connected libp2p peers.
    connected_peers_set: Arc<StdRwLock<std::collections::HashSet<PeerId>>>,

    /// Set of listen addresses (updated when swarm reports new listen addrs).
    listen_addrs: Arc<StdRwLock<Vec<Multiaddr>>>,

    /// Channel for sending commands to the swarm task.
    command_tx: mpsc::Sender<SwarmCommand>,

    /// Channel for receiving events from the swarm task.
    event_rx: Arc<Mutex<mpsc::Receiver<NetworkEvent>>>,

    /// Pending request-response operations (used by the swarm task, not read from struct).
    #[allow(dead_code)]
    pending_requests: PendingRequests,

    /// Network configuration.
    config: NetworkConfig,

    /// GossipSub topic for announcements.
    #[allow(dead_code)]
    announce_topic: IdentTopic,

    /// Current NAT status (updated by AutoNAT).
    nat_status: Arc<StdRwLock<NatStatus>>,
}

impl NetworkNode {
    /// Create a new network node.
    ///
    /// This starts the swarm task in the background.
    /// When NAT traversal is enabled (default), the node will automatically:
    /// - Use UPnP to map ports on the router
    /// - Detect NAT status via AutoNAT probes
    /// - Use relay nodes for inbound connections if behind NAT
    /// - Attempt DCUtR hole-punching to upgrade relayed connections
    pub async fn new(config: NetworkConfig) -> NetworkResult<Self> {
        // Derive identity: use provided secret for stable PeerId, or generate random
        let (private_key, public_key, keypair) = if let Some(secret) = &config.identity_secret {
            // Derive libp2p keypair from the Nodalync identity secret
            let kp = libp2p::identity::Keypair::ed25519_from_bytes(*secret).map_err(|e| {
                NetworkError::Transport(format!(
                    "Failed to derive libp2p keypair from identity secret: {}",
                    e
                ))
            })?;

            // Also derive the Nodalync keys from the same seed
            let signing_key = ed25519_dalek::SigningKey::from_bytes(secret);
            let verifying_key = signing_key.verifying_key();
            let priv_key = nodalync_crypto::PrivateKey::from_bytes(*secret);
            let pub_key = nodalync_crypto::PublicKey(verifying_key.to_bytes());

            (priv_key, pub_key, kp)
        } else {
            // Generate random identity (tests, ephemeral nodes)
            let (priv_key, pub_key) = generate_identity();
            let kp = libp2p::identity::Keypair::generate_ed25519();
            (priv_key, pub_key, kp)
        };

        let nodalync_peer_id = peer_id_from_public_key(&public_key);
        let local_peer_id = keypair.public().to_peer_id();

        info!("Creating network node with peer ID: {}", local_peer_id);

        // Build transport + behaviour based on NAT traversal config
        let use_relay = matches!(
            config.nat_traversal,
            NatTraversal::Full | NatTraversal::RelayOnly
        );

        let (transport, behaviour) = if use_relay {
            let (transport, relay_behaviour) =
                build_transport_with_relay(&keypair, config.idle_connection_timeout);
            let behaviour = NodalyncBehaviour::with_keypair_and_relay(
                local_peer_id,
                &keypair,
                &config,
                relay_behaviour,
            );
            (transport, behaviour)
        } else {
            let transport = build_transport(&keypair, config.idle_connection_timeout);
            let behaviour = NodalyncBehaviour::with_keypair(local_peer_id, &keypair, &config);
            (transport, behaviour)
        };

        // Build swarm
        let swarm_config = libp2p::swarm::Config::with_tokio_executor()
            .with_idle_connection_timeout(config.idle_connection_timeout);
        let mut swarm = Swarm::new(transport, behaviour, local_peer_id, swarm_config);

        // Start listening
        for addr in &config.listen_addresses {
            swarm.listen_on(addr.clone()).map_err(|e| {
                NetworkError::Transport(format!("failed to listen on {}: {}", addr, e))
            })?;
        }

        // Create channels
        let (command_tx, command_rx) = mpsc::channel(256);
        let (event_tx, event_rx) = mpsc::channel(256);

        // Create pending requests map
        let pending_requests: PendingRequests = Arc::new(RwLock::new(HashMap::new()));

        // Create peer mapper
        let peer_mapper = PeerIdMapper::new();

        // Clone for the swarm task
        let pending_requests_clone = pending_requests.clone();
        let peer_mapper_clone = peer_mapper.clone();
        let gossip_topic = config.gossipsub_topic.clone();
        let connected_peers_set = Arc::new(StdRwLock::new(std::collections::HashSet::new()));
        let connected_peers_clone = connected_peers_set.clone();
        let listen_addrs = Arc::new(StdRwLock::new(Vec::new()));
        let listen_addrs_clone = listen_addrs.clone();
        let nat_status = Arc::new(StdRwLock::new(NatStatus::Unknown));
        let nat_status_clone = nat_status.clone();

        // Subscribe to the announcement topic
        let announce_topic = IdentTopic::new(&config.gossipsub_topic);

        // Spawn the swarm task
        let swarm_ctx = SwarmContext {
            pending_requests: pending_requests_clone,
            peer_mapper: peer_mapper_clone,
            connected_peers: connected_peers_clone,
            listen_addrs: listen_addrs_clone,
            gossip_topic,
            nat_status: nat_status_clone,
        };
        tokio::spawn(async move {
            run_swarm(swarm, command_rx, event_tx, swarm_ctx).await;
        });

        Ok(Self {
            local_peer_id,
            nodalync_peer_id,
            private_key,
            peer_mapper,
            connected_peers_set,
            listen_addrs,
            command_tx,
            event_rx: Arc::new(Mutex::new(event_rx)),
            pending_requests,
            config,
            announce_topic,
            nat_status,
        })
    }

    /// Create a network node with a specific keypair.
    pub async fn with_keypair(
        private_key: PrivateKey,
        public_key: nodalync_crypto::PublicKey,
        keypair: libp2p::identity::Keypair,
        config: NetworkConfig,
    ) -> NetworkResult<Self> {
        let nodalync_peer_id = peer_id_from_public_key(&public_key);
        let local_peer_id = keypair.public().to_peer_id();

        info!("Creating network node with peer ID: {}", local_peer_id);

        // Build transport + behaviour based on NAT traversal config
        let use_relay = matches!(
            config.nat_traversal,
            NatTraversal::Full | NatTraversal::RelayOnly
        );

        let (transport, behaviour) = if use_relay {
            let (transport, relay_behaviour) =
                build_transport_with_relay(&keypair, config.idle_connection_timeout);
            let behaviour = NodalyncBehaviour::with_keypair_and_relay(
                local_peer_id,
                &keypair,
                &config,
                relay_behaviour,
            );
            (transport, behaviour)
        } else {
            let transport = build_transport(&keypair, config.idle_connection_timeout);
            let behaviour = NodalyncBehaviour::with_keypair(local_peer_id, &keypair, &config);
            (transport, behaviour)
        };

        // Build swarm
        let swarm_config = libp2p::swarm::Config::with_tokio_executor()
            .with_idle_connection_timeout(config.idle_connection_timeout);
        let mut swarm = Swarm::new(transport, behaviour, local_peer_id, swarm_config);

        // Start listening
        for addr in &config.listen_addresses {
            swarm.listen_on(addr.clone()).map_err(|e| {
                NetworkError::Transport(format!("failed to listen on {}: {}", addr, e))
            })?;
        }

        // Create channels
        let (command_tx, command_rx) = mpsc::channel(256);
        let (event_tx, event_rx) = mpsc::channel(256);

        // Create pending requests map
        let pending_requests: PendingRequests = Arc::new(RwLock::new(HashMap::new()));

        // Create peer mapper
        let peer_mapper = PeerIdMapper::new();

        // Clone for the swarm task
        let pending_requests_clone = pending_requests.clone();
        let peer_mapper_clone = peer_mapper.clone();
        let gossip_topic = config.gossipsub_topic.clone();
        let connected_peers_set = Arc::new(StdRwLock::new(std::collections::HashSet::new()));
        let connected_peers_clone = connected_peers_set.clone();
        let listen_addrs = Arc::new(StdRwLock::new(Vec::new()));
        let listen_addrs_clone = listen_addrs.clone();
        let nat_status = Arc::new(StdRwLock::new(NatStatus::Unknown));
        let nat_status_clone = nat_status.clone();

        let announce_topic = IdentTopic::new(&config.gossipsub_topic);

        // Spawn the swarm task
        let swarm_ctx = SwarmContext {
            pending_requests: pending_requests_clone,
            peer_mapper: peer_mapper_clone,
            connected_peers: connected_peers_clone,
            listen_addrs: listen_addrs_clone,
            gossip_topic,
            nat_status: nat_status_clone,
        };
        tokio::spawn(async move {
            run_swarm(swarm, command_rx, event_tx, swarm_ctx).await;
        });

        Ok(Self {
            local_peer_id,
            nodalync_peer_id,
            private_key,
            peer_mapper,
            connected_peers_set,
            listen_addrs,
            command_tx,
            event_rx: Arc::new(Mutex::new(event_rx)),
            pending_requests,
            config,
            announce_topic,
            nat_status,
        })
    }

    /// Bootstrap the node by connecting to bootstrap peers.
    /// If no bootstrap nodes are configured, this succeeds immediately (first node in network).
    pub async fn bootstrap(&self) -> NetworkResult<()> {
        // If no bootstrap nodes, we're the first node - nothing to do
        if self.config.bootstrap_nodes.is_empty() {
            tracing::info!("No bootstrap nodes configured - starting as first node in network");
            return Ok(());
        }

        tracing::info!(
            "Bootstrapping with {} node(s)",
            self.config.bootstrap_nodes.len()
        );

        // Add bootstrap nodes to the routing table AND dial them
        for (peer_id, addr) in &self.config.bootstrap_nodes {
            tracing::info!("Adding bootstrap node {} at {}", peer_id, addr);

            // Add address to Kademlia routing table
            self.command_tx
                .send(SwarmCommand::AddAddress {
                    peer: *peer_id,
                    addr: addr.clone(),
                })
                .await
                .map_err(|_| NetworkError::ChannelClosed)?;

            // Actually dial the bootstrap node
            let (tx, rx) = oneshot::channel();
            self.command_tx
                .send(SwarmCommand::Dial {
                    addr: addr.clone(),
                    response: tx,
                })
                .await
                .map_err(|_| NetworkError::ChannelClosed)?;

            // Wait for dial to complete (or fail)
            match rx.await {
                Ok(Ok(())) => {
                    tracing::info!("Successfully dialed bootstrap node {}", peer_id);
                }
                Ok(Err(e)) => {
                    tracing::warn!("Failed to dial bootstrap node {}: {}", peer_id, e);
                }
                Err(_) => {
                    tracing::warn!("Dial channel closed for bootstrap node {}", peer_id);
                }
            }
        }

        // Wait for connections to establish and routing table to populate
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        // Trigger Kademlia bootstrap to find closest peers
        let (tx, rx) = oneshot::channel();
        self.command_tx
            .send(SwarmCommand::Bootstrap { response: tx })
            .await
            .map_err(|_| NetworkError::ChannelClosed)?;

        // Wait for bootstrap query to complete
        let bootstrap_result = rx.await.map_err(|_| NetworkError::ChannelClosed)?;

        // Give more time for routing table to populate after bootstrap
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;

        tracing::info!(
            "Bootstrap complete, connected peers: {}",
            self.connected_peers().len()
        );

        bootstrap_result
    }

    /// Subscribe to the announcement topic.
    pub async fn subscribe_announcements(&self) -> NetworkResult<()> {
        let (tx, rx) = oneshot::channel();
        self.command_tx
            .send(SwarmCommand::GossipSubscribe {
                topic: self.config.gossipsub_topic.clone(),
                response: tx,
            })
            .await
            .map_err(|_| NetworkError::ChannelClosed)?;

        rx.await.map_err(|_| NetworkError::ChannelClosed)?
    }

    /// Get the current NAT status as detected by AutoNAT.
    pub fn nat_status(&self) -> NatStatus {
        self.nat_status
            .read()
            .map(|s| s.clone())
            .unwrap_or(NatStatus::Unknown)
    }

    /// Send a request with retry logic.
    async fn send_with_retry(&self, peer: PeerId, data: Vec<u8>) -> NetworkResult<Vec<u8>> {
        let mut last_error = None;

        for attempt in 0..self.config.max_retries {
            if attempt > 0 {
                // Exponential backoff
                let delay = self.config.retry_base_delay * (1 << attempt);
                tokio::time::sleep(delay).await;
            }

            let (tx, rx) = oneshot::channel();
            self.command_tx
                .send(SwarmCommand::SendRequest {
                    peer,
                    data: data.clone(),
                    response: tx,
                })
                .await
                .map_err(|_| NetworkError::ChannelClosed)?;

            match rx.await {
                Ok(Ok(response)) => return Ok(response),
                Ok(Err(e)) => {
                    warn!("Request attempt {} failed: {}", attempt + 1, e);
                    last_error = Some(e);
                }
                Err(_) => {
                    last_error = Some(NetworkError::ChannelClosed);
                }
            }
        }

        Err(last_error.unwrap_or(NetworkError::MaxRetriesExceeded {
            attempts: self.config.max_retries,
        }))
    }

    /// Create a signed message.
    fn create_signed_message(&self, message_type: MessageType, payload: Vec<u8>) -> Message {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        create_message(
            message_type,
            payload,
            self.nodalync_peer_id,
            timestamp,
            &self.private_key,
        )
    }
}

#[async_trait]
impl Network for NetworkNode {
    async fn dht_announce(&self, hash: Hash, payload: AnnouncePayload) -> NetworkResult<()> {
        let key = hash.0.to_vec();
        let value = encode_payload(&payload).map_err(|e| NetworkError::Encoding(e.to_string()))?;

        let (tx, rx) = oneshot::channel();
        self.command_tx
            .send(SwarmCommand::DhtPut {
                key,
                value,
                response: tx,
            })
            .await
            .map_err(|_| NetworkError::ChannelClosed)?;

        rx.await.map_err(|_| NetworkError::ChannelClosed)?
    }

    async fn dht_get(&self, hash: &Hash) -> NetworkResult<Option<AnnouncePayload>> {
        let key = hash.0.to_vec();

        let (tx, rx) = oneshot::channel();
        self.command_tx
            .send(SwarmCommand::DhtGet { key, response: tx })
            .await
            .map_err(|_| NetworkError::ChannelClosed)?;

        let result = rx.await.map_err(|_| NetworkError::ChannelClosed)??;

        match result {
            Some(data) => {
                let payload: AnnouncePayload =
                    decode_payload(&data).map_err(|e| NetworkError::Decoding(e.to_string()))?;
                Ok(Some(payload))
            }
            None => Ok(None),
        }
    }

    async fn dht_remove(&self, hash: &Hash) -> NetworkResult<()> {
        let key = hash.0.to_vec();

        let (tx, rx) = oneshot::channel();
        self.command_tx
            .send(SwarmCommand::DhtRemove { key, response: tx })
            .await
            .map_err(|_| NetworkError::ChannelClosed)?;

        rx.await.map_err(|_| NetworkError::ChannelClosed)?
    }

    async fn send(&self, peer: PeerId, message: Message) -> NetworkResult<Message> {
        let data = encode_message(&message).map_err(|e| NetworkError::Encoding(e.to_string()))?;
        let response_data = self.send_with_retry(peer, data).await?;
        let response =
            decode_message(&response_data).map_err(|e| NetworkError::Decoding(e.to_string()))?;
        Ok(response)
    }

    async fn broadcast(&self, message: Message) -> NetworkResult<()> {
        let data = encode_message(&message).map_err(|e| NetworkError::Encoding(e.to_string()))?;

        let (tx, rx) = oneshot::channel();
        self.command_tx
            .send(SwarmCommand::GossipPublish {
                topic: self.config.gossipsub_topic.clone(),
                data,
                response: tx,
            })
            .await
            .map_err(|_| NetworkError::ChannelClosed)?;

        rx.await.map_err(|_| NetworkError::ChannelClosed)?
    }

    async fn send_preview_request(
        &self,
        peer: PeerId,
        request: PreviewRequestPayload,
    ) -> NetworkResult<PreviewResponsePayload> {
        let payload =
            encode_payload(&request).map_err(|e| NetworkError::Encoding(e.to_string()))?;
        let message = self.create_signed_message(MessageType::PreviewRequest, payload);

        let response = self.send(peer, message).await?;

        if response.message_type != MessageType::PreviewResponse {
            return Err(NetworkError::InvalidResponseType {
                expected: "PreviewResponse".to_string(),
                got: format!("{:?}", response.message_type),
            });
        }

        decode_payload(&response.payload).map_err(|e| NetworkError::Decoding(e.to_string()))
    }

    async fn send_query(
        &self,
        peer: PeerId,
        request: QueryRequestPayload,
    ) -> NetworkResult<QueryResponsePayload> {
        let payload =
            encode_payload(&request).map_err(|e| NetworkError::Encoding(e.to_string()))?;
        let message = self.create_signed_message(MessageType::QueryRequest, payload);

        let response = self.send(peer, message).await?;

        match response.message_type {
            MessageType::QueryResponse => {
                decode_payload(&response.payload).map_err(|e| NetworkError::Decoding(e.to_string()))
            }
            MessageType::QueryError => {
                // Parse the error payload and return appropriate error
                let error_payload: QueryErrorPayload = decode_payload(&response.payload)
                    .map_err(|e| NetworkError::Decoding(e.to_string()))?;

                // Check if this is a ChannelRequired error with peer info
                if error_payload.error_code == nodalync_types::ErrorCode::ChannelNotFound
                    && (error_payload.required_channel_peer_id.is_some()
                        || error_payload.required_channel_libp2p_peer.is_some())
                {
                    return Err(NetworkError::ChannelRequired {
                        nodalync_peer_id: error_payload.required_channel_peer_id.map(|p| p.0),
                        libp2p_peer_id: error_payload.required_channel_libp2p_peer,
                    });
                }

                // Return generic query error
                Err(NetworkError::QueryError {
                    code: error_payload.error_code,
                    message: error_payload
                        .message
                        .unwrap_or_else(|| "Unknown error".to_string()),
                })
            }
            _ => Err(NetworkError::InvalidResponseType {
                expected: "QueryResponse or QueryError".to_string(),
                got: format!("{:?}", response.message_type),
            }),
        }
    }

    async fn send_search(
        &self,
        peer: PeerId,
        request: SearchPayload,
    ) -> NetworkResult<SearchResponsePayload> {
        let payload =
            encode_payload(&request).map_err(|e| NetworkError::Encoding(e.to_string()))?;
        let message = self.create_signed_message(MessageType::Search, payload);

        let response = self.send(peer, message).await?;

        if response.message_type != MessageType::SearchResponse {
            return Err(NetworkError::InvalidResponseType {
                expected: "SearchResponse".to_string(),
                got: format!("{:?}", response.message_type),
            });
        }

        decode_payload(&response.payload).map_err(|e| NetworkError::Decoding(e.to_string()))
    }

    async fn send_channel_open(
        &self,
        peer: PeerId,
        payload: ChannelOpenPayload,
    ) -> NetworkResult<Message> {
        let payload_bytes =
            encode_payload(&payload).map_err(|e| NetworkError::Encoding(e.to_string()))?;
        let message = self.create_signed_message(MessageType::ChannelOpen, payload_bytes);
        self.send(peer, message).await
    }

    async fn send_channel_close(
        &self,
        peer: PeerId,
        payload: ChannelClosePayload,
    ) -> NetworkResult<Message> {
        let payload_bytes =
            encode_payload(&payload).map_err(|e| NetworkError::Encoding(e.to_string()))?;
        let message = self.create_signed_message(MessageType::ChannelClose, payload_bytes);
        self.send(peer, message).await
    }

    async fn broadcast_settlement_confirm(
        &self,
        payload: SettleConfirmPayload,
    ) -> NetworkResult<()> {
        let payload_bytes =
            encode_payload(&payload).map_err(|e| NetworkError::Encoding(e.to_string()))?;
        let message = self.create_signed_message(MessageType::SettleConfirm, payload_bytes);
        self.broadcast(message).await
    }

    async fn broadcast_announce(&self, payload: AnnouncePayload) -> NetworkResult<()> {
        let payload_bytes =
            encode_payload(&payload).map_err(|e| NetworkError::Encoding(e.to_string()))?;
        let message = self.create_signed_message(MessageType::Announce, payload_bytes);
        self.broadcast(message).await
    }

    async fn broadcast_announce_update(&self, payload: AnnounceUpdatePayload) -> NetworkResult<()> {
        let payload_bytes =
            encode_payload(&payload).map_err(|e| NetworkError::Encoding(e.to_string()))?;
        let message = self.create_signed_message(MessageType::AnnounceUpdate, payload_bytes);
        self.broadcast(message).await
    }

    fn connected_peers(&self) -> Vec<PeerId> {
        self.connected_peers_set
            .read()
            .map(|set| set.iter().copied().collect())
            .unwrap_or_default()
    }

    fn listen_addresses(&self) -> Vec<Multiaddr> {
        self.listen_addrs
            .read()
            .map(|addrs| addrs.clone())
            .unwrap_or_default()
    }

    async fn dial(&self, addr: Multiaddr) -> NetworkResult<()> {
        let (tx, rx) = oneshot::channel();
        self.command_tx
            .send(SwarmCommand::Dial { addr, response: tx })
            .await
            .map_err(|_| NetworkError::ChannelClosed)?;

        rx.await.map_err(|_| NetworkError::ChannelClosed)?
    }

    async fn dial_peer(&self, peer: PeerId) -> NetworkResult<()> {
        let (tx, rx) = oneshot::channel();
        self.command_tx
            .send(SwarmCommand::DialPeer { peer, response: tx })
            .await
            .map_err(|_| NetworkError::ChannelClosed)?;

        rx.await.map_err(|_| NetworkError::ChannelClosed)?
    }

    async fn next_event(&self) -> NetworkResult<NetworkEvent> {
        let mut event_rx = self.event_rx.lock().await;
        event_rx.recv().await.ok_or(NetworkError::ChannelClosed)
    }

    async fn send_response(
        &self,
        request_id: libp2p::request_response::InboundRequestId,
        data: Vec<u8>,
    ) -> NetworkResult<()> {
        self.command_tx
            .send(SwarmCommand::SendResponse { request_id, data })
            .await
            .map_err(|_| NetworkError::ChannelClosed)
    }

    async fn send_signed_response(
        &self,
        request_id: libp2p::request_response::InboundRequestId,
        message_type: MessageType,
        payload: Vec<u8>,
    ) -> NetworkResult<()> {
        // Create a signed message
        let message = self.create_signed_message(message_type, payload);
        // Encode to wire format
        let data = encode_message(&message).map_err(|e| NetworkError::Encoding(e.to_string()))?;
        // Send via existing send_response
        self.send_response(request_id, data).await
    }

    fn local_peer_id(&self) -> PeerId {
        self.local_peer_id
    }

    fn nodalync_peer_id(&self, libp2p_peer: &PeerId) -> Option<NodalyncPeerId> {
        self.peer_mapper.to_nodalync(libp2p_peer)
    }

    fn libp2p_peer_id(&self, nodalync_peer: &NodalyncPeerId) -> Option<PeerId> {
        self.peer_mapper.to_libp2p(nodalync_peer)
    }

    fn register_peer_mapping(&self, libp2p_peer: PeerId, nodalync_peer: NodalyncPeerId) {
        // Register the mapping with a placeholder public key
        // The public key will be updated if we receive it via identify or other means
        let placeholder_pubkey = nodalync_crypto::PublicKey::from_bytes([0u8; 32]);
        self.peer_mapper
            .register(libp2p_peer, nodalync_peer, placeholder_pubkey);
    }
}

/// Run the swarm event loop.
async fn run_swarm(
    mut swarm: Swarm<NodalyncBehaviour>,
    mut command_rx: mpsc::Receiver<SwarmCommand>,
    event_tx: mpsc::Sender<NetworkEvent>,
    ctx: SwarmContext,
) {
    // Subscribe to the announcement topic
    let topic = IdentTopic::new(&ctx.gossip_topic);
    if let Err(e) = swarm.behaviour_mut().gossipsub.subscribe(&topic) {
        warn!("Failed to subscribe to gossipsub topic: {}", e);
    }

    // Pending DHT operations
    let mut pending_dht_puts: HashMap<kad::QueryId, oneshot::Sender<NetworkResult<()>>> =
        HashMap::new();
    let mut pending_dht_gets: HashMap<
        kad::QueryId,
        oneshot::Sender<NetworkResult<Option<Vec<u8>>>>,
    > = HashMap::new();

    // Pending inbound request response channels
    let mut pending_responses: HashMap<
        libp2p::request_response::InboundRequestId,
        ResponseChannel<NodalyncResponse>,
    > = HashMap::new();

    // Per-peer GossipSub rate limiter: 50 messages per 10 seconds
    let mut gossip_rate_limiter = GossipRateLimiter::new(50, std::time::Duration::from_secs(10));

    loop {
        tokio::select! {
            // Process swarm events
            event = swarm.select_next_some() => {
                match event {
                    SwarmEvent::Behaviour(NodalyncBehaviourEvent::Kademlia(kad_event)) => {
                        handle_kademlia_event(
                            kad_event,
                            &mut pending_dht_puts,
                            &mut pending_dht_gets,
                        );
                    }

                    SwarmEvent::Behaviour(NodalyncBehaviourEvent::RequestResponse(rr_event)) => {
                        handle_request_response_event(
                            rr_event,
                            &ctx.pending_requests,
                            &mut pending_responses,
                            &event_tx,
                        ).await;
                    }

                    SwarmEvent::Behaviour(NodalyncBehaviourEvent::Gossipsub(gs_event)) => {
                        if let libp2p::gossipsub::Event::Message { propagation_source, .. } = &gs_event {
                            if !gossip_rate_limiter.check(propagation_source) {
                                warn!("Rate limiting GossipSub messages from {}", propagation_source);
                                continue;
                            }
                        }
                        handle_gossipsub_event(gs_event, &event_tx).await;
                    }

                    SwarmEvent::Behaviour(NodalyncBehaviourEvent::Identify(id_event)) => {
                        handle_identify_event(id_event, &mut swarm, &ctx.peer_mapper);
                    }

                    SwarmEvent::Behaviour(NodalyncBehaviourEvent::Ping(ping_event)) => {
                        handle_ping_event(ping_event);
                    }

                    SwarmEvent::Behaviour(NodalyncBehaviourEvent::Mdns(mdns_event)) => {
                        handle_mdns_event(mdns_event, &mut swarm, &event_tx).await;
                    }

                    SwarmEvent::Behaviour(NodalyncBehaviourEvent::Autonat(autonat_event)) => {
                        handle_autonat_event(autonat_event, &ctx.nat_status, &event_tx).await;
                    }

                    SwarmEvent::Behaviour(NodalyncBehaviourEvent::RelayClient(relay_event)) => {
                        handle_relay_client_event(relay_event, &event_tx).await;
                    }

                    SwarmEvent::Behaviour(NodalyncBehaviourEvent::Dcutr(dcutr_event)) => {
                        handle_dcutr_event(dcutr_event, &event_tx).await;
                    }

                    SwarmEvent::Behaviour(NodalyncBehaviourEvent::Upnp(upnp_event)) => {
                        handle_upnp_event(upnp_event, &event_tx).await;
                    }

                    SwarmEvent::ConnectionEstablished { peer_id, num_established, .. } => {
                        debug!("Connection established with {} (total: {})", peer_id, num_established);
                        // Track connected peer
                        if let Ok(mut peers) = ctx.connected_peers.write() {
                            peers.insert(peer_id);
                        }
                        // Only send event on first connection
                        if num_established.get() == 1 {
                            let _ = event_tx.send(NetworkEvent::PeerConnected { peer: peer_id }).await;
                        }
                    }

                    SwarmEvent::ConnectionClosed { peer_id, num_established, cause, .. } => {
                        debug!(
                            "Connection closed with {} (remaining: {}, cause: {:?})",
                            peer_id, num_established, cause
                        );
                        // Only unregister if no connections remain
                        if num_established == 0 {
                            if let Ok(mut peers) = ctx.connected_peers.write() {
                                peers.remove(&peer_id);
                            }
                            ctx.peer_mapper.unregister(&peer_id);
                            let _ = event_tx.send(NetworkEvent::PeerDisconnected { peer: peer_id }).await;
                        }
                    }

                    SwarmEvent::NewListenAddr { address, .. } => {
                        info!("Listening on {}", address);
                        // Track the listen address
                        if let Ok(mut addrs) = ctx.listen_addrs.write() {
                            addrs.push(address.clone());
                        }
                        let _ = event_tx.send(NetworkEvent::NewListenAddr { address }).await;
                    }

                    _ => {}
                }
            }

            // Process commands
            Some(command) = command_rx.recv() => {
                match command {
                    SwarmCommand::Dial { addr, response } => {
                        let result = swarm.dial(addr.clone())
                            .map_err(|e| NetworkError::DialError(e.to_string()));
                        let _ = response.send(result);
                    }

                    SwarmCommand::DialPeer { peer, response } => {
                        let opts = DialOpts::peer_id(peer).build();
                        let result = swarm.dial(opts)
                            .map_err(|e| NetworkError::DialError(e.to_string()));
                        let _ = response.send(result);
                    }

                    SwarmCommand::SendRequest { peer, data, response } => {
                        let request_id = swarm
                            .behaviour_mut()
                            .request_response
                            .send_request(&peer, NodalyncRequest(data));
                        ctx.pending_requests.write().await.insert(request_id, response);
                    }

                    SwarmCommand::DhtPut { key, value, response } => {
                        let record = kad::Record::new(key, value);
                        match swarm.behaviour_mut().kademlia.put_record(record, kad::Quorum::One) {
                            Ok(query_id) => {
                                pending_dht_puts.insert(query_id, response);
                            }
                            Err(e) => {
                                let _ = response.send(Err(NetworkError::DhtError(e.to_string())));
                            }
                        }
                    }

                    SwarmCommand::DhtGet { key, response } => {
                        let query_id = swarm
                            .behaviour_mut()
                            .kademlia
                            .get_record(RecordKey::new(&key));
                        pending_dht_gets.insert(query_id, response);
                    }

                    SwarmCommand::DhtRemove { key, response } => {
                        swarm.behaviour_mut().kademlia.remove_record(&RecordKey::new(&key));
                        let _ = response.send(Ok(()));
                    }

                    SwarmCommand::GossipPublish { topic, data, response } => {
                        let topic = IdentTopic::new(&topic);
                        let result = swarm.behaviour_mut().gossipsub.publish(topic, data)
                            .map(|_| ())
                            .map_err(|e| NetworkError::GossipSubError(e.to_string()));
                        let _ = response.send(result);
                    }

                    SwarmCommand::GossipSubscribe { topic, response } => {
                        let topic = IdentTopic::new(&topic);
                        let result = swarm.behaviour_mut().gossipsub.subscribe(&topic)
                            .map(|_| ())
                            .map_err(|e| NetworkError::GossipSubError(e.to_string()));
                        let _ = response.send(result);
                    }

                    SwarmCommand::GetConnectedPeers { response } => {
                        let peers: Vec<PeerId> = swarm.connected_peers().cloned().collect();
                        let _ = response.send(peers);
                    }

                    SwarmCommand::GetListenAddresses { response } => {
                        let addrs: Vec<Multiaddr> = swarm.listeners().cloned().collect();
                        let _ = response.send(addrs);
                    }

                    SwarmCommand::AddAddress { peer, addr } => {
                        swarm.behaviour_mut().kademlia.add_address(&peer, addr);
                    }

                    SwarmCommand::Bootstrap { response } => {
                        match swarm.behaviour_mut().kademlia.bootstrap() {
                            Ok(_) => {
                                let _ = response.send(Ok(()));
                            }
                            Err(e) => {
                                let _ = response.send(Err(NetworkError::BootstrapFailed(e.to_string())));
                            }
                        }
                    }

                    SwarmCommand::SendResponse { request_id, data } => {
                        if let Some(channel) = pending_responses.remove(&request_id) {
                            let _ = swarm.behaviour_mut().request_response.send_response(
                                channel,
                                NodalyncResponse(data),
                            );
                        } else {
                            warn!("No response channel found for request {:?}", request_id);
                        }
                    }
                }
            }
        }
    }
}

/// Handle Kademlia events.
fn handle_kademlia_event(
    event: kad::Event,
    pending_puts: &mut HashMap<kad::QueryId, oneshot::Sender<NetworkResult<()>>>,
    pending_gets: &mut HashMap<kad::QueryId, oneshot::Sender<NetworkResult<Option<Vec<u8>>>>>,
) {
    if let kad::Event::OutboundQueryProgressed { id, result, .. } = event {
        match result {
            QueryResult::PutRecord(Ok(_)) => {
                if let Some(tx) = pending_puts.remove(&id) {
                    let _ = tx.send(Ok(()));
                }
            }
            QueryResult::PutRecord(Err(e)) => {
                if let Some(tx) = pending_puts.remove(&id) {
                    let _ = tx.send(Err(NetworkError::DhtError(format!("{:?}", e))));
                }
            }
            QueryResult::GetRecord(Ok(kad::GetRecordOk::FoundRecord(peer_record))) => {
                if let Some(tx) = pending_gets.remove(&id) {
                    let _ = tx.send(Ok(Some(peer_record.record.value)));
                }
            }
            QueryResult::GetRecord(Ok(kad::GetRecordOk::FinishedWithNoAdditionalRecord {
                ..
            })) => {
                // Query finished but might have already sent a result
            }
            QueryResult::GetRecord(Err(e)) => {
                if let Some(tx) = pending_gets.remove(&id) {
                    match e {
                        kad::GetRecordError::NotFound { .. } => {
                            let _ = tx.send(Ok(None));
                        }
                        _ => {
                            let _ = tx.send(Err(NetworkError::DhtError(format!("{:?}", e))));
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

/// Handle request-response events.
async fn handle_request_response_event(
    event: request_response::Event<NodalyncRequest, NodalyncResponse>,
    pending_requests: &PendingRequests,
    pending_responses: &mut HashMap<
        libp2p::request_response::InboundRequestId,
        ResponseChannel<NodalyncResponse>,
    >,
    event_tx: &mpsc::Sender<NetworkEvent>,
) {
    match event {
        request_response::Event::Message { peer, message } => {
            match message {
                request_response::Message::Request {
                    request_id,
                    request,
                    channel,
                } => {
                    // Store the response channel
                    pending_responses.insert(request_id, channel);
                    // Forward inbound request as event
                    let _ = event_tx
                        .send(NetworkEvent::InboundRequest {
                            peer,
                            request_id,
                            data: request.0,
                        })
                        .await;
                }
                request_response::Message::Response {
                    request_id,
                    response,
                } => {
                    // Complete pending request
                    if let Some(tx) = pending_requests.write().await.remove(&request_id) {
                        let _ = tx.send(Ok(response.0));
                    }
                }
            }
        }
        request_response::Event::OutboundFailure {
            request_id, error, ..
        } => {
            if let Some(tx) = pending_requests.write().await.remove(&request_id) {
                let _ = tx.send(Err(NetworkError::ConnectionFailed(error.to_string())));
            }
        }
        request_response::Event::InboundFailure { error, .. } => {
            warn!("Inbound request failed: {}", error);
        }
        _ => {}
    }
}

/// Per-peer rate limiter for GossipSub messages.
struct GossipRateLimiter {
    /// Map of peer -> (message count, window start)
    peers: HashMap<PeerId, (u32, std::time::Instant)>,
    /// Maximum messages per peer per window
    max_per_window: u32,
    /// Window duration
    window: std::time::Duration,
}

impl GossipRateLimiter {
    fn new(max_per_window: u32, window: std::time::Duration) -> Self {
        Self {
            peers: HashMap::new(),
            max_per_window,
            window,
        }
    }

    fn check(&mut self, peer: &PeerId) -> bool {
        let now = std::time::Instant::now();
        let entry = self.peers.entry(*peer).or_insert((0, now));
        if now.duration_since(entry.1) > self.window {
            *entry = (1, now);
            true
        } else if entry.0 < self.max_per_window {
            entry.0 += 1;
            true
        } else {
            false
        }
    }
}

/// Handle GossipSub events.
async fn handle_gossipsub_event(
    event: libp2p::gossipsub::Event,
    event_tx: &mpsc::Sender<NetworkEvent>,
) {
    if let libp2p::gossipsub::Event::Message { message, .. } = event {
        let _ = event_tx
            .send(NetworkEvent::BroadcastReceived {
                topic: message.topic.to_string(),
                data: message.data,
            })
            .await;
    }
}

/// Handle Identify events.
fn handle_identify_event(
    event: libp2p::identify::Event,
    swarm: &mut Swarm<NodalyncBehaviour>,
    _peer_mapper: &PeerIdMapper,
) {
    if let libp2p::identify::Event::Received { peer_id, info, .. } = event {
        debug!("Received identify from {}: {:?}", peer_id, info.protocols);

        // Add addresses to Kademlia
        for addr in info.listen_addrs {
            swarm.behaviour_mut().kademlia.add_address(&peer_id, addr);
        }
    }
}

/// Handle mDNS events.
///
/// When peers are discovered on the local network via mDNS, we:
/// 1. Add their addresses to the Kademlia routing table for DHT queries
/// 2. Dial them to establish a direct connection
/// 3. Emit a PeerDiscovered event so the application layer can react
async fn handle_mdns_event(
    event: mdns::Event,
    swarm: &mut Swarm<NodalyncBehaviour>,
    event_tx: &mpsc::Sender<NetworkEvent>,
) {
    match event {
        mdns::Event::Discovered(peers) => {
            for (peer_id, addr) in peers {
                info!("mDNS: discovered peer {} at {}", peer_id, addr);
                // Add to Kademlia routing table
                swarm
                    .behaviour_mut()
                    .kademlia
                    .add_address(&peer_id, addr.clone());
                // Dial the discovered peer
                if let Err(e) = swarm.dial(addr.clone()) {
                    debug!("mDNS: failed to dial discovered peer {}: {}", peer_id, e);
                }
                let _ = event_tx
                    .send(NetworkEvent::PeerDiscovered {
                        peer: peer_id,
                        address: addr,
                    })
                    .await;
            }
        }
        mdns::Event::Expired(peers) => {
            for (peer_id, addr) in peers {
                debug!("mDNS: peer {} at {} expired", peer_id, addr);
            }
        }
    }
}

/// Handle Ping events.
fn handle_ping_event(event: libp2p::ping::Event) {
    match event.result {
        Ok(rtt) => {
            debug!("Ping to {} succeeded: {:?}", event.peer, rtt);
        }
        Err(e) => {
            debug!("Ping to {} failed: {}", event.peer, e);
        }
    }
}

/// Handle AutoNAT events — detect NAT status.
async fn handle_autonat_event(
    event: libp2p::autonat::Event,
    nat_status: &Arc<StdRwLock<NatStatus>>,
    event_tx: &mpsc::Sender<NetworkEvent>,
) {
    match event {
        libp2p::autonat::Event::StatusChanged { old, new } => {
            let status = match new {
                libp2p::autonat::NatStatus::Public(_) => {
                    info!("AutoNAT: node is publicly reachable");
                    NatStatus::Public
                }
                libp2p::autonat::NatStatus::Private => {
                    info!("AutoNAT: node is behind NAT (private)");
                    NatStatus::Private
                }
                libp2p::autonat::NatStatus::Unknown => {
                    debug!("AutoNAT: status unknown");
                    NatStatus::Unknown
                }
            };
            // Update shared state
            if let Ok(mut s) = nat_status.write() {
                *s = status.clone();
            }
            let _ = event_tx
                .send(NetworkEvent::NatStatusChanged { status })
                .await;
            debug!("AutoNAT: status changed from {:?} to {:?}", old, new);
        }
        libp2p::autonat::Event::InboundProbe(probe) => {
            debug!("AutoNAT: inbound probe: {:?}", probe);
        }
        libp2p::autonat::Event::OutboundProbe(probe) => {
            debug!("AutoNAT: outbound probe: {:?}", probe);
        }
    }
}

/// Handle relay client events — track relay reservations.
async fn handle_relay_client_event(
    event: libp2p::relay::client::Event,
    event_tx: &mpsc::Sender<NetworkEvent>,
) {
    match event {
        libp2p::relay::client::Event::ReservationReqAccepted {
            relay_peer_id,
            renewal,
            ..
        } => {
            if renewal {
                debug!("Relay: reservation renewed with {}", relay_peer_id);
            } else {
                info!("Relay: reservation accepted by {}", relay_peer_id);
                let _ = event_tx
                    .send(NetworkEvent::RelayReservation {
                        relay: relay_peer_id,
                    })
                    .await;
            }
        }
        libp2p::relay::client::Event::OutboundCircuitEstablished { relay_peer_id, .. } => {
            info!("Relay: outbound circuit established via {}", relay_peer_id);
        }
        libp2p::relay::client::Event::InboundCircuitEstablished { src_peer_id, .. } => {
            info!("Relay: inbound circuit established from {}", src_peer_id);
        }
    }
}

/// Handle DCUtR events — direct connection upgrade through relay.
async fn handle_dcutr_event(event: libp2p::dcutr::Event, event_tx: &mpsc::Sender<NetworkEvent>) {
    // dcutr::Event is a struct with { remote_peer_id, result }
    match event.result {
        Ok(_connection_id) => {
            info!("DCUtR: hole-punch succeeded with {}", event.remote_peer_id);
            let _ = event_tx
                .send(NetworkEvent::HolePunched {
                    peer: event.remote_peer_id,
                })
                .await;
        }
        Err(e) => {
            debug!(
                "DCUtR: hole-punch failed with {}: {}",
                event.remote_peer_id, e
            );
        }
    }
}

/// Handle UPnP events — automatic port mapping.
async fn handle_upnp_event(event: libp2p::upnp::Event, event_tx: &mpsc::Sender<NetworkEvent>) {
    match event {
        libp2p::upnp::Event::NewExternalAddr(addr) => {
            info!("UPnP: mapped external address {}", addr);
            let _ = event_tx
                .send(NetworkEvent::UpnpMapped { address: addr })
                .await;
        }
        libp2p::upnp::Event::GatewayNotFound => {
            debug!("UPnP: no gateway found (router may not support UPnP)");
        }
        libp2p::upnp::Event::NonRoutableGateway => {
            debug!("UPnP: gateway is not routable (may be double-NAT)");
        }
        libp2p::upnp::Event::ExpiredExternalAddr(addr) => {
            debug!("UPnP: external address mapping expired: {}", addr);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_node() {
        let config = NetworkConfig::default();
        let node = NetworkNode::new(config).await;
        assert!(node.is_ok());
    }

    #[tokio::test]
    async fn test_node_has_peer_id() {
        let config = NetworkConfig::default();
        let node = NetworkNode::new(config).await.unwrap();

        // Should have a valid peer ID
        let peer_id = node.local_peer_id();
        assert!(!peer_id.to_string().is_empty());
    }

    #[test]
    fn test_gossip_rate_limiter_allows_within_limit() {
        let mut limiter = GossipRateLimiter::new(3, std::time::Duration::from_secs(10));
        let peer = PeerId::random();

        assert!(limiter.check(&peer));
        assert!(limiter.check(&peer));
        assert!(limiter.check(&peer));
        // 4th message should be rejected
        assert!(!limiter.check(&peer));
    }

    #[test]
    fn test_gossip_rate_limiter_independent_peers() {
        let mut limiter = GossipRateLimiter::new(2, std::time::Duration::from_secs(10));
        let peer1 = PeerId::random();
        let peer2 = PeerId::random();

        assert!(limiter.check(&peer1));
        assert!(limiter.check(&peer1));
        assert!(!limiter.check(&peer1)); // peer1 exhausted

        // peer2 should still be allowed
        assert!(limiter.check(&peer2));
        assert!(limiter.check(&peer2));
        assert!(!limiter.check(&peer2)); // peer2 exhausted
    }

    #[test]
    fn test_gossip_rate_limiter_resets_after_window() {
        let mut limiter = GossipRateLimiter::new(2, std::time::Duration::from_millis(0));
        let peer = PeerId::random();

        assert!(limiter.check(&peer));
        assert!(limiter.check(&peer));
        // Window is 0ms so it should immediately reset
        assert!(limiter.check(&peer));
    }
}
