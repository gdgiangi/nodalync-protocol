//! Cache storage for queried content.
//!
//! This module implements a hybrid cache storage system:
//! - Content bytes stored on filesystem (like content store)
//! - Metadata stored in SQLite for efficient queries and LRU eviction

use rusqlite::{params, Connection, OptionalExtension};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use nodalync_crypto::{Hash, PeerId, Timestamp};
use nodalync_wire::payload::PaymentReceipt;

use crate::error::{Result, StoreError};
use crate::traits::CacheStore;
use crate::types::CachedContent;

/// Hybrid filesystem + SQLite cache store.
///
/// Content bytes are stored on the filesystem for efficient I/O,
/// while metadata is stored in SQLite for querying and LRU eviction.
pub struct FsCacheStore {
    /// Root directory for cached content.
    cache_dir: PathBuf,
    /// Database connection for metadata.
    conn: Arc<Mutex<Connection>>,
}

impl FsCacheStore {
    /// Create a new cache store.
    pub fn new(cache_dir: impl AsRef<Path>, conn: Arc<Mutex<Connection>>) -> Result<Self> {
        let cache_dir = cache_dir.as_ref().to_path_buf();
        fs::create_dir_all(&cache_dir)?;
        Ok(Self { cache_dir, conn })
    }

    /// Get the filesystem path for a cached content hash.
    fn content_path(&self, hash: &Hash) -> PathBuf {
        let hex = format!("{}", hash);
        let prefix = &hex[..4]; // First 2 bytes = 4 hex chars
        self.cache_dir.join(prefix).join(&hex)
    }

    /// Ensure the parent directory exists for a hash.
    fn ensure_parent_dir(&self, hash: &Hash) -> Result<()> {
        let path = self.content_path(hash);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        Ok(())
    }

    /// Deserialize cache metadata from a database row.
    fn deserialize_metadata(row: &rusqlite::Row) -> rusqlite::Result<CacheMetadata> {
        let hash_bytes: Vec<u8> = row.get(0)?;
        let source_peer_bytes: Vec<u8> = row.get(1)?;
        let queried_at: i64 = row.get(2)?;
        let size_bytes: i64 = row.get(3)?;
        let payment_receipt_json: String = row.get(4)?;

        let payment_receipt: PaymentReceipt = serde_json::from_str(&payment_receipt_json)
            .unwrap_or_else(|_| PaymentReceipt {
                payment_id: Hash([0u8; 32]),
                amount: 0,
                timestamp: 0,
                channel_nonce: 0,
                distributor_signature: nodalync_crypto::Signature::from_bytes([0u8; 64]),
            });

        Ok(CacheMetadata {
            hash: bytes_to_hash(&hash_bytes),
            source_peer: bytes_to_peer_id(&source_peer_bytes),
            queried_at: queried_at as Timestamp,
            size_bytes: size_bytes as u64,
            payment_receipt,
        })
    }
}

/// Cache entry metadata (stored in SQLite).
struct CacheMetadata {
    hash: Hash,
    source_peer: PeerId,
    queried_at: Timestamp,
    #[allow(dead_code)] // Read from DB but used indirectly in eviction queries
    size_bytes: u64,
    payment_receipt: PaymentReceipt,
}

impl CacheStore for FsCacheStore {
    fn cache(&mut self, entry: CachedContent) -> Result<()> {
        // Store content on filesystem
        self.ensure_parent_dir(&entry.hash)?;
        let path = self.content_path(&entry.hash);
        let mut file = File::create(&path)?;
        file.write_all(&entry.content)?;
        file.sync_all()?;

        // Store metadata in SQLite
        let conn = self
            .conn
            .lock()
            .map_err(|_| StoreError::lock_poisoned("database connection lock poisoned"))?;
        let hash_bytes = entry.hash.0.to_vec();
        let source_peer_bytes = entry.source_peer.0.to_vec();
        let payment_receipt_json = serde_json::to_string(&entry.payment_proof)?;
        let size_bytes = entry.content.len() as i64;

        conn.execute(
            "INSERT OR REPLACE INTO cache (hash, source_peer, queried_at, size_bytes, payment_receipt)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                hash_bytes,
                source_peer_bytes,
                entry.queried_at as i64,
                size_bytes,
                payment_receipt_json
            ],
        )?;

        Ok(())
    }

    fn get(&self, hash: &Hash) -> Result<Option<CachedContent>> {
        // Check metadata first
        let conn = self
            .conn
            .lock()
            .map_err(|_| StoreError::lock_poisoned("database connection lock poisoned"))?;
        let hash_bytes = hash.0.to_vec();

        let metadata = conn
            .query_row(
                "SELECT hash, source_peer, queried_at, size_bytes, payment_receipt
                 FROM cache WHERE hash = ?1",
                [&hash_bytes],
                Self::deserialize_metadata,
            )
            .optional()?;

        let metadata = match metadata {
            Some(m) => m,
            None => return Ok(None),
        };

        // Load content from filesystem
        let path = self.content_path(hash);
        if !path.exists() {
            // Metadata exists but file is missing - remove stale metadata
            drop(conn);
            let conn = self
                .conn
                .lock()
                .map_err(|_| StoreError::lock_poisoned("database connection lock poisoned"))?;
            conn.execute("DELETE FROM cache WHERE hash = ?1", [&hash_bytes])?;
            return Ok(None);
        }

        let mut file = File::open(&path)?;
        let mut content = Vec::new();
        file.read_to_end(&mut content)?;

        Ok(Some(CachedContent {
            hash: metadata.hash,
            content,
            source_peer: metadata.source_peer,
            queried_at: metadata.queried_at,
            payment_proof: metadata.payment_receipt,
        }))
    }

    fn is_cached(&self, hash: &Hash) -> bool {
        let path = self.content_path(hash);
        if !path.exists() {
            return false;
        }

        // Also check metadata exists
        let conn = match self
            .conn
            .lock()
            .map_err(|_| StoreError::lock_poisoned("database connection lock poisoned"))
        {
            Ok(c) => c,
            Err(_) => return false,
        };
        let hash_bytes = hash.0.to_vec();
        conn.query_row("SELECT 1 FROM cache WHERE hash = ?1", [hash_bytes], |_| {
            Ok(true)
        })
        .optional()
        .unwrap_or(None)
        .unwrap_or(false)
    }

    fn evict(&mut self, max_size_bytes: u64) -> Result<u64> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| StoreError::lock_poisoned("database connection lock poisoned"))?;

        // Get current total size
        let current_size: i64 = conn.query_row(
            "SELECT COALESCE(SUM(size_bytes), 0) FROM cache",
            [],
            |row| row.get(0),
        )?;

        if (current_size as u64) <= max_size_bytes {
            return Ok(0);
        }

        // Get entries ordered by queried_at (oldest first) for LRU eviction
        let mut stmt =
            conn.prepare("SELECT hash, size_bytes FROM cache ORDER BY queried_at ASC")?;

        let entries: Vec<(Hash, u64)> = stmt
            .query_map([], |row| {
                let hash_bytes: Vec<u8> = row.get(0)?;
                let size: i64 = row.get(1)?;
                Ok((bytes_to_hash(&hash_bytes), size as u64))
            })?
            .filter_map(|r| r.ok())
            .collect();

        drop(stmt);

        // Evict until we're under the limit
        let mut freed = 0u64;
        let mut remaining_size = current_size as u64;

        for (hash, size) in entries {
            if remaining_size <= max_size_bytes {
                break;
            }

            // Delete from filesystem
            let path = self.content_path(&hash);
            if path.exists() {
                let _ = fs::remove_file(&path);
            }

            // Delete from database
            let hash_bytes = hash.0.to_vec();
            conn.execute("DELETE FROM cache WHERE hash = ?1", [&hash_bytes])?;

            freed += size;
            remaining_size -= size;
        }

        // Clean up empty directories
        if let Ok(entries) = fs::read_dir(&self.cache_dir) {
            for entry in entries.flatten() {
                if entry.path().is_dir() {
                    let _ = fs::remove_dir(entry.path()); // Only succeeds if empty
                }
            }
        }

        Ok(freed)
    }

    fn clear(&mut self) -> Result<()> {
        // Clear all cached content files
        if self.cache_dir.exists() {
            for entry in fs::read_dir(&self.cache_dir)?.flatten() {
                if entry.path().is_dir() {
                    let _ = fs::remove_dir_all(entry.path());
                }
            }
        }

        // Clear database
        let conn = self
            .conn
            .lock()
            .map_err(|_| StoreError::lock_poisoned("database connection lock poisoned"))?;
        conn.execute("DELETE FROM cache", [])?;

        Ok(())
    }

    fn total_size(&self) -> Result<u64> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| StoreError::lock_poisoned("database connection lock poisoned"))?;
        let size: i64 = conn.query_row(
            "SELECT COALESCE(SUM(size_bytes), 0) FROM cache",
            [],
            |row| row.get(0),
        )?;
        Ok(size as u64)
    }
}

impl FsCacheStore {
    /// Get the number of cached entries.
    pub fn count(&self) -> Result<u64> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| StoreError::lock_poisoned("database connection lock poisoned"))?;
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM cache", [], |row| row.get(0))?;
        Ok(count as u64)
    }

    /// Update the queried_at timestamp (for LRU refresh on access).
    pub fn touch(&mut self, hash: &Hash, timestamp: Timestamp) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| StoreError::lock_poisoned("database connection lock poisoned"))?;
        let hash_bytes = hash.0.to_vec();
        conn.execute(
            "UPDATE cache SET queried_at = ?2 WHERE hash = ?1",
            params![hash_bytes, timestamp as i64],
        )?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::initialize_schema;
    use nodalync_crypto::{content_hash, generate_identity, peer_id_from_public_key, Signature};
    use rusqlite::Connection;
    use tempfile::TempDir;

    fn setup_store() -> (FsCacheStore, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = temp_dir.path().join("cache");
        let conn = Connection::open_in_memory().unwrap();
        initialize_schema(&conn).unwrap();
        let store = FsCacheStore::new(cache_dir, Arc::new(Mutex::new(conn))).unwrap();
        (store, temp_dir)
    }

    fn test_cached_content(data: &[u8]) -> CachedContent {
        let (_, public_key) = generate_identity();
        let source_peer = peer_id_from_public_key(&public_key);

        CachedContent {
            hash: content_hash(data),
            content: data.to_vec(),
            source_peer,
            queried_at: 1234567890,
            payment_proof: PaymentReceipt {
                payment_id: content_hash(b"payment"),
                amount: 100,
                timestamp: 1234567890,
                channel_nonce: 1,
                distributor_signature: Signature::from_bytes([0u8; 64]),
            },
        }
    }

    #[test]
    fn test_cache_and_get() {
        let (mut store, _temp) = setup_store();
        let cached = test_cached_content(b"test content");

        store.cache(cached.clone()).unwrap();

        let loaded = store.get(&cached.hash).unwrap();
        assert!(loaded.is_some());

        let loaded = loaded.unwrap();
        assert_eq!(loaded.hash, cached.hash);
        assert_eq!(loaded.content, cached.content);
        assert_eq!(loaded.source_peer, cached.source_peer);
    }

    #[test]
    fn test_is_cached() {
        let (mut store, _temp) = setup_store();
        let cached = test_cached_content(b"content");

        assert!(!store.is_cached(&cached.hash));

        store.cache(cached.clone()).unwrap();
        assert!(store.is_cached(&cached.hash));
    }

    #[test]
    fn test_get_nonexistent() {
        let (store, _temp) = setup_store();
        let hash = content_hash(b"nonexistent");

        let result = store.get(&hash).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_clear() {
        let (mut store, _temp) = setup_store();

        store.cache(test_cached_content(b"content1")).unwrap();
        store.cache(test_cached_content(b"content2")).unwrap();

        assert_eq!(store.count().unwrap(), 2);

        store.clear().unwrap();

        assert_eq!(store.count().unwrap(), 0);
    }

    #[test]
    fn test_total_size() {
        let (mut store, _temp) = setup_store();

        let content1 = b"content1";
        let content2 = b"content2";

        store.cache(test_cached_content(content1)).unwrap();
        store.cache(test_cached_content(content2)).unwrap();

        let total = store.total_size().unwrap();
        assert_eq!(total, (content1.len() + content2.len()) as u64);
    }

    #[test]
    fn test_evict_lru() {
        let (mut store, _temp) = setup_store();

        // Add entries with different timestamps
        let mut c1 = test_cached_content(b"oldest");
        c1.queried_at = 1000;

        let mut c2 = test_cached_content(b"middle");
        c2.queried_at = 2000;

        let mut c3 = test_cached_content(b"newest");
        c3.queried_at = 3000;

        store.cache(c1.clone()).unwrap();
        store.cache(c2.clone()).unwrap();
        store.cache(c3.clone()).unwrap();

        // Evict to keep only the newest
        let max_size = c3.content.len() as u64;
        let freed = store.evict(max_size).unwrap();

        assert!(freed > 0);
        assert_eq!(store.count().unwrap(), 1);

        // Newest should remain
        assert!(store.is_cached(&c3.hash));
        assert!(!store.is_cached(&c1.hash));
        assert!(!store.is_cached(&c2.hash));
    }

    #[test]
    fn test_evict_no_op() {
        let (mut store, _temp) = setup_store();

        store.cache(test_cached_content(b"small")).unwrap();

        // Evict with high limit - should do nothing
        let freed = store.evict(1_000_000).unwrap();
        assert_eq!(freed, 0);
        assert_eq!(store.count().unwrap(), 1);
    }

    #[test]
    fn test_touch() {
        let (mut store, _temp) = setup_store();

        let mut cached = test_cached_content(b"content");
        cached.queried_at = 1000;

        store.cache(cached.clone()).unwrap();
        store.touch(&cached.hash, 9999).unwrap();

        // Add another entry with earlier timestamp
        let mut older = test_cached_content(b"older");
        older.queried_at = 5000;
        store.cache(older.clone()).unwrap();

        // Evict - the "older" one should be evicted because we touched the first
        let max_size = cached.content.len() as u64;
        store.evict(max_size).unwrap();

        assert!(store.is_cached(&cached.hash));
        assert!(!store.is_cached(&older.hash));
    }

    #[test]
    fn test_replace_existing() {
        let (mut store, _temp) = setup_store();

        let mut cached1 = test_cached_content(b"original");
        cached1.queried_at = 1000;

        let mut cached2 = test_cached_content(b"original"); // Same hash
        cached2.queried_at = 2000;

        store.cache(cached1).unwrap();
        store.cache(cached2.clone()).unwrap();

        // Should have only one entry with updated timestamp
        assert_eq!(store.count().unwrap(), 1);

        let loaded = store.get(&cached2.hash).unwrap().unwrap();
        assert_eq!(loaded.queried_at, 2000);
    }
}
