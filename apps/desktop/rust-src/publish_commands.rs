//! Tauri IPC commands for content publishing and node management.
//!
//! These commands expose the Nodalync protocol's publish flow to the
//! React frontend via Tauri's invoke system.

use std::path::PathBuf;
use std::sync::Arc;

use nodalync_crypto::{content_hash, Hash};
use nodalync_net::{Network, NetworkConfig, NetworkNode};
use nodalync_store::ManifestStore;
use nodalync_types::{Metadata, Visibility};
use serde::{Deserialize, Serialize};
use tauri::State;
use tokio::sync::Mutex;
use tracing::{info, warn};

use crate::protocol::ProtocolState;

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Parse a hex string into a Hash.
pub fn parse_hash(hex: &str) -> Result<Hash, String> {
    let hex = hex.trim();
    if hex.len() != 64 {
        return Err(format!("Invalid hash length: expected 64 hex chars, got {}", hex.len()));
    }
    let bytes: Vec<u8> = (0..64)
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16))
        .collect::<Result<Vec<u8>, _>>()
        .map_err(|e| format!("Invalid hex: {}", e))?;
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Ok(Hash(arr))
}

// ─── Response Types ──────────────────────────────────────────────────────────

/// Published content info returned to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishResult {
    pub hash: String,
    pub title: String,
    pub size: u64,
    pub price: u64,
    pub visibility: String,
    pub mentions: Option<usize>,
}

/// Content manifest summary for listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentItem {
    pub hash: String,
    pub title: String,
    pub size: u64,
    pub content_type: String,
    pub visibility: String,
    pub price: u64,
    pub version: u32,
    pub mention_count: Option<usize>,
}

/// Node status information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeStatus {
    pub initialized: bool,
    pub peer_id: Option<String>,
    pub network_active: bool,
    pub connected_peers: usize,
    pub content_count: usize,
    pub data_dir: String,
}

/// Identity info returned after init/unlock.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityInfo {
    pub name: Option<String>,
    pub peer_id: String,
    pub public_key: String,
    pub data_dir: String,
    pub created_at: Option<String>,
}

// ─── Identity Commands ───────────────────────────────────────────────────────

/// Check if a node identity exists (no password needed).
#[tauri::command]
pub async fn check_identity() -> Result<bool, String> {
    let data_dir = ProtocolState::default_data_dir();
    Ok(ProtocolState::identity_exists(&data_dir))
}

/// Initialize a new node identity.
///
/// Creates an Ed25519 keypair encrypted with the given password.
/// Optionally stores a display name for the user profile.
/// Returns the peer ID, public key, and profile info on success.
#[tauri::command]
pub async fn init_node(
    password: String,
    name: Option<String>,
    protocol: State<'_, Arc<Mutex<Option<ProtocolState>>>>,
) -> Result<IdentityInfo, String> {
    let data_dir = ProtocolState::default_data_dir();
    info!("Initializing new node at {}", data_dir.display());

    let state = ProtocolState::init_with_name(&data_dir, &password, name)
        .map_err(|e| format!("Failed to initialize node: {}", e))?;

    let info = IdentityInfo {
        name: state.profile.as_ref().map(|p| p.name.clone()),
        peer_id: state.peer_id.to_string(),
        public_key: hex::encode(state.public_key.0),
        data_dir: state.data_dir.display().to_string(),
        created_at: state.profile.as_ref().map(|p| p.created_at.clone()),
    };

    let mut guard = protocol.lock().await;
    *guard = Some(state);

    Ok(info)
}

/// Unlock an existing node identity.
///
/// Decrypts the keypair and initializes the protocol stack.
#[tauri::command]
pub async fn unlock_node(
    password: String,
    protocol: State<'_, Arc<Mutex<Option<ProtocolState>>>>,
) -> Result<IdentityInfo, String> {
    let data_dir = ProtocolState::default_data_dir();
    info!("Unlocking node at {}", data_dir.display());

    let state = ProtocolState::open(&data_dir, &password)
        .map_err(|e| format!("Failed to unlock node: {}", e))?;

    let info = IdentityInfo {
        name: state.profile.as_ref().map(|p| p.name.clone()),
        peer_id: state.peer_id.to_string(),
        public_key: hex::encode(state.public_key.0),
        data_dir: state.data_dir.display().to_string(),
        created_at: state.profile.as_ref().map(|p| p.created_at.clone()),
    };

    let mut guard = protocol.lock().await;
    *guard = Some(state);

    Ok(info)
}

/// Get current identity info (no password required - node must be unlocked).
///
/// Returns the node's name, public key, peer ID, and creation date.
/// Hephaestus uses this for the dashboard and profile display.
#[tauri::command]
pub async fn get_identity(
    protocol: State<'_, Arc<Mutex<Option<ProtocolState>>>>,
) -> Result<IdentityInfo, String> {
    let guard = protocol.lock().await;
    let state = guard.as_ref().ok_or("Node not initialized - unlock first")?;

    Ok(IdentityInfo {
        name: state.profile.as_ref().map(|p| p.name.clone()),
        peer_id: state.peer_id.to_string(),
        public_key: hex::encode(state.public_key.0),
        data_dir: state.data_dir.display().to_string(),
        created_at: state.profile.as_ref().map(|p| p.created_at.clone()),
    })
}

// ─── Publish Commands ────────────────────────────────────────────────────────

/// Publish a file to the Nodalync network.
///
/// Reads the file, creates content + manifest, extracts L1 mentions,
/// and (if network is active) announces to the DHT and broadcasts.
#[tauri::command]
pub async fn publish_file(
    file_path: String,
    title: Option<String>,
    description: Option<String>,
    price: Option<f64>,
    visibility: Option<String>,
    protocol: State<'_, Arc<Mutex<Option<ProtocolState>>>>,
) -> Result<PublishResult, String> {
    let mut guard = protocol.lock().await;
    let state = guard.as_mut().ok_or("Node not initialized - unlock first")?;

    let path = PathBuf::from(&file_path);
    if !path.exists() {
        return Err(format!("File not found: {}", file_path));
    }
    if path.is_dir() {
        return Err("Cannot publish a directory. Please specify a file.".into());
    }

    // Read file
    let content = std::fs::read(&path)
        .map_err(|e| format!("Failed to read file: {}", e))?;
    if content.is_empty() {
        return Err("Cannot publish an empty file.".into());
    }

    // Resolve title from filename if not provided
    let title = title.unwrap_or_else(|| {
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Untitled")
            .to_string()
    });

    // Convert price (NDL → tinybars)
    let price_units = price
        .map(|p| (p * 100_000_000.0) as u64)
        .unwrap_or(0);

    // Validate price
    if price_units > 0 {
        nodalync_econ::validate_price(price_units)
            .map_err(|e| format!("Invalid price: {}", e))?;
    }

    // Parse visibility (protocol has Private, Unlisted, Shared, Offline)
    let vis = match visibility.as_deref() {
        Some("private") => Visibility::Private,
        Some("unlisted") => Visibility::Unlisted,
        Some("shared") | Some("public") => Visibility::Shared,
        _ => Visibility::Shared, // default for publishing
    };

    // Create metadata
    let mut metadata = Metadata::new(&title, content.len() as u64);
    if let Some(desc) = description {
        metadata = metadata.with_description(&desc);
    }

    // Detect MIME type from extension
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        let mime = match ext.to_lowercase().as_str() {
            "txt" => "text/plain",
            "md" => "text/markdown",
            "html" | "htm" => "text/html",
            "json" => "application/json",
            "pdf" => "application/pdf",
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            _ => "application/octet-stream",
        };
        metadata = metadata.with_mime_type(mime);
    }

    // Check for duplicate
    let computed_hash = content_hash(&content);
    if let Ok(Some(_existing)) = state.ops.get_content_manifest(&computed_hash) {
        return Err(format!(
            "Content with this hash already exists ({}). Delete it first or use update.",
            computed_hash
        ));
    }

    // Create content (stores to filesystem + manifest DB)
    let hash = state.ops.create_content(&content, metadata)
        .map_err(|e| format!("Failed to create content: {}", e))?;

    // Extract L1 mentions
    let mentions = state.ops.extract_l1_summary(&hash).ok().map(|s| s.mention_count as usize);

    // Publish (visibility + price + network announce if connected)
    state.ops.publish_content(&hash, vis, price_units)
        .await
        .map_err(|e| format!("Failed to publish: {}", e))?;

    info!("Published: {} ({})", title, hash);

    Ok(PublishResult {
        hash: hash.to_string(),
        title,
        size: content.len() as u64,
        price: price_units,
        visibility: format!("{:?}", vis),
        mentions,
    })
}

/// Publish text content directly (not from a file).
///
/// Useful for the desktop app's "quick publish" UI.
#[tauri::command]
pub async fn publish_text(
    text: String,
    title: String,
    description: Option<String>,
    price: Option<f64>,
    visibility: Option<String>,
    protocol: State<'_, Arc<Mutex<Option<ProtocolState>>>>,
) -> Result<PublishResult, String> {
    let mut guard = protocol.lock().await;
    let state = guard.as_mut().ok_or("Node not initialized - unlock first")?;

    if text.is_empty() {
        return Err("Cannot publish empty text.".into());
    }

    let content = text.as_bytes();

    let price_units = price
        .map(|p| (p * 100_000_000.0) as u64)
        .unwrap_or(0);

    if price_units > 0 {
        nodalync_econ::validate_price(price_units)
            .map_err(|e| format!("Invalid price: {}", e))?;
    }

    let vis = match visibility.as_deref() {
        Some("private") => Visibility::Private,
        Some("unlisted") => Visibility::Unlisted,
        Some("shared") | Some("public") => Visibility::Shared,
        _ => Visibility::Shared,
    };

    let mut metadata = Metadata::new(&title, content.len() as u64);
    metadata = metadata.with_mime_type("text/plain");
    if let Some(desc) = description {
        metadata = metadata.with_description(&desc);
    }

    // Check duplicate
    let computed_hash = content_hash(content);
    if let Ok(Some(_)) = state.ops.get_content_manifest(&computed_hash) {
        return Err(format!("Content already exists ({})", computed_hash));
    }

    let hash = state.ops.create_content(content, metadata)
        .map_err(|e| format!("Failed to create content: {}", e))?;

    let mentions = state.ops.extract_l1_summary(&hash).ok().map(|s| s.mention_count as usize);

    state.ops.publish_content(&hash, vis, price_units)
        .await
        .map_err(|e| format!("Failed to publish: {}", e))?;

    info!("Published text: {} ({})", title, hash);

    Ok(PublishResult {
        hash: hash.to_string(),
        title,
        size: content.len() as u64,
        price: price_units,
        visibility: format!("{:?}", vis),
        mentions,
    })
}

// ─── Content Listing ─────────────────────────────────────────────────────────

/// List all published content on this node.
#[tauri::command]
pub async fn list_content(
    protocol: State<'_, Arc<Mutex<Option<ProtocolState>>>>,
) -> Result<Vec<ContentItem>, String> {
    let guard = protocol.lock().await;
    let state = guard.as_ref().ok_or("Node not initialized - unlock first")?;

    let filter = nodalync_store::ManifestFilter::new();
    let manifests = state.ops.state().manifests.list(filter)
        .map_err(|e| format!("Failed to list content: {}", e))?;

    let items: Vec<ContentItem> = manifests
        .into_iter()
        .map(|m| ContentItem {
            hash: m.hash.to_string(),
            title: m.metadata.title.clone(),
            size: m.metadata.content_size,
            content_type: format!("{:?}", m.content_type),
            visibility: format!("{:?}", m.visibility),
            price: m.economics.price,
            version: m.version.number,
            mention_count: None,
        })
        .collect();

    Ok(items)
}

/// Get details for a specific content item.
#[tauri::command]
pub async fn get_content_details(
    hash: String,
    protocol: State<'_, Arc<Mutex<Option<ProtocolState>>>>,
) -> Result<ContentItem, String> {
    let guard = protocol.lock().await;
    let state = guard.as_ref().ok_or("Node not initialized - unlock first")?;

    let hash_parsed = parse_hash(&hash)?;

    let manifest = state.ops.get_content_manifest(&hash_parsed)
        .map_err(|e| format!("Failed to get manifest: {}", e))?
        .ok_or_else(|| format!("Content not found: {}", hash))?;

    Ok(ContentItem {
        hash: manifest.hash.to_string(),
        title: manifest.metadata.title.clone(),
        size: manifest.metadata.content_size,
        content_type: format!("{:?}", manifest.content_type),
        visibility: format!("{:?}", manifest.visibility),
        price: manifest.economics.price,
        version: manifest.version.number,
        mention_count: None,
    })
}

/// Delete published content from this node.
#[tauri::command]
pub async fn delete_content(
    hash: String,
    protocol: State<'_, Arc<Mutex<Option<ProtocolState>>>>,
) -> Result<(), String> {
    let mut guard = protocol.lock().await;
    let state = guard.as_mut().ok_or("Node not initialized - unlock first")?;

    let hash_parsed = parse_hash(&hash)?;

    // Delete from content store
    use nodalync_store::ContentStore;
    state.ops.state_mut().content.delete(&hash_parsed)
        .map_err(|e| format!("Failed to delete content: {}", e))?;

    // Delete manifest
    state.ops.state_mut().manifests.delete(&hash_parsed)
        .map_err(|e| format!("Failed to delete manifest: {}", e))?;

    info!("Deleted content: {}", hash);
    Ok(())
}

// ─── Node Status ─────────────────────────────────────────────────────────────

/// Get current node status.
///
/// Works whether or not the node is initialized - returns partial info.
#[tauri::command]
pub async fn get_node_status(
    protocol: State<'_, Arc<Mutex<Option<ProtocolState>>>>,
) -> Result<NodeStatus, String> {
    let data_dir = ProtocolState::default_data_dir();
    let guard = protocol.lock().await;

    match guard.as_ref() {
        Some(state) => {
            let content_count = {
                let filter = nodalync_store::ManifestFilter::new();
                state.ops.state().manifests.list(filter)
                    .map(|v| v.len())
                    .unwrap_or(0)
            };

            let connected_peers = state.network
                .as_ref()
                .map(|n| n.connected_peers().len())
                .unwrap_or(0);

            Ok(NodeStatus {
                initialized: true,
                peer_id: Some(state.peer_id.to_string()),
                network_active: state.network.is_some(),
                connected_peers,
                content_count,
                data_dir: state.data_dir.display().to_string(),
            })
        }
        None => Ok(NodeStatus {
            initialized: ProtocolState::identity_exists(&data_dir),
            peer_id: None,
            network_active: false,
            connected_peers: 0,
            content_count: 0,
            data_dir: data_dir.display().to_string(),
        }),
    }
}

// ─── Network Commands ────────────────────────────────────────────────────────

/// Start the P2P network layer.
///
/// Must drop the MutexGuard before the async call to satisfy Send bounds.
/// Spawns a background event loop to process inbound peer requests.
#[tauri::command]
pub async fn start_network(
    protocol: State<'_, Arc<Mutex<Option<ProtocolState>>>>,
    event_loop: State<'_, Mutex<Option<crate::event_loop::EventLoopHandle>>>,
) -> Result<(), String> {
    // Check that protocol is initialized
    {
        let guard = protocol.lock().await;
        if guard.is_none() {
            return Err("Node not initialized - unlock first".into());
        }
    }

    // Create the network node outside the lock (this is the async part)
    let config = NetworkConfig::default();
    let node = NetworkNode::new(config)
        .await
        .map_err(|e| format!("Failed to create network node: {}", e))?;
    let node = Arc::new(node);

    // Store the network in protocol state
    {
        let mut guard = protocol.lock().await;
        let state = guard.as_mut().ok_or("Node not initialized - unlock first")?;
        state.ops.set_network(node.clone());
        state.network = Some(node.clone());
    }

    // Spawn the network event loop
    let protocol_arc = Arc::clone(&*protocol);
    let handle = crate::event_loop::spawn_event_loop(node, protocol_arc);

    // Store the event loop handle
    let mut el_guard = event_loop.lock().await;
    *el_guard = Some(handle);

    info!("P2P network started with event loop");
    Ok(())
}

/// Stop the P2P network layer.
///
/// Shuts down the event loop and clears the network from protocol state.
#[tauri::command]
pub async fn stop_network(
    protocol: State<'_, Arc<Mutex<Option<ProtocolState>>>>,
    event_loop: State<'_, Mutex<Option<crate::event_loop::EventLoopHandle>>>,
    health_monitor: State<'_, Mutex<Option<crate::health_monitor::HealthMonitorHandle>>>,
) -> Result<(), String> {
    // Stop the health monitor first
    let hm_handle = {
        let mut hm_guard = health_monitor.lock().await;
        hm_guard.take()
    };
    if let Some(handle) = hm_handle {
        handle.shutdown().await;
        info!("Health monitor stopped");
    }

    // Stop the event loop
    let el_handle = {
        let mut el_guard = event_loop.lock().await;
        el_guard.take()
    };
    if let Some(handle) = el_handle {
        handle.shutdown().await;
        info!("Network event loop stopped");
    }

    // Save known peers before stopping
    {
        let guard = protocol.lock().await;
        if let Some(state) = guard.as_ref() {
            if let Some(network) = &state.network {
                let mut store = crate::peer_store::PeerStore::load(&state.data_dir);
                for peer in network.connected_peers() {
                    let peer_str = peer.to_string();
                    let nodalync_id = network.nodalync_peer_id(&peer).map(|id| id.to_string());
                    store.record_peer(&peer_str, vec![], nodalync_id, false);
                }
                if let Err(e) = store.save(&state.data_dir) {
                    warn!("Failed to save peers on stop: {}", e);
                } else {
                    info!("Saved {} known peers on network stop", store.peers.len());
                }
            }
        }
    }

    // Now stop the network in protocol state
    let mut guard = protocol.lock().await;
    let state = guard.as_mut().ok_or("Node not initialized - unlock first")?;
    state.stop_network();
    Ok(())
}

/// Get list of connected peers.
#[tauri::command]
pub async fn get_peers(
    protocol: State<'_, Arc<Mutex<Option<ProtocolState>>>>,
) -> Result<Vec<String>, String> {
    let guard = protocol.lock().await;
    let state = guard.as_ref().ok_or("Node not initialized - unlock first")?;

    let peers = state.network
        .as_ref()
        .map(|n| n.connected_peers().iter().map(|p| p.to_string()).collect())
        .unwrap_or_default();

    Ok(peers)
}

// ─── Content Import Commands ─────────────────────────────────────────────────

/// Result of importing content (L0 add without network publish).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportResult {
    pub hash: String,
    pub title: String,
    pub size: u64,
    pub content_type: String,
    pub mentions: Option<usize>,
}

/// Import a file as L0 content without publishing to the network.
///
/// This is the "add knowledge" flow for the desktop app:
/// 1. Read and store the file locally
/// 2. Extract L1 mentions
/// 3. Return hash for further operations (graph linking, later publish)
///
/// Unlike `publish_file`, this does NOT set visibility/price or announce
/// to the network. The content stays local until explicitly published.
#[tauri::command]
pub async fn add_content(
    file_path: String,
    title: Option<String>,
    description: Option<String>,
    protocol: State<'_, Arc<Mutex<Option<ProtocolState>>>>,
) -> Result<ImportResult, String> {
    let mut guard = protocol.lock().await;
    let state = guard.as_mut().ok_or("Node not initialized - unlock first")?;

    let path = PathBuf::from(&file_path);
    if !path.exists() {
        return Err(format!("File not found: {}", file_path));
    }
    if path.is_dir() {
        return Err("Cannot import a directory. Please specify a file.".into());
    }

    // Read file
    let content = std::fs::read(&path)
        .map_err(|e| format!("Failed to read file: {}", e))?;
    if content.is_empty() {
        return Err("Cannot import an empty file.".into());
    }

    // Resolve title from filename if not provided
    let title = title.unwrap_or_else(|| {
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Untitled")
            .to_string()
    });

    // Detect MIME type from extension
    let mime = path.extension()
        .and_then(|e| e.to_str())
        .map(|ext| match ext.to_lowercase().as_str() {
            "txt" => "text/plain",
            "md" => "text/markdown",
            "html" | "htm" => "text/html",
            "json" => "application/json",
            "pdf" => "application/pdf",
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "csv" => "text/csv",
            "xml" => "application/xml",
            "yaml" | "yml" => "text/yaml",
            _ => "application/octet-stream",
        })
        .unwrap_or("application/octet-stream");

    // Check for duplicate
    let computed_hash = content_hash(&content);
    if let Ok(Some(_existing)) = state.ops.get_content_manifest(&computed_hash) {
        return Err(format!(
            "Content with this hash already exists ({}). Use the existing hash.",
            computed_hash
        ));
    }

    // Create metadata
    let mut metadata = Metadata::new(&title, content.len() as u64);
    metadata = metadata.with_mime_type(mime);
    if let Some(desc) = description {
        metadata = metadata.with_description(&desc);
    }

    // Store content locally (L0)
    let hash = state.ops.create_content(&content, metadata)
        .map_err(|e| format!("Failed to store content: {}", e))?;

    // Extract L1 mentions (best-effort — don't fail the import if extraction fails)
    let mentions = state.ops.extract_l1_summary(&hash)
        .ok()
        .map(|s| s.mention_count as usize);

    info!("Imported L0 content: {} ({}) — {} bytes, {:?} mentions",
        title, hash, content.len(), mentions);

    Ok(ImportResult {
        hash: hash.to_string(),
        title,
        size: content.len() as u64,
        content_type: mime.to_string(),
        mentions,
    })
}

/// Import text directly as L0 content without publishing to the network.
///
/// Same as `add_content` but for text input (paste, typed notes).
#[tauri::command]
pub async fn add_text_content(
    text: String,
    title: String,
    description: Option<String>,
    protocol: State<'_, Arc<Mutex<Option<ProtocolState>>>>,
) -> Result<ImportResult, String> {
    let mut guard = protocol.lock().await;
    let state = guard.as_mut().ok_or("Node not initialized - unlock first")?;

    let content = text.as_bytes();
    if content.is_empty() {
        return Err("Cannot import empty text.".into());
    }

    // Check for duplicate
    let computed_hash = content_hash(content);
    if let Ok(Some(_existing)) = state.ops.get_content_manifest(&computed_hash) {
        return Err(format!(
            "Content with this hash already exists ({}). Use the existing hash.",
            computed_hash
        ));
    }

    // Create metadata
    let mut metadata = Metadata::new(&title, content.len() as u64);
    metadata = metadata.with_mime_type("text/plain");
    if let Some(desc) = description {
        metadata = metadata.with_description(&desc);
    }

    // Store content locally (L0)
    let hash = state.ops.create_content(content, metadata)
        .map_err(|e| format!("Failed to store content: {}", e))?;

    // Extract L1 mentions
    let mentions = state.ops.extract_l1_summary(&hash)
        .ok()
        .map(|s| s.mention_count as usize);

    info!("Imported L0 text: {} ({}) — {} bytes, {:?} mentions",
        title, hash, content.len(), mentions);

    Ok(ImportResult {
        hash: hash.to_string(),
        title,
        size: content.len() as u64,
        content_type: "text/plain".to_string(),
        mentions,
    })
}
