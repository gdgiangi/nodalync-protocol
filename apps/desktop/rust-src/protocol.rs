//! Protocol state management for Nodalync Studio.
//!
//! Initializes and manages the protocol stack: identity, storage,
//! operations, and (optionally) networking.

use std::path::PathBuf;
use std::sync::Arc;

use nodalync_crypto::{peer_id_from_public_key, PeerId, PublicKey};
use nodalync_net::{NetworkConfig, NetworkNode};
use nodalync_ops::DefaultNodeOperations;
use nodalync_store::NodeState;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

/// User profile stored alongside the identity.
///
/// Persisted to `{data_dir}/identity/profile.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeProfile {
    /// Display name chosen during onboarding.
    pub name: String,
    /// ISO-8601 creation timestamp.
    pub created_at: String,
}

impl NodeProfile {
    /// Create a new profile with the current timestamp.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            created_at: chrono::Utc::now().to_rfc3339(),
        }
    }

    /// Load profile from disk. Returns None if file doesn't exist.
    pub fn load(data_dir: &PathBuf) -> Option<Self> {
        let path = data_dir.join("identity").join("profile.json");
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
    }

    /// Save profile to disk.
    pub fn save(&self, data_dir: &PathBuf) -> Result<(), ProtocolError> {
        let dir = data_dir.join("identity");
        std::fs::create_dir_all(&dir)
            .map_err(|e| ProtocolError::Store(format!("Failed to create identity dir: {}", e)))?;
        let path = dir.join("profile.json");
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| ProtocolError::Store(format!("Failed to serialize profile: {}", e)))?;
        std::fs::write(&path, json)
            .map_err(|e| ProtocolError::Store(format!("Failed to write profile: {}", e)))?;
        Ok(())
    }
}

/// Protocol state shared across Tauri commands.
///
/// Holds the node identity, operations engine, and optional network.
pub struct ProtocolState {
    /// Root directory for node data (~/.nodalync or app-specific).
    pub data_dir: PathBuf,
    /// Node peer ID (derived from public key).
    pub peer_id: PeerId,
    /// Public key for this node.
    pub public_key: PublicKey,
    /// Operations engine (content, publish, query, channels).
    pub ops: DefaultNodeOperations,
    /// Network node (None if offline / not yet started).
    pub network: Option<Arc<NetworkNode>>,
    /// User profile (name, creation date).
    pub profile: Option<NodeProfile>,
}

impl ProtocolState {
    /// Initialize protocol state from an existing identity.
    ///
    /// Loads the encrypted keypair from `data_dir/identity/`, opens the
    /// node state database, and creates the operations engine.
    ///
    /// Does NOT start networking — call `start_network()` separately.
    pub fn open(data_dir: &PathBuf, password: &str) -> Result<Self, ProtocolError> {
        info!("Opening protocol state from {}", data_dir.display());

        // Open node state (content store + SQLite manifests/channels/peers)
        let store_config = nodalync_store::NodeStateConfig::new(data_dir);
        let state = NodeState::open(store_config)
            .map_err(|e| ProtocolError::Store(format!("Failed to open node state: {}", e)))?;

        // Load identity
        let identity_store =
            nodalync_store::IdentityStore::new(data_dir.join("identity"))
                .map_err(|e| ProtocolError::Identity(format!("Failed to open identity store: {}", e)))?;

        if !identity_store.exists() {
            return Err(ProtocolError::NoIdentity);
        }

        let (private_key, public_key) = identity_store
            .load(password)
            .map_err(|e| ProtocolError::Identity(format!("Failed to load identity: {}", e)))?;

        let peer_id = peer_id_from_public_key(&public_key);
        info!("Loaded identity: peer_id={}", peer_id);

        // Create operations engine (local-only, no network yet)
        let mut ops = DefaultNodeOperations::with_defaults(state, peer_id);
        ops.set_private_key(private_key);

        // Load profile if it exists
        let profile = NodeProfile::load(data_dir);

        Ok(Self {
            data_dir: data_dir.clone(),
            peer_id,
            public_key,
            ops,
            network: None,
            profile,
        })
    }

    /// Initialize a brand-new node identity.
    ///
    /// Generates an Ed25519 keypair, encrypts it with the password,
    /// and stores it. Then opens the full protocol state.
    pub fn init(data_dir: &PathBuf, password: &str) -> Result<Self, ProtocolError> {
        Self::init_with_name(data_dir, password, None)
    }

    /// Initialize a brand-new node identity with an optional display name.
    pub fn init_with_name(
        data_dir: &PathBuf,
        password: &str,
        name: Option<String>,
    ) -> Result<Self, ProtocolError> {
        std::fs::create_dir_all(data_dir)
            .map_err(|e| ProtocolError::Store(format!("Failed to create data dir: {}", e)))?;

        info!("Initializing new node identity at {}", data_dir.display());

        let identity_store =
            nodalync_store::IdentityStore::new(data_dir.join("identity"))
                .map_err(|e| ProtocolError::Identity(format!("Failed to create identity store: {}", e)))?;

        if identity_store.exists() {
            return Err(ProtocolError::IdentityExists);
        }

        let _peer_id = identity_store
            .generate(password)
            .map_err(|e| ProtocolError::Identity(format!("Failed to generate identity: {}", e)))?;

        // Save profile with name
        let display_name = name.unwrap_or_else(|| "Anonymous".to_string());
        let profile = NodeProfile::new(&display_name);
        profile.save(data_dir)?;
        info!("Created profile for '{}'", display_name);

        // Now open normally (this will load the profile we just saved)
        Self::open(data_dir, password)
    }

    /// Check if an identity exists at the given data directory.
    pub fn identity_exists(data_dir: &PathBuf) -> bool {
        data_dir.join("identity").join("keypair.key").exists()
    }

    /// Start the P2P network layer.
    ///
    /// Configures libp2p with TCP+Noise+Yamux, Kademlia DHT, and GossipSub.
    /// Uses the node's identity for a stable PeerId across restarts.
    pub async fn start_network(&mut self) -> Result<(), ProtocolError> {
        if self.network.is_some() {
            warn!("Network already running");
            return Ok(());
        }

        info!("Starting P2P network...");
        let mut config = NetworkConfig::default();

        // Use the node's identity for stable PeerId
        if let Some(key) = self.ops.private_key() {
            config = config.with_identity_secret(*key.as_bytes());
        }

        let node = NetworkNode::new(config)
            .await
            .map_err(|e| ProtocolError::Network(format!("Failed to create network node: {}", e)))?;

        let node = Arc::new(node);
        self.ops.set_network(node.clone());
        self.network = Some(node);

        info!("P2P network started");
        Ok(())
    }

    /// Stop the P2P network layer.
    pub fn stop_network(&mut self) {
        if self.network.is_some() {
            self.ops.clear_network();
            self.network = None;
            info!("P2P network stopped");
        }
    }

    /// Get default data directory for this platform.
    pub fn default_data_dir() -> PathBuf {
        directories::ProjectDirs::from("com", "nodalync", "studio")
            .map(|dirs| dirs.data_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from(".nodalync-studio"))
    }
}

/// Protocol-layer errors.
#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
    #[error("No identity found — initialize first")]
    NoIdentity,

    #[error("Identity already exists")]
    IdentityExists,

    #[error("Identity error: {0}")]
    Identity(String),

    #[error("Store error: {0}")]
    Store(String),

    #[error("Network error: {0}")]
    Network(String),

    #[error("Publish error: {0}")]
    Publish(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),
}
