//! Mock CLI service for Sprint 1 desktop app demo
//! 
//! Simulates nodalync-cli functionality without requiring the full protocol stack.
//! This allows the desktop app to demonstrate all Sprint 1 features while the 
//! real CLI integration is completed in Sprint 2.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use chrono::{DateTime, Utc};

/// Simulated identity information
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MockIdentity {
    pub peer_id: String,
    pub public_key: String,
    pub created_at: DateTime<Utc>,
}

/// Simulated content hash and metadata
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MockContent {
    pub hash: String,
    pub title: String,
    pub content_type: String, // L0, L1, L2, L3
    pub size: u64,
    pub price: f64,
    pub visibility: String,
    pub created_at: DateTime<Utc>,
    pub l1_entities: Vec<String>,
    pub l2_relationships: Vec<MockRelationship>,
}

/// Simulated L2 entity relationship
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MockRelationship {
    pub from_entity: String,
    pub relationship: String,
    pub to_entity: String,
    pub confidence: f32,
}

/// Simulated earnings data
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MockEarnings {
    pub content_hash: String,
    pub total_earned: f64,
    pub query_count: u32,
    pub last_query: Option<DateTime<Utc>>,
}

/// Simulated network peer
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MockPeer {
    pub peer_id: String,
    pub address: String,
    pub content_count: u32,
    pub last_seen: DateTime<Utc>,
}

/// Mock CLI service that simulates protocol functionality
#[derive(Debug)]
pub struct MockCliService {
    identity: Option<MockIdentity>,
    local_content: Vec<MockContent>,
    network_content: Vec<MockContent>,
    peers: Vec<MockPeer>,
    earnings: HashMap<String, MockEarnings>,
}

impl MockCliService {
    /// Create new mock CLI service
    pub fn new() -> Self {
        let mut service = Self {
            identity: None,
            local_content: Vec::new(),
            network_content: Vec::new(),
            peers: Vec::new(),
            earnings: HashMap::new(),
        };
        
        // Initialize with demo data
        service.init_demo_data();
        service
    }

    /// Initialize with realistic demo data
    fn init_demo_data(&mut self) {
        // Demo network peers
        self.peers = vec![
            MockPeer {
                peer_id: "ndl1abc123def456789".to_string(),
                address: "192.168.1.100:8080".to_string(),
                content_count: 42,
                last_seen: Utc::now(),
            },
            MockPeer {
                peer_id: "ndl1xyz789ghi012345".to_string(),
                address: "10.0.0.25:8080".to_string(),
                content_count: 17,
                last_seen: Utc::now() - chrono::Duration::minutes(5),
            },
        ];

        // Demo network content
        self.network_content = vec![
            MockContent {
                hash: "hash_ai_research_paper_2024".to_string(),
                title: "Advances in Large Language Models 2024".to_string(),
                content_type: "L0".to_string(),
                size: 2_500_000,
                price: 0.05,
                visibility: "shared".to_string(),
                created_at: Utc::now() - chrono::Duration::days(5),
                l1_entities: vec!["GPT-4".to_string(), "Claude".to_string(), "AI Safety".to_string()],
                l2_relationships: vec![
                    MockRelationship {
                        from_entity: "GPT-4".to_string(),
                        relationship: "competes_with".to_string(),
                        to_entity: "Claude".to_string(),
                        confidence: 0.85,
                    }
                ],
            },
            MockContent {
                hash: "hash_crypto_whitepaper".to_string(),
                title: "Distributed Knowledge Attribution Protocol".to_string(),
                content_type: "L0".to_string(),
                size: 1_800_000,
                price: 0.12,
                visibility: "shared".to_string(),
                created_at: Utc::now() - chrono::Duration::days(12),
                l1_entities: vec!["Blockchain".to_string(), "IPFS".to_string(), "Attribution".to_string()],
                l2_relationships: vec![],
            },
        ];
    }

    /// Simulate identity creation
    pub async fn create_identity(&mut self) -> Result<MockIdentity, String> {
        // Simulate key generation delay
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        let identity = MockIdentity {
            peer_id: format!("ndl1{:016x}", rand::random::<u64>()),
            public_key: format!("ed25519_{:064x}", rand::random::<u64>()),
            created_at: Utc::now(),
        };

        self.identity = Some(identity.clone());
        Ok(identity)
    }

    /// Get current identity
    pub fn get_identity(&self) -> Option<&MockIdentity> {
        self.identity.as_ref()
    }

    /// Simulate content publishing
    pub async fn publish_content(
        &mut self,
        file_path: &str,
        title: Option<String>,
        price: Option<f64>,
    ) -> Result<MockContent, String> {
        if self.identity.is_none() {
            return Err("No identity created. Run 'nodalync init' first.".to_string());
        }

        // Simulate file processing
        tokio::time::sleep(tokio::time::Duration::from_millis(1500)).await;

        // Extract filename for demo
        let filename = std::path::Path::new(file_path)
            .file_name()
            .unwrap_or_default()
            .to_str()
            .unwrap_or("unknown");

        // Generate demo L1 entities based on filename/content
        let l1_entities = self.generate_demo_entities(filename);
        let l2_relationships = self.generate_demo_relationships(&l1_entities);

        let content = MockContent {
            hash: format!("hash_{:016x}", rand::random::<u64>()),
            title: title.unwrap_or_else(|| filename.to_string()),
            content_type: "L0".to_string(),
            size: rand::random::<u32>() as u64 % 10_000_000 + 100_000, // 100KB - 10MB
            price: price.unwrap_or(0.01),
            visibility: "shared".to_string(),
            created_at: Utc::now(),
            l1_entities,
            l2_relationships,
        };

        // Add to local content
        self.local_content.push(content.clone());

        // Initialize earnings tracking
        self.earnings.insert(content.hash.clone(), MockEarnings {
            content_hash: content.hash.clone(),
            total_earned: 0.0,
            query_count: 0,
            last_query: None,
        });

        Ok(content)
    }

    /// Generate demo entities based on filename
    fn generate_demo_entities(&self, filename: &str) -> Vec<String> {
        let lower_filename = filename.to_lowercase();
        let mut entities = Vec::new();

        // AI/ML related
        if lower_filename.contains("ai") || lower_filename.contains("ml") || lower_filename.contains("neural") {
            entities.extend(["Artificial Intelligence".to_string(), "Machine Learning".to_string(), "Neural Networks".to_string()]);
        }
        
        // Blockchain/Crypto
        if lower_filename.contains("blockchain") || lower_filename.contains("crypto") || lower_filename.contains("bitcoin") {
            entities.extend(["Blockchain".to_string(), "Cryptocurrency".to_string(), "Decentralization".to_string()]);
        }
        
        // Research/Academic
        if lower_filename.contains("research") || lower_filename.contains("paper") || lower_filename.contains("study") {
            entities.extend(["Research".to_string(), "Academic Study".to_string(), "Methodology".to_string()]);
        }

        // Default entities if no matches
        if entities.is_empty() {
            entities = vec!["Document".to_string(), "Information".to_string(), "Knowledge".to_string()];
        }

        entities
    }

    /// Generate demo relationships between entities
    fn generate_demo_relationships(&self, entities: &[String]) -> Vec<MockRelationship> {
        let mut relationships = Vec::new();

        // Create some demo relationships if we have enough entities
        if entities.len() >= 2 {
            relationships.push(MockRelationship {
                from_entity: entities[0].clone(),
                relationship: "relates_to".to_string(),
                to_entity: entities[1].clone(),
                confidence: 0.8,
            });
        }

        if entities.len() >= 3 {
            relationships.push(MockRelationship {
                from_entity: entities[1].clone(),
                relationship: "enables".to_string(),
                to_entity: entities[2].clone(),
                confidence: 0.7,
            });
        }

        relationships
    }

    /// Get local content
    pub fn get_local_content(&self) -> &[MockContent] {
        &self.local_content
    }

    /// Get network content
    pub fn get_network_content(&self) -> &[MockContent] {
        &self.network_content
    }

    /// Search content by query
    pub fn search_content(&self, query: &str, include_network: bool) -> Vec<MockContent> {
        let mut results = Vec::new();
        let query_lower = query.to_lowercase();

        // Search local content
        for content in &self.local_content {
            if content.title.to_lowercase().contains(&query_lower) ||
               content.l1_entities.iter().any(|e| e.to_lowercase().contains(&query_lower)) {
                results.push(content.clone());
            }
        }

        // Search network content if requested
        if include_network {
            for content in &self.network_content {
                if content.title.to_lowercase().contains(&query_lower) ||
                   content.l1_entities.iter().any(|e| e.to_lowercase().contains(&query_lower)) {
                    results.push(content.clone());
                }
            }
        }

        results
    }

    /// Simulate content query (with payment)
    pub async fn query_content(&mut self, hash: &str) -> Result<String, String> {
        // Find content
        let content = self.network_content.iter()
            .chain(self.local_content.iter())
            .find(|c| c.hash == hash)
            .ok_or_else(|| "Content not found".to_string())?;

        // Simulate network delay
        tokio::time::sleep(tokio::time::Duration::from_millis(800)).await;

        // Update earnings if this is our content
        if let Some(earnings) = self.earnings.get_mut(hash) {
            earnings.total_earned += content.price;
            earnings.query_count += 1;
            earnings.last_query = Some(Utc::now());
        }

        // Return simulated content
        Ok(format!(
            "# {}\n\nContent Type: {}\nPrice: {} HBAR\nEntities: {}\n\n[Simulated content for demo purposes]",
            content.title,
            content.content_type,
            content.price,
            content.l1_entities.join(", ")
        ))
    }

    /// Get earnings summary
    pub fn get_earnings(&self) -> Vec<&MockEarnings> {
        self.earnings.values().collect()
    }

    /// Get connected peers
    pub fn get_peers(&self) -> &[MockPeer] {
        &self.peers
    }

    /// Check if node is "running" (always true in mock)
    pub fn is_node_running(&self) -> bool {
        true
    }

    /// Get node status
    pub fn get_node_status(&self) -> HashMap<String, String> {
        let mut status = HashMap::new();
        status.insert("running".to_string(), "true".to_string());
        status.insert("uptime".to_string(), "2h 15m 30s".to_string());
        status.insert("peers".to_string(), self.peers.len().to_string());
        status.insert("local_content".to_string(), self.local_content.len().to_string());
        status.insert("version".to_string(), "nodalync-cli 0.10.1 (mock)".to_string());
        status
    }

    /// Calculate total earnings
    pub fn get_total_earnings(&self) -> f64 {
        self.earnings.values().map(|e| e.total_earned).sum()
    }
}

impl Default for MockCliService {
    fn default() -> Self {
        Self::new()
    }
}