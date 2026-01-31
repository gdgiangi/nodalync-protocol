//! Payment channel types.
//!
//! This module defines payment channel structures as specified
//! in Protocol Specification §5.3 and docs/modules/02-types.md.

use nodalync_crypto::{Hash, PeerId, Signature, Timestamp};
use serde::{Deserialize, Serialize};

use crate::enums::ChannelState;
use crate::provenance::ProvenanceEntry;
use crate::Amount;

/// A payment for a content query.
///
/// Payments are made through payment channels and include full
/// provenance information for revenue distribution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Payment {
    /// H(channel_id || nonce || amount || recipient) - unique identifier
    pub id: Hash,
    /// Channel this payment belongs to
    pub channel_id: Hash,
    /// Payment amount (in tinybars, 10^-8 HBAR)
    pub amount: Amount,
    /// Content owner receiving the payment
    pub recipient: PeerId,
    /// Content hash that was queried
    pub query_hash: Hash,
    /// Provenance entries for distribution to all root contributors
    pub provenance: Vec<ProvenanceEntry>,
    /// Payment timestamp
    pub timestamp: Timestamp,
    /// Signature from payer
    pub signature: Signature,
}

impl Payment {
    /// Create a new payment.
    ///
    /// Note: The id and signature should be computed by the caller.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: Hash,
        channel_id: Hash,
        amount: Amount,
        recipient: PeerId,
        query_hash: Hash,
        provenance: Vec<ProvenanceEntry>,
        timestamp: Timestamp,
        signature: Signature,
    ) -> Self {
        Self {
            id,
            channel_id,
            amount,
            recipient,
            query_hash,
            provenance,
            timestamp,
            signature,
        }
    }

    /// Get the total weight from provenance entries.
    pub fn total_provenance_weight(&self) -> u32 {
        self.provenance.iter().map(|e| e.weight).sum()
    }

    /// Get unique recipients from provenance entries.
    pub fn unique_provenance_owners(&self) -> Vec<PeerId> {
        let mut owners: Vec<PeerId> = self.provenance.iter().map(|e| e.owner).collect();
        owners.sort_by(|a, b| a.0.cmp(&b.0));
        owners.dedup();
        owners
    }
}

/// A payment channel between two peers.
///
/// Channels allow for off-chain payments that are periodically
/// settled on-chain in batches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Channel {
    /// Unique channel identifier: H(initiator || responder || nonce)
    pub channel_id: Hash,
    /// The other peer in this channel
    pub peer_id: PeerId,
    /// Current channel state
    pub state: ChannelState,
    /// Our balance in the channel
    pub my_balance: Amount,
    /// Their balance in the channel
    pub their_balance: Amount,
    /// Current nonce (incremented with each update)
    pub nonce: u64,
    /// Last state update timestamp
    pub last_update: Timestamp,
    /// Pending payments not yet settled
    pub pending_payments: Vec<Payment>,
    /// On-chain funding transaction ID (if channel was funded on-chain)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub funding_tx_id: Option<String>,
}

impl Channel {
    /// Create a new channel in Opening state.
    pub fn new(
        channel_id: Hash,
        peer_id: PeerId,
        my_deposit: Amount,
        timestamp: Timestamp,
    ) -> Self {
        Self {
            channel_id,
            peer_id,
            state: ChannelState::Opening,
            my_balance: my_deposit,
            their_balance: 0,
            nonce: 0,
            last_update: timestamp,
            pending_payments: Vec::new(),
            funding_tx_id: None,
        }
    }

    /// Create a new channel with on-chain funding transaction.
    pub fn with_funding(
        channel_id: Hash,
        peer_id: PeerId,
        my_deposit: Amount,
        timestamp: Timestamp,
        funding_tx_id: String,
    ) -> Self {
        Self {
            channel_id,
            peer_id,
            state: ChannelState::Opening,
            my_balance: my_deposit,
            their_balance: 0,
            nonce: 0,
            last_update: timestamp,
            pending_payments: Vec::new(),
            funding_tx_id: Some(funding_tx_id),
        }
    }

    /// Create an accepted channel (responder side).
    pub fn accepted(
        channel_id: Hash,
        peer_id: PeerId,
        their_deposit: Amount,
        my_deposit: Amount,
        timestamp: Timestamp,
    ) -> Self {
        Self {
            channel_id,
            peer_id,
            state: ChannelState::Open,
            my_balance: my_deposit,
            their_balance: their_deposit,
            nonce: 0,
            last_update: timestamp,
            pending_payments: Vec::new(),
            funding_tx_id: None,
        }
    }

    /// Set the funding transaction ID.
    pub fn set_funding_tx_id(&mut self, tx_id: String) {
        self.funding_tx_id = Some(tx_id);
    }

    /// Check if the channel is open and can process payments.
    pub fn is_open(&self) -> bool {
        self.state == ChannelState::Open
    }

    /// Check if the channel is in a terminal state.
    pub fn is_closed(&self) -> bool {
        matches!(self.state, ChannelState::Closed | ChannelState::Disputed)
    }

    /// Get the total balance in the channel.
    pub fn total_balance(&self) -> Amount {
        self.my_balance + self.their_balance
    }

    /// Check if we have sufficient balance for a payment.
    pub fn can_pay(&self, amount: Amount) -> bool {
        self.is_open() && self.my_balance >= amount
    }

    /// Check if they have sufficient balance for a payment.
    pub fn can_receive(&self, amount: Amount) -> bool {
        self.is_open() && self.their_balance >= amount
    }

    /// Get the total pending payment amount.
    pub fn pending_amount(&self) -> Amount {
        self.pending_payments.iter().map(|p| p.amount).sum()
    }

    /// Mark the channel as open.
    pub fn mark_open(&mut self, their_deposit: Amount, timestamp: Timestamp) {
        self.state = ChannelState::Open;
        self.their_balance = their_deposit;
        self.last_update = timestamp;
    }

    /// Mark the channel as closing.
    pub fn mark_closing(&mut self, timestamp: Timestamp) {
        self.state = ChannelState::Closing;
        self.last_update = timestamp;
    }

    /// Mark the channel as closed.
    pub fn mark_closed(&mut self, timestamp: Timestamp) {
        self.state = ChannelState::Closed;
        self.last_update = timestamp;
    }

    /// Mark the channel as disputed.
    pub fn mark_disputed(&mut self, timestamp: Timestamp) {
        self.state = ChannelState::Disputed;
        self.last_update = timestamp;
    }

    /// Record an outgoing payment (we pay them).
    ///
    /// Returns Ok(()) if successful, Err(amount) if insufficient balance.
    pub fn pay(&mut self, payment: Payment, timestamp: Timestamp) -> Result<(), Amount> {
        if !self.is_open() {
            return Err(payment.amount);
        }
        if self.my_balance < payment.amount {
            return Err(payment.amount);
        }

        self.my_balance -= payment.amount;
        self.their_balance += payment.amount;
        self.nonce += 1;
        self.last_update = timestamp;
        self.pending_payments.push(payment);
        Ok(())
    }

    /// Record an incoming payment (they pay us).
    ///
    /// Returns Ok(()) if successful, Err(amount) if insufficient balance.
    pub fn receive(&mut self, payment: Payment, timestamp: Timestamp) -> Result<(), Amount> {
        if !self.is_open() {
            return Err(payment.amount);
        }
        if self.their_balance < payment.amount {
            return Err(payment.amount);
        }

        self.their_balance -= payment.amount;
        self.my_balance += payment.amount;
        self.nonce += 1;
        self.last_update = timestamp;
        self.pending_payments.push(payment);
        Ok(())
    }

    /// Clear pending payments after settlement.
    pub fn clear_pending(&mut self) {
        self.pending_payments.clear();
    }
}

impl Default for Channel {
    fn default() -> Self {
        Self {
            channel_id: Hash([0u8; 32]),
            peer_id: PeerId([0u8; 20]),
            state: ChannelState::Opening,
            my_balance: 0,
            their_balance: 0,
            nonce: 0,
            last_update: 0,
            pending_payments: Vec::new(),
            funding_tx_id: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::enums::Visibility;
    use nodalync_crypto::{content_hash, generate_identity, peer_id_from_public_key};

    fn test_hash(data: &[u8]) -> Hash {
        content_hash(data)
    }

    fn test_peer_id() -> PeerId {
        let (_, public_key) = generate_identity();
        peer_id_from_public_key(&public_key)
    }

    fn test_signature() -> Signature {
        // Create a dummy signature for testing
        Signature([0u8; 64])
    }

    fn test_payment(amount: Amount) -> Payment {
        let channel_id = test_hash(b"channel");
        let query_hash = test_hash(b"content");
        let recipient = test_peer_id();
        let entry = ProvenanceEntry::new(query_hash, recipient, Visibility::Shared);

        Payment::new(
            test_hash(b"payment"),
            channel_id,
            amount,
            recipient,
            query_hash,
            vec![entry],
            1234567890,
            test_signature(),
        )
    }

    #[test]
    fn test_payment_new() {
        let payment = test_payment(100);

        assert_eq!(payment.amount, 100);
        assert_eq!(payment.provenance.len(), 1);
    }

    #[test]
    fn test_payment_provenance_weight() {
        let channel_id = test_hash(b"channel");
        let query_hash = test_hash(b"content");
        let recipient = test_peer_id();

        let entry1 =
            ProvenanceEntry::with_weight(test_hash(b"src1"), test_peer_id(), Visibility::Shared, 2);
        let entry2 =
            ProvenanceEntry::with_weight(test_hash(b"src2"), test_peer_id(), Visibility::Shared, 3);

        let payment = Payment::new(
            test_hash(b"payment"),
            channel_id,
            100,
            recipient,
            query_hash,
            vec![entry1, entry2],
            1234567890,
            test_signature(),
        );

        assert_eq!(payment.total_provenance_weight(), 5);
    }

    #[test]
    fn test_channel_new() {
        let channel_id = test_hash(b"channel");
        let peer_id = test_peer_id();
        let timestamp = 1234567890u64;

        let channel = Channel::new(channel_id, peer_id, 1000, timestamp);

        assert_eq!(channel.channel_id, channel_id);
        assert_eq!(channel.peer_id, peer_id);
        assert_eq!(channel.state, ChannelState::Opening);
        assert_eq!(channel.my_balance, 1000);
        assert_eq!(channel.their_balance, 0);
        assert_eq!(channel.nonce, 0);
        assert!(!channel.is_open());
    }

    #[test]
    fn test_channel_mark_open() {
        let channel_id = test_hash(b"channel");
        let peer_id = test_peer_id();

        let mut channel = Channel::new(channel_id, peer_id, 1000, 1000);
        channel.mark_open(500, 2000);

        assert!(channel.is_open());
        assert_eq!(channel.their_balance, 500);
        assert_eq!(channel.total_balance(), 1500);
    }

    #[test]
    fn test_channel_pay() {
        let channel_id = test_hash(b"channel");
        let peer_id = test_peer_id();

        let mut channel = Channel::new(channel_id, peer_id, 1000, 1000);
        channel.mark_open(500, 2000);

        let payment = test_payment(100);
        assert!(channel.pay(payment, 3000).is_ok());

        assert_eq!(channel.my_balance, 900);
        assert_eq!(channel.their_balance, 600);
        assert_eq!(channel.nonce, 1);
        assert_eq!(channel.pending_payments.len(), 1);
    }

    #[test]
    fn test_channel_pay_insufficient_balance() {
        let channel_id = test_hash(b"channel");
        let peer_id = test_peer_id();

        let mut channel = Channel::new(channel_id, peer_id, 100, 1000);
        channel.mark_open(0, 2000);

        let payment = test_payment(200); // More than balance
        assert!(channel.pay(payment, 3000).is_err());

        // Balance unchanged
        assert_eq!(channel.my_balance, 100);
    }

    #[test]
    fn test_channel_receive() {
        let channel_id = test_hash(b"channel");
        let peer_id = test_peer_id();

        let mut channel = Channel::new(channel_id, peer_id, 500, 1000);
        channel.mark_open(1000, 2000);

        let payment = test_payment(100);
        assert!(channel.receive(payment, 3000).is_ok());

        assert_eq!(channel.my_balance, 600);
        assert_eq!(channel.their_balance, 900);
        assert_eq!(channel.nonce, 1);
    }

    #[test]
    fn test_channel_states() {
        let channel_id = test_hash(b"channel");
        let peer_id = test_peer_id();

        let mut channel = Channel::new(channel_id, peer_id, 1000, 1000);

        assert_eq!(channel.state, ChannelState::Opening);
        assert!(!channel.is_closed());

        channel.mark_open(500, 2000);
        assert_eq!(channel.state, ChannelState::Open);

        channel.mark_closing(3000);
        assert_eq!(channel.state, ChannelState::Closing);
        assert!(!channel.is_closed());

        channel.mark_closed(4000);
        assert_eq!(channel.state, ChannelState::Closed);
        assert!(channel.is_closed());
    }

    #[test]
    fn test_channel_disputed() {
        let channel_id = test_hash(b"channel");
        let peer_id = test_peer_id();

        let mut channel = Channel::new(channel_id, peer_id, 1000, 1000);
        channel.mark_open(500, 2000);
        channel.mark_disputed(3000);

        assert_eq!(channel.state, ChannelState::Disputed);
        assert!(channel.is_closed());
    }

    #[test]
    fn test_channel_pending_amount() {
        let channel_id = test_hash(b"channel");
        let peer_id = test_peer_id();

        let mut channel = Channel::new(channel_id, peer_id, 1000, 1000);
        channel.mark_open(500, 2000);

        assert_eq!(channel.pending_amount(), 0);

        let _ = channel.pay(test_payment(100), 3000);
        let _ = channel.pay(test_payment(50), 4000);

        assert_eq!(channel.pending_amount(), 150);

        channel.clear_pending();
        assert_eq!(channel.pending_amount(), 0);
    }

    #[test]
    fn test_channel_serialization() {
        let channel_id = test_hash(b"channel");
        let peer_id = test_peer_id();

        let channel = Channel::new(channel_id, peer_id, 1000, 1234567890);

        let json = serde_json::to_string(&channel).unwrap();
        let deserialized: Channel = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.channel_id, channel.channel_id);
        assert_eq!(deserialized.my_balance, channel.my_balance);
        assert_eq!(deserialized.state, channel.state);
    }
}
