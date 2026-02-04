//! Channel operations implementation.
//!
//! This module implements payment channel operations as specified
//! in Protocol Specification §7.3.

use nodalync_crypto::{content_hash, sign, Hash, PeerId, PrivateKey, Signature};
use nodalync_store::ChannelStore;
use nodalync_types::{
    Amount, Channel, Manifest, Payment, PendingClose, PendingDispute, ProvenanceEntry,
};
use nodalync_valid::{construct_payment_message, sign_channel_close, Validator};
use nodalync_wire::{
    ChannelBalances, ChannelCloseAckPayload, ChannelClosePayload, ChannelOpenPayload,
    ChannelUpdatePayload,
};
use rand::Rng;

use crate::error::{OpsError, OpsResult};
use crate::extraction::L1Extractor;
use crate::node_ops::{current_timestamp, NodeOperations};

impl<V, E> NodeOperations<V, E>
where
    V: Validator,
    E: L1Extractor,
{
    /// Generate a channel ID from peer IDs and a nonce.
    fn generate_channel_id(&self, peer: &PeerId, nonce: u64) -> Hash {
        let mut data = Vec::with_capacity(20 + 20 + 8);
        data.extend_from_slice(&self.peer_id().0);
        data.extend_from_slice(&peer.0);
        data.extend_from_slice(&nonce.to_be_bytes());
        content_hash(&data)
    }

    /// Open a new payment channel with a peer.
    ///
    /// Spec §7.3.1:
    /// 1. Generates channel_id from hash(my_peer_id || peer_id || nonce)
    /// 2. Creates on-chain channel (if settlement available)
    /// 3. Creates Channel with state=Opening (includes funding_tx_id if on-chain)
    /// 4. Stores locally
    /// 5. Sends ChannelOpen message (if network available)
    ///
    /// Returns the channel. If settlement is configured, the channel will have
    /// a `funding_tx_id` with the on-chain transaction ID.
    pub async fn open_payment_channel(
        &mut self,
        peer: &PeerId,
        deposit: Amount,
    ) -> OpsResult<Channel> {
        let timestamp = current_timestamp();

        // Check if channel already exists
        if self.state.channels.get(peer)?.is_some() {
            return Err(OpsError::ChannelAlreadyExists);
        }

        // Validate minimum deposit
        if deposit < self.config.channel.min_deposit {
            return Err(OpsError::invalid_operation(format!(
                "deposit {} below minimum {}",
                deposit, self.config.channel.min_deposit
            )));
        }

        // 1. Generate channel ID
        let nonce: u64 = rand::thread_rng().gen();
        let channel_id = self.generate_channel_id(peer, nonce);

        // 2. Create on-chain channel if settlement is configured
        let (channel, funding_tx) = if let Some(settlement) = self.settlement().cloned() {
            let on_chain_channel_id = nodalync_settle::ChannelId::new(channel_id);
            match settlement
                .open_channel(&on_chain_channel_id, peer, deposit)
                .await
            {
                Ok(tx_id) => {
                    let funding_tx_id = tx_id.to_string();
                    tracing::info!(
                        channel_id = %channel_id,
                        tx_id = %tx_id,
                        deposit = deposit,
                        "Channel opened on-chain"
                    );
                    let channel = Channel::with_funding(
                        channel_id,
                        *peer,
                        deposit,
                        timestamp,
                        funding_tx_id.clone(),
                    );
                    (channel, Some(funding_tx_id.into_bytes()))
                }
                Err(e) => {
                    return Err(OpsError::SettlementFailed(format!(
                        "failed to open channel on-chain: {}",
                        e
                    )));
                }
            }
        } else {
            // No settlement configured, create off-chain channel only
            (Channel::new(channel_id, *peer, deposit, timestamp), None)
        };

        // 3. Store locally
        self.state.channels.create(peer, channel.clone())?;

        // 4. Send ChannelOpen message (if network available)
        if let Some(network) = self.network().cloned() {
            if let Some(libp2p_peer) = network.libp2p_peer_id(peer) {
                // Include our Hedera account if settlement is configured
                let hedera_account = self.settlement().map(|s| s.get_own_account_string());

                let payload = ChannelOpenPayload {
                    channel_id,
                    initial_balance: deposit,
                    funding_tx,
                    hedera_account,
                };
                // Best effort - don't fail if network send fails
                let _ = network.send_channel_open(libp2p_peer, payload).await;
            }
        }

        Ok(channel)
    }

    /// Open a new payment channel with a peer using their libp2p peer ID directly.
    ///
    /// This is useful when you have the libp2p peer ID (from an announcement)
    /// but don't have the Nodalync peer ID mapping yet. The channel will be
    /// stored using the remote peer's Nodalync peer ID extracted from the response.
    ///
    /// Returns the created channel and the remote's Nodalync peer ID.
    pub async fn open_payment_channel_to_libp2p(
        &mut self,
        libp2p_peer: nodalync_net::PeerId,
        deposit: Amount,
    ) -> OpsResult<(Channel, PeerId)> {
        let timestamp = current_timestamp();

        // Validate minimum deposit
        if deposit < self.config.channel.min_deposit {
            return Err(OpsError::invalid_operation(format!(
                "deposit {} below minimum {}",
                deposit, self.config.channel.min_deposit
            )));
        }

        let Some(network) = self.network().cloned() else {
            return Err(OpsError::invalid_operation("network not available"));
        };

        // Generate a temporary channel ID (will be finalized after response)
        let nonce: u64 = rand::thread_rng().gen();
        // Use our own peer ID as placeholder for channel ID generation
        let channel_id = self.generate_channel_id(&self.peer_id(), nonce);

        // Include our Hedera account if settlement is configured
        let hedera_account = self.settlement().map(|s| s.get_own_account_string());

        let payload = ChannelOpenPayload {
            channel_id,
            initial_balance: deposit,
            funding_tx: None,
            hedera_account,
        };

        // Send ChannelOpen and get response
        let response = network
            .send_channel_open(libp2p_peer, payload)
            .await
            .map_err(|e| OpsError::invalid_operation(format!("channel open failed: {}", e)))?;

        // Extract the remote's Nodalync peer ID from the response message
        let remote_nodalync_id = response.sender;

        // Register the peer ID mapping so future lookups work
        network.register_peer_mapping(libp2p_peer, remote_nodalync_id);

        // Check if channel already exists with this peer
        if self.state.channels.get(&remote_nodalync_id)?.is_some() {
            return Err(OpsError::ChannelAlreadyExists);
        }

        // Decode the ChannelAccept payload
        let accept: nodalync_wire::ChannelAcceptPayload =
            nodalync_wire::decode_payload(&response.payload)
                .map_err(|e| OpsError::invalid_operation(format!("decode error: {}", e)))?;

        // Register peer's Hedera account if provided (enables on-chain channels)
        let mut funding_tx_id: Option<String> = None;
        if let Some(peer_hedera) = &accept.hedera_account {
            if let Some(settlement) = self.settlement().cloned() {
                if let Ok(account_id) = nodalync_settle::AccountId::from_string(peer_hedera) {
                    settlement.register_peer_account(&remote_nodalync_id, account_id);
                    tracing::debug!(
                        peer = %remote_nodalync_id,
                        hedera_account = %peer_hedera,
                        "Registered peer's Hedera account"
                    );

                    // NOW open the channel on-chain (peer account is registered)
                    let on_chain_channel_id = nodalync_settle::ChannelId::new(accept.channel_id);
                    match settlement
                        .open_channel(&on_chain_channel_id, &remote_nodalync_id, deposit)
                        .await
                    {
                        Ok(tx_id) => {
                            funding_tx_id = Some(tx_id.to_string());
                            tracing::info!(
                                channel_id = %accept.channel_id,
                                tx_id = %tx_id,
                                deposit = deposit,
                                "Channel opened on-chain"
                            );
                        }
                        Err(e) => {
                            return Err(OpsError::SettlementFailed(format!(
                                "failed to open channel on-chain for libp2p peer: {}",
                                e
                            )));
                        }
                    }
                }
            }
        }

        // Create the channel in Open state (since we received Accept)
        let mut channel = Channel::accepted(
            accept.channel_id,
            remote_nodalync_id,
            accept.initial_balance,
            deposit,
            timestamp,
        );

        // Set the funding transaction ID if we opened on-chain
        if let Some(tx_id) = funding_tx_id {
            channel.set_funding_tx_id(tx_id);
        }

        // Store the channel keyed by the remote's Nodalync peer ID
        self.state
            .channels
            .create(&remote_nodalync_id, channel.clone())?;

        tracing::info!(
            channel_id = %channel_id,
            remote_peer = %remote_nodalync_id,
            our_deposit = deposit,
            their_deposit = accept.initial_balance,
            "Payment channel opened via libp2p"
        );

        Ok((channel, remote_nodalync_id))
    }

    /// Accept an incoming channel open request.
    ///
    /// Spec §7.3.2:
    /// 1. Validates no existing channel
    /// 2. Creates reciprocal Channel state
    /// 3. Stores
    pub fn accept_payment_channel(
        &mut self,
        channel_id: &Hash,
        peer: &PeerId,
        their_deposit: Amount,
        my_deposit: Amount,
    ) -> OpsResult<Channel> {
        let timestamp = current_timestamp();

        // 1. Validate no existing channel
        if self.state.channels.get(peer)?.is_some() {
            return Err(OpsError::ChannelAlreadyExists);
        }

        // 2. Create reciprocal Channel state
        let channel = Channel::accepted(*channel_id, *peer, their_deposit, my_deposit, timestamp);

        // 3. Store
        self.state.channels.create(peer, channel.clone())?;

        Ok(channel)
    }

    /// Update channel state with a payment.
    pub fn update_payment_channel(&mut self, peer: &PeerId, payment: Payment) -> OpsResult<()> {
        let timestamp = current_timestamp();

        // Get channel
        let mut channel = self
            .state
            .channels
            .get(peer)?
            .ok_or(OpsError::ChannelNotFound)?;

        // Verify channel is open
        if !channel.is_open() {
            return Err(OpsError::ChannelNotOpen);
        }

        // Record payment (determines if we're paying or receiving)
        if payment.recipient == self.peer_id() {
            // We're receiving
            channel
                .receive(payment.clone(), timestamp)
                .map_err(|_| OpsError::InsufficientChannelBalance)?;
        } else {
            // We're paying
            channel
                .pay(payment.clone(), timestamp)
                .map_err(|_| OpsError::InsufficientChannelBalance)?;
        }

        // Store updated channel
        self.state.channels.update(peer, &channel)?;

        // Add payment to pending
        self.state.channels.add_payment(peer, payment)?;

        Ok(())
    }

    /// Close a payment channel cooperatively.
    ///
    /// Attempts cooperative close with signature exchange:
    /// 1. Gets channel and validates state
    /// 2. Signs close message with our private key
    /// 3. Sends ChannelClose with our signature to peer
    /// 4. Waits for ChannelCloseAck with peer's signature
    /// 5. Submits to chain with both signatures
    /// 6. Updates state to Closed
    ///
    /// If the peer is unresponsive, returns `CloseResult::PeerUnresponsive`
    /// and the user should use `dispute_payment_channel()` instead.
    ///
    /// Requires the private key for signing the close message.
    pub async fn close_payment_channel(
        &mut self,
        peer: &PeerId,
        private_key: &PrivateKey,
    ) -> OpsResult<crate::error::CloseResult> {
        use crate::error::CloseResult;

        let timestamp = current_timestamp();

        // 1. Get channel and validate state
        let mut channel = self
            .state
            .channels
            .get(peer)?
            .ok_or(OpsError::ChannelNotFound)?;

        // Cannot close already closed channel
        if channel.is_closed() {
            return Err(OpsError::invalid_operation("channel already closed"));
        }

        // Cannot close if there's already a pending close or dispute
        if channel.pending_close.is_some() {
            return Err(OpsError::invalid_operation(
                "channel already has a pending close",
            ));
        }
        if channel.pending_dispute.is_some() {
            return Err(OpsError::invalid_operation("channel has a pending dispute"));
        }

        // 2. Compute final balances and sign close message
        let my_balance = channel.my_balance;
        let their_balance = channel.their_balance;
        let nonce = channel.nonce;
        let final_balances = ChannelBalances::new(my_balance, their_balance);

        // Sign close message: channel_id || nonce || initiator_balance || responder_balance
        let initiator_signature = sign_channel_close(
            private_key,
            &channel.channel_id,
            nonce,
            my_balance,
            their_balance,
        );

        // Store pending close state
        let pending_close = PendingClose::new_as_initiator(
            (my_balance, their_balance),
            nonce,
            initiator_signature,
            timestamp,
        );
        channel.pending_close = Some(pending_close);
        self.state.channels.update(peer, &channel)?;

        // 3. Send ChannelClose with our signature to peer
        let responder_signature = if let Some(network) = self.network().cloned() {
            if let Some(libp2p_peer) = network.libp2p_peer_id(peer) {
                let payload = ChannelClosePayload {
                    channel_id: channel.channel_id,
                    nonce,
                    final_balances,
                    initiator_signature,
                };

                // Send and wait for response
                match network.send_channel_close(libp2p_peer, payload).await {
                    Ok(response) => {
                        // Decode the ChannelCloseAck response
                        match nodalync_wire::decode_payload::<ChannelCloseAckPayload>(
                            &response.payload,
                        ) {
                            Ok(ack) => Some(ack.responder_signature),
                            Err(e) => {
                                tracing::warn!(
                                    error = %e,
                                    "Failed to decode ChannelCloseAck response"
                                );
                                None
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            peer = %peer,
                            error = %e,
                            "Peer unresponsive for cooperative close"
                        );
                        None
                    }
                }
            } else {
                tracing::warn!(
                    peer = %peer,
                    "No libp2p peer ID mapping for cooperative close"
                );
                None
            }
        } else {
            // No network - proceed with off-chain close only
            None
        };

        // 4. If peer responded, update pending_close with their signature
        if let Some(sig) = responder_signature {
            let mut pending = channel.pending_close.take().unwrap();
            pending.add_responder_signature(sig);
            channel.pending_close = Some(pending);
            self.state.channels.update(peer, &channel)?;
        } else {
            // Peer didn't respond - return with suggestion to dispute
            return Ok(CloseResult::PeerUnresponsive {
                suggestion: "Peer did not respond to cooperative close. \
                    Use 'nodalync dispute-channel' to initiate a dispute-based close (24-hour wait)."
                    .to_string(),
            });
        }

        // 5. Submit to chain with both signatures (if settlement available)
        let pending = channel.pending_close.as_ref().unwrap();
        let both_signatures = vec![
            pending.initiator_signature,
            pending.responder_signature.unwrap(),
        ];

        let result = if let Some(settlement) = self.settlement().cloned() {
            let channel_id = nodalync_settle::ChannelId::new(channel.channel_id);

            match settlement
                .close_channel(&channel_id, &final_balances, &both_signatures)
                .await
            {
                Ok(tx_id) => {
                    tracing::info!(
                        channel_id = %channel.channel_id,
                        tx_id = %tx_id,
                        my_balance = my_balance,
                        their_balance = their_balance,
                        "Channel closed on-chain with cooperative signatures"
                    );
                    CloseResult::Success {
                        transaction_id: tx_id.to_string(),
                        final_balances: (my_balance, their_balance),
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        channel_id = %channel.channel_id,
                        error = %e,
                        "On-chain cooperative close failed"
                    );
                    CloseResult::OnChainFailed {
                        error: e.to_string(),
                    }
                }
            }
        } else {
            // No settlement configured - off-chain close only
            CloseResult::SuccessOffChain {
                final_balances: (my_balance, their_balance),
            }
        };

        // 6. Update state to Closed (only if successful)
        if result.is_success() {
            channel.mark_closing(timestamp);
            channel.pending_close = None;
            self.state.channels.update(peer, &channel)?;

            channel.mark_closed(timestamp);
            self.state.channels.update(peer, &channel)?;
        }

        Ok(result)
    }

    /// Dispute a channel with latest signed state.
    ///
    /// Initiates the 24-hour dispute period on-chain. Use this when:
    /// - Peer is unresponsive to cooperative close
    /// - You suspect the peer might try to close with an old state
    ///
    /// After 24 hours, call `resolve_dispute()` to finalize.
    pub async fn dispute_payment_channel(
        &mut self,
        peer: &PeerId,
        private_key: &PrivateKey,
    ) -> OpsResult<String> {
        let timestamp = current_timestamp();

        // Get channel
        let mut channel = self
            .state
            .channels
            .get(peer)?
            .ok_or(OpsError::ChannelNotFound)?;

        // Cannot dispute closed channel
        if channel.is_closed() {
            return Err(OpsError::invalid_operation("channel already closed"));
        }

        // Cannot dispute if already disputing
        if channel.pending_dispute.is_some() {
            return Err(OpsError::invalid_operation(
                "channel already has a pending dispute",
            ));
        }

        // Get settlement layer
        let settlement = self
            .settlement()
            .cloned()
            .ok_or_else(|| OpsError::invalid_operation("settlement layer required for disputes"))?;

        // Prepare dispute state
        let my_balance = channel.my_balance;
        let their_balance = channel.their_balance;
        let nonce = channel.nonce;

        // Sign the state
        let signature = sign_channel_close(
            private_key,
            &channel.channel_id,
            nonce,
            my_balance,
            their_balance,
        );

        // Create ChannelUpdatePayload for dispute
        let state = ChannelUpdatePayload {
            channel_id: channel.channel_id,
            nonce,
            balances: ChannelBalances::new(my_balance, their_balance),
            payments: vec![],
            signature,
        };

        // Submit dispute to chain
        let channel_id = nodalync_settle::ChannelId::new(channel.channel_id);
        let tx_id = settlement
            .dispute_channel(&channel_id, &state)
            .await
            .map_err(|e| {
                OpsError::invalid_operation(format!("dispute submission failed: {}", e))
            })?;

        tracing::info!(
            channel_id = %channel.channel_id,
            tx_id = %tx_id,
            nonce = nonce,
            my_balance = my_balance,
            their_balance = their_balance,
            "Dispute initiated on-chain"
        );

        // Store pending dispute state
        let pending_dispute = PendingDispute::new(
            tx_id.to_string(),
            timestamp,
            nonce,
            my_balance,
            their_balance,
        );
        channel.pending_dispute = Some(pending_dispute);
        channel.mark_disputed(timestamp);
        self.state.channels.update(peer, &channel)?;

        Ok(tx_id.to_string())
    }

    /// Resolve a dispute after the 24-hour period has elapsed.
    ///
    /// Finalizes the channel close using the latest state submitted during
    /// the dispute period.
    pub async fn resolve_dispute(&mut self, peer: &PeerId) -> OpsResult<String> {
        let timestamp = current_timestamp();

        // Get channel
        let mut channel = self
            .state
            .channels
            .get(peer)?
            .ok_or(OpsError::ChannelNotFound)?;

        // Must have a pending dispute
        let pending = channel
            .pending_dispute
            .as_ref()
            .ok_or_else(|| OpsError::invalid_operation("no pending dispute to resolve"))?;

        // Check if dispute period has elapsed
        if !pending.can_resolve(timestamp) {
            let remaining_ms = pending.time_until_resolution(timestamp);
            let remaining_hours = remaining_ms as f64 / (60.0 * 60.0 * 1000.0);
            return Err(OpsError::invalid_operation(format!(
                "dispute period not yet elapsed ({:.1} hours remaining)",
                remaining_hours
            )));
        }

        // Get settlement layer
        let settlement = self
            .settlement()
            .cloned()
            .ok_or_else(|| OpsError::invalid_operation("settlement layer required"))?;

        // Resolve dispute on-chain
        let channel_id = nodalync_settle::ChannelId::new(channel.channel_id);
        let tx_id = settlement.resolve_dispute(&channel_id).await.map_err(|e| {
            OpsError::invalid_operation(format!("dispute resolution failed: {}", e))
        })?;

        tracing::info!(
            channel_id = %channel.channel_id,
            tx_id = %tx_id,
            "Dispute resolved on-chain"
        );

        // Update state to Closed
        channel.pending_dispute = None;
        channel.mark_closed(timestamp);
        self.state.channels.update(peer, &channel)?;

        Ok(tx_id.to_string())
    }

    /// Check if a channel has a pending dispute and when it can be resolved.
    ///
    /// Returns `Some((dispute_tx_id, can_resolve, remaining_ms))` if there's a pending dispute.
    pub fn get_pending_dispute_status(
        &self,
        peer: &PeerId,
    ) -> OpsResult<Option<(String, bool, u64)>> {
        let channel = self
            .state
            .channels
            .get(peer)?
            .ok_or(OpsError::ChannelNotFound)?;

        if let Some(dispute) = &channel.pending_dispute {
            let now = current_timestamp();
            let can_resolve = dispute.can_resolve(now);
            let remaining = dispute.time_until_resolution(now);
            Ok(Some((
                dispute.dispute_tx_id.clone(),
                can_resolve,
                remaining,
            )))
        } else {
            Ok(None)
        }
    }

    /// Get channel with a peer.
    pub fn get_payment_channel(&self, peer: &PeerId) -> OpsResult<Option<Channel>> {
        Ok(self.state.channels.get(peer)?)
    }

    /// Check if we have an open channel with a peer.
    pub fn has_open_channel(&self, peer: &PeerId) -> OpsResult<bool> {
        match self.state.channels.get(peer)? {
            Some(channel) => Ok(channel.is_open()),
            None => Ok(false),
        }
    }

    /// Get available balance in channel with a peer.
    pub fn get_channel_balance(&self, peer: &PeerId) -> OpsResult<Option<Amount>> {
        match self.state.channels.get(peer)? {
            Some(channel) if channel.is_open() => Ok(Some(channel.my_balance)),
            _ => Ok(None),
        }
    }

    /// Get the next payment nonce for a channel.
    ///
    /// Returns `channel.nonce + 1` if channel exists and is open.
    pub fn get_next_payment_nonce(&self, peer: &PeerId) -> OpsResult<u64> {
        match self.state.channels.get(peer)? {
            Some(channel) if channel.is_open() => Ok(channel.nonce + 1),
            Some(_) => Err(OpsError::ChannelNotOpen),
            None => Err(OpsError::ChannelNotFound),
        }
    }
}

/// Sign a payment with the given private key.
///
/// Constructs the payment message and signs it, returning the updated signature.
pub fn sign_payment(private_key: &PrivateKey, payment: &Payment) -> Signature {
    let message = construct_payment_message(payment);
    sign(private_key, &message)
}

/// Create a signed payment for a query.
///
/// This function creates a payment with proper signature for submitting
/// a query request to a content owner.
pub fn create_signed_payment(
    private_key: &PrivateKey,
    channel: &Channel,
    amount: Amount,
    recipient: PeerId,
    query_hash: Hash,
    provenance: Vec<ProvenanceEntry>,
) -> (Payment, u64) {
    let timestamp = current_timestamp();
    let nonce = channel.nonce + 1;

    // Create payment ID from content hash of payment details
    let payment_id = content_hash(
        &[
            channel.channel_id.0.as_slice(),
            &nonce.to_be_bytes(),
            &amount.to_be_bytes(),
            &recipient.0,
        ]
        .concat(),
    );

    // Create payment without signature first
    let mut payment = Payment::new(
        payment_id,
        channel.channel_id,
        amount,
        recipient,
        query_hash,
        provenance,
        timestamp,
        Signature::from_bytes([0u8; 64]), // Placeholder
    );

    // Sign the payment
    payment.signature = sign_payment(private_key, &payment);

    (payment, nonce)
}

/// Create a signed payment from a manifest.
///
/// Convenience function that extracts provenance from the manifest.
pub fn create_signed_payment_for_manifest(
    private_key: &PrivateKey,
    channel: &Channel,
    manifest: &Manifest,
    amount: Amount,
) -> (Payment, u64) {
    create_signed_payment(
        private_key,
        channel,
        amount,
        manifest.owner,
        manifest.hash,
        manifest.provenance.root_l0l1.clone(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node_ops::DefaultNodeOperations;
    use nodalync_crypto::{generate_identity, peer_id_from_public_key, Signature};
    use nodalync_store::NodeStateConfig;
    use nodalync_types::ChannelState;
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

    #[allow(dead_code)]
    fn test_payment(channel_id: Hash, amount: Amount, recipient: PeerId) -> Payment {
        Payment::new(
            content_hash(b"payment"),
            channel_id,
            amount,
            recipient,
            content_hash(b"query"),
            vec![],
            current_timestamp(),
            Signature::from_bytes([0u8; 64]),
        )
    }

    #[tokio::test]
    async fn test_open_channel() {
        let (mut ops, _temp) = create_test_ops();
        let peer = test_peer_id();

        let channel = ops
            .open_payment_channel(&peer, 100_0000_0000)
            .await
            .unwrap();

        assert_eq!(channel.peer_id, peer);
        assert_eq!(channel.state, ChannelState::Opening);
        assert_eq!(channel.my_balance, 100_0000_0000);
        assert_eq!(channel.their_balance, 0);
    }

    #[tokio::test]
    async fn test_open_channel_already_exists() {
        let (mut ops, _temp) = create_test_ops();
        let peer = test_peer_id();

        ops.open_payment_channel(&peer, 100_0000_0000)
            .await
            .unwrap();

        // Second open should fail
        let result = ops.open_payment_channel(&peer, 100_0000_0000).await;
        assert!(matches!(result, Err(OpsError::ChannelAlreadyExists)));
    }

    #[tokio::test]
    async fn test_open_channel_min_deposit() {
        let (mut ops, _temp) = create_test_ops();
        let peer = test_peer_id();

        // Below minimum deposit
        let result = ops.open_payment_channel(&peer, 10).await;
        assert!(matches!(result, Err(OpsError::InvalidOperation(_))));
    }

    #[test]
    fn test_accept_channel() {
        let (mut ops, _temp) = create_test_ops();
        let peer = test_peer_id();
        let channel_id = content_hash(b"channel");

        let channel = ops
            .accept_payment_channel(&channel_id, &peer, 500, 500)
            .unwrap();

        assert_eq!(channel.channel_id, channel_id);
        assert_eq!(channel.state, ChannelState::Open);
        assert_eq!(channel.my_balance, 500);
        assert_eq!(channel.their_balance, 500);
    }

    #[tokio::test]
    async fn test_close_channel() {
        let (mut ops, _temp) = create_test_ops();
        let (private_key, _public_key) = generate_identity();
        let peer = test_peer_id();
        let channel_id = content_hash(b"channel");

        // Accept a channel first (so it's open)
        ops.accept_payment_channel(&channel_id, &peer, 500, 500)
            .unwrap();

        // Close the channel (no network, so will report peer unresponsive)
        let result = ops
            .close_payment_channel(&peer, &private_key)
            .await
            .unwrap();

        // Should report peer unresponsive since there's no network
        assert!(matches!(
            result,
            crate::error::CloseResult::PeerUnresponsive { .. }
        ));

        // Channel should have a pending close
        let channel = ops.get_payment_channel(&peer).unwrap().unwrap();
        assert!(channel.pending_close.is_some());
    }

    #[tokio::test]
    async fn test_close_channel_cooperative() {
        let (mut ops, _temp) = create_test_ops();
        let (private_key, _public_key) = generate_identity();
        let peer = test_peer_id();
        let channel_id = content_hash(b"channel");

        // Accept a channel first (so it's open)
        ops.accept_payment_channel(&channel_id, &peer, 500, 500)
            .unwrap();

        // Try cooperative close (will fail with PeerUnresponsive since no network)
        let result = ops
            .close_payment_channel(&peer, &private_key)
            .await
            .unwrap();

        // Should report peer unresponsive since there's no network to send the close request
        assert!(matches!(
            result,
            crate::error::CloseResult::PeerUnresponsive { .. }
        ));

        // Channel should still be open with a pending close (persisted to DB)
        let channel = ops.get_payment_channel(&peer).unwrap().unwrap();
        assert_eq!(channel.state, ChannelState::Open);
        assert!(channel.pending_close.is_some());

        // Verify pending close details
        let pending = channel.pending_close.unwrap();
        assert!(pending.we_initiated);
        assert_eq!(pending.final_balances, (500, 500));
        assert!(pending.responder_signature.is_none()); // Peer didn't respond
    }

    #[tokio::test]
    async fn test_has_open_channel() {
        let (mut ops, _temp) = create_test_ops();
        let peer = test_peer_id();

        // No channel
        assert!(!ops.has_open_channel(&peer).unwrap());

        // Open channel (Opening state)
        ops.open_payment_channel(&peer, 100_0000_0000)
            .await
            .unwrap();
        assert!(!ops.has_open_channel(&peer).unwrap()); // Opening != Open

        // Accept channel from different peer
        let peer2 = test_peer_id();
        let channel_id = content_hash(b"channel2");
        ops.accept_payment_channel(&channel_id, &peer2, 500, 500)
            .unwrap();
        assert!(ops.has_open_channel(&peer2).unwrap());
    }

    #[test]
    fn test_get_channel_balance() {
        let (mut ops, _temp) = create_test_ops();
        let peer = test_peer_id();
        let channel_id = content_hash(b"channel");

        // No channel
        assert!(ops.get_channel_balance(&peer).unwrap().is_none());

        // Accept channel
        ops.accept_payment_channel(&channel_id, &peer, 500, 1000)
            .unwrap();

        // Check balance
        let balance = ops.get_channel_balance(&peer).unwrap().unwrap();
        assert_eq!(balance, 1000);
    }

    #[test]
    fn test_payment_signature_roundtrip() {
        use nodalync_crypto::verify;
        use nodalync_valid::construct_payment_message;

        // Generate keypair
        let (private_key, public_key) = generate_identity();
        let owner = peer_id_from_public_key(&public_key);
        let channel_id = content_hash(b"test-channel");
        let query_hash = content_hash(b"test-query");

        // Create a channel
        let mut channel = Channel::new(channel_id, owner, 1000, current_timestamp());
        channel.mark_open(1000, 2000);

        // Create and sign payment
        let (payment, nonce) =
            create_signed_payment(&private_key, &channel, 100, owner, query_hash, vec![]);

        // Verify signature
        let message = construct_payment_message(&payment);
        assert!(verify(&public_key, &message, &payment.signature));
        assert_eq!(nonce, 1); // First payment nonce should be 1
    }

    #[test]
    fn test_payment_signature_invalid_key_fails() {
        use nodalync_crypto::verify;
        use nodalync_valid::construct_payment_message;

        // Generate two keypairs
        let (private_key, _) = generate_identity();
        let (_, wrong_public_key) = generate_identity();

        let owner = test_peer_id();
        let channel_id = content_hash(b"test-channel");
        let query_hash = content_hash(b"test-query");

        // Create a channel
        let mut channel = Channel::new(channel_id, owner, 1000, current_timestamp());
        channel.mark_open(1000, 2000);

        // Create and sign payment
        let (payment, _) =
            create_signed_payment(&private_key, &channel, 100, owner, query_hash, vec![]);

        // Verify with wrong key should fail
        let message = construct_payment_message(&payment);
        assert!(!verify(&wrong_public_key, &message, &payment.signature));
    }

    #[test]
    fn test_get_next_payment_nonce() {
        let (mut ops, _temp) = create_test_ops();
        let peer = test_peer_id();
        let channel_id = content_hash(b"channel");

        // No channel should fail
        assert!(matches!(
            ops.get_next_payment_nonce(&peer),
            Err(OpsError::ChannelNotFound)
        ));

        // Accept channel (creates open channel)
        ops.accept_payment_channel(&channel_id, &peer, 500, 1000)
            .unwrap();

        // Should get nonce = 1 (channel nonce starts at 0)
        assert_eq!(ops.get_next_payment_nonce(&peer).unwrap(), 1);
    }

    #[test]
    fn test_update_payment_channel_success() {
        let (mut ops, _temp) = create_test_ops();
        let peer = test_peer_id();
        let channel_id = content_hash(b"update-channel");

        // Accept a channel (creates open channel)
        ops.accept_payment_channel(&channel_id, &peer, 500, 1000)
            .unwrap();

        // Create a payment where we are paying
        let payment = Payment::new(
            content_hash(b"pay1"),
            channel_id,
            100,
            peer, // recipient is the peer (we are paying)
            content_hash(b"query1"),
            vec![],
            current_timestamp(),
            Signature::from_bytes([0u8; 64]),
        );

        // Update channel with payment
        ops.update_payment_channel(&peer, payment).unwrap();

        // Verify balance decreased
        let channel = ops.get_payment_channel(&peer).unwrap().unwrap();
        assert_eq!(channel.my_balance, 900);
    }

    #[test]
    fn test_update_payment_channel_insufficient_balance() {
        let (mut ops, _temp) = create_test_ops();
        let peer = test_peer_id();
        let channel_id = content_hash(b"insuff-channel");

        // Accept a channel with small balance
        ops.accept_payment_channel(&channel_id, &peer, 500, 100)
            .unwrap();

        // Try to pay more than our balance
        let payment = Payment::new(
            content_hash(b"pay-big"),
            channel_id,
            200, // more than our 100 balance
            peer,
            content_hash(b"query"),
            vec![],
            current_timestamp(),
            Signature::from_bytes([0u8; 64]),
        );

        let result = ops.update_payment_channel(&peer, payment);
        assert!(
            matches!(result, Err(OpsError::InsufficientChannelBalance)),
            "Should fail with InsufficientChannelBalance: {:?}",
            result
        );
    }

    #[test]
    fn test_get_payment_channel_existing() {
        let (mut ops, _temp) = create_test_ops();
        let peer = test_peer_id();
        let channel_id = content_hash(b"get-channel");

        // Accept a channel
        ops.accept_payment_channel(&channel_id, &peer, 500, 1000)
            .unwrap();

        // Retrieve it
        let channel = ops.get_payment_channel(&peer).unwrap();
        assert!(channel.is_some());
        let channel = channel.unwrap();
        assert_eq!(channel.channel_id, channel_id);
        assert_eq!(channel.my_balance, 1000);
        assert_eq!(channel.their_balance, 500);
    }

    #[test]
    fn test_get_payment_channel_none() {
        let (ops, _temp) = create_test_ops();
        let unknown_peer = test_peer_id();

        // Get channel for unknown peer should return None
        let channel = ops.get_payment_channel(&unknown_peer).unwrap();
        assert!(channel.is_none());
    }

    #[test]
    fn test_create_signed_payment_for_manifest() {
        use nodalync_types::Metadata;

        let (private_key, public_key) = generate_identity();
        let owner = peer_id_from_public_key(&public_key);
        let channel_id = content_hash(b"manifest-channel");
        let content_hash_val = content_hash(b"manifest-content");

        // Create a manifest
        let metadata = Metadata::new("Test", 100);
        let mut manifest = nodalync_types::Manifest::new_l0(
            content_hash_val,
            owner,
            metadata,
            current_timestamp(),
        );
        manifest.economics.price = 200;

        // Create a channel
        let mut channel = Channel::new(channel_id, owner, 5000, current_timestamp());
        channel.mark_open(5000, current_timestamp());

        // Create signed payment from manifest
        let (payment, nonce) =
            create_signed_payment_for_manifest(&private_key, &channel, &manifest, 200);

        // Verify the payment matches the manifest price
        assert_eq!(payment.amount, 200);
        assert_eq!(payment.recipient, owner);
        assert_eq!(payment.query_hash, content_hash_val);
        assert_eq!(nonce, 1);
    }

    #[test]
    fn test_sign_payment_deterministic() {
        let (private_key, public_key) = generate_identity();
        let owner = peer_id_from_public_key(&public_key);
        let channel_id = content_hash(b"determ-channel");
        let query_hash = content_hash(b"determ-query");

        // Create a consistent payment
        let payment = Payment::new(
            content_hash(b"determ-payment"),
            channel_id,
            100,
            owner,
            query_hash,
            vec![],
            1234567890000, // fixed timestamp
            Signature::from_bytes([0u8; 64]),
        );

        // Sign twice with the same key
        let sig1 = sign_payment(&private_key, &payment);
        let sig2 = sign_payment(&private_key, &payment);

        // Signatures should be identical (Ed25519 is deterministic)
        assert_eq!(
            sig1, sig2,
            "Signing same data should produce same signature"
        );
    }
}
