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

use rusqlite::Connection;

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
        let mut state = NodeState::open_in_memory().unwrap();

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
}
