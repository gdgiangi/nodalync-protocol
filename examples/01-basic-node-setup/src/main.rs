//! # Basic Node Setup Example
//! 
//! This example demonstrates the fundamental building blocks of the Nodalync protocol:
//! - Creating a node identity
//! - Content validation and hashing
//! - Basic storage operations
//! - Error handling patterns
//!
//! Run with: `cargo run`
//! Debug logging: `RUST_LOG=debug cargo run`

use std::collections::HashMap;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tracing::{info, warn, error, debug};
use uuid::Uuid;

// Import core Nodalync types
use nodalync_types::{
    NodeId, ContentHash, ContentItem, CreatorId, 
    AttributionWeight, ValidationResult
};
use nodalync_crypto::{Identity, Signature, KeyPair};
use nodalync_valid::ContentValidator;
use nodalync_store::{StorageEngine, ContentStore};

/// Configuration for our basic node
#[derive(Debug, Serialize, Deserialize)]
struct NodeConfig {
    /// Human-readable node name
    pub name: String,
    /// Directory for storing node data
    pub data_dir: String,
    /// Content validation strictness level
    pub validation_level: ValidationLevel,
}

#[derive(Debug, Serialize, Deserialize)]
enum ValidationLevel {
    Strict,
    Standard,
    Permissive,
}

/// Represents a basic Nodalync node
struct BasicNode {
    /// Unique node identifier
    id: NodeId,
    /// Cryptographic identity for signing
    identity: Identity,
    /// Content validator
    validator: ContentValidator,
    /// Local content storage
    store: ContentStore,
    /// Node configuration
    config: NodeConfig,
}

impl BasicNode {
    /// Create a new node with the given configuration
    pub async fn new(config: NodeConfig) -> Result<Self> {
        info!("Creating new Nodalync node: {}", config.name);

        // Generate cryptographic identity
        let identity = Identity::generate()
            .context("Failed to generate node identity")?;
        
        let node_id = NodeId::from_identity(&identity)?;
        info!("Generated node ID: {}", node_id);

        // Initialize content validator
        let validator = ContentValidator::new();
        debug!("Content validator initialized");

        // Set up storage engine
        let store = ContentStore::new(&config.data_dir).await
            .context("Failed to initialize content storage")?;
        info!("Storage initialized at: {}", config.data_dir);

        Ok(BasicNode {
            id: node_id,
            identity,
            validator,
            store,
            config,
        })
    }

    /// Validate and store content from a creator
    pub async fn process_content(
        &mut self, 
        creator_id: CreatorId,
        content_data: &[u8],
        metadata: HashMap<String, String>,
    ) -> Result<ContentHash> {
        info!("Processing content from creator: {}", creator_id);

        // Hash the content
        let content_hash = ContentHash::from_data(content_data)?;
        debug!("Generated content hash: {}", content_hash);

        // Create content item
        let content_item = ContentItem {
            hash: content_hash.clone(),
            creator_id: creator_id.clone(),
            data_size: content_data.len() as u64,
            metadata: metadata.clone(),
            created_at: std::time::SystemTime::now(),
        };

        // Validate content based on configuration
        let validation_result = match self.config.validation_level {
            ValidationLevel::Strict => {
                self.validator.validate_strict(&content_item)?
            },
            ValidationLevel::Standard => {
                self.validator.validate_standard(&content_item)?
            },
            ValidationLevel::Permissive => {
                // In permissive mode, we still check basic structure
                self.validator.validate_basic(&content_item)?
            },
        };

        match validation_result {
            ValidationResult::Valid => {
                info!("✅ Content validation passed");
            },
            ValidationResult::Warning(msg) => {
                warn!("⚠️  Content validation warning: {}", msg);
            },
            ValidationResult::Invalid(reason) => {
                error!("❌ Content validation failed: {}", reason);
                anyhow::bail!("Content validation failed: {}", reason);
            },
        }

        // Store the content
        self.store.store_content(&content_item, content_data).await
            .context("Failed to store content")?;

        info!("✅ Content stored successfully");
        Ok(content_hash)
    }

    /// Sign content with node identity
    pub fn sign_content(&self, content_hash: &ContentHash) -> Result<Signature> {
        debug!("Signing content hash: {}", content_hash);
        
        let signature = self.identity.sign(content_hash.as_bytes())?;
        
        info!("✅ Content signed successfully");
        Ok(signature)
    }

    /// Retrieve content by hash
    pub async fn get_content(&self, hash: &ContentHash) -> Result<Option<(ContentItem, Vec<u8>)>> {
        debug!("Retrieving content: {}", hash);
        
        let result = self.store.get_content(hash).await?;
        
        match &result {
            Some(_) => info!("✅ Content found"),
            None => info!("ℹ️  Content not found"),
        }
        
        Ok(result)
    }

    /// Calculate attribution weights for creators
    pub fn calculate_attribution(&self, content_hashes: &[ContentHash]) -> Result<HashMap<CreatorId, AttributionWeight>> {
        info!("Calculating attribution for {} content items", content_hashes.len());
        
        let mut attribution_map = HashMap::new();
        
        // In a real implementation, this would use the economics crate
        // For this example, we'll use simple equal distribution
        let weight_per_creator = AttributionWeight::new(1.0 / content_hashes.len() as f64)?;
        
        // This is simplified - in practice you'd look up creators from storage
        for (i, _hash) in content_hashes.iter().enumerate() {
            let creator_id = CreatorId::new(&format!("creator-{}", i))?;
            attribution_map.insert(creator_id, weight_per_creator);
        }
        
        info!("✅ Attribution calculated for {} creators", attribution_map.len());
        Ok(attribution_map)
    }

    /// Get node status information
    pub fn status(&self) -> NodeStatus {
        NodeStatus {
            node_id: self.id.clone(),
            name: self.config.name.clone(),
            validation_level: format!("{:?}", self.config.validation_level),
            is_healthy: true, // In practice, this would check various health metrics
        }
    }
}

#[derive(Debug, Serialize)]
struct NodeStatus {
    node_id: NodeId,
    name: String,
    validation_level: String,
    is_healthy: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter("debug,hyper=info,reqwest=info")
        .init();

    info!("🚀 Starting Nodalync Basic Node Setup Example");

    // Create node configuration
    let config = NodeConfig {
        name: "Example Node".to_string(),
        data_dir: "./example_data".to_string(),
        validation_level: ValidationLevel::Standard,
    };

    // Initialize the node
    let mut node = BasicNode::new(config).await
        .context("Failed to create node")?;

    info!("📊 Node Status: {}", serde_json::to_string_pretty(&node.status())?);

    // Example 1: Process some content
    info!("\n🔍 Example 1: Processing Content");
    
    let creator_id = CreatorId::new("alice@example.com")?;
    let content_data = b"Hello, Nodalync! This is some example content.";
    let mut metadata = HashMap::new();
    metadata.insert("title".to_string(), "Example Content".to_string());
    metadata.insert("type".to_string(), "text/plain".to_string());

    let content_hash = node.process_content(
        creator_id.clone(),
        content_data,
        metadata,
    ).await?;

    // Example 2: Sign the content
    info!("\n✍️  Example 2: Signing Content");
    
    let signature = node.sign_content(&content_hash)?;
    info!("Content signature: {}", signature);

    // Example 3: Retrieve the content
    info!("\n📥 Example 3: Retrieving Content");
    
    if let Some((content_item, data)) = node.get_content(&content_hash).await? {
        info!("Retrieved content item: {:?}", content_item);
        info!("Content data: {}", String::from_utf8_lossy(&data));
    }

    // Example 4: Calculate attribution
    info!("\n⚖️  Example 4: Attribution Calculation");
    
    let content_hashes = vec![content_hash];
    let attribution = node.calculate_attribution(&content_hashes)?;
    info!("Attribution weights: {:#?}", attribution);

    // Example 5: Demonstrate error handling
    info!("\n❌ Example 5: Error Handling");
    
    // Try to get non-existent content
    let fake_hash = ContentHash::from_data(b"nonexistent content")?;
    match node.get_content(&fake_hash).await {
        Ok(Some(_)) => unreachable!(),
        Ok(None) => info!("✅ Correctly handled missing content"),
        Err(e) => error!("Unexpected error: {}", e),
    }

    info!("\n🎉 Basic Node Setup Example Complete!");
    info!("   Next steps:");
    info!("   • Try modifying the validation level in NodeConfig");
    info!("   • Experiment with different content types");
    info!("   • Check out the network communication example");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn create_test_node() -> Result<BasicNode> {
        let temp_dir = TempDir::new()?;
        let config = NodeConfig {
            name: "Test Node".to_string(),
            data_dir: temp_dir.path().to_string_lossy().to_string(),
            validation_level: ValidationLevel::Standard,
        };
        BasicNode::new(config).await
    }

    #[tokio::test]
    async fn test_node_creation() -> Result<()> {
        let node = create_test_node().await?;
        let status = node.status();
        
        assert_eq!(status.name, "Test Node");
        assert!(status.is_healthy);
        
        Ok(())
    }

    #[tokio::test]
    async fn test_content_processing() -> Result<()> {
        let mut node = create_test_node().await?;
        
        let creator_id = CreatorId::new("test@example.com")?;
        let content_data = b"test content";
        let metadata = HashMap::new();

        let hash = node.process_content(creator_id, content_data, metadata).await?;
        
        // Verify we can retrieve the content
        let retrieved = node.get_content(&hash).await?;
        assert!(retrieved.is_some());
        
        let (item, data) = retrieved.unwrap();
        assert_eq!(data, content_data);
        assert_eq!(item.data_size, content_data.len() as u64);
        
        Ok(())
    }

    #[tokio::test]
    async fn test_content_signing() -> Result<()> {
        let node = create_test_node().await?;
        let content_hash = ContentHash::from_data(b"test content")?;
        
        let signature = node.sign_content(&content_hash)?;
        
        // Verify signature is not empty (actual verification would require more setup)
        assert!(!signature.as_bytes().is_empty());
        
        Ok(())
    }

    #[tokio::test]
    async fn test_attribution_calculation() -> Result<()> {
        let node = create_test_node().await?;
        let hashes = vec![
            ContentHash::from_data(b"content 1")?,
            ContentHash::from_data(b"content 2")?,
        ];
        
        let attribution = node.calculate_attribution(&hashes)?;
        
        assert_eq!(attribution.len(), 2);
        
        // Each creator should get equal weight (0.5)
        for weight in attribution.values() {
            assert_eq!(weight.value(), 0.5);
        }
        
        Ok(())
    }
}