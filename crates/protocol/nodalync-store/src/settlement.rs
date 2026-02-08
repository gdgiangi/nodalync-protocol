//! Settlement queue storage.
//!
//! This module implements the settlement queue for storing pending
//! distributions until they are batch-settled on-chain.

use rusqlite::{params, Connection, OptionalExtension};
use std::sync::{Arc, Mutex};

use nodalync_crypto::{Hash, PeerId, Timestamp};
use nodalync_types::Amount;

use crate::error::{Result, StoreError};
use crate::traits::SettlementQueueStore;
use crate::types::QueuedDistribution;

/// SQLite-based settlement queue.
pub struct SqliteSettlementQueue {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteSettlementQueue {
    /// Create a new settlement queue with the given database connection.
    pub fn new(conn: Arc<Mutex<Connection>>) -> Self {
        Self { conn }
    }

    /// Deserialize a queued distribution from a database row.
    fn deserialize_distribution(row: &rusqlite::Row) -> rusqlite::Result<QueuedDistribution> {
        let payment_id_bytes: Vec<u8> = row.get(1)?;
        let recipient_bytes: Vec<u8> = row.get(2)?;
        let amount: i64 = row.get(3)?;
        let source_hash_bytes: Vec<u8> = row.get(4)?;
        let queued_at: i64 = row.get(5)?;

        Ok(QueuedDistribution {
            payment_id: bytes_to_hash(&payment_id_bytes),
            recipient: bytes_to_peer_id(&recipient_bytes),
            amount: amount as Amount,
            source_hash: bytes_to_hash(&source_hash_bytes),
            queued_at: queued_at as Timestamp,
        })
    }
}

impl SettlementQueueStore for SqliteSettlementQueue {
    fn enqueue(&mut self, distribution: QueuedDistribution) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| StoreError::lock_poisoned("database connection lock poisoned"))?;

        let payment_id_bytes = distribution.payment_id.0.to_vec();
        let recipient_bytes = distribution.recipient.0.to_vec();
        let source_hash_bytes = distribution.source_hash.0.to_vec();

        conn.execute(
            "INSERT OR IGNORE INTO settlement_queue (payment_id, recipient, amount, source_hash, queued_at, settled)
             VALUES (?1, ?2, ?3, ?4, ?5, 0)",
            params![
                payment_id_bytes,
                recipient_bytes,
                distribution.amount as i64,
                source_hash_bytes,
                distribution.queued_at as i64,
            ],
        )?;

        Ok(())
    }

    fn get_pending(&self) -> Result<Vec<QueuedDistribution>> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| StoreError::lock_poisoned("database connection lock poisoned"))?;

        let mut stmt = conn.prepare(
            "SELECT id, payment_id, recipient, amount, source_hash, queued_at
             FROM settlement_queue WHERE settled = 0 ORDER BY queued_at ASC",
        )?;

        let distributions: Vec<QueuedDistribution> = stmt
            .query_map([], Self::deserialize_distribution)?
            .filter_map(|r| r.ok())
            .collect();

        Ok(distributions)
    }

    fn get_pending_for(&self, recipient: &PeerId) -> Result<Vec<QueuedDistribution>> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| StoreError::lock_poisoned("database connection lock poisoned"))?;
        let recipient_bytes = recipient.0.to_vec();

        let mut stmt = conn.prepare(
            "SELECT id, payment_id, recipient, amount, source_hash, queued_at
             FROM settlement_queue WHERE settled = 0 AND recipient = ?1 ORDER BY queued_at ASC",
        )?;

        let distributions: Vec<QueuedDistribution> = stmt
            .query_map([recipient_bytes], Self::deserialize_distribution)?
            .filter_map(|r| r.ok())
            .collect();

        Ok(distributions)
    }

    fn get_pending_total(&self) -> Result<Amount> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| StoreError::lock_poisoned("database connection lock poisoned"))?;

        let total: i64 = conn.query_row(
            "SELECT COALESCE(SUM(amount), 0) FROM settlement_queue WHERE settled = 0",
            [],
            |row| row.get(0),
        )?;

        Ok(total as Amount)
    }

    fn mark_settled(&mut self, payment_ids: &[Hash], batch_id: &Hash) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| StoreError::lock_poisoned("database connection lock poisoned"))?;
        let batch_id_bytes = batch_id.0.to_vec();

        for payment_id in payment_ids {
            let payment_id_bytes = payment_id.0.to_vec();
            conn.execute(
                "UPDATE settlement_queue SET settled = 1, batch_id = ?2 WHERE payment_id = ?1",
                params![payment_id_bytes, batch_id_bytes],
            )?;
        }

        Ok(())
    }

    fn get_last_settlement_time(&self) -> Result<Option<Timestamp>> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| StoreError::lock_poisoned("database connection lock poisoned"))?;

        let value: Option<String> = conn
            .query_row(
                "SELECT value FROM settlement_meta WHERE key = 'last_settlement_time'",
                [],
                |row| row.get(0),
            )
            .optional()?;

        match value {
            Some(v) => {
                let timestamp: Timestamp = v
                    .parse()
                    .map_err(|_| StoreError::invalid_data("Invalid last_settlement_time format"))?;
                Ok(Some(timestamp))
            }
            None => Ok(None),
        }
    }

    fn set_last_settlement_time(&mut self, timestamp: Timestamp) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| StoreError::lock_poisoned("database connection lock poisoned"))?;

        conn.execute(
            "INSERT OR REPLACE INTO settlement_meta (key, value) VALUES ('last_settlement_time', ?1)",
            [timestamp.to_string()],
        )?;

        Ok(())
    }
}

impl SqliteSettlementQueue {
    /// Get count of pending distributions.
    pub fn pending_count(&self) -> Result<u64> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| StoreError::lock_poisoned("database connection lock poisoned"))?;
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM settlement_queue WHERE settled = 0",
            [],
            |row| row.get(0),
        )?;
        Ok(count as u64)
    }

    /// Get pending total for a specific recipient.
    pub fn get_pending_total_for(&self, recipient: &PeerId) -> Result<Amount> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| StoreError::lock_poisoned("database connection lock poisoned"))?;
        let recipient_bytes = recipient.0.to_vec();

        let total: i64 = conn.query_row(
            "SELECT COALESCE(SUM(amount), 0) FROM settlement_queue WHERE settled = 0 AND recipient = ?1",
            [recipient_bytes],
            |row| row.get(0),
        )?;

        Ok(total as Amount)
    }

    /// Get distributions for a specific batch.
    pub fn get_batch(&self, batch_id: &Hash) -> Result<Vec<QueuedDistribution>> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| StoreError::lock_poisoned("database connection lock poisoned"))?;
        let batch_id_bytes = batch_id.0.to_vec();

        let mut stmt = conn.prepare(
            "SELECT id, payment_id, recipient, amount, source_hash, queued_at
             FROM settlement_queue WHERE batch_id = ?1",
        )?;

        let distributions: Vec<QueuedDistribution> = stmt
            .query_map([batch_id_bytes], Self::deserialize_distribution)?
            .filter_map(|r| r.ok())
            .collect();

        Ok(distributions)
    }

    /// Aggregate pending distributions by recipient.
    ///
    /// Returns a map of recipient -> total amount.
    pub fn aggregate_by_recipient(&self) -> Result<Vec<(PeerId, Amount)>> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| StoreError::lock_poisoned("database connection lock poisoned"))?;

        let mut stmt = conn.prepare(
            "SELECT recipient, SUM(amount) FROM settlement_queue
             WHERE settled = 0 GROUP BY recipient ORDER BY SUM(amount) DESC",
        )?;

        let aggregates: Vec<(PeerId, Amount)> = stmt
            .query_map([], |row| {
                let recipient_bytes: Vec<u8> = row.get(0)?;
                let total: i64 = row.get(1)?;
                Ok((bytes_to_peer_id(&recipient_bytes), total as Amount))
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(aggregates)
    }

    /// Delete settled distributions older than a timestamp.
    ///
    /// Used for cleanup of old settled records.
    pub fn cleanup_settled(&mut self, older_than: Timestamp) -> Result<u64> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| StoreError::lock_poisoned("database connection lock poisoned"))?;

        let deleted = conn.execute(
            "DELETE FROM settlement_queue WHERE settled = 1 AND queued_at < ?1",
            [older_than as i64],
        )?;

        Ok(deleted as u64)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::initialize_schema;
    use nodalync_crypto::{content_hash, generate_identity, peer_id_from_public_key};
    use rusqlite::Connection;

    fn setup_queue() -> SqliteSettlementQueue {
        let conn = Connection::open_in_memory().unwrap();
        initialize_schema(&conn).unwrap();
        SqliteSettlementQueue::new(Arc::new(Mutex::new(conn)))
    }

    fn test_distribution(recipient: PeerId, amount: Amount) -> QueuedDistribution {
        QueuedDistribution {
            payment_id: content_hash(&amount.to_be_bytes()),
            recipient,
            amount,
            source_hash: content_hash(b"source"),
            queued_at: 1234567890,
        }
    }

    fn test_peer_id() -> PeerId {
        let (_, public_key) = generate_identity();
        peer_id_from_public_key(&public_key)
    }

    #[test]
    fn test_enqueue_and_get_pending() {
        let mut queue = setup_queue();
        let recipient = test_peer_id();
        let dist = test_distribution(recipient, 100);

        queue.enqueue(dist.clone()).unwrap();

        let pending = queue.get_pending().unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].amount, dist.amount);
        assert_eq!(pending[0].recipient, dist.recipient);
    }

    #[test]
    fn test_get_pending_for_recipient() {
        let mut queue = setup_queue();

        let peer1 = test_peer_id();
        let peer2 = test_peer_id();

        queue.enqueue(test_distribution(peer1, 100)).unwrap();
        queue.enqueue(test_distribution(peer1, 200)).unwrap();
        queue.enqueue(test_distribution(peer2, 50)).unwrap();

        let peer1_pending = queue.get_pending_for(&peer1).unwrap();
        assert_eq!(peer1_pending.len(), 2);

        let peer2_pending = queue.get_pending_for(&peer2).unwrap();
        assert_eq!(peer2_pending.len(), 1);
    }

    #[test]
    fn test_get_pending_total() {
        let mut queue = setup_queue();

        let peer1 = test_peer_id();
        let peer2 = test_peer_id();

        queue.enqueue(test_distribution(peer1, 100)).unwrap();
        queue.enqueue(test_distribution(peer1, 200)).unwrap();
        queue.enqueue(test_distribution(peer2, 50)).unwrap();

        let total = queue.get_pending_total().unwrap();
        assert_eq!(total, 350);
    }

    #[test]
    fn test_mark_settled() {
        let mut queue = setup_queue();
        let recipient = test_peer_id();

        let dist1 = test_distribution(recipient, 100);
        let dist2 = test_distribution(recipient, 200);

        queue.enqueue(dist1.clone()).unwrap();
        queue.enqueue(dist2.clone()).unwrap();

        let batch_id = content_hash(b"batch1");
        queue.mark_settled(&[dist1.payment_id], &batch_id).unwrap();

        let pending = queue.get_pending().unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].payment_id, dist2.payment_id);
    }

    #[test]
    fn test_settlement_time() {
        let mut queue = setup_queue();

        // Initially no settlement time
        assert!(queue.get_last_settlement_time().unwrap().is_none());

        // Set settlement time
        queue.set_last_settlement_time(1234567890).unwrap();

        let time = queue.get_last_settlement_time().unwrap();
        assert_eq!(time, Some(1234567890));

        // Update settlement time
        queue.set_last_settlement_time(9999999999).unwrap();

        let time = queue.get_last_settlement_time().unwrap();
        assert_eq!(time, Some(9999999999));
    }

    #[test]
    fn test_pending_count() {
        let mut queue = setup_queue();
        let recipient = test_peer_id();

        assert_eq!(queue.pending_count().unwrap(), 0);

        queue.enqueue(test_distribution(recipient, 100)).unwrap();
        queue.enqueue(test_distribution(recipient, 200)).unwrap();

        assert_eq!(queue.pending_count().unwrap(), 2);
    }

    #[test]
    fn test_get_pending_total_for() {
        let mut queue = setup_queue();

        let peer1 = test_peer_id();
        let peer2 = test_peer_id();

        queue.enqueue(test_distribution(peer1, 100)).unwrap();
        queue.enqueue(test_distribution(peer1, 200)).unwrap();
        queue.enqueue(test_distribution(peer2, 50)).unwrap();

        assert_eq!(queue.get_pending_total_for(&peer1).unwrap(), 300);
        assert_eq!(queue.get_pending_total_for(&peer2).unwrap(), 50);
    }

    #[test]
    fn test_aggregate_by_recipient() {
        let mut queue = setup_queue();

        let peer1 = test_peer_id();
        let peer2 = test_peer_id();

        queue.enqueue(test_distribution(peer1, 100)).unwrap();
        queue.enqueue(test_distribution(peer1, 200)).unwrap();
        queue.enqueue(test_distribution(peer2, 50)).unwrap();

        let aggregates = queue.aggregate_by_recipient().unwrap();
        assert_eq!(aggregates.len(), 2);

        // Should be sorted by amount descending
        assert_eq!(aggregates[0].1, 300); // peer1
        assert_eq!(aggregates[1].1, 50); // peer2
    }

    #[test]
    fn test_get_batch() {
        let mut queue = setup_queue();
        let recipient = test_peer_id();

        let dist1 = test_distribution(recipient, 100);
        let dist2 = test_distribution(recipient, 200);

        queue.enqueue(dist1.clone()).unwrap();
        queue.enqueue(dist2.clone()).unwrap();

        let batch_id = content_hash(b"batch1");
        queue
            .mark_settled(&[dist1.payment_id, dist2.payment_id], &batch_id)
            .unwrap();

        let batch = queue.get_batch(&batch_id).unwrap();
        assert_eq!(batch.len(), 2);
    }

    #[test]
    fn test_enqueue_duplicate_payment_id_recipient_ignored() {
        // Regression test: enqueuing the same payment_id+recipient twice
        // should not create duplicates (INSERT OR IGNORE + UNIQUE index).
        let mut queue = setup_queue();
        let recipient = test_peer_id();

        let dist = QueuedDistribution {
            payment_id: content_hash(b"same-payment"),
            recipient,
            amount: 100,
            source_hash: content_hash(b"source"),
            queued_at: 1234567890,
        };

        // First enqueue succeeds
        queue.enqueue(dist.clone()).unwrap();

        // Second enqueue with same payment_id+recipient is silently ignored
        queue.enqueue(dist.clone()).unwrap();

        let pending = queue.get_pending().unwrap();
        assert_eq!(pending.len(), 1, "Duplicate payment_id+recipient should be ignored");
        assert_eq!(pending[0].amount, 100);
    }

    #[test]
    fn test_cleanup_settled() {
        let mut queue = setup_queue();
        let recipient = test_peer_id();

        let mut dist1 = test_distribution(recipient, 100);
        dist1.queued_at = 1000;

        let mut dist2 = test_distribution(recipient, 200);
        dist2.queued_at = 2000;

        queue.enqueue(dist1.clone()).unwrap();
        queue.enqueue(dist2.clone()).unwrap();

        let batch_id = content_hash(b"batch");
        queue
            .mark_settled(&[dist1.payment_id, dist2.payment_id], &batch_id)
            .unwrap();

        // Cleanup entries older than 1500
        let deleted = queue.cleanup_settled(1500).unwrap();
        assert_eq!(deleted, 1);

        let batch = queue.get_batch(&batch_id).unwrap();
        assert_eq!(batch.len(), 1);
    }
}
