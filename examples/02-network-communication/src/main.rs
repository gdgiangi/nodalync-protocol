//! # Network Communication Example
//! 
//! This example demonstrates multi-node communication in the Nodalync protocol:
//! - Setting up network peers using libp2p
//! - Peer discovery via Kademlia DHT
//! - Content sharing and gossip
//! - Network health monitoring
//! - Message validation and routing
//!
//! Run with: `cargo run`
//! Multi-node: `cargo run -- --role bootstrap` and `cargo run -- --role peer`

use std::collections::HashMap;
use std::time::Duration;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tracing::{info, warn, debug, error};
use tokio::time::{sleep, timeout};
use futures::StreamExt;

// Import Nodalync networking
use nodalync_net::{NetworkManager, PeerEvent, NetworkConfig};
use nodalync_types::{NodeId, ContentHash, ContentItem, CreatorId};
use nodalync_crypto::Identity;
use nodalync_wire::{WireMessage, MessageType};
use nodalync_valid::ContentValidator;

/// Network node roles
#[derive(Debug, Clone)]
enum NodeRole {
    Bootstrap,  // Helps other nodes discover the network
    Peer,      // Regular network participant
}

/// Network node configuration
#[derive(Debug, Clone)]
struct NetworkNodeConfig {
    role: NodeRole,
    listen_port: u16,
    bootstrap_addresses: Vec<String>,
    node_name: String,
}

/// A network-enabled Nodalync node
struct NetworkNode {
    id: NodeId,
    identity: Identity,
    network: NetworkManager,
    validator: ContentValidator,
    config: NetworkNodeConfig,
    content_cache: HashMap<ContentHash, ContentItem>,
    peer_count: usize,
}

impl NetworkNode {
    /// Create a new network node
    pub async fn new(config: NetworkNodeConfig) -> Result<Self> {
        info!("🌐 Creating network node: {} ({}:{})", 
              config.node_name, config.role.as_str(), config.listen_port);

        // Generate identity
        let identity = Identity::generate()
            .context("Failed to generate identity")?;
        let node_id = NodeId::from_identity(&identity)?;

        // Configure networking
        let net_config = NetworkConfig {
            listen_port: config.listen_port,
            bootstrap_peers: config.bootstrap_addresses.clone(),
            enable_gossip: true,
            enable_discovery: true,
        };

        // Initialize network manager
        let network = NetworkManager::new(net_config, identity.clone()).await
            .context("Failed to initialize network")?;

        let validator = ContentValidator::new();

        Ok(NetworkNode {
            id: node_id,
            identity,
            network,
            validator,
            config,
            content_cache: HashMap::new(),
            peer_count: 0,
        })
    }

    /// Start the network node
    pub async fn start(&mut self) -> Result<()> {
        info!("🚀 Starting network node: {}", self.id);

        // Start networking
        self.network.start().await
            .context("Failed to start network")?;

        // Connect to bootstrap peers if we're not a bootstrap node
        if !matches!(self.config.role, NodeRole::Bootstrap) {
            self.connect_to_bootstrap_peers().await?;
        }

        info!("✅ Network node started successfully");
        Ok(())
    }

    /// Connect to bootstrap peers for network discovery
    async fn connect_to_bootstrap_peers(&mut self) -> Result<()> {
        info!("🔍 Connecting to bootstrap peers...");

        for addr in &self.config.bootstrap_addresses {
            match self.network.connect_to_peer(addr).await {
                Ok(_) => {
                    info!("✅ Connected to bootstrap peer: {}", addr);
                    self.peer_count += 1;
                },
                Err(e) => {
                    warn!("⚠️  Failed to connect to {}: {}", addr, e);
                }
            }
        }

        if self.peer_count > 0 {
            info!("🌟 Connected to {} bootstrap peers", self.peer_count);
        } else {
            warn!("⚠️  No bootstrap peers available - running in isolated mode");
        }

        Ok(())
    }

    /// Share content with the network
    pub async fn share_content(
        &mut self, 
        creator_id: CreatorId,
        content_data: &[u8],
        metadata: HashMap<String, String>,
    ) -> Result<ContentHash> {
        info!("📤 Sharing content from creator: {}", creator_id);

        // Create content item
        let content_hash = ContentHash::from_data(content_data)?;
        let content_item = ContentItem {
            hash: content_hash.clone(),
            creator_id: creator_id.clone(),
            data_size: content_data.len() as u64,
            metadata: metadata.clone(),
            created_at: std::time::SystemTime::now(),
        };

        // Validate content
        let validation_result = self.validator.validate_standard(&content_item)?;
        if !validation_result.is_valid() {
            anyhow::bail!("Content validation failed: {:?}", validation_result);
        }

        // Store in local cache
        self.content_cache.insert(content_hash.clone(), content_item.clone());

        // Create wire message for network sharing
        let wire_msg = WireMessage::new(
            MessageType::ContentShare,
            self.id.clone(),
            serde_json::to_vec(&content_item)?,
        )?;

        // Broadcast to network
        self.network.broadcast_message(wire_msg).await
            .context("Failed to broadcast content")?;

        info!("✅ Content shared with network: {}", content_hash);
        Ok(content_hash)
    }

    /// Request content from the network
    pub async fn request_content(&mut self, content_hash: &ContentHash) -> Result<Option<ContentItem>> {
        info!("📥 Requesting content from network: {}", content_hash);

        // Check local cache first
        if let Some(content) = self.content_cache.get(content_hash) {
            info!("✅ Content found in local cache");
            return Ok(Some(content.clone()));
        }

        // Create request message
        let request_msg = WireMessage::new(
            MessageType::ContentRequest,
            self.id.clone(),
            content_hash.as_bytes().to_vec(),
        )?;

        // Send request to network
        self.network.broadcast_message(request_msg).await
            .context("Failed to send content request")?;

        // Wait for response (with timeout)
        let timeout_duration = Duration::from_secs(10);
        match timeout(timeout_duration, self.wait_for_content_response(content_hash)).await {
            Ok(Some(content)) => {
                info!("✅ Content received from network");
                Ok(Some(content))
            },
            Ok(None) => {
                info!("ℹ️  Content not found in network");
                Ok(None)
            },
            Err(_) => {
                warn!("⏰ Content request timed out");
                Ok(None)
            }
        }
    }

    /// Wait for content response from network
    async fn wait_for_content_response(&mut self, content_hash: &ContentHash) -> Option<ContentItem> {
        // Listen for network events
        let mut event_stream = self.network.event_stream();
        
        while let Some(event) = event_stream.next().await {
            match event {
                PeerEvent::MessageReceived { message, peer_id } => {
                    if let Ok(content_item) = serde_json::from_slice::<ContentItem>(&message.payload) {
                        if &content_item.hash == content_hash {
                            debug!("📨 Received content from peer: {}", peer_id);
                            self.content_cache.insert(content_hash.clone(), content_item.clone());
                            return Some(content_item);
                        }
                    }
                },
                PeerEvent::PeerConnected { peer_id } => {
                    debug!("🤝 Peer connected: {}", peer_id);
                    self.peer_count += 1;
                },
                PeerEvent::PeerDisconnected { peer_id } => {
                    debug!("👋 Peer disconnected: {}", peer_id);
                    if self.peer_count > 0 {
                        self.peer_count -= 1;
                    }
                },
                _ => {}
            }
        }
        
        None
    }

    /// Process incoming network messages
    pub async fn handle_network_events(&mut self) -> Result<()> {
        info!("👂 Listening for network events...");
        
        let mut event_stream = self.network.event_stream();
        
        while let Some(event) = event_stream.next().await {
            match event {
                PeerEvent::MessageReceived { message, peer_id } => {
                    self.handle_message(message, peer_id).await?;
                },
                PeerEvent::PeerConnected { peer_id } => {
                    info!("🤝 New peer connected: {}", peer_id);
                    self.peer_count += 1;
                },
                PeerEvent::PeerDisconnected { peer_id } => {
                    info!("👋 Peer disconnected: {}", peer_id);
                    if self.peer_count > 0 {
                        self.peer_count -= 1;
                    }
                },
                PeerEvent::DiscoveryUpdate { discovered_peers } => {
                    info!("🔍 Discovered {} new peers", discovered_peers.len());
                },
            }
        }
        
        Ok(())
    }

    /// Handle incoming messages
    async fn handle_message(&mut self, message: WireMessage, peer_id: String) -> Result<()> {
        debug!("📨 Received message type {:?} from {}", message.message_type, peer_id);

        match message.message_type {
            MessageType::ContentShare => {
                // Someone is sharing content
                if let Ok(content_item) = serde_json::from_slice::<ContentItem>(&message.payload) {
                    info!("📦 Received content share: {}", content_item.hash);
                    
                    // Validate the content
                    match self.validator.validate_standard(&content_item) {
                        Ok(result) if result.is_valid() => {
                            self.content_cache.insert(content_item.hash.clone(), content_item);
                            info!("✅ Content validated and cached");
                        },
                        Ok(_) => {
                            warn!("⚠️  Content validation failed - not caching");
                        },
                        Err(e) => {
                            error!("❌ Content validation error: {}", e);
                        }
                    }
                }
            },
            MessageType::ContentRequest => {
                // Someone is requesting content
                if let Ok(content_hash) = ContentHash::from_bytes(&message.payload) {
                    debug!("📤 Content request for: {}", content_hash);
                    
                    if let Some(content_item) = self.content_cache.get(&content_hash) {
                        // We have the content - send it back
                        let response_msg = WireMessage::new(
                            MessageType::ContentResponse,
                            self.id.clone(),
                            serde_json::to_vec(content_item)?,
                        )?;
                        
                        // Send directly to requesting peer
                        self.network.send_to_peer(&peer_id, response_msg).await?;
                        info!("📤 Sent content to requesting peer");
                    }
                }
            },
            MessageType::ContentResponse => {
                // Response to our content request
                if let Ok(content_item) = serde_json::from_slice::<ContentItem>(&message.payload) {
                    info!("📥 Received content response: {}", content_item.hash);
                    self.content_cache.insert(content_item.hash.clone(), content_item);
                }
            },
            _ => {
                debug!("🤔 Unhandled message type: {:?}", message.message_type);
            }
        }

        Ok(())
    }

    /// Get network status
    pub fn network_status(&self) -> NetworkStatus {
        NetworkStatus {
            node_id: self.id.clone(),
            name: self.config.node_name.clone(),
            role: self.config.role.as_str().to_string(),
            peer_count: self.peer_count,
            content_count: self.content_cache.len(),
            is_connected: self.peer_count > 0,
        }
    }
}

impl NodeRole {
    fn as_str(&self) -> &'static str {
        match self {
            NodeRole::Bootstrap => "bootstrap",
            NodeRole::Peer => "peer",
        }
    }
}

#[derive(Debug, Serialize)]
struct NetworkStatus {
    node_id: NodeId,
    name: String,
    role: String,
    peer_count: usize,
    content_count: usize,
    is_connected: bool,
}

/// Parse command line arguments
fn parse_args() -> Result<NetworkNodeConfig> {
    let args: Vec<String> = std::env::args().collect();
    
    // Default configuration
    let mut config = NetworkNodeConfig {
        role: NodeRole::Peer,
        listen_port: 9000,
        bootstrap_addresses: vec!["127.0.0.1:8000".to_string()],
        node_name: "Network Node".to_string(),
    };
    
    // Parse simple arguments
    for i in 1..args.len() {
        match args[i].as_str() {
            "--role" if i + 1 < args.len() => {
                config.role = match args[i + 1].as_str() {
                    "bootstrap" => NodeRole::Bootstrap,
                    "peer" => NodeRole::Peer,
                    _ => return Err(anyhow::anyhow!("Invalid role. Use 'bootstrap' or 'peer'")),
                };
            },
            "--port" if i + 1 < args.len() => {
                config.listen_port = args[i + 1].parse()
                    .context("Invalid port number")?;
            },
            "--name" if i + 1 < args.len() => {
                config.node_name = args[i + 1].clone();
            },
            _ => {}
        }
    }
    
    // Bootstrap nodes use different port and don't connect to others
    if matches!(config.role, NodeRole::Bootstrap) {
        config.listen_port = 8000;
        config.bootstrap_addresses.clear();
        config.node_name = "Bootstrap Node".to_string();
    }
    
    Ok(config)
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter("debug,libp2p=info,yamux=info")
        .init();

    info!("🌐 Starting Nodalync Network Communication Example");

    // Parse configuration
    let config = parse_args()?;
    info!("Configuration: {:?}", config);

    // Create and start node
    let mut node = NetworkNode::new(config).await?;
    node.start().await?;

    // Show status
    info!("📊 Network Status: {}", serde_json::to_string_pretty(&node.network_status())?);

    // Example scenarios based on role
    match node.config.role {
        NodeRole::Bootstrap => {
            info!("\n🏁 Running as Bootstrap Node");
            info!("   Waiting for peers to connect...");
            
            // Bootstrap nodes primarily listen for connections
            tokio::spawn(async move {
                let _ = node.handle_network_events().await;
            });
            
            // Keep running
            loop {
                sleep(Duration::from_secs(10)).await;
                info!("🔄 Bootstrap node active - {} peers connected", node.peer_count);
            }
        },
        NodeRole::Peer => {
            info!("\n👥 Running as Peer Node");
            
            // Give time to connect to bootstrap
            sleep(Duration::from_secs(2)).await;
            
            // Example 1: Share some content
            info!("\n📤 Example 1: Sharing Content");
            let creator_id = CreatorId::new("alice@example.com")?;
            let content_data = b"Hello from the network! This is shared content.";
            let mut metadata = HashMap::new();
            metadata.insert("type".to_string(), "announcement".to_string());
            metadata.insert("timestamp".to_string(), chrono::Utc::now().to_rfc3339());

            let content_hash = node.share_content(creator_id, content_data, metadata).await?;
            
            // Example 2: Wait a bit and request content
            info!("\n📥 Example 2: Requesting Content");
            sleep(Duration::from_secs(1)).await;
            
            match node.request_content(&content_hash).await? {
                Some(content) => {
                    info!("✅ Successfully retrieved content: {:?}", content);
                },
                None => {
                    warn!("⚠️  Content not found in network");
                }
            }
            
            // Example 3: Monitor network events
            info!("\n👂 Example 3: Monitoring Network Events");
            info!("   Listening for network activity for 30 seconds...");
            
            // Handle events for a limited time in this example
            let event_handler = tokio::spawn(async move {
                let _ = node.handle_network_events().await;
            });
            
            sleep(Duration::from_secs(30)).await;
            event_handler.abort();
        }
    }

    info!("\n🎉 Network Communication Example Complete!");
    info!("   Tips:");
    info!("   • Run multiple instances to see peer-to-peer communication");
    info!("   • Try: cargo run -- --role bootstrap (in one terminal)");
    info!("   • Then: cargo run -- --role peer (in other terminals)");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_bootstrap_node_creation() -> Result<()> {
        let config = NetworkNodeConfig {
            role: NodeRole::Bootstrap,
            listen_port: 8001, // Different port for testing
            bootstrap_addresses: vec![],
            node_name: "Test Bootstrap".to_string(),
        };

        let node = NetworkNode::new(config).await?;
        let status = node.network_status();
        
        assert_eq!(status.role, "bootstrap");
        assert_eq!(status.peer_count, 0);
        
        Ok(())
    }

    #[tokio::test]
    async fn test_peer_node_creation() -> Result<()> {
        let config = NetworkNodeConfig {
            role: NodeRole::Peer,
            listen_port: 9001, // Different port for testing
            bootstrap_addresses: vec!["127.0.0.1:8001".to_string()],
            node_name: "Test Peer".to_string(),
        };

        let node = NetworkNode::new(config).await?;
        let status = node.network_status();
        
        assert_eq!(status.role, "peer");
        assert!(!status.is_connected); // Not connected yet in test
        
        Ok(())
    }
}