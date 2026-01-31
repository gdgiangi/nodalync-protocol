//! Channel operations implementation.
//!
//! This module implements payment channel operations as specified
//! in Protocol Specification §7.3.

use nodalync_crypto::{content_hash, sign, Hash, PeerId, PrivateKey, Signature};
use nodalync_store::ChannelStore;
use nodalync_types::{Amount, Channel, Manifest, Payment, ProvenanceEntry};
use nodalync_valid::{construct_payment_message, Validator};
use nodalync_wire::{ChannelBalances, ChannelClosePayload, ChannelOpenPayload};
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
            match settlement.open_channel(peer, deposit).await {
                Ok(on_chain_id) => {
                    // Use the on-chain channel ID as funding reference
                    let funding_tx_id = on_chain_id.to_string();
                    tracing::info!(
                        channel_id = %channel_id,
                        on_chain_id = %on_chain_id,
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
                    tracing::warn!(
                        channel_id = %channel_id,
                        error = %e,
                        "On-chain channel open failed, proceeding with off-chain only"
                    );
                    (Channel::new(channel_id, *peer, deposit, timestamp), None)
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
                let payload = ChannelOpenPayload {
                    channel_id,
                    initial_balance: deposit,
                    funding_tx,
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

        let payload = ChannelOpenPayload {
            channel_id,
            initial_balance: deposit,
            funding_tx: None,
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

        // Create the channel in Open state (since we received Accept)
        let channel = Channel::accepted(
            accept.channel_id,
            remote_nodalync_id,
            accept.initial_balance,
            deposit,
            timestamp,
        );

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

    /// Close a payment channel.
    ///
    /// Spec §7.3.3:
    /// 1. Gets channel
    /// 2. Computes final balances
    /// 3. Closes on-chain channel (if settlement available and channel was funded on-chain)
    /// 4. Sends ChannelClose message (if network available)
    /// 5. Updates state to Closed
    ///
    /// Returns the settlement transaction ID if the channel was closed on-chain.
    pub async fn close_payment_channel(&mut self, peer: &PeerId) -> OpsResult<Option<String>> {
        let timestamp = current_timestamp();

        // 1. Get channel
        let mut channel = self
            .state
            .channels
            .get(peer)?
            .ok_or(OpsError::ChannelNotFound)?;

        // Cannot close already closed channel
        if channel.is_closed() {
            return Err(OpsError::invalid_operation("channel already closed"));
        }

        // 2. Compute final balances (already stored in channel state)
        let my_balance = channel.my_balance;
        let their_balance = channel.their_balance;
        let final_balances = ChannelBalances::new(my_balance, their_balance);

        // 3. Close on-chain channel if settlement is configured and channel was funded
        let settlement_tx = if let Some(settlement) = self.settlement().cloned() {
            // Convert channel_id Hash to ChannelId
            let channel_id = nodalync_settle::ChannelId::new(channel.channel_id);

            match settlement
                .close_channel(&channel_id, &final_balances, &[])
                .await
            {
                Ok(tx_id) => {
                    tracing::info!(
                        channel_id = %channel.channel_id,
                        tx_id = %tx_id,
                        my_balance = my_balance,
                        their_balance = their_balance,
                        "Channel closed on-chain"
                    );
                    Some(tx_id.to_string())
                }
                Err(e) => {
                    // Log but don't fail - channel may not have been opened on-chain
                    tracing::warn!(
                        channel_id = %channel.channel_id,
                        error = %e,
                        "On-chain channel close failed (channel may be off-chain only)"
                    );
                    None
                }
            }
        } else {
            None
        };

        // 4. Send ChannelClose message (if network available)
        if let Some(network) = self.network().cloned() {
            if let Some(libp2p_peer) = network.libp2p_peer_id(peer) {
                let payload = ChannelClosePayload {
                    channel_id: channel.channel_id,
                    final_balances,
                    settlement_tx: settlement_tx
                        .as_ref()
                        .map(|s| s.as_bytes().to_vec())
                        .unwrap_or_default(),
                };
                // Best effort - don't fail if network send fails
                let _ = network.send_channel_close(libp2p_peer, payload).await;
            }
        }

        // 5. Update state to Closing then Closed
        channel.mark_closing(timestamp);
        self.state.channels.update(peer, &channel)?;

        // For MVP, we immediately mark as closed
        // In full implementation, this would happen after on-chain confirmation
        channel.mark_closed(timestamp);
        self.state.channels.update(peer, &channel)?;

        Ok(settlement_tx)
    }

    /// Dispute a channel with latest signed state.
    ///
    /// Spec §7.3.4:
    /// 1. (Submit dispute to chain - stub for MVP)
    /// 2. Updates state to Disputed
    pub fn dispute_payment_channel(&mut self, peer: &PeerId) -> OpsResult<()> {
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

        // 1. Submit dispute to chain (stub for MVP)
        // In full implementation: self.settlement.submit_dispute(&channel)?;

        // 2. Update state to Disputed
        channel.mark_disputed(timestamp);
        self.state.channels.update(peer, &channel)?;

        Ok(())
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
        let peer = test_peer_id();
        let channel_id = content_hash(b"channel");

        // Accept a channel first (so it's open)
        ops.accept_payment_channel(&channel_id, &peer, 500, 500)
            .unwrap();

        // Close the channel (no settlement configured, so no tx_id returned)
        let tx_id = ops.close_payment_channel(&peer).await.unwrap();
        assert!(tx_id.is_none());

        // Verify closed
        let channel = ops.get_payment_channel(&peer).unwrap().unwrap();
        assert_eq!(channel.state, ChannelState::Closed);
    }

    #[test]
    fn test_dispute_channel() {
        let (mut ops, _temp) = create_test_ops();
        let peer = test_peer_id();
        let channel_id = content_hash(b"channel");

        // Accept a channel first (so it's open)
        ops.accept_payment_channel(&channel_id, &peer, 500, 500)
            .unwrap();

        // Dispute the channel
        ops.dispute_payment_channel(&peer).unwrap();

        // Verify disputed
        let channel = ops.get_payment_channel(&peer).unwrap().unwrap();
        assert_eq!(channel.state, ChannelState::Disputed);
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
}
