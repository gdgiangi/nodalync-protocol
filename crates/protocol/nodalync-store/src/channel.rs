//! Payment channel storage.
//!
//! This module implements storage for payment channels and pending payments.

use rusqlite::{params, Connection, OptionalExtension};
use std::sync::{Arc, Mutex};

use nodalync_crypto::{Hash, PeerId, Signature, Timestamp};
use nodalync_types::{
    Amount, Channel, ChannelState, Payment, PendingClose, PendingDispute, ProvenanceEntry,
};

use crate::error::{Result, StoreError};
use crate::traits::ChannelStore;

/// SQLite-based channel store.
pub struct SqliteChannelStore {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteChannelStore {
    /// Create a new channel store with the given database connection.
    pub fn new(conn: Arc<Mutex<Connection>>) -> Self {
        Self { conn }
    }

    /// Serialize a channel for storage.
    #[allow(clippy::type_complexity)]
    fn serialize_channel(
        peer: &PeerId,
        channel: &Channel,
    ) -> (
        Vec<u8>,
        Vec<u8>,
        u8,
        i64,
        i64,
        i64,
        i64,
        Option<String>,
        Option<String>,
        Option<String>,
    ) {
        let pending_close_json = channel
            .pending_close
            .as_ref()
            .and_then(|pc| serde_json::to_string(pc).ok());
        let pending_dispute_json = channel
            .pending_dispute
            .as_ref()
            .and_then(|pd| serde_json::to_string(pd).ok());

        (
            peer.0.to_vec(),
            channel.channel_id.0.to_vec(),
            channel.state as u8,
            channel.my_balance as i64,
            channel.their_balance as i64,
            channel.nonce as i64,
            channel.last_update as i64,
            pending_close_json,
            pending_dispute_json,
            channel.funding_tx_id.clone(),
        )
    }

    /// Deserialize a channel from a database row.
    fn deserialize_channel(row: &rusqlite::Row) -> rusqlite::Result<Channel> {
        let channel_id_bytes: Vec<u8> = row.get(1)?;
        let state_u8: u8 = row.get(2)?;
        let my_balance: i64 = row.get(3)?;
        let their_balance: i64 = row.get(4)?;
        let nonce: i64 = row.get(5)?;
        let last_update: i64 = row.get(6)?;

        // Get pending_close and pending_dispute (may be NULL in older schemas)
        let pending_close_json: Option<String> = row.get(7).ok().flatten();
        let pending_dispute_json: Option<String> = row.get(8).ok().flatten();

        let peer_id_bytes: Vec<u8> = row.get(0)?;
        let peer_id = bytes_to_peer_id(&peer_id_bytes);

        // Deserialize pending close/dispute from JSON
        let pending_close: Option<PendingClose> = pending_close_json
            .as_ref()
            .and_then(|json| serde_json::from_str(json).ok());
        let pending_dispute: Option<PendingDispute> = pending_dispute_json
            .as_ref()
            .and_then(|json| serde_json::from_str(json).ok());

        Ok(Channel {
            channel_id: bytes_to_hash(&channel_id_bytes),
            peer_id,
            state: u8_to_channel_state(state_u8),
            my_balance: my_balance as Amount,
            their_balance: their_balance as Amount,
            nonce: nonce as u64,
            last_update: last_update as Timestamp,
            pending_payments: Vec::new(), // Loaded separately
            funding_tx_id: row.get::<_, Option<String>>(9).ok().flatten(),
            pending_close,
            pending_dispute,
        })
    }

    /// Serialize a payment for storage.
    #[allow(clippy::type_complexity)]
    fn serialize_payment(
        peer: &PeerId,
        payment: &Payment,
    ) -> Result<(
        Vec<u8>, // id
        Vec<u8>, // channel_peer
        Vec<u8>, // channel_id
        i64,     // amount
        Vec<u8>, // recipient
        Vec<u8>, // query_hash
        String,  // provenance (JSON)
        i64,     // timestamp
        Vec<u8>, // signature
    )> {
        let provenance_json = serde_json::to_string(&payment.provenance)?;

        Ok((
            payment.id.0.to_vec(),
            peer.0.to_vec(),
            payment.channel_id.0.to_vec(),
            payment.amount as i64,
            payment.recipient.0.to_vec(),
            payment.query_hash.0.to_vec(),
            provenance_json,
            payment.timestamp as i64,
            payment.signature.0.to_vec(),
        ))
    }

    /// Deserialize a payment from a database row.
    fn deserialize_payment(row: &rusqlite::Row) -> rusqlite::Result<Payment> {
        let id_bytes: Vec<u8> = row.get(0)?;
        let channel_id_bytes: Vec<u8> = row.get(2)?;
        let amount: i64 = row.get(3)?;
        let recipient_bytes: Vec<u8> = row.get(4)?;
        let query_hash_bytes: Vec<u8> = row.get(5)?;
        let provenance_json: String = row.get(6)?;
        let timestamp: i64 = row.get(7)?;
        let signature_bytes: Vec<u8> = row.get(8)?;

        let provenance: Vec<ProvenanceEntry> =
            serde_json::from_str(&provenance_json).unwrap_or_default();

        Ok(Payment {
            id: bytes_to_hash(&id_bytes),
            channel_id: bytes_to_hash(&channel_id_bytes),
            amount: amount as Amount,
            recipient: bytes_to_peer_id(&recipient_bytes),
            query_hash: bytes_to_hash(&query_hash_bytes),
            provenance,
            timestamp: timestamp as Timestamp,
            signature: bytes_to_signature(&signature_bytes),
        })
    }
}

impl ChannelStore for SqliteChannelStore {
    fn create(&mut self, peer: &PeerId, channel: Channel) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| StoreError::lock_poisoned("database connection lock poisoned"))?;

        // Check if channel already exists
        let peer_bytes = peer.0.to_vec();
        let exists: bool = conn
            .query_row(
                "SELECT 1 FROM channels WHERE peer_id = ?1",
                [&peer_bytes],
                |_| Ok(true),
            )
            .optional()?
            .unwrap_or(false);

        if exists {
            return Err(StoreError::invalid_data("Channel already exists for peer"));
        }

        let (
            peer_bytes,
            channel_id,
            state,
            my_balance,
            their_balance,
            nonce,
            last_update,
            pending_close,
            pending_dispute,
            funding_tx_id,
        ) = Self::serialize_channel(peer, &channel);

        conn.execute(
            "INSERT INTO channels (peer_id, channel_id, state, my_balance, their_balance, nonce, last_update, pending_close, pending_dispute, funding_tx_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![peer_bytes, channel_id, state, my_balance, their_balance, nonce, last_update, pending_close, pending_dispute, funding_tx_id],
        )?;

        Ok(())
    }

    fn get(&self, peer: &PeerId) -> Result<Option<Channel>> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| StoreError::lock_poisoned("database connection lock poisoned"))?;
        let peer_bytes = peer.0.to_vec();

        let channel = conn
            .query_row(
                "SELECT peer_id, channel_id, state, my_balance, their_balance, nonce, last_update, pending_close, pending_dispute, funding_tx_id
                 FROM channels WHERE peer_id = ?1",
                [&peer_bytes],
                Self::deserialize_channel,
            )
            .optional()?;

        // Load pending payments if channel exists
        if let Some(mut channel) = channel {
            let payments = self.load_pending_payments(&conn, peer)?;
            channel.pending_payments = payments;
            Ok(Some(channel))
        } else {
            Ok(None)
        }
    }

    fn update(&mut self, peer: &PeerId, channel: &Channel) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| StoreError::lock_poisoned("database connection lock poisoned"))?;

        let (
            peer_bytes,
            channel_id,
            state,
            my_balance,
            their_balance,
            nonce,
            last_update,
            pending_close,
            pending_dispute,
            funding_tx_id,
        ) = Self::serialize_channel(peer, channel);

        let rows_affected = conn.execute(
            "UPDATE channels SET
                channel_id = ?2, state = ?3, my_balance = ?4, their_balance = ?5,
                nonce = ?6, last_update = ?7, pending_close = ?8, pending_dispute = ?9,
                funding_tx_id = ?10
             WHERE peer_id = ?1",
            params![
                peer_bytes,
                channel_id,
                state,
                my_balance,
                their_balance,
                nonce,
                last_update,
                pending_close,
                pending_dispute,
                funding_tx_id
            ],
        )?;

        if rows_affected == 0 {
            return Err(StoreError::ChannelNotFound);
        }

        Ok(())
    }

    fn list_open(&self) -> Result<Vec<(PeerId, Channel)>> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| StoreError::lock_poisoned("database connection lock poisoned"))?;

        let mut stmt = conn.prepare(
            "SELECT peer_id, channel_id, state, my_balance, their_balance, nonce, last_update, pending_close, pending_dispute, funding_tx_id
             FROM channels WHERE state = ?1",
        )?;

        let channels: Vec<(PeerId, Channel)> = stmt
            .query_map([ChannelState::Open as u8], |row| {
                let peer_bytes: Vec<u8> = row.get(0)?;
                let peer_id = bytes_to_peer_id(&peer_bytes);
                let channel = Self::deserialize_channel(row)?;
                Ok((peer_id, channel))
            })?
            .filter_map(|r| r.ok())
            .collect();

        // Load pending payments for each channel
        let channels_with_payments: Vec<(PeerId, Channel)> = channels
            .into_iter()
            .map(|(peer_id, mut channel)| {
                if let Ok(payments) = self.load_pending_payments(&conn, &peer_id) {
                    channel.pending_payments = payments;
                }
                (peer_id, channel)
            })
            .collect();

        Ok(channels_with_payments)
    }

    fn clear_all(&mut self) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| StoreError::lock_poisoned("database connection lock poisoned"))?;
        // Try to clear pending_payments if the table exists (ignore errors if it doesn't)
        let _ = conn.execute("DELETE FROM pending_payments", []);
        // Clear channels table
        conn.execute("DELETE FROM channels", [])?;
        Ok(())
    }

    fn add_payment(&mut self, peer: &PeerId, payment: Payment) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| StoreError::lock_poisoned("database connection lock poisoned"))?;

        let (
            id,
            channel_peer,
            channel_id,
            amount,
            recipient,
            query_hash,
            provenance,
            timestamp,
            signature,
        ) = Self::serialize_payment(peer, &payment)?;

        conn.execute(
            "INSERT INTO payments (id, channel_peer, channel_id, amount, recipient, query_hash, provenance, timestamp, signature, settled)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 0)",
            params![id, channel_peer, channel_id, amount, recipient, query_hash, provenance, timestamp, signature],
        )?;

        Ok(())
    }

    fn get_pending_payments(&self, peer: &PeerId) -> Result<Vec<Payment>> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| StoreError::lock_poisoned("database connection lock poisoned"))?;
        self.load_pending_payments(&conn, peer)
    }

    fn clear_payments(&mut self, peer: &PeerId, payment_ids: &[Hash]) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| StoreError::lock_poisoned("database connection lock poisoned"))?;
        let peer_bytes = peer.0.to_vec();

        for payment_id in payment_ids {
            let id_bytes = payment_id.0.to_vec();
            conn.execute(
                "UPDATE payments SET settled = 1 WHERE channel_peer = ?1 AND id = ?2",
                params![peer_bytes, id_bytes],
            )?;
        }

        Ok(())
    }
}

impl SqliteChannelStore {
    /// Load pending (unsettled) payments for a peer.
    fn load_pending_payments(&self, conn: &Connection, peer: &PeerId) -> Result<Vec<Payment>> {
        let peer_bytes = peer.0.to_vec();

        let mut stmt = conn.prepare(
            "SELECT id, channel_peer, channel_id, amount, recipient, query_hash, provenance, timestamp, signature
             FROM payments WHERE channel_peer = ?1 AND settled = 0",
        )?;

        let payments: Vec<Payment> = stmt
            .query_map([peer_bytes], Self::deserialize_payment)?
            .filter_map(|r| r.ok())
            .collect();

        Ok(payments)
    }

    /// Delete a channel and all its payments.
    pub fn delete(&mut self, peer: &PeerId) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| StoreError::lock_poisoned("database connection lock poisoned"))?;
        let peer_bytes = peer.0.to_vec();

        conn.execute(
            "DELETE FROM payments WHERE channel_peer = ?1",
            [&peer_bytes],
        )?;
        conn.execute("DELETE FROM channels WHERE peer_id = ?1", [&peer_bytes])?;

        Ok(())
    }
}

/// Convert bytes to Hash.
fn bytes_to_hash(bytes: &[u8]) -> Hash {
    let mut arr = [0u8; 32];
    if bytes.len() >= 32 {
        arr.copy_from_slice(&bytes[..32]);
    }
    Hash(arr)
}

/// Convert bytes to PeerId.
fn bytes_to_peer_id(bytes: &[u8]) -> PeerId {
    let mut arr = [0u8; 20];
    if bytes.len() >= 20 {
        arr.copy_from_slice(&bytes[..20]);
    }
    PeerId::from_bytes(arr)
}

/// Convert bytes to Signature.
fn bytes_to_signature(bytes: &[u8]) -> Signature {
    let mut arr = [0u8; 64];
    if bytes.len() >= 64 {
        arr.copy_from_slice(&bytes[..64]);
    }
    Signature::from_bytes(arr)
}

/// Convert u8 to ChannelState.
fn u8_to_channel_state(v: u8) -> ChannelState {
    match v {
        0 => ChannelState::Opening,
        1 => ChannelState::Open,
        2 => ChannelState::Closing,
        3 => ChannelState::Closed,
        4 => ChannelState::Disputed,
        _ => ChannelState::Opening,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::initialize_schema;
    use nodalync_crypto::{content_hash, generate_identity, peer_id_from_public_key};
    use nodalync_types::Visibility;
    use rusqlite::Connection;

    fn setup_store() -> SqliteChannelStore {
        let conn = Connection::open_in_memory().unwrap();
        initialize_schema(&conn).unwrap();
        SqliteChannelStore::new(Arc::new(Mutex::new(conn)))
    }

    fn test_peer_id() -> PeerId {
        let (_, public_key) = generate_identity();
        peer_id_from_public_key(&public_key)
    }

    fn test_channel(peer_id: PeerId) -> Channel {
        let channel_id = content_hash(b"channel");
        Channel::new(channel_id, peer_id, 1000, 1234567890)
    }

    fn test_payment() -> Payment {
        let (_, public_key) = generate_identity();
        let recipient = peer_id_from_public_key(&public_key);

        Payment {
            id: content_hash(b"payment"),
            channel_id: content_hash(b"channel"),
            amount: 100,
            recipient,
            query_hash: content_hash(b"content"),
            provenance: vec![ProvenanceEntry::new(
                content_hash(b"source"),
                recipient,
                Visibility::Shared,
            )],
            timestamp: 1234567890,
            signature: Signature::from_bytes([0u8; 64]),
        }
    }

    #[test]
    fn test_create_and_get() {
        let mut store = setup_store();
        let peer = test_peer_id();
        let channel = test_channel(peer);

        store.create(&peer, channel.clone()).unwrap();

        let loaded = store.get(&peer).unwrap();
        assert!(loaded.is_some());

        let loaded = loaded.unwrap();
        assert_eq!(loaded.channel_id, channel.channel_id);
        assert_eq!(loaded.my_balance, channel.my_balance);
        assert_eq!(loaded.state, ChannelState::Opening);
    }

    #[test]
    fn test_create_duplicate() {
        let mut store = setup_store();
        let peer = test_peer_id();
        let channel = test_channel(peer);

        store.create(&peer, channel.clone()).unwrap();

        let result = store.create(&peer, channel);
        assert!(result.is_err());
    }

    #[test]
    fn test_get_nonexistent() {
        let store = setup_store();
        let peer = test_peer_id();

        let result = store.get(&peer).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_update() {
        let mut store = setup_store();
        let peer = test_peer_id();
        let mut channel = test_channel(peer);

        store.create(&peer, channel.clone()).unwrap();

        channel.state = ChannelState::Open;
        channel.their_balance = 500;
        channel.nonce = 1;

        store.update(&peer, &channel).unwrap();

        let loaded = store.get(&peer).unwrap().unwrap();
        assert_eq!(loaded.state, ChannelState::Open);
        assert_eq!(loaded.their_balance, 500);
        assert_eq!(loaded.nonce, 1);
    }

    #[test]
    fn test_update_nonexistent() {
        let mut store = setup_store();
        let peer = test_peer_id();
        let channel = test_channel(peer);

        let result = store.update(&peer, &channel);
        assert!(matches!(result, Err(StoreError::ChannelNotFound)));
    }

    #[test]
    fn test_list_open() {
        let mut store = setup_store();

        // Create opening channel
        let peer1 = test_peer_id();
        let channel1 = test_channel(peer1);
        store.create(&peer1, channel1).unwrap();

        // Create open channel
        let peer2 = test_peer_id();
        let mut channel2 = test_channel(peer2);
        channel2.state = ChannelState::Open;
        store.create(&peer2, channel2).unwrap();

        // Create closed channel
        let peer3 = test_peer_id();
        let mut channel3 = test_channel(peer3);
        channel3.state = ChannelState::Closed;
        store.create(&peer3, channel3).unwrap();

        // Only open channel should be listed
        let open_channels = store.list_open().unwrap();
        assert_eq!(open_channels.len(), 1);
        assert_eq!(open_channels[0].0, peer2);
    }

    #[test]
    fn test_add_and_get_payments() {
        let mut store = setup_store();
        let peer = test_peer_id();
        let channel = test_channel(peer);

        store.create(&peer, channel).unwrap();

        let payment = test_payment();
        store.add_payment(&peer, payment.clone()).unwrap();

        let payments = store.get_pending_payments(&peer).unwrap();
        assert_eq!(payments.len(), 1);
        assert_eq!(payments[0].amount, payment.amount);
    }

    #[test]
    fn test_clear_payments() {
        let mut store = setup_store();
        let peer = test_peer_id();
        let channel = test_channel(peer);

        store.create(&peer, channel).unwrap();

        let payment1 = test_payment();
        let mut payment2 = test_payment();
        payment2.id = content_hash(b"payment2");

        store.add_payment(&peer, payment1.clone()).unwrap();
        store.add_payment(&peer, payment2.clone()).unwrap();

        // Clear one payment
        store.clear_payments(&peer, &[payment1.id]).unwrap();

        let pending = store.get_pending_payments(&peer).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, payment2.id);
    }

    #[test]
    fn test_channel_with_pending_payments() {
        let mut store = setup_store();
        let peer = test_peer_id();
        let channel = test_channel(peer);

        store.create(&peer, channel).unwrap();

        let payment = test_payment();
        store.add_payment(&peer, payment).unwrap();

        // Get should include pending payments
        let loaded = store.get(&peer).unwrap().unwrap();
        assert_eq!(loaded.pending_payments.len(), 1);
    }

    #[test]
    fn test_delete_channel() {
        let mut store = setup_store();
        let peer = test_peer_id();
        let channel = test_channel(peer);

        store.create(&peer, channel).unwrap();
        store.add_payment(&peer, test_payment()).unwrap();

        store.delete(&peer).unwrap();

        assert!(store.get(&peer).unwrap().is_none());
        assert!(store.get_pending_payments(&peer).unwrap().is_empty());
    }

    #[test]
    fn test_pending_close_persistence() {
        let mut store = setup_store();
        let peer = test_peer_id();
        let mut channel = test_channel(peer);

        // Create channel without pending close
        store.create(&peer, channel.clone()).unwrap();

        // Add pending close
        let pending_close = PendingClose::new_as_initiator(
            (500, 500), // final_balances
            5,          // nonce
            Signature::from_bytes([1u8; 64]),
            1234567890, // initiated_at
        );
        channel.pending_close = Some(pending_close);
        store.update(&peer, &channel).unwrap();

        // Reload and verify pending close was persisted
        let loaded = store.get(&peer).unwrap().unwrap();
        assert!(loaded.pending_close.is_some());
        let pc = loaded.pending_close.unwrap();
        assert_eq!(pc.final_balances, (500, 500));
        assert_eq!(pc.nonce, 5);
        assert!(pc.we_initiated);
        assert_eq!(pc.initiated_at, 1234567890);
    }

    #[test]
    fn test_pending_dispute_persistence() {
        let mut store = setup_store();
        let peer = test_peer_id();
        let mut channel = test_channel(peer);

        // Create channel without pending dispute
        store.create(&peer, channel.clone()).unwrap();

        // Add pending dispute
        let pending_dispute = PendingDispute::new(
            "0.0.123456@1234567890.000000000".to_string(),
            1234567890, // initiated_at
            5,          // nonce
            400,        // initiator_balance
            600,        // responder_balance
        );
        channel.pending_dispute = Some(pending_dispute);
        store.update(&peer, &channel).unwrap();

        // Reload and verify pending dispute was persisted
        let loaded = store.get(&peer).unwrap().unwrap();
        assert!(loaded.pending_dispute.is_some());
        let pd = loaded.pending_dispute.unwrap();
        assert_eq!(pd.dispute_tx_id, "0.0.123456@1234567890.000000000");
        assert_eq!(pd.initiated_at, 1234567890);
        assert_eq!(pd.disputed_state, (5, 400, 600));
    }

    #[test]
    fn test_clear_pending_close() {
        let mut store = setup_store();
        let peer = test_peer_id();
        let mut channel = test_channel(peer);

        // Create channel with pending close
        let pending_close = PendingClose::new_as_initiator(
            (500, 500),
            5,
            Signature::from_bytes([1u8; 64]),
            1234567890,
        );
        channel.pending_close = Some(pending_close);
        store.create(&peer, channel.clone()).unwrap();

        // Verify it was saved
        let loaded = store.get(&peer).unwrap().unwrap();
        assert!(loaded.pending_close.is_some());

        // Clear pending close
        channel.pending_close = None;
        store.update(&peer, &channel).unwrap();

        // Verify it was cleared
        let loaded = store.get(&peer).unwrap().unwrap();
        assert!(loaded.pending_close.is_none());
    }

    #[test]
    fn test_funding_tx_id_roundtrip() {
        let mut store = setup_store();
        let peer = test_peer_id();
        let mut channel = test_channel(peer);

        // Set a funding_tx_id
        channel.funding_tx_id = Some("0.0.12345@1234567890.000000000".to_string());
        store.create(&peer, channel.clone()).unwrap();

        // Reload and verify it persisted
        let loaded = store.get(&peer).unwrap().unwrap();
        assert_eq!(
            loaded.funding_tx_id,
            Some("0.0.12345@1234567890.000000000".to_string())
        );

        // Update to a different value
        channel.funding_tx_id = Some("0.0.99999@9999999999.000000000".to_string());
        store.update(&peer, &channel).unwrap();

        let loaded = store.get(&peer).unwrap().unwrap();
        assert_eq!(
            loaded.funding_tx_id,
            Some("0.0.99999@9999999999.000000000".to_string())
        );

        // Clear funding_tx_id
        channel.funding_tx_id = None;
        store.update(&peer, &channel).unwrap();

        let loaded = store.get(&peer).unwrap().unwrap();
        assert!(loaded.funding_tx_id.is_none());
    }

    #[test]
    fn test_funding_tx_id_none_by_default() {
        let mut store = setup_store();
        let peer = test_peer_id();
        let channel = test_channel(peer);

        // Channel::new should have funding_tx_id = None
        assert!(channel.funding_tx_id.is_none());

        store.create(&peer, channel).unwrap();

        // Reload and verify it's None
        let loaded = store.get(&peer).unwrap().unwrap();
        assert!(
            loaded.funding_tx_id.is_none(),
            "funding_tx_id should be None by default"
        );
    }
}
