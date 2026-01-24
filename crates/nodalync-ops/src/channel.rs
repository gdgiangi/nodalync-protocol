//! Channel operations implementation.
//!
//! This module implements payment channel operations as specified
//! in Protocol Specification §7.3.

use nodalync_crypto::{content_hash, Hash, PeerId};
use nodalync_store::ChannelStore;
use nodalync_types::{Amount, Channel, Payment};
use nodalync_valid::Validator;
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
    /// 2. Creates Channel with state=Opening
    /// 3. Stores locally
    /// 4. (Send ChannelOpen - stub for MVP)
    pub fn open_payment_channel(
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

        // 2. Create Channel with state=Opening
        let channel = Channel::new(channel_id, *peer, deposit, timestamp);

        // 3. Store locally
        self.state.channels.create(peer, channel.clone())?;

        // 4. Send ChannelOpen (stub for MVP)
        // In full implementation: self.network.send_channel_open(peer, &channel)?;

        Ok(channel)
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
            channel.receive(payment.clone(), timestamp).map_err(|_| {
                OpsError::InsufficientChannelBalance
            })?;
        } else {
            // We're paying
            channel.pay(payment.clone(), timestamp).map_err(|_| {
                OpsError::InsufficientChannelBalance
            })?;
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
    /// 3. (Submit to settlement - stub for MVP)
    /// 4. Updates state to Closed
    pub fn close_payment_channel(&mut self, peer: &PeerId) -> OpsResult<()> {
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
        let _my_balance = channel.my_balance;
        let _their_balance = channel.their_balance;

        // 3. Submit to settlement (stub for MVP)
        // In full implementation: self.settlement.submit_close(&channel)?;

        // 4. Update state to Closing then Closed
        channel.mark_closing(timestamp);
        self.state.channels.update(peer, &channel)?;

        // For MVP, we immediately mark as closed
        // In full implementation, this would happen after on-chain confirmation
        channel.mark_closed(timestamp);
        self.state.channels.update(peer, &channel)?;

        Ok(())
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

    #[test]
    fn test_open_channel() {
        let (mut ops, _temp) = create_test_ops();
        let peer = test_peer_id();

        let channel = ops.open_payment_channel(&peer, 100_0000_0000).unwrap();

        assert_eq!(channel.peer_id, peer);
        assert_eq!(channel.state, ChannelState::Opening);
        assert_eq!(channel.my_balance, 100_0000_0000);
        assert_eq!(channel.their_balance, 0);
    }

    #[test]
    fn test_open_channel_already_exists() {
        let (mut ops, _temp) = create_test_ops();
        let peer = test_peer_id();

        ops.open_payment_channel(&peer, 100_0000_0000).unwrap();

        // Second open should fail
        let result = ops.open_payment_channel(&peer, 100_0000_0000);
        assert!(matches!(result, Err(OpsError::ChannelAlreadyExists)));
    }

    #[test]
    fn test_open_channel_min_deposit() {
        let (mut ops, _temp) = create_test_ops();
        let peer = test_peer_id();

        // Below minimum deposit
        let result = ops.open_payment_channel(&peer, 10);
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

    #[test]
    fn test_close_channel() {
        let (mut ops, _temp) = create_test_ops();
        let peer = test_peer_id();
        let channel_id = content_hash(b"channel");

        // Accept a channel first (so it's open)
        ops.accept_payment_channel(&channel_id, &peer, 500, 500).unwrap();

        // Close the channel
        ops.close_payment_channel(&peer).unwrap();

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
        ops.accept_payment_channel(&channel_id, &peer, 500, 500).unwrap();

        // Dispute the channel
        ops.dispute_payment_channel(&peer).unwrap();

        // Verify disputed
        let channel = ops.get_payment_channel(&peer).unwrap().unwrap();
        assert_eq!(channel.state, ChannelState::Disputed);
    }

    #[test]
    fn test_has_open_channel() {
        let (mut ops, _temp) = create_test_ops();
        let peer = test_peer_id();

        // No channel
        assert!(!ops.has_open_channel(&peer).unwrap());

        // Open channel (Opening state)
        ops.open_payment_channel(&peer, 100_0000_0000).unwrap();
        assert!(!ops.has_open_channel(&peer).unwrap()); // Opening != Open

        // Accept channel from different peer
        let peer2 = test_peer_id();
        let channel_id = content_hash(b"channel2");
        ops.accept_payment_channel(&channel_id, &peer2, 500, 500).unwrap();
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
        ops.accept_payment_channel(&channel_id, &peer, 500, 1000).unwrap();

        // Check balance
        let balance = ops.get_channel_balance(&peer).unwrap().unwrap();
        assert_eq!(balance, 1000);
    }
}
