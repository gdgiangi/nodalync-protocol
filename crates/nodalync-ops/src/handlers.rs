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
    QueryRequestPayload, QueryResponsePayload, SearchPayload, SearchResponsePayload,
    SearchResult as WireSearchResult, VersionInfo, VersionRequestPayload, VersionResponsePayload,
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
    /// 3. Validate payment amount
    /// 4. Validate payment signature for paid content (channel, nonce, signature)
    /// 5. Update channel state (credit)
    /// 6. Calculate ALL distributions via distribute_revenue()
    /// 7. Enqueue ALL to settlement queue
    /// 8. Update manifest economics
    /// 9. Check settlement trigger (threshold/interval)
    /// 10. Load and return content with receipt
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

        // 3. Validate payment amount
        if payment_amount < manifest.economics.price {
            return Err(OpsError::PaymentInsufficient);
        }

        // 4. Validate payment signature for paid content
        // Payment channels are REQUIRED for paid content queries.
        if manifest.economics.price > 0 {
            match self.state.channels.get(requester)? {
                Some(channel) if channel.is_open() => {
                    // Full payment validation: signature, nonce, amount, provenance
                    // Note: We pass None for public key lookup in MVP. Full signature
                    // verification requires a peer key registry.
                    nodalync_valid::validate_payment(
                        &request.payment,
                        &channel,
                        &manifest,
                        None, // TODO: Get requester's public key from peer registry
                        request.payment_nonce,
                    )
                    .map_err(|e| OpsError::PaymentValidationFailed(e.to_string()))?;

                    // Verify payment nonce is strictly greater than channel nonce (replay prevention)
                    if request.payment_nonce <= channel.nonce {
                        return Err(OpsError::PaymentValidationFailed(format!(
                            "payment nonce {} must be > channel nonce {}",
                            request.payment_nonce, channel.nonce
                        )));
                    }
                }
                Some(_) => {
                    // Channel exists but not open - require open channel for paid content
                    return Err(OpsError::ChannelNotOpen);
                }
                None => {
                    // No channel exists - require channel for paid content
                    tracing::warn!(
                        requester = %requester,
                        hash = %request.hash,
                        price = manifest.economics.price,
                        "Paid content requested without payment channel"
                    );
                    return Err(OpsError::ChannelRequired);
                }
            }
        }

        // 5. Update channel state (credit - they pay us)
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

                // Update nonce to prevent replay attacks
                // Set to request nonce to track the highest seen nonce
                if request.payment_nonce > channel.nonce {
                    channel.nonce = request.payment_nonce;
                }

                self.state.channels.update(requester, &channel)?;
                self.state.channels.add_payment(requester, payment)?;
            }
        }

        // 6. Calculate ALL distributions via distribute_revenue()
        let distributions = distribute_revenue(
            payment_amount,
            &manifest.owner,
            &manifest.provenance.root_l0l1,
        );

        // 7. Enqueue ALL to settlement queue
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

        // 8. Update manifest economics
        manifest.economics.record_query(payment_amount);
        manifest.updated_at = timestamp;
        self.state.manifests.update(&manifest)?;

        // 9. Check settlement trigger (threshold/interval)
        // Don't fail the query if settlement fails
        let _ = self.trigger_settlement_batch().await;

        // 10. Load and return content
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

    /// Handle an incoming search request.
    ///
    /// 1. Search local manifests matching query
    /// 2. Apply filters (content type, max price)
    /// 3. Return SearchResponsePayload with results
    pub fn handle_search_request(
        &mut self,
        _requester: &PeerId,
        request: &SearchPayload,
    ) -> OpsResult<SearchResponsePayload> {
        use nodalync_store::ManifestFilter;
        use nodalync_types::L1Summary;

        let query = request.query.to_lowercase();
        let limit = request.limit.min(100);

        // Build filter for shared content only
        let mut filter = ManifestFilter::new()
            .with_text_query(&query)
            .with_visibility(Visibility::Shared)
            .limit(limit);

        // Apply content type filter if specified
        if let Some(ref filters) = request.filters {
            if let Some(ref content_types) = filters.content_types {
                if let Some(ct) = content_types.first() {
                    filter = filter.with_content_type(*ct);
                }
            }
        }

        // Search local manifests
        let manifests = self.state.manifests.list(filter)?;

        // Convert to SearchResult
        let results: Vec<WireSearchResult> = manifests
            .iter()
            .map(|m| {
                // Extract L1 summary if available
                let l1_summary = self
                    .extract_l1_summary(&m.hash)
                    .unwrap_or_else(|_| L1Summary::empty(m.hash));

                // Calculate simple relevance score based on title match
                let relevance_score = if m.metadata.title.to_lowercase().contains(&query) {
                    1.0
                } else {
                    0.5
                };

                WireSearchResult {
                    hash: m.hash,
                    content_type: m.content_type,
                    title: m.metadata.title.clone(),
                    owner: m.owner,
                    l1_summary,
                    price: m.economics.price,
                    total_queries: m.economics.total_queries,
                    relevance_score,
                }
            })
            .collect();

        // Apply max_price filter if specified
        let results = if let Some(ref filters) = request.filters {
            if let Some(max_price) = filters.max_price {
                results
                    .into_iter()
                    .filter(|r| r.price <= max_price)
                    .collect()
            } else {
                results
            }
        } else {
            results
        };

        let total_count = results.len() as u64;

        Ok(SearchResponsePayload {
            results,
            total_count,
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
                    MessageType::Search => {
                        let request: SearchPayload =
                            decode_payload(&message.payload).map_err(|e| {
                                OpsError::invalid_operation(format!("decode error: {}", e))
                            })?;
                        debug!("Received search request for query: {}", request.query);
                        let response = self.handle_search_request(&nodalync_peer, &request)?;
                        let response_bytes =
                            nodalync_wire::encode_payload(&response).map_err(|e| {
                                OpsError::invalid_operation(format!("encoding error: {}", e))
                            })?;
                        Ok(Some((MessageType::SearchResponse, response_bytes)))
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
    use nodalync_types::{Metadata, ProvenanceEntry};
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
        create_test_payment_with_provenance(
            amount,
            recipient,
            query_hash,
            content_hash(b"channel"),
            vec![],
        )
    }

    fn create_test_payment_with_provenance(
        amount: u64,
        recipient: PeerId,
        query_hash: nodalync_crypto::Hash,
        channel_id: nodalync_crypto::Hash,
        provenance: Vec<ProvenanceEntry>,
    ) -> Payment {
        Payment::new(
            content_hash(b"payment"),
            channel_id,
            amount,
            recipient,
            query_hash,
            provenance,
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

        // Open a channel with the requester (required for paid content)
        let channel_id = content_hash(b"test-query-channel");
        ops.accept_payment_channel(&channel_id, &requester, 500, 1000)
            .unwrap();

        let manifest = ops.get_content_manifest(&hash).unwrap().unwrap();
        let payment = create_test_payment_with_provenance(
            100,
            manifest.owner,
            hash,
            channel_id,
            manifest.provenance.root_l0l1.clone(),
        );

        let request = QueryRequestPayload {
            hash,
            query: None,
            payment,
            version_spec: None,
            payment_nonce: 1,
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

        // Open a channel with the requester (required for paid content)
        let channel_id = content_hash(b"test-insufficient-channel");
        ops.accept_payment_channel(&channel_id, &requester, 500, 1000)
            .unwrap();

        let manifest = ops.get_content_manifest(&hash).unwrap().unwrap();
        let payment = create_test_payment(100, manifest.owner, hash); // Less than 1000

        let request = QueryRequestPayload {
            hash,
            query: None,
            payment,
            version_spec: None,
            payment_nonce: 1,
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

        // Open a channel with the requester (required for paid content)
        let channel_id = content_hash(b"test-settlement-channel");
        ops.accept_payment_channel(&channel_id, &requester, 500, 1000)
            .unwrap();

        let manifest = ops.get_content_manifest(&hash).unwrap().unwrap();
        let payment = create_test_payment_with_provenance(
            100,
            manifest.owner,
            hash,
            channel_id,
            manifest.provenance.root_l0l1.clone(),
        );

        let request = QueryRequestPayload {
            hash,
            query: None,
            payment,
            version_spec: None,
            payment_nonce: 1,
        };
        ops.handle_query_request(&requester, &request)
            .await
            .unwrap();

        // Now we should have pending distributions
        let pending = ops.get_pending_settlement_total().unwrap();
        assert_eq!(pending, 100); // Full payment amount
    }

    #[tokio::test]
    async fn test_channel_nonce_updates_after_payment() {
        let (mut ops, _temp) = create_test_ops();
        let content = b"Premium knowledge content";
        let requester = test_peer_id();

        // Create and publish paid content
        let meta = Metadata::new("Premium Knowledge", content.len() as u64);
        let hash = ops.create_content(content, meta).unwrap();
        ops.publish_content(&hash, Visibility::Shared, 100)
            .await
            .unwrap();

        // Open a channel with the requester
        let channel_id = content_hash(b"test-channel-nonce");
        ops.accept_payment_channel(&channel_id, &requester, 500, 1000)
            .unwrap();

        // Get the channel nonce (should be 0 initially)
        let channel = ops.state.channels.get(&requester).unwrap().unwrap();
        assert_eq!(channel.nonce, 0);

        // Get manifest to create a payment with correct provenance
        let manifest = ops.state.manifests.load(&hash).unwrap().unwrap();

        // Create payment with correct provenance
        let payment = Payment::new(
            content_hash(b"payment1"),
            channel_id,
            100,
            ops.peer_id(),
            hash,
            manifest.provenance.root_l0l1.clone(),
            current_timestamp(),
            Signature::from_bytes([0u8; 64]),
        );

        // First query with nonce 1 should succeed
        let request = QueryRequestPayload {
            hash,
            query: None,
            payment: payment.clone(),
            version_spec: None,
            payment_nonce: 1,
        };
        let result = ops.handle_query_request(&requester, &request).await;
        if let Err(ref e) = result {
            eprintln!("First query failed: {:?}", e);
        }
        assert!(result.is_ok(), "First query should succeed: {:?}", result);

        // Channel nonce should be updated to 1
        let channel = ops.state.channels.get(&requester).unwrap().unwrap();
        assert_eq!(channel.nonce, 1);

        // Second query with same nonce should fail (replay attack prevention)
        let request2 = QueryRequestPayload {
            hash,
            query: None,
            payment: payment.clone(),
            version_spec: None,
            payment_nonce: 1, // Same nonce as before - should fail
        };
        let result2 = ops.handle_query_request(&requester, &request2).await;
        assert!(
            matches!(result2, Err(OpsError::PaymentValidationFailed(_))),
            "Replay with same nonce should fail: {:?}",
            result2
        );

        // Query with higher nonce should succeed
        let payment3 = Payment::new(
            content_hash(b"payment3"),
            channel_id,
            100,
            ops.peer_id(),
            hash,
            manifest.provenance.root_l0l1.clone(),
            current_timestamp(),
            Signature::from_bytes([0u8; 64]),
        );
        let request3 = QueryRequestPayload {
            hash,
            query: None,
            payment: payment3,
            version_spec: None,
            payment_nonce: 2,
        };
        let result3 = ops.handle_query_request(&requester, &request3).await;
        assert!(
            result3.is_ok(),
            "Higher nonce should succeed: {:?}",
            result3
        );

        // Channel nonce should now be 2
        let channel = ops.state.channels.get(&requester).unwrap().unwrap();
        assert_eq!(channel.nonce, 2);
    }

    #[tokio::test]
    async fn test_free_content_no_channel_needed() {
        let (mut ops, _temp) = create_test_ops();
        let content = b"Free content for everyone";
        let requester = test_peer_id();

        // Create and publish free content (price = 0)
        let meta = Metadata::new("Free Knowledge", content.len() as u64);
        let hash = ops.create_content(content, meta).unwrap();
        ops.publish_content(&hash, Visibility::Shared, 0)
            .await
            .unwrap();

        // Query without channel should work for free content
        let payment = create_test_payment(0, ops.peer_id(), hash);
        let request = QueryRequestPayload {
            hash,
            query: None,
            payment,
            version_spec: None,
            payment_nonce: 0,
        };

        let result = ops.handle_query_request(&requester, &request).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().content, content.to_vec());
    }

    #[tokio::test]
    async fn test_paid_content_requires_channel() {
        let (mut ops, _temp) = create_test_ops();
        let content = b"Premium paid content";
        let requester = test_peer_id();

        // Create and publish paid content (price > 0)
        let meta = Metadata::new("Premium Knowledge", content.len() as u64);
        let hash = ops.create_content(content, meta).unwrap();
        ops.publish_content(&hash, Visibility::Shared, 100)
            .await
            .unwrap();

        // Query WITHOUT opening a channel should fail with ChannelRequired
        let manifest = ops.get_content_manifest(&hash).unwrap().unwrap();
        let payment = create_test_payment(100, manifest.owner, hash);
        let request = QueryRequestPayload {
            hash,
            query: None,
            payment,
            version_spec: None,
            payment_nonce: 1,
        };

        let result = ops.handle_query_request(&requester, &request).await;
        assert!(
            matches!(result, Err(OpsError::ChannelRequired)),
            "Paid content without channel should return ChannelRequired: {:?}",
            result
        );
    }
}
