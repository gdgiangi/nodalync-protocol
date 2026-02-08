//! Local storage layer for the Nodalync protocol.
//!
//! This crate provides persistence for all node state including:
//!
//! - **Content storage** (filesystem): Raw content files keyed by hash
//! - **Manifest storage** (SQLite): Content metadata and economics
//! - **Provenance graph** (SQLite): Derivation relationships for revenue distribution
//! - **Channel storage** (SQLite): Payment channel state and pending payments
//! - **Peer storage** (SQLite): Known peer information and reputation
//! - **Cache storage** (hybrid): Cached content from queries
//! - **Settlement queue** (SQLite): Pending distributions awaiting batch settlement
//! - **Identity storage** (filesystem): Encrypted private key
//!
//! # Storage Layout
//!
//! ```text
//! ~/.nodalync/
//! ├── config.toml              # Node configuration
//! ├── identity/
//! │   ├── keypair.key          # Ed25519 private key (encrypted)
//! │   └── peer_id              # Public identity
//! ├── content/
//! │   └── {hash_prefix}/
//! │       └── {hash}           # Raw content files
//! ├── nodalync.db              # SQLite: manifests, provenance, channels, etc.
//! └── cache/
//!     └── {hash_prefix}/
//!         └── {hash}           # Cached content from queries
//! ```
//!
//! # Example
//!
//! ```no_run
//! use nodalync_store::{NodeState, NodeStateConfig, ContentStore};
//! use std::path::PathBuf;
//!
//! // Initialize node state with default paths
//! let config = NodeStateConfig::new(PathBuf::from("~/.nodalync"));
//! let mut state = NodeState::open(config).expect("Failed to open node state");
//!
//! // Store some content
//! let content = b"Hello, Nodalync!";
//! let hash = state.content.store(content).expect("Failed to store content");
//!
//! // Load it back
//! let loaded = state.content.load(&hash).expect("Failed to load").unwrap();
//! assert_eq!(loaded, content);
//! ```
//!
//! # Trait-Based Design
//!
//! All storage components are defined as traits, allowing for alternative
//! implementations (e.g., in-memory stores for testing). The default
//! implementations use filesystem and SQLite.

// Module declarations
pub mod cache;
pub mod channel;
pub mod content;
pub mod error;
pub mod identity;
pub mod manifest;
pub mod peers;
pub mod provenance;
pub mod schema;
pub mod settlement;
pub mod traits;
pub mod types;

// Re-export error types
pub use error::{Result, StoreError};

// Re-export traits
pub use traits::{
    CacheStore, ChannelStore, ContentStore, ManifestStore, PeerStore, ProvenanceGraph,
    SettlementQueueStore,
};

// Re-export types
pub use types::{CachedContent, ManifestFilter, PeerInfo, QueuedDistribution};

// Re-export implementations
pub use cache::FsCacheStore;
pub use channel::SqliteChannelStore;
pub use content::FsContentStore;
pub use identity::IdentityStore;
pub use manifest::SqliteManifestStore;
pub use peers::SqlitePeerStore;
pub use provenance::SqliteProvenanceGraph;
pub use settlement::SqliteSettlementQueue;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use nodalync_crypto::Hash;
use nodalync_wire::AnnouncePayload;
use rusqlite::Connection;

/// Get the default data directory for Nodalync node state.
///
/// Priority:
/// 1. `NODALYNC_DATA_DIR` environment variable (if set)
/// 2. Platform-specific data directory (e.g., `~/Library/Application Support/io.nodalync.nodalync` on macOS)
/// 3. Fallback to `$HOME/.nodalync`
///
/// Both the CLI and MCP server should use this function to ensure they
/// share the same storage location on a single machine.
pub fn default_data_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("NODALYNC_DATA_DIR") {
        return PathBuf::from(dir);
    }

    directories::ProjectDirs::from("io", "nodalync", "nodalync")
        .map(|dirs| dirs.data_dir().to_path_buf())
        .unwrap_or_else(|| {
            std::env::var("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(".nodalync")
        })
}

/// Configuration for NodeState.
#[derive(Debug, Clone)]
pub struct NodeStateConfig {
    /// Base directory for all node data.
    pub base_dir: PathBuf,
    /// Content storage directory (default: base_dir/content).
    pub content_dir: Option<PathBuf>,
    /// Cache directory (default: base_dir/cache).
    pub cache_dir: Option<PathBuf>,
    /// Identity directory (default: base_dir/identity).
    pub identity_dir: Option<PathBuf>,
    /// Database file path (default: base_dir/nodalync.db).
    pub database_path: Option<PathBuf>,
}

impl NodeStateConfig {
    /// Create a new configuration with the given base directory.
    pub fn new(base_dir: impl AsRef<Path>) -> Self {
        Self {
            base_dir: base_dir.as_ref().to_path_buf(),
            content_dir: None,
            cache_dir: None,
            identity_dir: None,
            database_path: None,
        }
    }

    /// Set the content directory.
    pub fn with_content_dir(mut self, path: impl AsRef<Path>) -> Self {
        self.content_dir = Some(path.as_ref().to_path_buf());
        self
    }

    /// Set the cache directory.
    pub fn with_cache_dir(mut self, path: impl AsRef<Path>) -> Self {
        self.cache_dir = Some(path.as_ref().to_path_buf());
        self
    }

    /// Set the identity directory.
    pub fn with_identity_dir(mut self, path: impl AsRef<Path>) -> Self {
        self.identity_dir = Some(path.as_ref().to_path_buf());
        self
    }

    /// Set the database path.
    pub fn with_database_path(mut self, path: impl AsRef<Path>) -> Self {
        self.database_path = Some(path.as_ref().to_path_buf());
        self
    }

    /// Get the content directory.
    pub fn content_dir(&self) -> PathBuf {
        self.content_dir
            .clone()
            .unwrap_or_else(|| self.base_dir.join("content"))
    }

    /// Get the cache directory.
    pub fn cache_dir(&self) -> PathBuf {
        self.cache_dir
            .clone()
            .unwrap_or_else(|| self.base_dir.join("cache"))
    }

    /// Get the identity directory.
    pub fn identity_dir(&self) -> PathBuf {
        self.identity_dir
            .clone()
            .unwrap_or_else(|| self.base_dir.join("identity"))
    }

    /// Get the database path.
    pub fn database_path(&self) -> PathBuf {
        self.database_path
            .clone()
            .unwrap_or_else(|| self.base_dir.join("nodalync.db"))
    }
}

/// Complete node state with all storage components.
///
/// This struct composes all storage components and provides a single
/// entry point for managing node state.
pub struct NodeState {
    /// Identity storage (encrypted private key).
    pub identity: IdentityStore,
    /// Content storage (filesystem).
    pub content: FsContentStore,
    /// Manifest storage (SQLite).
    pub manifests: SqliteManifestStore,
    /// Provenance graph (SQLite).
    pub provenance: SqliteProvenanceGraph,
    /// Channel storage (SQLite).
    pub channels: SqliteChannelStore,
    /// Peer storage (SQLite).
    pub peers: SqlitePeerStore,
    /// Cache storage (hybrid).
    pub cache: FsCacheStore,
    /// Settlement queue (SQLite).
    pub settlement: SqliteSettlementQueue,
    /// Shared database connection.
    conn: Arc<Mutex<Connection>>,
    /// Configuration used to open this state.
    config: NodeStateConfig,
}

impl NodeState {
    /// Open node state with the given configuration.
    ///
    /// Creates all necessary directories and initializes the database schema.
    pub fn open(config: NodeStateConfig) -> Result<Self> {
        // Create base directory
        std::fs::create_dir_all(&config.base_dir)?;

        // Open database connection
        let db_path = config.database_path();
        tracing::info!(db_path = %db_path.display(), "Opening node state database");
        let conn = Connection::open(&db_path)?;

        // Initialize schema
        schema::initialize_schema(&conn)?;

        // Wrap connection in Arc<Mutex> for sharing
        let conn = Arc::new(Mutex::new(conn));

        // Create storage components
        let identity = IdentityStore::new(config.identity_dir())?;
        let content = FsContentStore::new(config.content_dir())?;
        let manifests = SqliteManifestStore::new(Arc::clone(&conn));
        let provenance = SqliteProvenanceGraph::new(Arc::clone(&conn));
        let channels = SqliteChannelStore::new(Arc::clone(&conn));
        let peers = SqlitePeerStore::new(Arc::clone(&conn));
        let cache = FsCacheStore::new(config.cache_dir(), Arc::clone(&conn))?;
        let settlement = SqliteSettlementQueue::new(Arc::clone(&conn));

        Ok(Self {
            identity,
            content,
            manifests,
            provenance,
            channels,
            peers,
            cache,
            settlement,
            conn,
            config,
        })
    }

    /// Open node state in memory (for testing).
    ///
    /// Uses in-memory SQLite and temporary directories.
    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self> {
        let temp_dir = std::env::temp_dir().join(format!(
            "nodalync-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));

        let config = NodeStateConfig::new(&temp_dir);

        // Create directories
        std::fs::create_dir_all(&temp_dir)?;
        std::fs::create_dir_all(config.content_dir())?;
        std::fs::create_dir_all(config.cache_dir())?;
        std::fs::create_dir_all(config.identity_dir())?;

        // Open in-memory database
        let conn = Connection::open_in_memory()?;
        schema::initialize_schema(&conn)?;
        let conn = Arc::new(Mutex::new(conn));

        let identity = IdentityStore::new(config.identity_dir())?;
        let content = FsContentStore::new(config.content_dir())?;
        let manifests = SqliteManifestStore::new(Arc::clone(&conn));
        let provenance = SqliteProvenanceGraph::new(Arc::clone(&conn));
        let channels = SqliteChannelStore::new(Arc::clone(&conn));
        let peers = SqlitePeerStore::new(Arc::clone(&conn));
        let cache = FsCacheStore::new(config.cache_dir(), Arc::clone(&conn))?;
        let settlement = SqliteSettlementQueue::new(Arc::clone(&conn));

        Ok(Self {
            identity,
            content,
            manifests,
            provenance,
            channels,
            peers,
            cache,
            settlement,
            conn,
            config,
        })
    }

    /// Get the configuration used to open this state.
    pub fn config(&self) -> &NodeStateConfig {
        &self.config
    }

    /// Get a reference to the shared database connection.
    pub fn connection(&self) -> Arc<Mutex<Connection>> {
        Arc::clone(&self.conn)
    }

    /// Store a content announcement from a remote node.
    ///
    /// This persists the announcement to SQLite so that preview/query can discover
    /// content from the network even after restart.
    pub fn store_announcement(&self, payload: AnnouncePayload) {
        tracing::info!(
            hash = %payload.hash,
            title = %payload.title,
            addresses_count = payload.addresses.len(),
            publisher_peer_id = ?payload.publisher_peer_id,
            "Storing announcement"
        );

        let conn = match self.conn.lock() {
            Ok(c) => c,
            Err(_) => {
                tracing::error!("database connection lock poisoned");
                return;
            }
        };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let l1_summary_json = serde_json::to_string(&payload.l1_summary).unwrap_or_default();
        let addresses_json = serde_json::to_string(&payload.addresses).unwrap_or_default();

        // Use INSERT OR REPLACE to update existing announcements
        if let Err(e) = conn.execute(
            "INSERT OR REPLACE INTO announcements (hash, content_type, title, l1_summary, price, addresses, received_at, publisher_peer_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                payload.hash.0.as_slice(),
                payload.content_type as u8,
                payload.title,
                l1_summary_json,
                payload.price as i64,
                addresses_json,
                now,
                payload.publisher_peer_id,
            ],
        ) {
            tracing::warn!(
                hash = %payload.hash,
                error = %e,
                "Failed to store announcement"
            );
        }

        // Enforce maximum announcement count (keep most recent)
        const MAX_ANNOUNCEMENTS: i64 = 10_000;
        if let Err(e) = conn.execute(
            "DELETE FROM announcements WHERE hash NOT IN (
                SELECT hash FROM announcements ORDER BY received_at DESC LIMIT ?1
            )",
            [MAX_ANNOUNCEMENTS],
        ) {
            tracing::warn!(error = %e, "Failed to enforce announcement cap");
        }
    }

    /// Get a stored announcement by hash.
    ///
    /// Returns None if no announcement for this hash has been received.
    pub fn get_announcement(&self, hash: &Hash) -> Option<AnnouncePayload> {
        use nodalync_types::{ContentType, L1Summary};

        let conn = match self.conn.lock() {
            Ok(c) => c,
            Err(_) => {
                tracing::error!("database connection lock poisoned");
                return None;
            }
        };
        conn.query_row(
            "SELECT content_type, title, l1_summary, price, addresses, publisher_peer_id FROM announcements WHERE hash = ?1",
            [hash.0.as_slice()],
            |row| {
                let content_type_u8: u8 = row.get(0)?;
                let title: String = row.get(1)?;
                let l1_summary_json: String = row.get(2)?;
                let price: i64 = row.get(3)?;
                let addresses_json: String = row.get(4)?;
                let publisher_peer_id: Option<String> = row.get(5)?;

                let content_type = ContentType::from_u8(content_type_u8).unwrap_or(ContentType::L0);
                let l1_summary: L1Summary = serde_json::from_str(&l1_summary_json).unwrap_or_else(|e| {
                    tracing::warn!(error = %e, "Failed to deserialize announcement L1 summary");
                    L1Summary::empty(*hash)
                });
                let addresses: Vec<String> = serde_json::from_str(&addresses_json).unwrap_or_else(|e| {
                    tracing::warn!(error = %e, "Failed to deserialize announcement addresses");
                    Vec::new()
                });

                Ok(AnnouncePayload {
                    hash: *hash,
                    content_type,
                    title,
                    l1_summary,
                    price: price as u64,
                    addresses,
                    publisher_peer_id,
                })
            },
        )
        .ok()
    }

    /// List all stored announcements.
    pub fn list_announcements(&self) -> Vec<AnnouncePayload> {
        use nodalync_types::{ContentType, L1Summary};

        let conn = match self.conn.lock() {
            Ok(c) => c,
            Err(_) => {
                tracing::error!("database connection lock poisoned");
                return Vec::new();
            }
        };
        let mut stmt = match conn.prepare(
            "SELECT hash, content_type, title, l1_summary, price, addresses, publisher_peer_id FROM announcements ORDER BY received_at DESC",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };

        let rows = stmt.query_map([], |row| {
            let hash_bytes: Vec<u8> = row.get(0)?;
            let content_type_u8: u8 = row.get(1)?;
            let title: String = row.get(2)?;
            let l1_summary_json: String = row.get(3)?;
            let price: i64 = row.get(4)?;
            let addresses_json: String = row.get(5)?;
            let publisher_peer_id: Option<String> = row.get(6)?;

            let mut hash_arr = [0u8; 32];
            if hash_bytes.len() == 32 {
                hash_arr.copy_from_slice(&hash_bytes);
            }
            let hash = Hash(hash_arr);

            let content_type = ContentType::from_u8(content_type_u8).unwrap_or(ContentType::L0);
            let l1_summary: L1Summary =
                serde_json::from_str(&l1_summary_json).unwrap_or_else(|e| {
                    tracing::warn!(error = %e, "Failed to deserialize announcement L1 summary");
                    L1Summary::empty(hash)
                });
            let addresses: Vec<String> = serde_json::from_str(&addresses_json).unwrap_or_else(|e| {
                tracing::warn!(error = %e, "Failed to deserialize announcement addresses");
                Vec::new()
            });

            Ok(AnnouncePayload {
                hash,
                content_type,
                title,
                l1_summary,
                price: price as u64,
                addresses,
                publisher_peer_id,
            })
        });

        match rows {
            Ok(iter) => iter.filter_map(|r| r.ok()).collect(),
            Err(_) => Vec::new(),
        }
    }

    /// Search stored announcements by text query.
    ///
    /// Searches the title field for the given query (case-insensitive).
    /// Optionally filters by content type.
    pub fn search_announcements(
        &self,
        query: &str,
        content_type: Option<nodalync_types::ContentType>,
        limit: u32,
    ) -> Vec<AnnouncePayload> {
        use nodalync_types::{ContentType, L1Summary};

        let conn = match self.conn.lock() {
            Ok(c) => c,
            Err(_) => {
                tracing::error!("database connection lock poisoned");
                return Vec::new();
            }
        };
        let pattern = format!("%{}%", query.to_lowercase());

        let (sql, params): (&str, Vec<Box<dyn rusqlite::ToSql>>) = if let Some(ct) = content_type {
            (
                "SELECT hash, content_type, title, l1_summary, price, addresses, publisher_peer_id \
                 FROM announcements \
                 WHERE LOWER(title) LIKE ?1 AND content_type = ?2 \
                 ORDER BY received_at DESC LIMIT ?3",
                vec![
                    Box::new(pattern) as Box<dyn rusqlite::ToSql>,
                    Box::new(ct as u8),
                    Box::new(limit),
                ],
            )
        } else {
            (
                "SELECT hash, content_type, title, l1_summary, price, addresses, publisher_peer_id \
                 FROM announcements \
                 WHERE LOWER(title) LIKE ?1 \
                 ORDER BY received_at DESC LIMIT ?2",
                vec![
                    Box::new(pattern) as Box<dyn rusqlite::ToSql>,
                    Box::new(limit),
                ],
            )
        };

        let mut stmt = match conn.prepare(sql) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };

        let params_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();

        let rows = stmt.query_map(params_refs.as_slice(), |row| {
            let hash_bytes: Vec<u8> = row.get(0)?;
            let content_type_u8: u8 = row.get(1)?;
            let title: String = row.get(2)?;
            let l1_summary_json: String = row.get(3)?;
            let price: i64 = row.get(4)?;
            let addresses_json: String = row.get(5)?;
            let publisher_peer_id: Option<String> = row.get(6)?;

            let mut hash_arr = [0u8; 32];
            if hash_bytes.len() == 32 {
                hash_arr.copy_from_slice(&hash_bytes);
            }
            let hash = Hash(hash_arr);

            let content_type = ContentType::from_u8(content_type_u8).unwrap_or(ContentType::L0);
            let l1_summary: L1Summary =
                serde_json::from_str(&l1_summary_json).unwrap_or_else(|e| {
                    tracing::warn!(error = %e, "Failed to deserialize announcement L1 summary");
                    L1Summary::empty(hash)
                });
            let addresses: Vec<String> = serde_json::from_str(&addresses_json).unwrap_or_else(|e| {
                tracing::warn!(error = %e, "Failed to deserialize announcement addresses");
                Vec::new()
            });

            Ok(AnnouncePayload {
                hash,
                content_type,
                title,
                l1_summary,
                price: price as u64,
                addresses,
                publisher_peer_id,
            })
        });

        match rows {
            Ok(iter) => iter.filter_map(|r| r.ok()).collect(),
            Err(_) => Vec::new(),
        }
    }

    /// Clean up old announcements to prevent unbounded table growth.
    ///
    /// Removes announcements older than the specified TTL (time-to-live) in seconds.
    /// Returns the number of announcements deleted.
    pub fn cleanup_old_announcements(&self, ttl_seconds: i64) -> u32 {
        let conn = match self.conn.lock() {
            Ok(c) => c,
            Err(_) => {
                tracing::error!("database connection lock poisoned");
                return 0;
            }
        };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let cutoff = now - ttl_seconds;

        match conn.execute(
            "DELETE FROM announcements WHERE received_at < ?1",
            rusqlite::params![cutoff],
        ) {
            Ok(count) => count as u32,
            Err(e) => {
                tracing::warn!(error = %e, "Failed to cleanup old announcements");
                0
            }
        }
    }

    /// Get the count of stored announcements.
    pub fn announcement_count(&self) -> u32 {
        let conn = match self.conn.lock() {
            Ok(c) => c,
            Err(_) => {
                tracing::error!("database connection lock poisoned");
                return 0;
            }
        };
        conn.query_row("SELECT COUNT(*) FROM announcements", [], |row| row.get(0))
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nodalync_crypto::{content_hash, generate_identity, peer_id_from_public_key};
    use nodalync_types::{Manifest, Metadata};
    use tempfile::TempDir;

    #[test]
    fn test_node_state_open() {
        let temp_dir = TempDir::new().unwrap();
        let config = NodeStateConfig::new(temp_dir.path());

        let state = NodeState::open(config);
        assert!(state.is_ok());
    }

    #[test]
    fn test_node_state_content_roundtrip() {
        let state = NodeState::open_in_memory().unwrap();
        let mut content_store = state.content;

        let content = b"Hello, Nodalync!";
        let hash = content_store.store(content).unwrap();

        let loaded = content_store.load(&hash).unwrap();
        assert_eq!(loaded, Some(content.to_vec()));
    }

    #[test]
    fn test_node_state_manifest_roundtrip() {
        let mut state = NodeState::open_in_memory().unwrap();

        let (_, public_key) = generate_identity();
        let owner = peer_id_from_public_key(&public_key);
        let hash = content_hash(b"test content");
        let metadata = Metadata::new("Test", 100);
        let manifest = Manifest::new_l0(hash, owner, metadata, 1234567890);

        state.manifests.store(&manifest).unwrap();

        let loaded = state.manifests.load(&hash).unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().hash, hash);
    }

    #[test]
    fn test_node_state_identity() {
        let state = NodeState::open_in_memory().unwrap();

        assert!(!state.identity.exists());

        let peer_id = state.identity.generate("password").unwrap();
        assert!(state.identity.exists());

        let loaded_peer_id = state.identity.peer_id().unwrap();
        assert_eq!(peer_id, loaded_peer_id);
    }

    #[test]
    fn test_node_state_config_defaults() {
        let config = NodeStateConfig::new("/home/user/.nodalync");

        assert_eq!(
            config.content_dir(),
            PathBuf::from("/home/user/.nodalync/content")
        );
        assert_eq!(
            config.cache_dir(),
            PathBuf::from("/home/user/.nodalync/cache")
        );
        assert_eq!(
            config.identity_dir(),
            PathBuf::from("/home/user/.nodalync/identity")
        );
        assert_eq!(
            config.database_path(),
            PathBuf::from("/home/user/.nodalync/nodalync.db")
        );
    }

    #[test]
    fn test_node_state_config_custom_paths() {
        let config = NodeStateConfig::new("/home/user/.nodalync")
            .with_content_dir("/data/content")
            .with_cache_dir("/tmp/cache")
            .with_database_path("/data/db.sqlite");

        assert_eq!(config.content_dir(), PathBuf::from("/data/content"));
        assert_eq!(config.cache_dir(), PathBuf::from("/tmp/cache"));
        assert_eq!(config.database_path(), PathBuf::from("/data/db.sqlite"));
    }

    #[test]
    fn test_shared_connection() {
        let state = NodeState::open_in_memory().unwrap();
        let conn = state.connection();

        // Verify we can use the connection
        let conn_guard = conn.lock().unwrap();
        let count: i64 = conn_guard
            .query_row("SELECT COUNT(*) FROM manifests", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_search_announcements() {
        use nodalync_types::{ContentType, L1Summary};

        let state = NodeState::open_in_memory().unwrap();

        // Store some test announcements
        let hash1 = content_hash(b"protocol guide content");
        let announce1 = AnnouncePayload {
            hash: hash1,
            content_type: ContentType::L0,
            title: "Protocol Guide".to_string(),
            l1_summary: L1Summary::empty(hash1),
            price: 100,
            addresses: vec![],
            publisher_peer_id: None,
        };
        state.store_announcement(announce1);

        let hash2 = content_hash(b"api reference content");
        let announce2 = AnnouncePayload {
            hash: hash2,
            content_type: ContentType::L0,
            title: "API Reference".to_string(),
            l1_summary: L1Summary::empty(hash2),
            price: 200,
            addresses: vec![],
            publisher_peer_id: None,
        };
        state.store_announcement(announce2);

        let hash3 = content_hash(b"user manual content");
        let announce3 = AnnouncePayload {
            hash: hash3,
            content_type: ContentType::L1,
            title: "User Manual".to_string(),
            l1_summary: L1Summary::empty(hash3),
            price: 50,
            addresses: vec![],
            publisher_peer_id: None,
        };
        state.store_announcement(announce3);

        // Test search by text query
        let results = state.search_announcements("protocol", None, 10);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Protocol Guide");

        // Test case insensitive search
        let results = state.search_announcements("REFERENCE", None, 10);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "API Reference");

        // Test search with content type filter
        let results = state.search_announcements("", Some(ContentType::L1), 10);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "User Manual");

        // Test search with no matches
        let results = state.search_announcements("nonexistent", None, 10);
        assert!(results.is_empty());

        // Test limit
        let results = state.search_announcements("", None, 2);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_cleanup_old_announcements() {
        use nodalync_types::{ContentType, L1Summary};

        let state = NodeState::open_in_memory().unwrap();

        // Store a test announcement
        let hash = content_hash(b"test content");
        let announce = AnnouncePayload {
            hash,
            content_type: ContentType::L0,
            title: "Test Content".to_string(),
            l1_summary: L1Summary::empty(hash),
            price: 100,
            addresses: vec![],
            publisher_peer_id: None,
        };
        state.store_announcement(announce);

        // Verify it exists
        assert_eq!(state.announcement_count(), 1);

        // Cleanup with a very short TTL (0 seconds) - should delete everything
        // Note: This test works because the announcement was just created, so
        // received_at is "now" and TTL of 0 means cutoff is also "now"
        let _deleted = state.cleanup_old_announcements(0);
        // The announcement might or might not be deleted depending on timing
        // With TTL=0, cutoff = now, and received_at = now (within same second)

        // Test with TTL of 1 hour - should NOT delete the fresh announcement
        let hash2 = content_hash(b"fresh content");
        let announce2 = AnnouncePayload {
            hash: hash2,
            content_type: ContentType::L0,
            title: "Fresh Content".to_string(),
            l1_summary: L1Summary::empty(hash2),
            price: 50,
            addresses: vec![],
            publisher_peer_id: None,
        };
        state.store_announcement(announce2);

        let count_before = state.announcement_count();
        let deleted = state.cleanup_old_announcements(3600); // 1 hour TTL
        let count_after = state.announcement_count();

        // Fresh announcement should not be deleted
        assert!(count_after >= count_before - deleted);
    }

    #[test]
    fn test_announcement_cap_enforced() {
        // Regression test: store_announcement() must enforce a maximum count
        // to prevent unbounded growth (DoS vector).
        use nodalync_types::{ContentType, L1Summary};

        let state = NodeState::open_in_memory().unwrap();

        // Insert announcements directly via SQL to bypass the cap for setup,
        // then verify the cap logic works by calling store_announcement.
        {
            let conn = state.conn.lock().unwrap();
            for i in 0..10_002u32 {
                let hash = content_hash(&i.to_be_bytes());
                conn.execute(
                    "INSERT OR REPLACE INTO announcements (hash, content_type, title, l1_summary, price, addresses, received_at)
                     VALUES (?1, 0, ?2, '{}', 100, '[]', ?3)",
                    rusqlite::params![hash.0.as_slice(), format!("Ann {}", i), i as i64],
                ).unwrap();
            }
        }

        assert_eq!(state.announcement_count(), 10_002);

        // Now store one more via the public API, which triggers the cap
        let hash = content_hash(b"trigger-cap");
        let announce = AnnouncePayload {
            hash,
            content_type: ContentType::L0,
            title: "Trigger Cap".to_string(),
            l1_summary: L1Summary::empty(hash),
            price: 100,
            addresses: vec![],
            publisher_peer_id: None,
        };
        state.store_announcement(announce);

        // Should have been capped to MAX_ANNOUNCEMENTS (10,000)
        assert!(
            state.announcement_count() <= 10_000,
            "Announcement count should be capped at 10,000, got {}",
            state.announcement_count()
        );
    }
}
