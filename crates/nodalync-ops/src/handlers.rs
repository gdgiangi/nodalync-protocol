//! Incoming message handlers.
//!
//! This module implements handlers for incoming protocol messages,
//! processing requests from other nodes.

use nodalync_crypto::{content_hash, PeerId, Signature};
use nodalync_econ::distribute_revenue;
use nodalync_net::NetworkEvent;
use nodalync_store::{
    ChannelStore, ContentStore, ManifestStore, QueuedDistribution, SettlementQueueStore,
};
use nodalync_types::{Channel, Payment, Visibility};
use nodalync_valid::Validator;
use nodalync_wire::{
    decode_message, decode_payload, AnnouncePayload, ChannelAcceptPayload, ChannelClosePayload,
    ChannelOpenPayload, MessageType, PaymentReceipt, PreviewRequestPayload, PreviewResponsePayload,
    QueryRequestPayload, QueryResponsePayload, VersionInfo, VersionRequestPayload,
    VersionResponsePayload,
};
use tracing::{debug, info};

use crate::error::{OpsError, OpsResult};
use crate::extraction::L1Extractor;
use crate::node_ops::{current_timestamp, NodeOperations};

impl<V, E> NodeOperations<V, E>
where
    V: Validator,
    E: L1Extractor,
{
    /// Handle an incoming preview request.
    ///
    /// 1. Load manifest
    /// 2. Validate access
    /// 3. Get L1Summary
    /// 4. Return PreviewResponsePayload
    pub fn handle_preview_request(
        &mut self,
        _requester: &PeerId,
        request: &PreviewRequestPayload,
    ) -> OpsResult<PreviewResponsePayload> {
        // 1. Load manifest
        let manifest = self
            .state
            .manifests
            .load(&request.hash)?
            .ok_or(OpsError::ManifestNotFound(request.hash))?;

        // 2. Validate access (basic visibility check)
        if manifest.visibility == Visibility::Private {
            return Err(OpsError::AccessDenied);
        }

        // 3. Get L1Summary
        let l1_summary = self.extract_l1_summary(&request.hash)?;

        // 4. Return response
        Ok(PreviewResponsePayload {
            hash: request.hash,
            manifest,
            l1_summary,
        })
    }

    /// Handle an incoming query request.
    ///
    /// CRITICAL: This handler:
    /// 1. Load manifest
    /// 2. Validate access
    /// 3. Validate payment
    /// 4. Update channel state (credit)
    /// 5. Calculate ALL distributions via distribute_revenue()
    /// 6. Enqueue ALL to settlement queue
    /// 7. Update manifest economics
    /// 8. Check settlement trigger (threshold/interval)
    /// 9. Load and return content with receipt
    pub async fn handle_query_request(
        &mut self,
        requester: &PeerId,
        request: &QueryRequestPayload,
    ) -> OpsResult<QueryResponsePayload> {
        let timestamp = current_timestamp();
        let payment_amount = request.payment.amount;

        // 1. Load manifest
        let mut manifest = self
            .state
            .manifests
            .load(&request.hash)?
            .ok_or(OpsError::ManifestNotFound(request.hash))?;

        // 2. Validate access
        if manifest.visibility == Visibility::Private {
            return Err(OpsError::AccessDenied);
        }

        // Check access control
        self.validator.validate_access(requester, &manifest)?;

        // 3. Validate payment
        if payment_amount < manifest.economics.price {
            return Err(OpsError::PaymentInsufficient);
        }

        // 4. Update channel state (credit - they pay us)
        if let Some(mut channel) = self.state.channels.get(requester)? {
            if channel.is_open() && payment_amount > 0 {
                // Create payment record
                let payment_id = content_hash(
                    &[
                        request.hash.0.as_slice(),
                        &timestamp.to_be_bytes(),
                        &payment_amount.to_be_bytes(),
                    ]
                    .concat(),
                );

                let payment = Payment::new(
                    payment_id,
                    channel.channel_id,
                    payment_amount,
                    self.peer_id(),
                    request.hash,
                    manifest.provenance.root_l0l1.clone(),
                    timestamp,
                    Signature::from_bytes([0u8; 64]), // Stub signature
                );

                // Credit our side (receive payment)
                channel
                    .receive(payment.clone(), timestamp)
                    .map_err(|_| OpsError::InsufficientChannelBalance)?;

                self.state.channels.update(requester, &channel)?;
                self.state.channels.add_payment(requester, payment)?;
            }
        }

        // 5. Calculate ALL distributions via distribute_revenue()
        let distributions = distribute_revenue(
            payment_amount,
            &manifest.owner,
            &manifest.provenance.root_l0l1,
        );

        // 6. Enqueue ALL to settlement queue
        let payment_id =
            content_hash(&[request.hash.0.as_slice(), &timestamp.to_be_bytes()].concat());

        for dist in &distributions {
            let queued = QueuedDistribution::new(
                payment_id,
                dist.recipient,
                dist.amount,
                request.hash,
                timestamp,
            );
            self.state.settlement.enqueue(queued)?;
        }

        // 7. Update manifest economics
        manifest.economics.record_query(payment_amount);
        manifest.updated_at = timestamp;
        self.state.manifests.update(&manifest)?;

        // 8. Check settlement trigger (threshold/interval)
        // Don't fail the query if settlement fails
        let _ = self.trigger_settlement_batch().await;

        // 9. Load and return content
        let content = self
            .state
            .content
            .load(&request.hash)?
            .ok_or(OpsError::NotFound(request.hash))?;

        let receipt = PaymentReceipt {
            payment_id,
            amount: payment_amount,
            timestamp,
            channel_nonce: 0, // Would come from channel
            distributor_signature: Signature::from_bytes([0u8; 64]),
        };

        Ok(QueryResponsePayload {
            hash: request.hash,
            content,
            manifest,
            payment_receipt: receipt,
        })
    }

    /// Handle an incoming version request.
    ///
    /// 1. Get all versions for root
    /// 2. Find latest if requested
    /// 3. Return VersionResponsePayload
    pub fn handle_version_request(
        &self,
        _requester: &PeerId,
        request: &VersionRequestPayload,
    ) -> OpsResult<VersionResponsePayload> {
        // Get all versions
        let manifests = self.state.manifests.get_versions(&request.version_root)?;

        let versions: Vec<VersionInfo> = manifests
            .iter()
            .map(|m| VersionInfo {
                hash: m.hash,
                number: m.version.number,
                timestamp: m.version.timestamp,
                visibility: m.visibility,
                price: m.economics.price,
            })
            .collect();

        // Find latest version hash
        let latest = versions
            .iter()
            .max_by_key(|v| v.number)
            .map(|v| v.hash)
            .unwrap_or(request.version_root);

        Ok(VersionResponsePayload {
            version_root: request.version_root,
            versions,
            latest,
        })
    }

    /// Handle an incoming channel open request.
    ///
    /// 1. Validate no existing channel
    /// 2. Create channel state
    /// 3. Return ChannelAcceptPayload
    pub fn handle_channel_open(
        &mut self,
        requester: &PeerId,
        request: &ChannelOpenPayload,
    ) -> OpsResult<ChannelAcceptPayload> {
        let timestamp = current_timestamp();

        // 1. Validate no existing channel
        if self.state.channels.get(requester)?.is_some() {
            return Err(OpsError::ChannelAlreadyExists);
        }

        // 2. Create channel state
        // For MVP, we auto-accept with a matching deposit
        let my_deposit = request.initial_balance; // Match their deposit

        let channel = Channel::accepted(
            request.channel_id,
            *requester,
            request.initial_balance,
            my_deposit,
            timestamp,
        );

        self.state.channels.create(requester, channel)?;

        // 3. Return accept payload
        Ok(ChannelAcceptPayload {
            channel_id: request.channel_id,
            initial_balance: my_deposit,
            funding_tx: None, // No on-chain funding for MVP
        })
    }

    /// Handle an incoming channel close request.
    ///
    /// 1. Verify channel exists
    /// 2. Verify final state
    /// 3. (Submit settlement - stub)
    pub fn handle_channel_close(
        &mut self,
        requester: &PeerId,
        request: &ChannelClosePayload,
    ) -> OpsResult<()> {
        let timestamp = current_timestamp();

        // 1. Verify channel exists
        let mut channel = self
            .state
            .channels
            .get(requester)?
            .ok_or(OpsError::ChannelNotFound)?;

        // Verify channel ID matches
        if channel.channel_id != request.channel_id {
            return Err(OpsError::invalid_operation("channel ID mismatch"));
        }

        // 2. Verify final state (basic check)
        // In full implementation, verify signatures and state

        // 3. Mark channel as closed
        channel.mark_closing(timestamp);
        self.state.channels.update(requester, &channel)?;

        channel.mark_closed(timestamp);
        self.state.channels.update(requester, &channel)?;

        Ok(())
    }

    /// Handle a broadcast announcement from GossipSub.
    ///
    /// When we receive an announcement, we:
    /// 1. Decode the AnnouncePayload
    /// 2. Log the announcement for debugging
    /// 3. Store it in the announcements cache for later lookup
    ///
    /// This allows preview/query to discover content from remote nodes.
    fn handle_broadcast_announcement(&mut self, topic: &str, data: &[u8]) -> OpsResult<()> {
        // Only process announcements on the announce topic
        if !topic.contains("/nodalync/announce") {
            return Ok(());
        }

        // Try to decode the wire protocol message
        match decode_message(data) {
            Ok(message) => {
                // Check if this is an ANNOUNCE message
                if message.message_type != MessageType::Announce {
                    debug!(
                        "Ignoring non-announce broadcast message: {:?}",
                        message.message_type
                    );
                    return Ok(());
                }

                // Decode the AnnouncePayload from the message payload
                match decode_payload::<AnnouncePayload>(&message.payload) {
                    Ok(payload) => {
                        info!(
                            hash = %payload.hash,
                            title = %payload.title,
                            price = payload.price,
                            addresses = ?payload.addresses,
                            "Received content announcement"
                        );

                        // Store the announcement in our cache for later lookup
                        // This allows preview/query to find content from remote nodes
                        self.state.store_announcement(payload);
                        Ok(())
                    }
                    Err(e) => {
                        debug!("Failed to decode announcement payload: {}", e);
                        Ok(()) // Don't fail on decode errors
                    }
                }
            }
            Err(e) => {
                debug!("Failed to decode broadcast message: {}", e);
                Ok(()) // Don't fail on decode errors
            }
        }
    }

    /// Handle an incoming network event.
    ///
    /// This is the main entry point for processing network events and
    /// dispatching to the appropriate handler. Returns an optional response
    /// that should be encoded and sent back to the peer.
    ///
    /// The response is returned as (MessageType, serialized_payload) for the
    /// caller to construct the actual Message envelope.
    pub async fn handle_network_event(
        &mut self,
        event: NetworkEvent,
    ) -> OpsResult<Option<(MessageType, Vec<u8>)>> {
        match event {
            NetworkEvent::InboundRequest { peer, data, .. } => {
                // First decode the full wire message (header + payload + signature)
                let message = match decode_message(&data) {
                    Ok(msg) => msg,
                    Err(e) => {
                        debug!(
                            "Failed to decode message: {}, data length: {}",
                            e,
                            data.len()
                        );
                        return Ok(None);
                    }
                };

                debug!(
                    "Received message type {:?} from peer {}, payload length: {}",
                    message.message_type,
                    peer,
                    message.payload.len()
                );

                // Get the Nodalync peer ID if we have a mapping
                let nodalync_peer = if let Some(network) = self.network() {
                    network.nodalync_peer_id(&peer).unwrap_or(PeerId([0u8; 20]))
                } else {
                    PeerId([0u8; 20])
                };

                // Handle the request based on message type
                match message.message_type {
                    MessageType::PreviewRequest => {
                        let request: PreviewRequestPayload = decode_payload(&message.payload)
                            .map_err(|e| {
                                OpsError::invalid_operation(format!("decode error: {}", e))
                            })?;
                        debug!("Received preview request for hash: {}", request.hash);
                        let response = self.handle_preview_request(&nodalync_peer, &request)?;
                        let response_bytes =
                            nodalync_wire::encode_payload(&response).map_err(|e| {
                                OpsError::invalid_operation(format!("encoding error: {}", e))
                            })?;
                        Ok(Some((MessageType::PreviewResponse, response_bytes)))
                    }
                    MessageType::QueryRequest => {
                        let request: QueryRequestPayload = decode_payload(&message.payload)
                            .map_err(|e| {
                                OpsError::invalid_operation(format!("decode error: {}", e))
                            })?;
                        debug!("Received query request for hash: {}", request.hash);
                        let response = self.handle_query_request(&nodalync_peer, &request).await?;
                        let response_bytes =
                            nodalync_wire::encode_payload(&response).map_err(|e| {
                                OpsError::invalid_operation(format!("encoding error: {}", e))
                            })?;
                        Ok(Some((MessageType::QueryResponse, response_bytes)))
                    }
                    MessageType::VersionRequest => {
                        let request: VersionRequestPayload = decode_payload(&message.payload)
                            .map_err(|e| {
                                OpsError::invalid_operation(format!("decode error: {}", e))
                            })?;
                        debug!(
                            "Received version request for root: {}",
                            request.version_root
                        );
                        let response = self.handle_version_request(&nodalync_peer, &request)?;
                        let response_bytes =
                            nodalync_wire::encode_payload(&response).map_err(|e| {
                                OpsError::invalid_operation(format!("encoding error: {}", e))
                            })?;
                        Ok(Some((MessageType::VersionResponse, response_bytes)))
                    }
                    MessageType::ChannelOpen => {
                        let request: ChannelOpenPayload = decode_payload(&message.payload)
                            .map_err(|e| {
                                OpsError::invalid_operation(format!("decode error: {}", e))
                            })?;
                        debug!("Received channel open request");
                        let response = self.handle_channel_open(&nodalync_peer, &request)?;
                        let response_bytes =
                            nodalync_wire::encode_payload(&response).map_err(|e| {
                                OpsError::invalid_operation(format!("encoding error: {}", e))
                            })?;
                        Ok(Some((MessageType::ChannelAccept, response_bytes)))
                    }
                    MessageType::ChannelClose => {
                        let request: ChannelClosePayload = decode_payload(&message.payload)
                            .map_err(|e| {
                                OpsError::invalid_operation(format!("decode error: {}", e))
                            })?;
                        debug!("Received channel close request");
                        self.handle_channel_close(&nodalync_peer, &request)?;
                        Ok(None) // No response needed for close
                    }
                    _ => {
                        debug!("Unhandled message type: {:?}", message.message_type);
                        Ok(None)
                    }
                }
            }
            NetworkEvent::PeerConnected { peer } => {
                // Log peer connection (could track connected peers in state)
                let _ = peer;
                Ok(None)
            }
            NetworkEvent::PeerDisconnected { peer } => {
                // Log peer disconnection
                let _ = peer;
                Ok(None)
            }
            NetworkEvent::BroadcastReceived { topic, data } => {
                // Handle content announcements from GossipSub
                self.handle_broadcast_announcement(&topic, &data)?;
                Ok(None)
            }
            _ => {
                // Other events don't require action from ops layer
                Ok(None)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node_ops::DefaultNodeOperations;
    use nodalync_crypto::{content_hash, generate_identity, peer_id_from_public_key};
    use nodalync_store::{NodeStateConfig, SettlementQueueStore};
    use nodalync_types::Metadata;
    use nodalync_wire::ChannelBalances;
    use tempfile::TempDir;

    fn create_test_ops() -> (DefaultNodeOperations, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let config = NodeStateConfig::new(temp_dir.path());
        let state = nodalync_store::NodeState::open(config).unwrap();

        let (_, public_key) = generate_identity();
        let peer_id = peer_id_from_public_key(&public_key);

        let ops = DefaultNodeOperations::with_defaults(state, peer_id);
        (ops, temp_dir)
    }

    fn test_peer_id() -> PeerId {
        let (_, public_key) = generate_identity();
        peer_id_from_public_key(&public_key)
    }

    fn create_test_payment(
        amount: u64,
        recipient: PeerId,
        query_hash: nodalync_crypto::Hash,
    ) -> Payment {
        Payment::new(
            content_hash(b"payment"),
            content_hash(b"channel"),
            amount,
            recipient,
            query_hash,
            vec![],
            current_timestamp(),
            Signature::from_bytes([0u8; 64]),
        )
    }

    #[tokio::test]
    async fn test_handle_preview_request() {
        let (mut ops, _temp) = create_test_ops();

        // Create and publish content
        let content = b"Test content for preview";
        let meta = Metadata::new("Preview Test", content.len() as u64);
        let hash = ops.create_content(content, meta).unwrap();
        ops.publish_content(&hash, Visibility::Shared, 100)
            .await
            .unwrap();

        // Handle preview request
        let requester = test_peer_id();
        let request = PreviewRequestPayload { hash };

        let response = ops.handle_preview_request(&requester, &request).unwrap();

        assert_eq!(response.hash, hash);
        assert_eq!(response.manifest.economics.price, 100);
    }

    #[test]
    fn test_handle_preview_request_private() {
        let (mut ops, _temp) = create_test_ops();

        // Create private content
        let content = b"Private content";
        let meta = Metadata::new("Private", content.len() as u64);
        let hash = ops.create_content(content, meta).unwrap();
        // Don't publish - stays private

        // Handle preview request
        let requester = test_peer_id();
        let request = PreviewRequestPayload { hash };

        let result = ops.handle_preview_request(&requester, &request);
        assert!(matches!(result, Err(OpsError::AccessDenied)));
    }

    #[tokio::test]
    async fn test_handle_query_request() {
        let (mut ops, _temp) = create_test_ops();

        // Create and publish content
        let content = b"Test content for query";
        let meta = Metadata::new("Query Test", content.len() as u64);
        let hash = ops.create_content(content, meta).unwrap();
        ops.publish_content(&hash, Visibility::Shared, 100)
            .await
            .unwrap();

        // Handle query request
        let requester = test_peer_id();
        let manifest = ops.get_content_manifest(&hash).unwrap().unwrap();
        let payment = create_test_payment(100, manifest.owner, hash);

        let request = QueryRequestPayload {
            hash,
            query: None,
            payment,
            version_spec: None,
        };

        let response = ops
            .handle_query_request(&requester, &request)
            .await
            .unwrap();

        assert_eq!(response.hash, hash);
        assert_eq!(response.content, content.to_vec());
        assert_eq!(response.payment_receipt.amount, 100);
    }

    #[tokio::test]
    async fn test_handle_query_request_insufficient_payment() {
        let (mut ops, _temp) = create_test_ops();

        // Create and publish content with price
        let content = b"Paid content";
        let meta = Metadata::new("Paid", content.len() as u64);
        let hash = ops.create_content(content, meta).unwrap();
        ops.publish_content(&hash, Visibility::Shared, 1000)
            .await
            .unwrap();

        // Handle query request with insufficient payment
        let requester = test_peer_id();
        let manifest = ops.get_content_manifest(&hash).unwrap().unwrap();
        let payment = create_test_payment(100, manifest.owner, hash); // Less than 1000

        let request = QueryRequestPayload {
            hash,
            query: None,
            payment,
            version_spec: None,
        };

        let result = ops.handle_query_request(&requester, &request).await;
        assert!(matches!(result, Err(OpsError::PaymentInsufficient)));
    }

    #[test]
    fn test_handle_version_request() {
        let (mut ops, _temp) = create_test_ops();

        // Create content with versions
        let content1 = b"Version 1";
        let meta1 = Metadata::new("v1", content1.len() as u64);
        let hash1 = ops.create_content(content1, meta1).unwrap();

        let content2 = b"Version 2";
        let meta2 = Metadata::new("v2", content2.len() as u64);
        let _hash2 = ops.update_content(&hash1, content2, meta2).unwrap();

        // Handle version request
        let requester = test_peer_id();
        let request = VersionRequestPayload {
            version_root: hash1,
        };

        let response = ops.handle_version_request(&requester, &request).unwrap();

        assert_eq!(response.version_root, hash1);
        assert!(!response.versions.is_empty());
        // latest is a Hash, not Option
        assert!(response.latest.0.iter().any(|&b| b != 0));
    }

    #[test]
    fn test_handle_channel_open() {
        let (mut ops, _temp) = create_test_ops();

        let requester = test_peer_id();
        let channel_id = content_hash(b"test channel");

        let request = ChannelOpenPayload {
            channel_id,
            initial_balance: 1000,
            funding_tx: None,
        };

        let response = ops.handle_channel_open(&requester, &request).unwrap();

        assert_eq!(response.channel_id, channel_id);
        assert_eq!(response.initial_balance, 1000);

        // Verify channel was created
        let channel = ops.get_payment_channel(&requester).unwrap().unwrap();
        assert!(channel.is_open());
    }

    #[test]
    fn test_handle_channel_close() {
        let (mut ops, _temp) = create_test_ops();

        // First open a channel
        let requester = test_peer_id();
        let channel_id = content_hash(b"test channel");

        let open_request = ChannelOpenPayload {
            channel_id,
            initial_balance: 1000,
            funding_tx: None,
        };
        ops.handle_channel_open(&requester, &open_request).unwrap();

        // Now close it
        let close_request = ChannelClosePayload {
            channel_id,
            final_balances: ChannelBalances::new(1000, 1000),
            settlement_tx: vec![],
        };

        ops.handle_channel_close(&requester, &close_request)
            .unwrap();

        // Verify channel is closed
        let channel = ops.get_payment_channel(&requester).unwrap().unwrap();
        assert!(channel.is_closed());
    }

    #[tokio::test]
    async fn test_query_enqueues_distributions() {
        let (mut ops, _temp) = create_test_ops();

        // Set a recent last_settlement_time so the interval-based trigger doesn't fire
        // (current time - last_settlement must be < 1 hour for settlement NOT to trigger)
        let recent_time = current_timestamp();
        ops.state
            .settlement
            .set_last_settlement_time(recent_time)
            .unwrap();

        // Create and publish content
        let content = b"Content for distribution test";
        let meta = Metadata::new("Dist Test", content.len() as u64);
        let hash = ops.create_content(content, meta).unwrap();
        ops.publish_content(&hash, Visibility::Shared, 100)
            .await
            .unwrap();

        // Initially no pending distributions
        assert_eq!(ops.get_pending_settlement_total().unwrap(), 0);

        // Handle query request
        let requester = test_peer_id();
        let manifest = ops.get_content_manifest(&hash).unwrap().unwrap();
        let payment = create_test_payment(100, manifest.owner, hash);

        let request = QueryRequestPayload {
            hash,
            query: None,
            payment,
            version_spec: None,
        };
        ops.handle_query_request(&requester, &request)
            .await
            .unwrap();

        // Now we should have pending distributions
        let pending = ops.get_pending_settlement_total().unwrap();
        assert_eq!(pending, 100); // Full payment amount
    }
}
