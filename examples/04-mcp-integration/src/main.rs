//! # MCP Integration Example
//! 
//! This example demonstrates how AI agents integrate with the Nodalync protocol
//! via Model Context Protocol (MCP) for:
//! - Content attribution tracking
//! - Usage budget management
//! - Creator compensation
//! - Real-time usage monitoring
//!
//! Run with: `cargo run`

use std::collections::HashMap;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tracing::{info, debug, warn};
use tokio::time::{sleep, Duration};

// Import Nodalync MCP integration
use nodalync_mcp::{MCPServer, MCPConfig, BudgetTracker, ContentUsageEvent};
use nodalync_types::{ContentHash, CreatorId, NodeId};
use nodalync_crypto::Identity;
use nodalync_econ::{PaymentChannel, UsageCredit, AttributionWeight};

/// AI agent that uses Nodalync for content attribution
#[derive(Debug)]
struct AIAgent {
    /// Agent identifier
    id: String,
    /// Budget for content usage
    budget_tracker: BudgetTracker,
    /// MCP server connection
    mcp_server: MCPServer,
    /// Usage history
    usage_history: Vec<ContentUsageEvent>,
}

/// Configuration for AI agent
#[derive(Debug, Serialize, Deserialize)]
struct AgentConfig {
    agent_id: String,
    initial_budget: f64,
    max_cost_per_request: f64,
    attribution_threshold: f64,
}

impl AIAgent {
    /// Create a new AI agent with Nodalync integration
    pub async fn new(config: AgentConfig) -> Result<Self> {
        info!("🤖 Creating AI Agent: {}", config.agent_id);

        // Configure MCP server
        let mcp_config = MCPConfig {
            server_name: format!("nodalync-agent-{}", config.agent_id),
            version: "0.1.0".to_string(),
            capabilities: vec![
                "content_attribution".to_string(),
                "budget_tracking".to_string(),
                "usage_monitoring".to_string(),
            ],
        };

        // Initialize MCP server
        let mcp_server = MCPServer::new(mcp_config).await
            .context("Failed to create MCP server")?;

        // Set up budget tracker
        let budget_tracker = BudgetTracker::new(
            config.initial_budget,
            config.max_cost_per_request,
        )?;

        info!("✅ AI Agent initialized with ${:.2} budget", config.initial_budget);

        Ok(AIAgent {
            id: config.agent_id,
            budget_tracker,
            mcp_server,
            usage_history: Vec::new(),
        })
    }

    /// Start the MCP server
    pub async fn start(&mut self) -> Result<()> {
        info!("🚀 Starting MCP server for agent: {}", self.id);
        
        self.mcp_server.start().await
            .context("Failed to start MCP server")?;
            
        info!("✅ MCP server running - AI agents can now connect");
        Ok(())
    }

    /// Simulate using content with attribution tracking
    pub async fn use_content(&mut self, content_info: ContentUsageRequest) -> Result<ContentUsageResult> {
        info!("📚 Agent using content: {}", content_info.content_id);

        // Check budget before proceeding
        if !self.budget_tracker.can_afford(content_info.estimated_cost)? {
            warn!("💰 Insufficient budget for content usage");
            return Ok(ContentUsageResult {
                success: false,
                cost: 0.0,
                attribution: HashMap::new(),
                error: Some("Insufficient budget".to_string()),
            });
        }

        // Reserve budget for this operation
        let reservation = self.budget_tracker.reserve_budget(content_info.estimated_cost)?;
        debug!("💳 Reserved ${:.4} for content usage", content_info.estimated_cost);

        // Simulate content analysis and usage
        let analysis_result = self.analyze_content(&content_info).await?;
        
        // Calculate attribution based on content analysis
        let attribution = self.calculate_attribution(&analysis_result)?;
        
        // Calculate actual cost based on usage
        let actual_cost = self.calculate_usage_cost(&analysis_result, &attribution)?;
        
        // Finalize budget transaction
        self.budget_tracker.finalize_reservation(reservation, actual_cost)?;
        
        // Create payment channels for creators
        self.create_creator_payments(&attribution, actual_cost).await?;
        
        // Record usage event
        let usage_event = ContentUsageEvent {
            agent_id: self.id.clone(),
            content_hash: analysis_result.content_hash,
            creators: attribution.keys().cloned().collect(),
            cost: actual_cost,
            timestamp: std::time::SystemTime::now(),
            attribution_weights: attribution.clone(),
        };
        
        self.usage_history.push(usage_event.clone());
        
        // Report to MCP server for monitoring
        self.mcp_server.report_usage_event(usage_event).await?;
        
        info!("✅ Content usage complete - Cost: ${:.4}", actual_cost);
        
        Ok(ContentUsageResult {
            success: true,
            cost: actual_cost,
            attribution,
            error: None,
        })
    }

    /// Analyze content to determine attribution
    async fn analyze_content(&self, request: &ContentUsageRequest) -> Result<ContentAnalysis> {
        debug!("🔍 Analyzing content: {}", request.content_id);
        
        // Simulate content analysis (in reality, this would use ML models)
        sleep(Duration::from_millis(100)).await;
        
        // Create mock content hash
        let content_hash = ContentHash::from_data(request.content_id.as_bytes())?;
        
        // Simulate finding multiple creators
        let creator_contributions = vec![
            CreatorContribution {
                creator_id: CreatorId::new("author@example.com")?,
                contribution_type: ContributionType::OriginalContent,
                confidence: 0.9,
                tokens_used: request.tokens_used * 0.7, // 70% original content
            },
            CreatorContribution {
                creator_id: CreatorId::new("editor@example.com")?,
                contribution_type: ContributionType::EditorialWork,
                confidence: 0.8,
                tokens_used: request.tokens_used * 0.2, // 20% editorial
            },
            CreatorContribution {
                creator_id: CreatorId::new("data@example.com")?,
                contribution_type: ContributionType::DataSource,
                confidence: 0.6,
                tokens_used: request.tokens_used * 0.1, // 10% data
            },
        ];
        
        Ok(ContentAnalysis {
            content_hash,
            total_tokens: request.tokens_used,
            creator_contributions,
            analysis_confidence: 0.85,
        })
    }

    /// Calculate attribution weights for creators
    fn calculate_attribution(&self, analysis: &ContentAnalysis) -> Result<HashMap<CreatorId, AttributionWeight>> {
        debug!("⚖️ Calculating attribution weights");
        
        let mut attribution = HashMap::new();
        let mut total_weight = 0.0;
        
        for contribution in &analysis.creator_contributions {
            // Weight by tokens used and confidence
            let weight = (contribution.tokens_used / analysis.total_tokens as f64) * contribution.confidence;
            total_weight += weight;
            
            attribution.insert(
                contribution.creator_id.clone(),
                AttributionWeight::new(weight)?,
            );
        }
        
        // Normalize weights to sum to 1.0
        if total_weight > 0.0 {
            for weight in attribution.values_mut() {
                *weight = AttributionWeight::new(weight.value() / total_weight)?;
            }
        }
        
        debug!("✅ Attribution calculated for {} creators", attribution.len());
        Ok(attribution)
    }

    /// Calculate usage cost based on analysis
    fn calculate_usage_cost(&self, analysis: &ContentAnalysis, attribution: &HashMap<CreatorId, AttributionWeight>) -> Result<f64> {
        // Base cost per token
        let base_cost_per_token = 0.0001; // $0.0001 per token
        
        // Quality multiplier based on analysis confidence
        let quality_multiplier = 1.0 + (analysis.analysis_confidence - 0.5) * 0.5;
        
        // Attribution complexity multiplier
        let complexity_multiplier = 1.0 + (attribution.len() as f64 - 1.0) * 0.1;
        
        let total_cost = analysis.total_tokens as f64 
            * base_cost_per_token 
            * quality_multiplier 
            * complexity_multiplier;
            
        debug!("💰 Cost calculation: {} tokens × ${:.6} × {:.2} × {:.2} = ${:.4}",
               analysis.total_tokens, base_cost_per_token, quality_multiplier, complexity_multiplier, total_cost);
               
        Ok(total_cost)
    }

    /// Create payment channels for creators
    async fn create_creator_payments(&mut self, attribution: &HashMap<CreatorId, AttributionWeight>, total_cost: f64) -> Result<()> {
        debug!("💳 Creating payment channels for {} creators", attribution.len());
        
        for (creator_id, weight) in attribution {
            let creator_payment = total_cost * weight.value();
            
            if creator_payment > 0.001 { // Only pay if above minimum threshold
                // Create payment channel (simplified)
                let _payment_channel = PaymentChannel::new(
                    self.id.clone(),
                    creator_id.clone(),
                    creator_payment,
                )?;
                
                info!("💸 Payment scheduled: ${:.4} to {}", creator_payment, creator_id);
            }
        }
        
        Ok(())
    }

    /// Get agent status and usage statistics
    pub fn get_status(&self) -> AgentStatus {
        let total_spent = self.usage_history.iter()
            .map(|event| event.cost)
            .sum();
            
        let unique_creators = self.usage_history.iter()
            .flat_map(|event| &event.creators)
            .collect::<std::collections::HashSet<_>>()
            .len();
            
        AgentStatus {
            agent_id: self.id.clone(),
            remaining_budget: self.budget_tracker.remaining_budget(),
            total_spent,
            usage_events: self.usage_history.len(),
            unique_creators,
            is_active: true,
        }
    }

    /// Generate usage report
    pub fn generate_usage_report(&self) -> UsageReport {
        let mut creator_payments = HashMap::new();
        let mut content_types = HashMap::new();
        
        for event in &self.usage_history {
            // Aggregate payments by creator
            for (creator, weight) in &event.attribution_weights {
                let payment = event.cost * weight.value();
                *creator_payments.entry(creator.clone()).or_insert(0.0) += payment;
            }
            
            // Track content hash usage
            *content_types.entry(event.content_hash.clone()).or_insert(0) += 1;
        }
        
        UsageReport {
            agent_id: self.id.clone(),
            reporting_period: "session".to_string(),
            total_cost: self.usage_history.iter().map(|e| e.cost).sum(),
            creator_payments,
            content_usage: content_types,
            events_count: self.usage_history.len(),
        }
    }
}

// Supporting data structures

#[derive(Debug)]
struct ContentUsageRequest {
    content_id: String,
    tokens_used: u32,
    estimated_cost: f64,
    usage_type: UsageType,
}

#[derive(Debug)]
enum UsageType {
    Analysis,
    Generation,
    Translation,
    Summarization,
}

#[derive(Debug)]
struct ContentUsageResult {
    success: bool,
    cost: f64,
    attribution: HashMap<CreatorId, AttributionWeight>,
    error: Option<String>,
}

#[derive(Debug)]
struct ContentAnalysis {
    content_hash: ContentHash,
    total_tokens: u32,
    creator_contributions: Vec<CreatorContribution>,
    analysis_confidence: f64,
}

#[derive(Debug)]
struct CreatorContribution {
    creator_id: CreatorId,
    contribution_type: ContributionType,
    confidence: f64,
    tokens_used: f64,
}

#[derive(Debug)]
enum ContributionType {
    OriginalContent,
    EditorialWork,
    DataSource,
    Translation,
    Curation,
}

#[derive(Debug, Serialize)]
struct AgentStatus {
    agent_id: String,
    remaining_budget: f64,
    total_spent: f64,
    usage_events: usize,
    unique_creators: usize,
    is_active: bool,
}

#[derive(Debug, Serialize)]
struct UsageReport {
    agent_id: String,
    reporting_period: String,
    total_cost: f64,
    creator_payments: HashMap<CreatorId, f64>,
    content_usage: HashMap<ContentHash, u32>,
    events_count: usize,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter("debug,hyper=info")
        .init();

    info!("🤖 Starting Nodalync MCP Integration Example");

    // Create AI agent configuration
    let config = AgentConfig {
        agent_id: "claude-assistant".to_string(),
        initial_budget: 10.0, // $10 budget
        max_cost_per_request: 1.0,
        attribution_threshold: 0.01,
    };

    // Initialize AI agent
    let mut agent = AIAgent::new(config).await?;
    
    // Start MCP server
    agent.start().await?;
    
    info!("📊 Initial Agent Status: {}", serde_json::to_string_pretty(&agent.get_status())?);

    // Example 1: Use some content for analysis
    info!("\n📚 Example 1: Content Analysis");
    
    let usage_request = ContentUsageRequest {
        content_id: "article-12345".to_string(),
        tokens_used: 1500,
        estimated_cost: 0.15,
        usage_type: UsageType::Analysis,
    };
    
    let result = agent.use_content(usage_request).await?;
    
    if result.success {
        info!("✅ Content analysis complete");
        info!("   Cost: ${:.4}", result.cost);
        info!("   Creators compensated: {}", result.attribution.len());
    } else {
        warn!("❌ Content usage failed: {:?}", result.error);
    }

    // Example 2: Generate content with attribution
    info!("\n✍️ Example 2: Content Generation");
    
    let generation_request = ContentUsageRequest {
        content_id: "generated-content-456".to_string(),
        tokens_used: 800,
        estimated_cost: 0.08,
        usage_type: UsageType::Generation,
    };
    
    let result = agent.use_content(generation_request).await?;
    info!("✅ Generation complete - Cost: ${:.4}", result.cost);

    // Example 3: Translation task
    info!("\n🌍 Example 3: Translation Task");
    
    let translation_request = ContentUsageRequest {
        content_id: "translation-789".to_string(),
        tokens_used: 1200,
        estimated_cost: 0.12,
        usage_type: UsageType::Translation,
    };
    
    let result = agent.use_content(translation_request).await?;
    info!("✅ Translation complete - Cost: ${:.4}", result.cost);

    // Show final status
    info!("\n📊 Final Agent Status:");
    let final_status = agent.get_status();
    info!("   Remaining Budget: ${:.2}", final_status.remaining_budget);
    info!("   Total Spent: ${:.4}", final_status.total_spent);
    info!("   Usage Events: {}", final_status.usage_events);
    info!("   Creators Compensated: {}", final_status.unique_creators);

    // Generate usage report
    info!("\n📋 Usage Report:");
    let report = agent.generate_usage_report();
    info!("{}", serde_json::to_string_pretty(&report)?);

    info!("\n🎉 MCP Integration Example Complete!");
    info!("   Key Benefits Demonstrated:");
    info!("   • ✅ Automatic content attribution");
    info!("   • 💰 Real-time budget tracking");
    info!("   • 👥 Creator compensation");
    info!("   • 📊 Usage monitoring and reporting");
    info!("   • 🔗 Seamless AI agent integration");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_agent_creation() -> Result<()> {
        let config = AgentConfig {
            agent_id: "test-agent".to_string(),
            initial_budget: 5.0,
            max_cost_per_request: 1.0,
            attribution_threshold: 0.01,
        };

        let agent = AIAgent::new(config).await?;
        let status = agent.get_status();
        
        assert_eq!(status.agent_id, "test-agent");
        assert_eq!(status.remaining_budget, 5.0);
        assert_eq!(status.usage_events, 0);
        
        Ok(())
    }

    #[tokio::test]
    async fn test_content_usage() -> Result<()> {
        let config = AgentConfig {
            agent_id: "test-agent".to_string(),
            initial_budget: 1.0,
            max_cost_per_request: 0.5,
            attribution_threshold: 0.01,
        };

        let mut agent = AIAgent::new(config).await?;
        
        let request = ContentUsageRequest {
            content_id: "test-content".to_string(),
            tokens_used: 100,
            estimated_cost: 0.01,
            usage_type: UsageType::Analysis,
        };

        let result = agent.use_content(request).await?;
        
        assert!(result.success);
        assert!(result.cost > 0.0);
        assert!(!result.attribution.is_empty());
        
        let final_status = agent.get_status();
        assert!(final_status.remaining_budget < 1.0);
        assert_eq!(final_status.usage_events, 1);
        
        Ok(())
    }
}