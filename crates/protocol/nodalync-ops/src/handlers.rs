//! Incoming message handlers.
//!
//! This module implements handlers for incoming protocol messages,
//! processing requests from other nodes.

use nodalync_crypto::{content_hash, Hash, PeerId, PrivateKey, Signature};
use nodalync_econ::distribute_revenue;
use nodalync_net::NetworkEvent;
use nodalync_store::{ChannelStore, ContentStore, ManifestStore, PeerStore};
use nodalync_types::{Channel, ChannelState, Payment, Visibility};
use nodalync_valid::Validator;
use nodalync_wire::{
    decode_message, decode_payload, AnnouncePayload, ChannelAcceptPayload, ChannelCloseAckPayload,
    ChannelClosePayload, ChannelOpenPayload, MessageType, PaymentReceipt, PreviewRequestPayload,
    PreviewResponsePayload, QueryRequestPayload, QueryResponsePayload, SearchPayload,
    SearchResponsePayload, SearchResult as WireSearchResult, VersionInfo, VersionRequestPayload,
    VersionResponsePayload,
};
use tracing::{debug, info, warn};

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
        if matches!(
            manifest.visibility,
            Visibility::Private | Visibility::Offline
        ) {
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
        if matches!(
            manifest.visibility,
            Visibility::Private | Visibility::Offline
        ) {
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
                    let requester_pubkey = self
                        .state
                        .peers
                        .get(requester)
                        .ok()
                        .flatten()
                        .map(|info| info.public_key)
                        .filter(|pk| pk.0 != [0u8; 32]);

                    nodalync_valid::validate_payment(
                        &request.payment,
                        &channel,
                        &manifest,
                        requester_pubkey.as_ref(),
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
                // Create payment record (include nonce for uniqueness across rapid queries)
                let payment_id = content_hash(
                    &[
                        request.hash.0.as_slice(),
                        &timestamp.to_be_bytes(),
                        &payment_amount.to_be_bytes(),
                        &request.payment_nonce.to_be_bytes(),
                    ]
                    .concat(),
                );

                let payment_sig = match self.private_key() {
                    Some(pk) => {
                        // Create a temporary payment to compute the signing message
                        let tmp = Payment::new(
                            payment_id,
                            channel.channel_id,
                            payment_amount,
                            self.peer_id(),
                            request.hash,
                            manifest.provenance.root_l0l1.clone(),
                            timestamp,
                            Signature::from_bytes([0u8; 64]),
                        );
                        let msg = nodalync_valid::construct_payment_message(&tmp);
                        nodalync_crypto::sign(pk, &msg)
                    }
                    None => Signature::from_bytes([0u8; 64]),
                };
                let payment = Payment::new(
                    payment_id,
                    channel.channel_id,
                    payment_amount,
                    self.peer_id(),
                    request.hash,
                    manifest.provenance.root_l0l1.clone(),
                    timestamp,
                    payment_sig,
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

        // 6. Generate payment ID (include nonce for uniqueness)
        let payment_id = content_hash(
            &[
                request.hash.0.as_slice(),
                &timestamp.to_be_bytes(),
                &request.payment_nonce.to_be_bytes(),
            ]
            .concat(),
        );

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
                let settle_sig = match self.private_key() {
                    Some(pk) => {
                        let tmp = Payment::new(
                            payment_id,
                            Hash([0u8; 32]),
                            payment_amount,
                            manifest.owner,
                            request.hash,
                            manifest.provenance.root_l0l1.clone(),
                            timestamp,
                            Signature::from_bytes([0u8; 64]),
                        );
                        let msg = nodalync_valid::construct_payment_message(&tmp);
                        nodalync_crypto::sign(pk, &msg)
                    }
                    None => Signature::from_bytes([0u8; 64]),
                };
                let payment = Payment::new(
                    payment_id,
                    Hash([0u8; 32]),
                    payment_amount,
                    manifest.owner,
                    request.hash,
                    manifest.provenance.root_l0l1.clone(),
                    timestamp,
                    settle_sig,
                );

                let batch = nodalync_econ::create_settlement_batch(&[payment]);

                // Submit to chain and WAIT for confirmation (with timeout)
                let settlement_timeout =
                    std::time::Duration::from_millis(self.config.settlement_timeout_ms);
                let settle_result =
                    tokio::time::timeout(settlement_timeout, settlement.settle_batch(&batch)).await;

                match settle_result {
                    Ok(Ok(tx_id)) => {
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
                    Ok(Err(e)) => {
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
                    Err(_elapsed) => {
                        tracing::error!(
                            payment_id = %payment_id,
                            hash = %request.hash,
                            timeout_ms = self.config.settlement_timeout_ms,
                            "On-chain settlement TIMED OUT - refusing to deliver content"
                        );
                        return Err(OpsError::SettlementFailed(
                            "settlement timed out".to_string(),
                        ));
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

        let receipt_sig = match self.private_key() {
            Some(pk) => {
                let msg = nodalync_valid::construct_receipt_message(
                    &payment_id,
                    payment_amount,
                    timestamp,
                    request.payment_nonce,
                );
                nodalync_crypto::sign(pk, &msg)
            }
            None => Signature::from_bytes([0u8; 64]),
        };
        let receipt = PaymentReceipt {
            payment_id,
            amount: payment_amount,
            timestamp,
            channel_nonce: request.payment_nonce,
            distributor_signature: receipt_sig,
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
    /// Handle an incoming search request.
    ///
    /// Searches local content, then optionally forwards the query to connected
    /// peers if `hop_count < max_hops` (multi-hop search forwarding).
    /// This extends content discovery beyond directly connected peers — a node
    /// A↔B↔C can discover content on C even without a direct A↔C connection.
    pub async fn handle_search_request(
        &mut self,
        _requester: &PeerId,
        request: &SearchPayload,
    ) -> OpsResult<SearchResponsePayload> {
        use nodalync_store::ManifestFilter;
        use nodalync_types::L1Summary;
        use std::collections::HashSet;

        let query = request.query.to_lowercase();
        let limit = request.limit.min(100);

        // Track seen hashes for dedup across local + forwarded results
        let mut seen_hashes = HashSet::new();

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

        // Get our listen addresses to include in results for reconnection
        let publisher_addresses: Vec<String> = self
            .network()
            .map(|n| n.listen_addresses().iter().map(|a| a.to_string()).collect())
            .unwrap_or_default();

        // Convert to SearchResult
        let mut results: Vec<WireSearchResult> = manifests
            .iter()
            .map(|m| {
                seen_hashes.insert(m.hash);

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
                    publisher_addresses: publisher_addresses.clone(),
                }
            })
            .collect();

        // Apply max_price filter if specified
        if let Some(ref filters) = request.filters {
            if let Some(max_price) = filters.max_price {
                results.retain(|r| r.price <= max_price);
            }
        }

        // --- Multi-hop forwarding ---
        // If hop_count < max_hops AND we have a network, forward to our peers.
        // Cap at 3 hops max to prevent abuse, and limit to 3 peers for forwarding.
        let effective_max_hops = request.max_hops.min(3);
        if request.hop_count < effective_max_hops {
            if let Some(network) = self.network().cloned() {
                let our_peer_id = network.local_peer_id().to_string();

                // Build visited set for loop prevention
                let visited: HashSet<String> = request.visited_peers.iter().cloned().collect();

                // Build forwarded search payload with incremented hop count
                let mut forwarded_visited = request.visited_peers.clone();
                forwarded_visited.push(our_peer_id.clone());

                let forward_payload = SearchPayload {
                    query: request.query.clone(),
                    filters: request.filters.clone(),
                    limit: request.limit,
                    offset: request.offset,
                    max_hops: effective_max_hops,
                    hop_count: request.hop_count + 1,
                    visited_peers: forwarded_visited,
                };

                // Select peers to forward to (exclude visited peers, limit to 3)
                let forward_peers: Vec<_> = network
                    .connected_peers()
                    .into_iter()
                    .filter(|p| !visited.contains(&p.to_string()))
                    .take(3)
                    .collect();

                if !forward_peers.is_empty() {
                    tracing::debug!(
                        hop = request.hop_count + 1,
                        max_hops = effective_max_hops,
                        peer_count = forward_peers.len(),
                        query = %request.query,
                        "Forwarding search to peers"
                    );

                    // Forward concurrently with 3-second timeout per peer
                    let forward_futures: Vec<_> = forward_peers
                        .into_iter()
                        .map(|peer| {
                            let net = network.clone();
                            let payload = forward_payload.clone();
                            async move {
                                tokio::time::timeout(
                                    std::time::Duration::from_secs(3),
                                    net.send_search(peer, payload),
                                )
                                .await
                            }
                        })
                        .collect();

                    let responses = futures::future::join_all(forward_futures).await;

                    for result in responses {
                        if let Ok(Ok(response)) = result {
                            for r in response.results {
                                if seen_hashes.insert(r.hash) {
                                    results.push(r);
                                }
                            }
                        }
                    }
                }
            }
        }

        // Truncate to limit
        results.truncate(limit as usize);
        let total_count = results.len() as u64;

        Ok(SearchResponsePayload {
            results,
            total_count,
        })
    }

    /// Handle an incoming channel open request.
    ///
    /// SECURITY: This handler implements several security measures to prevent abuse:
    /// 1. Caps accepted deposits to `max_accept_deposit` from config
    /// 2. Rejects deposits below minimum with `ChannelDepositTooLow` error
    /// 3. Auto-deposit is gated by `auto_deposit_on_channel_open` config flag
    /// 4. Rate limits auto-deposits with a global cooldown
    /// 5. Uses fixed deposit amount from config (never derived from peer request)
    ///
    /// Steps:
    /// 1. Validate no existing channel
    /// 2. Cap and validate deposit amount
    /// 3. Register peer's Hedera account if provided
    /// 4. Auto-deposit to settlement contract if enabled and needed
    /// 5. Create channel state with capped deposit
    /// 6. Return ChannelAcceptPayload with our Hedera account
    pub async fn handle_channel_open(
        &mut self,
        requester: &PeerId,
        request: &ChannelOpenPayload,
    ) -> OpsResult<ChannelAcceptPayload> {
        let timestamp = current_timestamp();

        // 1. Validate no existing channel
        if self.state.channels.get(requester)?.is_some() {
            return Err(OpsError::ChannelAlreadyExists);
        }

        // 2. Cap the deposit to max_accept_deposit (SECURITY: prevents unbounded commitment)
        let capped_deposit = request
            .initial_balance
            .min(self.config.channel.max_accept_deposit);

        // Reject if capped deposit is below minimum (SECURITY: prevents dust channels)
        if capped_deposit < self.config.channel.min_deposit {
            return Err(OpsError::ChannelDepositTooLow {
                provided: capped_deposit,
                minimum: self.config.channel.min_deposit,
            });
        }

        debug!(
            peer = %requester,
            requested = request.initial_balance,
            capped = capped_deposit,
            max_accept = self.config.channel.max_accept_deposit,
            "Channel open request - deposit capped"
        );

        // 3. Register peer's Hedera account if provided (enables on-chain channels)
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

        // 4. Auto-deposit if enabled and needed (SECURITY: gated by config flag)
        // Only triggers if:
        // - auto_deposit_on_channel_open is true
        // - Settlement is configured
        // - Cooldown has elapsed
        // - Balance is below threshold
        if self.config.channel.auto_deposit_on_channel_open {
            if let Some(settlement) = self.settlement().cloned() {
                // Check cooldown (SECURITY: prevents rapid deposit spam)
                if self.can_auto_deposit() {
                    // Check current contract balance
                    if let Ok(balance) = settlement.get_balance().await {
                        if balance < self.config.channel.auto_deposit_min_balance {
                            // Use fixed deposit amount from config (SECURITY: never derived from peer request)
                            let deposit_amount = self.config.channel.auto_deposit_amount;
                            info!(
                                current_balance = balance,
                                min_balance = self.config.channel.auto_deposit_min_balance,
                                deposit_amount = deposit_amount,
                                "Auto-depositing to settlement contract for channel acceptance"
                            );

                            match settlement.deposit(deposit_amount).await {
                                Ok(tx_id) => {
                                    info!(tx_id = %tx_id, "Auto-deposit successful for channel acceptance");
                                    // Record the deposit time (SECURITY: sets cooldown)
                                    self.mark_auto_deposit();
                                }
                                Err(e) => {
                                    warn!(error = %e, "Auto-deposit failed, channel acceptance may fail");
                                    // Continue anyway - maybe balance is enough for this channel
                                }
                            }
                        }
                    }
                } else {
                    debug!(
                        cooldown_secs = self.config.channel.auto_deposit_cooldown_secs,
                        "Auto-deposit skipped due to cooldown"
                    );
                }
            }
        }

        // 5. Create channel state with CAPPED deposit
        // We match the capped deposit, not the original request
        let my_deposit = capped_deposit;

        let channel = Channel::accepted(
            request.channel_id,
            *requester,
            capped_deposit, // Their deposit (capped)
            my_deposit,     // Our deposit (matches capped amount)
            timestamp,
        );

        self.state.channels.create(requester, channel)?;

        // 6. Return accept payload with our Hedera account
        let hedera_account = self.settlement().map(|s| s.get_own_account_string());

        Ok(ChannelAcceptPayload {
            channel_id: request.channel_id,
            initial_balance: my_deposit, // Report the capped/actual deposit
            funding_tx: None,            // No on-chain funding for MVP
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

        // 2. Verify initiator's signature using peer key registry
        // Soft-fail: if peer key is unknown, skip verification (consistent with C1/C2)
        let requester_pubkey = self
            .state
            .peers
            .get(requester)
            .ok()
            .flatten()
            .map(|info| info.public_key)
            .filter(|pk| pk.0 != [0u8; 32]);

        if let Some(pubkey) = requester_pubkey {
            let valid = verify_channel_close_signature(
                &pubkey,
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
        } else {
            tracing::debug!(
                requester = %requester,
                "No public key for close requester - skipping signature verification"
            );
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

                // SECURITY: Verify message signature before trusting sender identity.
                let sender_pubkey = self
                    .state
                    .peers
                    .get(&message.sender)
                    .ok()
                    .flatten()
                    .map(|info| info.public_key)
                    .filter(|pk| pk.0 != [0u8; 32]);

                if let Some(pubkey) = &sender_pubkey {
                    if !nodalync_wire::verify_message_signature(&message, pubkey) {
                        tracing::warn!(
                            sender = %message.sender,
                            msg_type = ?message.message_type,
                            "Message signature verification FAILED - rejecting"
                        );
                        return Ok(None);
                    }
                } else {
                    // Peer key not yet known — soft-fail during bootstrap.
                    tracing::debug!(
                        sender = %message.sender,
                        "No public key for sender - skipping signature verification"
                    );
                }

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
                        let response = self.handle_channel_open(&nodalync_peer, &request).await?;
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

                        match self.private_key().cloned() {
                            Some(pk) => {
                                let ack = self.handle_channel_close_request(
                                    &nodalync_peer,
                                    &request,
                                    &pk,
                                )?;
                                let response_bytes =
                                    nodalync_wire::encode_payload(&ack).map_err(|e| {
                                        OpsError::invalid_operation(format!(
                                            "encoding error: {}",
                                            e
                                        ))
                                    })?;
                                Ok(Some((MessageType::ChannelCloseAck, response_bytes)))
                            }
                            None => Err(OpsError::invalid_operation(
                                "private key required for channel close",
                            )),
                        }
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
                        let response = self.handle_search_request(&nodalync_peer, &request).await?;
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
    use std::sync::Arc;
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

    #[tokio::test]
    async fn test_handle_channel_open() {
        let (mut ops, _temp) = create_test_ops();

        let requester = test_peer_id();
        let channel_id = content_hash(b"test channel");

        // Use deposit above minimum (100 HBAR in tinybars)
        let deposit = 200_0000_0000; // 200 HBAR

        let request = ChannelOpenPayload {
            channel_id,
            initial_balance: deposit,
            funding_tx: None,
            hedera_account: Some("0.0.12345".to_string()),
        };

        let response = ops.handle_channel_open(&requester, &request).await.unwrap();

        assert_eq!(response.channel_id, channel_id);
        assert_eq!(response.initial_balance, deposit);

        // Verify channel was created
        let channel = ops.get_payment_channel(&requester).unwrap().unwrap();
        assert!(channel.is_open());
    }

    #[tokio::test]
    async fn test_handle_channel_close() {
        let (private_key, public_key) = generate_identity();
        let peer_id = peer_id_from_public_key(&public_key);

        let temp_dir = TempDir::new().unwrap();
        let config = NodeStateConfig::new(temp_dir.path());
        let state = nodalync_store::NodeState::open(config).unwrap();
        let mut ops = DefaultNodeOperations::with_defaults(state, peer_id);

        // First open a channel with deposit above minimum (100 HBAR)
        let requester = test_peer_id();
        let channel_id = content_hash(b"test channel");
        let deposit = 200_0000_0000; // 200 HBAR in tinybars

        let open_request = ChannelOpenPayload {
            channel_id,
            initial_balance: deposit,
            funding_tx: None,
            hedera_account: None,
        };
        ops.handle_channel_open(&requester, &open_request)
            .await
            .unwrap();

        // Close it — requester is unknown peer, so soft-fail skips sig check
        let close_request = ChannelClosePayload {
            channel_id,
            nonce: 0,
            final_balances: ChannelBalances::new(deposit, deposit),
            initiator_signature: Signature::from_bytes([0u8; 64]),
        };

        let _ack = ops
            .handle_channel_close_request(&requester, &close_request, &private_key)
            .unwrap();

        // Verify channel is closing
        let channel = ops.get_payment_channel(&requester).unwrap().unwrap();
        assert!(channel.state == nodalync_types::ChannelState::Closing || channel.is_closed());
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

    #[tokio::test]
    async fn test_handle_search_request_matches_content() {
        let (mut ops, _temp) = create_test_ops();

        // Create and publish content
        let content = b"Blockchain and distributed ledger technology overview";
        let meta = Metadata::new("Blockchain Guide", content.len() as u64);
        let hash = ops.create_content(content, meta).unwrap();
        ops.publish_content(&hash, Visibility::Shared, 50)
            .await
            .unwrap();

        // Search for matching query
        let requester = test_peer_id();
        let request = SearchPayload {
            query: "blockchain".to_string(),
            filters: None,
            limit: 10,
            offset: 0,
            max_hops: 0,
            hop_count: 0,
            visited_peers: vec![],
        };

        let response = ops
            .handle_search_request(&requester, &request)
            .await
            .unwrap();
        assert!(
            response.total_count >= 1,
            "Should find at least one matching result"
        );
        assert!(response.results.iter().any(|r| r.hash == hash));
    }

    #[tokio::test]
    async fn test_handle_search_request_empty_results() {
        let (mut ops, _temp) = create_test_ops();

        // Search for content that does not exist
        let requester = test_peer_id();
        let request = SearchPayload {
            query: "nonexistent_term_xyz_12345".to_string(),
            filters: None,
            limit: 10,
            offset: 0,
            max_hops: 0,
            hop_count: 0,
            visited_peers: vec![],
        };

        let response = ops
            .handle_search_request(&requester, &request)
            .await
            .unwrap();
        assert_eq!(response.total_count, 0);
        assert!(response.results.is_empty());
    }

    #[tokio::test]
    async fn test_handle_network_event_unknown_type() {
        let (mut ops, _temp) = create_test_ops();

        // Create a network event with a PeerConnected type (not a request)
        let peer = nodalync_net::PeerId::random();
        let event = NetworkEvent::PeerConnected { peer };

        let result = ops.handle_network_event(event).await.unwrap();
        assert!(result.is_none(), "PeerConnected should return None");
    }

    // =========================================================================
    // Channel Open Security Tests
    // =========================================================================

    #[tokio::test]
    async fn test_channel_open_caps_deposit() {
        use crate::config::{ChannelConfig, OpsConfig};

        let temp_dir = TempDir::new().unwrap();
        let config = NodeStateConfig::new(temp_dir.path());
        let state = nodalync_store::NodeState::open(config).unwrap();

        let (_, public_key) = generate_identity();
        let peer_id = peer_id_from_public_key(&public_key);

        // Set max_accept_deposit to 500 HBAR (50_000_000_000 tinybars)
        let ops_config = OpsConfig::default()
            .with_channel(ChannelConfig::default().with_max_accept_deposit(500_0000_0000));

        let mut ops = DefaultNodeOperations::with_config(state, peer_id, ops_config);

        let requester = test_peer_id();
        let channel_id = content_hash(b"cap test channel");

        // Request with u64::MAX deposit (malicious attempt)
        let request = ChannelOpenPayload {
            channel_id,
            initial_balance: u64::MAX,
            funding_tx: None,
            hedera_account: None,
        };

        let response = ops.handle_channel_open(&requester, &request).await.unwrap();

        // Response should have capped deposit
        assert_eq!(
            response.initial_balance, 500_0000_0000,
            "Deposit should be capped to max_accept_deposit"
        );

        // Verify channel was created with capped deposit
        let channel = ops.get_payment_channel(&requester).unwrap().unwrap();
        assert_eq!(
            channel.their_balance, 500_0000_0000,
            "Channel their_balance should be capped"
        );
        assert_eq!(
            channel.my_balance, 500_0000_0000,
            "Channel my_balance should match capped deposit"
        );
    }

    #[tokio::test]
    async fn test_channel_open_rejects_below_minimum() {
        use crate::config::{ChannelConfig, OpsConfig};

        let temp_dir = TempDir::new().unwrap();
        let config = NodeStateConfig::new(temp_dir.path());
        let state = nodalync_store::NodeState::open(config).unwrap();

        let (_, public_key) = generate_identity();
        let peer_id = peer_id_from_public_key(&public_key);

        // Set min_deposit to 100 HBAR
        let ops_config =
            OpsConfig::default().with_channel(ChannelConfig::new(100_0000_0000, 1000_0000_0000));

        let mut ops = DefaultNodeOperations::with_config(state, peer_id, ops_config);

        let requester = test_peer_id();
        let channel_id = content_hash(b"min test channel");

        // Request with deposit below minimum (1 tinybar)
        let request = ChannelOpenPayload {
            channel_id,
            initial_balance: 1,
            funding_tx: None,
            hedera_account: None,
        };

        let result = ops.handle_channel_open(&requester, &request).await;
        assert!(
            matches!(
                result,
                Err(OpsError::ChannelDepositTooLow {
                    provided: 1,
                    minimum: 100_0000_0000
                })
            ),
            "Should reject deposit below minimum: {:?}",
            result
        );
    }

    #[tokio::test]
    async fn test_channel_open_no_auto_deposit_when_disabled() {
        use crate::config::{ChannelConfig, OpsConfig};
        use nodalync_test_utils::MockSettlement;

        let temp_dir = TempDir::new().unwrap();
        let config = NodeStateConfig::new(temp_dir.path());
        let state = nodalync_store::NodeState::open(config).unwrap();

        let (_, public_key) = generate_identity();
        let peer_id = peer_id_from_public_key(&public_key);

        // Disable auto-deposit on channel open (default)
        let ops_config =
            OpsConfig::default().with_channel(ChannelConfig::default().with_auto_deposit(false));

        // Create mock settlement with low balance to trigger deposit check
        let mock_settle = Arc::new(MockSettlement::new().with_balance(0));

        let mut ops = DefaultNodeOperations::with_config_and_settlement(
            state,
            peer_id,
            ops_config,
            mock_settle.clone(),
        );

        let requester = test_peer_id();
        let channel_id = content_hash(b"no auto deposit channel");

        let request = ChannelOpenPayload {
            channel_id,
            initial_balance: 200_0000_0000,
            funding_tx: None,
            hedera_account: None,
        };

        ops.handle_channel_open(&requester, &request).await.unwrap();

        // No auto-deposit should have occurred
        assert!(
            mock_settle.deposits().is_empty(),
            "No deposit should occur when auto_deposit_on_channel_open is false"
        );
    }

    #[tokio::test]
    async fn test_channel_open_auto_deposit_uses_config_amount() {
        use crate::config::{ChannelConfig, OpsConfig};
        use nodalync_test_utils::MockSettlement;

        let temp_dir = TempDir::new().unwrap();
        let config = NodeStateConfig::new(temp_dir.path());
        let state = nodalync_store::NodeState::open(config).unwrap();

        let (_, public_key) = generate_identity();
        let peer_id = peer_id_from_public_key(&public_key);

        // Enable auto-deposit with specific amount
        let config_deposit_amount = 300_0000_0000; // 300 HBAR
        let ops_config = OpsConfig::default().with_channel(
            ChannelConfig::default()
                .with_auto_deposit(true)
                .with_auto_deposit_amount(config_deposit_amount)
                .with_auto_deposit_min_balance(100_0000_0000),
        );

        // Create mock settlement with low balance to trigger auto-deposit
        let mock_settle = Arc::new(MockSettlement::new().with_balance(0));

        let mut ops = DefaultNodeOperations::with_config_and_settlement(
            state,
            peer_id,
            ops_config,
            mock_settle.clone(),
        );

        let requester = test_peer_id();
        let channel_id = content_hash(b"auto deposit amount channel");

        // Request with large deposit (should NOT affect auto-deposit amount)
        let request = ChannelOpenPayload {
            channel_id,
            initial_balance: 1000_0000_0000, // 1000 HBAR request
            funding_tx: None,
            hedera_account: None,
        };

        ops.handle_channel_open(&requester, &request).await.unwrap();

        // Auto-deposit should use config amount, NOT the request amount
        let deposits = mock_settle.deposits();
        assert_eq!(deposits.len(), 1, "Should have exactly one deposit");
        assert_eq!(
            deposits[0], config_deposit_amount,
            "Deposit should be config amount, not derived from request"
        );
    }

    #[tokio::test]
    async fn test_channel_open_skips_deposit_if_balance_sufficient() {
        use crate::config::{ChannelConfig, OpsConfig};
        use nodalync_test_utils::MockSettlement;

        let temp_dir = TempDir::new().unwrap();
        let config = NodeStateConfig::new(temp_dir.path());
        let state = nodalync_store::NodeState::open(config).unwrap();

        let (_, public_key) = generate_identity();
        let peer_id = peer_id_from_public_key(&public_key);

        // Enable auto-deposit
        let ops_config = OpsConfig::default().with_channel(
            ChannelConfig::default()
                .with_auto_deposit(true)
                .with_auto_deposit_min_balance(100_0000_0000),
        );

        // Create mock settlement with HIGH balance (above threshold)
        let mock_settle = Arc::new(MockSettlement::new().with_balance(500_0000_0000));

        let mut ops = DefaultNodeOperations::with_config_and_settlement(
            state,
            peer_id,
            ops_config,
            mock_settle.clone(),
        );

        let requester = test_peer_id();
        let channel_id = content_hash(b"sufficient balance channel");

        let request = ChannelOpenPayload {
            channel_id,
            initial_balance: 200_0000_0000,
            funding_tx: None,
            hedera_account: None,
        };

        ops.handle_channel_open(&requester, &request).await.unwrap();

        // No deposit should occur when balance is sufficient
        assert!(
            mock_settle.deposits().is_empty(),
            "No deposit should occur when balance is above threshold"
        );
    }

    #[tokio::test]
    async fn test_channel_open_cooldown_prevents_rapid_deposits() {
        use crate::config::{ChannelConfig, OpsConfig};
        use nodalync_test_utils::MockSettlement;

        let temp_dir = TempDir::new().unwrap();
        let config = NodeStateConfig::new(temp_dir.path());
        let state = nodalync_store::NodeState::open(config).unwrap();

        let (_, public_key) = generate_identity();
        let peer_id = peer_id_from_public_key(&public_key);

        // Enable auto-deposit with very short cooldown for test
        let ops_config = OpsConfig::default().with_channel(
            ChannelConfig::default()
                .with_auto_deposit(true)
                .with_auto_deposit_min_balance(100_0000_0000)
                .with_auto_deposit_cooldown(300), // 5 minutes
        );

        // Create mock settlement with low balance
        let mock_settle = Arc::new(MockSettlement::new().with_balance(0));

        let mut ops = DefaultNodeOperations::with_config_and_settlement(
            state,
            peer_id,
            ops_config,
            mock_settle.clone(),
        );

        // First channel open - should trigger deposit
        let requester1 = test_peer_id();
        let channel_id1 = content_hash(b"cooldown test channel 1");
        let request1 = ChannelOpenPayload {
            channel_id: channel_id1,
            initial_balance: 200_0000_0000,
            funding_tx: None,
            hedera_account: None,
        };
        ops.handle_channel_open(&requester1, &request1)
            .await
            .unwrap();

        // Second channel open immediately after - should NOT trigger deposit (cooldown)
        let requester2 = test_peer_id();
        let channel_id2 = content_hash(b"cooldown test channel 2");
        let request2 = ChannelOpenPayload {
            channel_id: channel_id2,
            initial_balance: 200_0000_0000,
            funding_tx: None,
            hedera_account: None,
        };
        ops.handle_channel_open(&requester2, &request2)
            .await
            .unwrap();

        // Only ONE deposit should have occurred
        let deposits = mock_settle.deposits();
        assert_eq!(
            deposits.len(),
            1,
            "Should have only one deposit due to cooldown"
        );
    }

    #[tokio::test]
    async fn test_channel_open_no_overflow_on_max_initial_balance() {
        // This test verifies no panic or overflow when processing u64::MAX
        let (mut ops, _temp) = create_test_ops();

        let requester = test_peer_id();
        let channel_id = content_hash(b"overflow test channel");

        let request = ChannelOpenPayload {
            channel_id,
            initial_balance: u64::MAX, // Maximum possible value
            funding_tx: None,
            hedera_account: None,
        };

        // Should not panic - should cap the deposit
        let result = ops.handle_channel_open(&requester, &request).await;
        assert!(result.is_ok(), "Should not panic on u64::MAX: {:?}", result);

        // Verify the deposit was capped (default max_accept_deposit is 500 HBAR)
        let response = result.unwrap();
        assert!(
            response.initial_balance <= 500_0000_0000,
            "Deposit should be capped"
        );
    }
}
