//! Incoming message handlers.
//!
//! This module implements handlers for incoming protocol messages,
//! processing requests from other nodes.

use nodalync_crypto::{content_hash, Hash, PeerId, PrivateKey, PublicKey, Signature};
use nodalync_econ::distribute_revenue;
use nodalync_net::NetworkEvent;
use nodalync_store::{ChannelStore, ContentStore, ManifestStore};
use nodalync_types::{Channel, ChannelState, Payment, Visibility};
use nodalync_valid::Validator;
use nodalync_wire::{
    decode_message, decode_payload, AnnouncePayload, ChannelAcceptPayload, ChannelCloseAckPayload,
    ChannelClosePayload, ChannelOpenPayload, MessageType, PaymentReceipt, PreviewRequestPayload,
    PreviewResponsePayload, QueryRequestPayload, QueryResponsePayload, SearchPayload,
    SearchResponsePayload, SearchResult as WireSearchResult, VersionInfo, VersionRequestPayload,
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
    /// CRITICAL: This handler ensures TRUSTLESS operation by requiring
    /// on-chain settlement confirmation BEFORE delivering content.
    ///
    /// Flow:
    /// 1. Load manifest
    /// 2. Validate access
    /// 3. Validate payment amount
    /// 4. Validate payment signature for paid content (channel, nonce, signature)
    /// 5. Update channel state (credit)
    /// 6. Generate payment ID
    /// 7. Calculate 95/5 distribution (5% synthesis fee to owner, 95% to root L0/L1 contributors)
    /// 8. **IMMEDIATE ON-CHAIN SETTLEMENT** - blocks until confirmed
    /// 9. Update manifest economics (only after settlement)
    /// 10. Load and return content with receipt
    ///
    /// If settlement fails, the query is REJECTED and no content is delivered.
    /// This ensures creators are always paid before content is released.
    /// The 95/5 distribution split is a CORE PROTOCOL FEATURE.
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

        // 6. Generate payment ID
        let payment_id =
            content_hash(&[request.hash.0.as_slice(), &timestamp.to_be_bytes()].concat());

        // 7. Calculate 95/5 distribution (CORE PROTOCOL FEATURE)
        // - 5% synthesis fee goes to the content owner
        // - 95% root pool is distributed proportionally to foundational L0/L1 contributors
        let distributions = distribute_revenue(
            payment_amount,
            &manifest.owner,
            &manifest.provenance.root_l0l1,
        );

        // Log the distribution split for transparency
        for dist in &distributions {
            tracing::debug!(
                recipient = %dist.recipient,
                amount = dist.amount,
                source_hash = %dist.source_hash,
                "Distribution calculated for settlement"
            );
        }

        tracing::info!(
            payment_id = %payment_id,
            total_amount = payment_amount,
            num_recipients = distributions.len(),
            "95/5 distribution calculated: 5% synthesis fee to owner, 95% to {} root contributors",
            manifest.provenance.root_l0l1.len()
        );

        // 8. IMMEDIATE ON-CHAIN SETTLEMENT (required before content delivery)
        // Content is ONLY delivered after payment is confirmed on-chain.
        // This ensures trustless operation - no content without verified payment.
        // The settlement batch includes ALL distributions from the 95/5 split.
        let transaction_id = if payment_amount > 0 {
            if let Some(settlement) = self.settlement().cloned() {
                // Create a single-payment batch for immediate settlement
                // Note: create_settlement_batch internally calls distribute_revenue
                // to compute the same 95/5 split for all recipients
                let payment = Payment::new(
                    payment_id,
                    Hash([0u8; 32]),
                    payment_amount,
                    manifest.owner,
                    request.hash,
                    manifest.provenance.root_l0l1.clone(),
                    timestamp,
                    nodalync_crypto::Signature::from_bytes([0u8; 64]),
                );

                let batch = nodalync_econ::create_settlement_batch(&[payment]);

                // Submit to chain and WAIT for confirmation
                match settlement.settle_batch(&batch).await {
                    Ok(tx_id) => {
                        tracing::info!(
                            payment_id = %payment_id,
                            tx_id = %tx_id,
                            amount = payment_amount,
                            num_recipients = distributions.len(),
                            hash = %request.hash,
                            "Payment settled on-chain with 95/5 distribution before content delivery"
                        );
                        Some(tx_id.to_string())
                    }
                    Err(e) => {
                        tracing::error!(
                            payment_id = %payment_id,
                            error = %e,
                            hash = %request.hash,
                            "On-chain settlement FAILED - refusing to deliver content"
                        );
                        return Err(OpsError::SettlementFailed(format!(
                            "Payment settlement failed: {}. Content will not be delivered without confirmed payment.",
                            e
                        )));
                    }
                }
            } else {
                // No settlement configured - cannot process paid queries trustlessly
                tracing::error!(
                    hash = %request.hash,
                    price = payment_amount,
                    "Paid query received but no settlement configured"
                );
                return Err(OpsError::SettlementRequired);
            }
        } else {
            None // Free content, no settlement needed
        };

        // 9. Update manifest economics (only after successful settlement)
        manifest.economics.record_query(payment_amount);
        manifest.updated_at = timestamp;
        self.state.manifests.update(&manifest)?;

        // 10. Load and return content (settlement confirmed)
        let content = self
            .state
            .content
            .load(&request.hash)?
            .ok_or(OpsError::NotFound(request.hash))?;

        let receipt = PaymentReceipt {
            payment_id,
            amount: payment_amount,
            timestamp,
            channel_nonce: request.payment_nonce,
            distributor_signature: Signature::from_bytes([0u8; 64]),
        };

        tracing::info!(
            hash = %request.hash,
            payment_amount = payment_amount,
            transaction_id = ?transaction_id,
            "Content delivered after settlement confirmation"
        );

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
    /// 2. Register peer's Hedera account if provided
    /// 3. Create channel state
    /// 4. Return ChannelAcceptPayload with our Hedera account
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

        // 2. Register peer's Hedera account if provided (enables on-chain channels)
        if let Some(peer_hedera) = &request.hedera_account {
            if let Some(settlement) = self.settlement() {
                if let Ok(account_id) = nodalync_settle::AccountId::from_string(peer_hedera) {
                    settlement.register_peer_account(requester, account_id);
                    debug!(
                        peer = %requester,
                        hedera_account = %peer_hedera,
                        "Registered peer's Hedera account from channel open"
                    );
                }
            }
        }

        // 3. Create channel state
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

        // 4. Return accept payload with our Hedera account
        let hedera_account = self.settlement().map(|s| s.get_own_account_string());

        Ok(ChannelAcceptPayload {
            channel_id: request.channel_id,
            initial_balance: my_deposit,
            funding_tx: None, // No on-chain funding for MVP
            hedera_account,
        })
    }

    /// Handle an incoming channel accept response.
    ///
    /// Transitions channel from Opening to Open when peer accepts.
    /// This is called on the initiator side when they receive the
    /// ChannelAccept message from the responder.
    pub fn handle_channel_accept(
        &mut self,
        peer: &PeerId,
        response: &ChannelAcceptPayload,
    ) -> OpsResult<()> {
        let timestamp = current_timestamp();

        // Get the channel we opened with this peer
        let mut channel = self
            .state
            .channels
            .get(peer)?
            .ok_or(OpsError::ChannelNotFound)?;

        // Verify the channel ID matches
        if channel.channel_id != response.channel_id {
            return Err(OpsError::invalid_operation("channel ID mismatch"));
        }

        // Verify channel is in Opening state
        if channel.state != ChannelState::Opening {
            return Err(OpsError::invalid_operation(format!(
                "channel not in Opening state: {:?}",
                channel.state
            )));
        }

        // Register peer's Hedera account if provided (enables on-chain channels)
        if let Some(peer_hedera) = &response.hedera_account {
            if let Some(settlement) = self.settlement() {
                if let Ok(account_id) = nodalync_settle::AccountId::from_string(peer_hedera) {
                    settlement.register_peer_account(peer, account_id);
                    debug!(
                        peer = %peer,
                        hedera_account = %peer_hedera,
                        "Registered peer's Hedera account from channel accept"
                    );
                }
            }
        }

        // Transition to Open state with their deposit
        channel.mark_open(response.initial_balance, timestamp);
        self.state.channels.update(peer, &channel)?;

        debug!(
            channel_id = %response.channel_id,
            their_deposit = response.initial_balance,
            "Channel accepted and opened"
        );

        Ok(())
    }

    /// Handle an incoming channel close request.
    ///
    /// This is called on the responder side when receiving a cooperative close request.
    ///
    /// 1. Verify channel exists and ID matches
    /// 2. Verify the initiator's signature
    /// 3. Verify proposed balances match our local state
    /// 4. Sign and return our signature
    ///
    /// The initiator will then submit both signatures to the chain.
    pub fn handle_channel_close_request(
        &mut self,
        requester: &PeerId,
        request: &ChannelClosePayload,
        private_key: &PrivateKey,
        requester_public_key: Option<&PublicKey>,
    ) -> OpsResult<ChannelCloseAckPayload> {
        use nodalync_types::PendingClose;
        use nodalync_valid::{sign_channel_close, verify_channel_close_signature};

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

        // Cannot close already closed channel
        if channel.is_closed() {
            return Err(OpsError::invalid_operation("channel already closed"));
        }

        // Cannot close if already disputing
        if channel.pending_dispute.is_some() {
            return Err(OpsError::invalid_operation("channel has pending dispute"));
        }

        // 2. Verify initiator's signature (if we have their public key)
        // Note: In a production system, we'd look up the peer's public key
        // from a registry. For now, signature verification is optional.
        if let Some(pubkey) = requester_public_key {
            let valid = verify_channel_close_signature(
                pubkey,
                &request.channel_id,
                request.nonce,
                request.final_balances.initiator,
                request.final_balances.responder,
                &request.initiator_signature,
            );
            if !valid {
                return Err(OpsError::invalid_operation(
                    "invalid initiator signature on close request",
                ));
            }
        }

        // 3. Verify proposed balances match our local state
        // Note: From our perspective as responder:
        // - Their initiator_balance is what THEY think they should get
        // - Their responder_balance is what WE should get (from their POV)
        //
        // From our local channel state:
        // - their_balance is what THEY have
        // - my_balance is what WE have
        //
        // So: initiator_balance should == their_balance (what they have)
        //     responder_balance should == my_balance (what we have)
        if request.final_balances.initiator != channel.their_balance
            || request.final_balances.responder != channel.my_balance
        {
            tracing::warn!(
                channel_id = %channel.channel_id,
                proposed_initiator = request.final_balances.initiator,
                proposed_responder = request.final_balances.responder,
                our_their_balance = channel.their_balance,
                our_my_balance = channel.my_balance,
                "Close request balances don't match local state"
            );
            return Err(OpsError::invalid_operation(
                "proposed balances don't match local state",
            ));
        }

        // Verify nonce matches
        if request.nonce != channel.nonce {
            tracing::warn!(
                channel_id = %channel.channel_id,
                proposed_nonce = request.nonce,
                our_nonce = channel.nonce,
                "Close request nonce doesn't match"
            );
            // We allow closing with a higher nonce (they may have payments we haven't seen yet)
            // but not a lower nonce (potential replay attack)
            if request.nonce < channel.nonce {
                return Err(OpsError::invalid_operation(
                    "proposed nonce is lower than local state",
                ));
            }
        }

        // 4. Sign the close message as responder
        // We sign with the same parameters they sent (after validation)
        let responder_signature = sign_channel_close(
            private_key,
            &request.channel_id,
            request.nonce,
            request.final_balances.initiator,
            request.final_balances.responder,
        );

        // Store pending close state (as responder)
        let pending_close = PendingClose::new_as_responder(
            (
                request.final_balances.initiator,
                request.final_balances.responder,
            ),
            request.nonce,
            request.initiator_signature,
            timestamp,
        );
        channel.pending_close = Some(pending_close);
        channel.mark_closing(timestamp);
        self.state.channels.update(requester, &channel)?;

        debug!(
            channel_id = %request.channel_id,
            "Signed channel close acknowledgment"
        );

        Ok(ChannelCloseAckPayload {
            channel_id: request.channel_id,
            responder_signature,
        })
    }

    /// Handle an incoming channel close request (legacy without signature).
    ///
    /// This is a backward-compatible handler that doesn't require signing.
    /// The channel is closed immediately without on-chain settlement.
    pub fn handle_channel_close(
        &mut self,
        requester: &PeerId,
        request: &ChannelClosePayload,
    ) -> OpsResult<()> {
        let timestamp = current_timestamp();

        // Verify channel exists
        let mut channel = self
            .state
            .channels
            .get(requester)?
            .ok_or(OpsError::ChannelNotFound)?;

        // Verify channel ID matches
        if channel.channel_id != request.channel_id {
            return Err(OpsError::invalid_operation("channel ID mismatch"));
        }

        // Mark channel as closed (no on-chain settlement)
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
                    "Received message type {:?} from peer {}, sender {:?}, payload length: {}",
                    message.message_type,
                    peer,
                    message.sender,
                    message.payload.len()
                );

                // Use the sender from the signed message - this is the authoritative Nodalync PeerId.
                // The sender is included in the message hash that's signed, so we can trust it
                // as long as the signature is valid (TODO: add signature verification here).
                let nodalync_peer = message.sender;

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

                        // Handle query request and convert errors to QueryError responses
                        match self.handle_query_request(&nodalync_peer, &request).await {
                            Ok(response) => {
                                let response_bytes = nodalync_wire::encode_payload(&response)
                                    .map_err(|e| {
                                        OpsError::invalid_operation(format!(
                                            "encoding error: {}",
                                            e
                                        ))
                                    })?;
                                Ok(Some((MessageType::QueryResponse, response_bytes)))
                            }
                            Err(OpsError::ChannelRequired) => {
                                // Return QueryError with our peer IDs so client can open channel
                                use nodalync_wire::QueryErrorPayload;
                                let error_payload = QueryErrorPayload {
                                    hash: request.hash,
                                    error_code: nodalync_types::ErrorCode::ChannelNotFound,
                                    message: Some(
                                        "Payment channel required for paid content".to_string(),
                                    ),
                                    required_channel_peer_id: Some(self.peer_id()),
                                    required_channel_libp2p_peer: self
                                        .network()
                                        .map(|n| n.local_peer_id().to_string()),
                                };
                                info!(
                                    requester = %nodalync_peer,
                                    our_peer_id = %self.peer_id(),
                                    "Returning ChannelRequired error with peer info"
                                );
                                let error_bytes = nodalync_wire::encode_payload(&error_payload)
                                    .map_err(|e| {
                                        OpsError::invalid_operation(format!(
                                            "encoding error: {}",
                                            e
                                        ))
                                    })?;
                                Ok(Some((MessageType::QueryError, error_bytes)))
                            }
                            Err(e) => {
                                // For other errors, return QueryError without peer info
                                use nodalync_wire::QueryErrorPayload;
                                let error_payload = QueryErrorPayload {
                                    hash: request.hash,
                                    error_code: e.error_code(),
                                    message: Some(e.to_string()),
                                    required_channel_peer_id: None,
                                    required_channel_libp2p_peer: None,
                                };
                                let error_bytes = nodalync_wire::encode_payload(&error_payload)
                                    .map_err(|e| {
                                        OpsError::invalid_operation(format!(
                                            "encoding error: {}",
                                            e
                                        ))
                                    })?;
                                Ok(Some((MessageType::QueryError, error_bytes)))
                            }
                        }
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
                        // Use legacy handler for now (no private key available here)
                        // A production implementation would need to pass the node's private key
                        self.handle_channel_close(&nodalync_peer, &request)?;
                        Ok(None) // No response needed for legacy close
                    }
                    MessageType::ChannelCloseAck => {
                        // This is handled by the initiator when they receive the response
                        // No action needed here as it's processed in close_payment_channel()
                        debug!("Received channel close ack (handled by initiator)");
                        Ok(None)
                    }
                    MessageType::ChannelAccept => {
                        let response: ChannelAcceptPayload = decode_payload(&message.payload)
                            .map_err(|e| {
                                OpsError::invalid_operation(format!("decode error: {}", e))
                            })?;
                        debug!("Received channel accept response");
                        self.handle_channel_accept(&nodalync_peer, &response)?;
                        Ok(None) // No response needed for accept
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
    use nodalync_store::NodeStateConfig;
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

        // Paid content queries require on-chain settlement to be configured.
        // Without settlement, the handler correctly rejects the query.
        let result = ops.handle_query_request(&requester, &request).await;
        assert!(
            matches!(result, Err(OpsError::SettlementRequired)),
            "Paid queries without settlement must return SettlementRequired: {:?}",
            result
        );
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
            hedera_account: Some("0.0.12345".to_string()),
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
            hedera_account: None,
        };
        ops.handle_channel_open(&requester, &open_request).unwrap();

        // Now close it
        let close_request = ChannelClosePayload {
            channel_id,
            nonce: 0,
            final_balances: ChannelBalances::new(1000, 1000),
            initiator_signature: Signature::from_bytes([0u8; 64]),
        };

        ops.handle_channel_close(&requester, &close_request)
            .unwrap();

        // Verify channel is closed
        let channel = ops.get_payment_channel(&requester).unwrap().unwrap();
        assert!(channel.is_closed());
    }

    #[tokio::test]
    async fn test_handle_channel_accept_success() {
        let (mut ops, _temp) = create_test_ops();
        let peer = test_peer_id();

        // Simulate initiator opening a channel (creates in Opening state)
        let channel = ops
            .open_payment_channel(&peer, 100_0000_0000)
            .await
            .unwrap();

        // Verify channel is in Opening state
        let stored_channel = ops.get_payment_channel(&peer).unwrap().unwrap();
        assert_eq!(stored_channel.state, nodalync_types::ChannelState::Opening);

        // Simulate receiving ChannelAccept from peer
        let accept_response = ChannelAcceptPayload {
            channel_id: channel.channel_id,
            initial_balance: 100_0000_0000, // Their matching deposit
            funding_tx: None,
            hedera_account: Some("0.0.54321".to_string()),
        };

        ops.handle_channel_accept(&peer, &accept_response).unwrap();

        // Verify channel is now Open
        let opened_channel = ops.get_payment_channel(&peer).unwrap().unwrap();
        assert!(opened_channel.is_open());
        assert_eq!(opened_channel.their_balance, 100_0000_0000);
    }

    #[tokio::test]
    async fn test_handle_channel_accept_wrong_channel_id() {
        let (mut ops, _temp) = create_test_ops();
        let peer = test_peer_id();

        // Open a channel
        ops.open_payment_channel(&peer, 100_0000_0000)
            .await
            .unwrap();

        // Try to accept with wrong channel ID
        let wrong_accept = ChannelAcceptPayload {
            channel_id: content_hash(b"wrong channel"),
            initial_balance: 100_0000_0000,
            funding_tx: None,
            hedera_account: None,
        };

        let result = ops.handle_channel_accept(&peer, &wrong_accept);
        assert!(matches!(result, Err(OpsError::InvalidOperation(_))));
    }

    #[test]
    fn test_handle_channel_accept_wrong_state() {
        let (mut ops, _temp) = create_test_ops();
        let peer = test_peer_id();
        let channel_id = content_hash(b"test channel");

        // Accept a channel (creates in Open state directly)
        ops.accept_payment_channel(&channel_id, &peer, 500, 500)
            .unwrap();

        // Try to accept again (channel already Open)
        let accept = ChannelAcceptPayload {
            channel_id,
            initial_balance: 500,
            funding_tx: None,
            hedera_account: None,
        };

        let result = ops.handle_channel_accept(&peer, &accept);
        assert!(matches!(result, Err(OpsError::InvalidOperation(_))));
    }

    #[test]
    fn test_handle_channel_accept_no_channel() {
        let (mut ops, _temp) = create_test_ops();
        let peer = test_peer_id();

        // Try to accept without having opened a channel
        let accept = ChannelAcceptPayload {
            channel_id: content_hash(b"nonexistent"),
            initial_balance: 500,
            funding_tx: None,
            hedera_account: None,
        };

        let result = ops.handle_channel_accept(&peer, &accept);
        assert!(matches!(result, Err(OpsError::ChannelNotFound)));
    }

    #[tokio::test]
    async fn test_paid_query_requires_settlement() {
        // This test verifies that paid content queries are rejected without
        // on-chain settlement configured. This is a CRITICAL security property:
        // content providers must not deliver content without payment confirmation.
        let (mut ops, _temp) = create_test_ops();

        // Create and publish content
        let content = b"Content for distribution test";
        let meta = Metadata::new("Dist Test", content.len() as u64);
        let hash = ops.create_content(content, meta).unwrap();
        ops.publish_content(&hash, Visibility::Shared, 100)
            .await
            .unwrap();

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

        // Without settlement configured, paid queries MUST be rejected
        let result = ops.handle_query_request(&requester, &request).await;
        assert!(
            matches!(result, Err(OpsError::SettlementRequired)),
            "Paid queries without settlement MUST return SettlementRequired for trustless operation: {:?}",
            result
        );
    }

    #[tokio::test]
    async fn test_nonce_updated_before_settlement_for_replay_protection() {
        // This test validates that channel nonces are updated BEFORE settlement
        // is attempted. This is a deliberate security choice:
        //
        // If we waited until after settlement to update the nonce, and settlement
        // succeeded but we crashed before updating the nonce, a replay attack
        // could occur (same payment nonce reused).
        //
        // By updating the nonce early, we ensure that even if settlement fails,
        // the same nonce cannot be reused. The client must increment the nonce
        // for any retry. This is more conservative and prevents double-spend.
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

        // Query with nonce 1 - without settlement, will fail at settlement step
        let request = QueryRequestPayload {
            hash,
            query: None,
            payment: payment.clone(),
            version_spec: None,
            payment_nonce: 1,
        };
        let result = ops.handle_query_request(&requester, &request).await;
        assert!(
            matches!(result, Err(OpsError::SettlementRequired)),
            "Paid queries require settlement: {:?}",
            result
        );

        // Channel nonce IS updated (for replay protection) even though settlement failed.
        // This prevents the same nonce from being reused in a retry attack.
        let channel = ops.state.channels.get(&requester).unwrap().unwrap();
        assert_eq!(
            channel.nonce, 1,
            "Nonce should be updated for replay protection even when settlement fails"
        );

        // Attempting to reuse the same nonce should fail
        let request2 = QueryRequestPayload {
            hash,
            query: None,
            payment: payment.clone(),
            version_spec: None,
            payment_nonce: 1, // Same nonce - should fail
        };
        let result2 = ops.handle_query_request(&requester, &request2).await;
        assert!(
            matches!(result2, Err(OpsError::PaymentValidationFailed(_))),
            "Replay with same nonce must fail: {:?}",
            result2
        );
    }

    #[tokio::test]
    async fn test_replay_attack_rejected_before_settlement() {
        // This test verifies that replay attacks (same nonce) are rejected
        // even without settlement configured. This is a security validation.
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
        let channel_id = content_hash(b"test-replay-channel");
        ops.accept_payment_channel(&channel_id, &requester, 500, 1000)
            .unwrap();

        // Manually set the channel nonce to simulate a prior payment
        {
            let mut channel = ops.state.channels.get(&requester).unwrap().unwrap();
            channel.nonce = 5;
            ops.state.channels.update(&requester, &channel).unwrap();
        }

        let manifest = ops.state.manifests.load(&hash).unwrap().unwrap();

        // Try to replay with an old nonce (should fail BEFORE settlement check)
        let payment = Payment::new(
            content_hash(b"replay-payment"),
            channel_id,
            100,
            ops.peer_id(),
            hash,
            manifest.provenance.root_l0l1.clone(),
            current_timestamp(),
            Signature::from_bytes([0u8; 64]),
        );

        let request = QueryRequestPayload {
            hash,
            query: None,
            payment,
            version_spec: None,
            payment_nonce: 3, // Old nonce (current is 5)
        };

        let result = ops.handle_query_request(&requester, &request).await;
        assert!(
            matches!(result, Err(OpsError::PaymentValidationFailed(_))),
            "Replay with old nonce should fail with PaymentValidationFailed: {:?}",
            result
        );
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
