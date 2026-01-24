//! Peer information storage.
//!
//! This module implements storage for known peer information including
//! network addresses, last seen time, and reputation.

use rusqlite::{params, Connection, OptionalExtension};
use std::sync::{Arc, Mutex};

use nodalync_crypto::{PeerId, PublicKey, Timestamp};

use crate::error::{Result, StoreError};
use crate::traits::PeerStore;
use crate::types::PeerInfo;

/// SQLite-based peer store.
pub struct SqlitePeerStore {
    conn: Arc<Mutex<Connection>>,
}

impl SqlitePeerStore {
    /// Create a new peer store with the given database connection.
    pub fn new(conn: Arc<Mutex<Connection>>) -> Self {
        Self { conn }
    }

    /// Deserialize a peer info from a database row.
    fn deserialize_peer(row: &rusqlite::Row) -> rusqlite::Result<PeerInfo> {
        let peer_id_bytes: Vec<u8> = row.get(0)?;
        let public_key_bytes: Vec<u8> = row.get(1)?;
        let addresses_json: String = row.get(2)?;
        let last_seen: i64 = row.get(3)?;
        let reputation: i64 = row.get(4)?;

        let addresses: Vec<String> = serde_json::from_str(&addresses_json).unwrap_or_default();

        Ok(PeerInfo {
            peer_id: bytes_to_peer_id(&peer_id_bytes),
            public_key: bytes_to_public_key(&public_key_bytes),
            addresses,
            last_seen: last_seen as Timestamp,
            reputation,
        })
    }
}

impl PeerStore for SqlitePeerStore {
    fn upsert(&mut self, peer: &PeerInfo) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        let peer_id_bytes = peer.peer_id.0.to_vec();
        let public_key_bytes = peer.public_key.0.to_vec();
        let addresses_json = serde_json::to_string(&peer.addresses)?;
        let last_seen = peer.last_seen as i64;
        let reputation = peer.reputation;

        conn.execute(
            "INSERT INTO peers (peer_id, public_key, addresses, last_seen, reputation)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(peer_id) DO UPDATE SET
                 public_key = excluded.public_key,
                 addresses = excluded.addresses,
                 last_seen = excluded.last_seen,
                 reputation = excluded.reputation",
            params![peer_id_bytes, public_key_bytes, addresses_json, last_seen, reputation],
        )?;

        Ok(())
    }

    fn get(&self, peer_id: &PeerId) -> Result<Option<PeerInfo>> {
        let conn = self.conn.lock().unwrap();
        let peer_id_bytes = peer_id.0.to_vec();

        let peer = conn
            .query_row(
                "SELECT peer_id, public_key, addresses, last_seen, reputation
                 FROM peers WHERE peer_id = ?1",
                [peer_id_bytes],
                Self::deserialize_peer,
            )
            .optional()?;

        Ok(peer)
    }

    fn list(&self) -> Result<Vec<PeerInfo>> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn.prepare(
            "SELECT peer_id, public_key, addresses, last_seen, reputation
             FROM peers ORDER BY last_seen DESC",
        )?;

        let peers: Vec<PeerInfo> = stmt
            .query_map([], Self::deserialize_peer)?
            .filter_map(|r| r.ok())
            .collect();

        Ok(peers)
    }

    fn update_last_seen(&mut self, peer_id: &PeerId, timestamp: Timestamp) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let peer_id_bytes = peer_id.0.to_vec();

        let rows_affected = conn.execute(
            "UPDATE peers SET last_seen = ?2 WHERE peer_id = ?1",
            params![peer_id_bytes, timestamp as i64],
        )?;

        if rows_affected == 0 {
            return Err(StoreError::PeerNotFound);
        }

        Ok(())
    }

    fn update_reputation(&mut self, peer_id: &PeerId, delta: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let peer_id_bytes = peer_id.0.to_vec();

        let rows_affected = conn.execute(
            "UPDATE peers SET reputation = reputation + ?2 WHERE peer_id = ?1",
            params![peer_id_bytes, delta],
        )?;

        if rows_affected == 0 {
            return Err(StoreError::PeerNotFound);
        }

        Ok(())
    }

    fn delete(&mut self, peer_id: &PeerId) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let peer_id_bytes = peer_id.0.to_vec();

        conn.execute("DELETE FROM peers WHERE peer_id = ?1", [peer_id_bytes])?;

        Ok(())
    }
}

impl SqlitePeerStore {
    /// List peers with reputation above a threshold.
    pub fn list_by_reputation(&self, min_reputation: i64) -> Result<Vec<PeerInfo>> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn.prepare(
            "SELECT peer_id, public_key, addresses, last_seen, reputation
             FROM peers WHERE reputation >= ?1 ORDER BY reputation DESC",
        )?;

        let peers: Vec<PeerInfo> = stmt
            .query_map([min_reputation], Self::deserialize_peer)?
            .filter_map(|r| r.ok())
            .collect();

        Ok(peers)
    }

    /// List peers seen within a time window.
    pub fn list_recently_seen(&self, since: Timestamp) -> Result<Vec<PeerInfo>> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn.prepare(
            "SELECT peer_id, public_key, addresses, last_seen, reputation
             FROM peers WHERE last_seen >= ?1 ORDER BY last_seen DESC",
        )?;

        let peers: Vec<PeerInfo> = stmt
            .query_map([since as i64], Self::deserialize_peer)?
            .filter_map(|r| r.ok())
            .collect();

        Ok(peers)
    }

    /// Count total known peers.
    pub fn count(&self) -> Result<u64> {
        let conn = self.conn.lock().unwrap();

        let count: i64 = conn.query_row("SELECT COUNT(*) FROM peers", [], |row| row.get(0))?;

        Ok(count as u64)
    }
}

/// Convert bytes to PeerId.
fn bytes_to_peer_id(bytes: &[u8]) -> PeerId {
    let mut arr = [0u8; 20];
    if bytes.len() >= 20 {
        arr.copy_from_slice(&bytes[..20]);
    }
    PeerId::from_bytes(arr)
}

/// Convert bytes to PublicKey.
fn bytes_to_public_key(bytes: &[u8]) -> PublicKey {
    let mut arr = [0u8; 32];
    if bytes.len() >= 32 {
        arr.copy_from_slice(&bytes[..32]);
    }
    PublicKey::from_bytes(arr)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::initialize_schema;
    use nodalync_crypto::{generate_identity, peer_id_from_public_key};
    use rusqlite::Connection;

    fn setup_store() -> SqlitePeerStore {
        let conn = Connection::open_in_memory().unwrap();
        initialize_schema(&conn).unwrap();
        SqlitePeerStore::new(Arc::new(Mutex::new(conn)))
    }

    fn test_peer_info() -> PeerInfo {
        let (_, public_key) = generate_identity();
        let peer_id = peer_id_from_public_key(&public_key);
        PeerInfo::new(
            peer_id,
            public_key,
            vec!["/ip4/127.0.0.1/tcp/9000".to_string()],
            1234567890,
        )
    }

    #[test]
    fn test_upsert_and_get() {
        let mut store = setup_store();
        let peer = test_peer_info();

        store.upsert(&peer).unwrap();

        let loaded = store.get(&peer.peer_id).unwrap();
        assert!(loaded.is_some());

        let loaded = loaded.unwrap();
        assert_eq!(loaded.peer_id, peer.peer_id);
        assert_eq!(loaded.public_key, peer.public_key);
        assert_eq!(loaded.addresses, peer.addresses);
        assert_eq!(loaded.last_seen, peer.last_seen);
        assert_eq!(loaded.reputation, 0);
    }

    #[test]
    fn test_upsert_updates_existing() {
        let mut store = setup_store();
        let mut peer = test_peer_info();

        store.upsert(&peer).unwrap();

        // Update the peer
        peer.addresses.push("/ip4/192.168.1.1/tcp/9000".to_string());
        peer.reputation = 10;
        store.upsert(&peer).unwrap();

        let loaded = store.get(&peer.peer_id).unwrap().unwrap();
        assert_eq!(loaded.addresses.len(), 2);
        assert_eq!(loaded.reputation, 10);
    }

    #[test]
    fn test_get_nonexistent() {
        let store = setup_store();
        let (_, public_key) = generate_identity();
        let peer_id = peer_id_from_public_key(&public_key);

        let result = store.get(&peer_id).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_list() {
        let mut store = setup_store();

        let peer1 = test_peer_info();
        let peer2 = test_peer_info();

        store.upsert(&peer1).unwrap();
        store.upsert(&peer2).unwrap();

        let peers = store.list().unwrap();
        assert_eq!(peers.len(), 2);
    }

    #[test]
    fn test_update_last_seen() {
        let mut store = setup_store();
        let peer = test_peer_info();

        store.upsert(&peer).unwrap();

        let new_timestamp = 9999999999u64;
        store.update_last_seen(&peer.peer_id, new_timestamp).unwrap();

        let loaded = store.get(&peer.peer_id).unwrap().unwrap();
        assert_eq!(loaded.last_seen, new_timestamp);
    }

    #[test]
    fn test_update_last_seen_nonexistent() {
        let mut store = setup_store();
        let (_, public_key) = generate_identity();
        let peer_id = peer_id_from_public_key(&public_key);

        let result = store.update_last_seen(&peer_id, 1234567890);
        assert!(matches!(result, Err(StoreError::PeerNotFound)));
    }

    #[test]
    fn test_update_reputation() {
        let mut store = setup_store();
        let peer = test_peer_info();

        store.upsert(&peer).unwrap();

        store.update_reputation(&peer.peer_id, 10).unwrap();
        let loaded = store.get(&peer.peer_id).unwrap().unwrap();
        assert_eq!(loaded.reputation, 10);

        store.update_reputation(&peer.peer_id, -5).unwrap();
        let loaded = store.get(&peer.peer_id).unwrap().unwrap();
        assert_eq!(loaded.reputation, 5);
    }

    #[test]
    fn test_delete() {
        let mut store = setup_store();
        let peer = test_peer_info();

        store.upsert(&peer).unwrap();
        store.delete(&peer.peer_id).unwrap();

        let loaded = store.get(&peer.peer_id).unwrap();
        assert!(loaded.is_none());
    }

    #[test]
    fn test_delete_nonexistent() {
        let mut store = setup_store();
        let (_, public_key) = generate_identity();
        let peer_id = peer_id_from_public_key(&public_key);

        // Should not error
        store.delete(&peer_id).unwrap();
    }

    #[test]
    fn test_list_by_reputation() {
        let mut store = setup_store();

        let mut peer1 = test_peer_info();
        peer1.reputation = 5;

        let mut peer2 = test_peer_info();
        peer2.reputation = 15;

        let mut peer3 = test_peer_info();
        peer3.reputation = -5;

        store.upsert(&peer1).unwrap();
        store.upsert(&peer2).unwrap();
        store.upsert(&peer3).unwrap();

        let good_peers = store.list_by_reputation(10).unwrap();
        assert_eq!(good_peers.len(), 1);
        assert_eq!(good_peers[0].reputation, 15);

        let ok_peers = store.list_by_reputation(0).unwrap();
        assert_eq!(ok_peers.len(), 2);
    }

    #[test]
    fn test_list_recently_seen() {
        let mut store = setup_store();

        let mut peer1 = test_peer_info();
        peer1.last_seen = 1000;

        let mut peer2 = test_peer_info();
        peer2.last_seen = 2000;

        let mut peer3 = test_peer_info();
        peer3.last_seen = 3000;

        store.upsert(&peer1).unwrap();
        store.upsert(&peer2).unwrap();
        store.upsert(&peer3).unwrap();

        let recent = store.list_recently_seen(1500).unwrap();
        assert_eq!(recent.len(), 2);
    }

    #[test]
    fn test_count() {
        let mut store = setup_store();

        assert_eq!(store.count().unwrap(), 0);

        store.upsert(&test_peer_info()).unwrap();
        store.upsert(&test_peer_info()).unwrap();

        assert_eq!(store.count().unwrap(), 2);
    }
}
