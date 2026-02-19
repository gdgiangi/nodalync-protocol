//! Tauri IPC commands for HIP-991 native fee management.
//!
//! These commands expose Hedera's HIP-991 "Permissionless Revenue-Generating
//! Topic IDs" to the Studio frontend. This is the D2 deliverable: on-chain
//! fee collection for knowledge publication via native HCS topic fees.
//!
//! # Architecture
//!
//! ```text
//! Frontend (React)          Tauri IPC              nodalync-settle
//! ┌──────────────┐   ──►  ┌─────────────────┐  ──►  ┌──────────────────┐
//! │ Fee Settings │        │ create_fee_topic │       │ TopicFeeManager  │
//! │ Revenue Dash │        │ submit_to_topic  │       │   create_topic() │
//! │ Topic Status │        │ get_topic_info   │       │   submit()       │
//! └──────────────┘   ◄──  │ get_revenue      │  ◄──  │   get_revenue()  │
//!                         └─────────────────┘       └──────────────────┘
//! ```
//!
//! # Data Persistence
//!
//! Topic configuration is stored in `{data_dir}/studio/hip991_config.json`.
//! This includes the active topic ID, fee amount, and Hedera credentials path.

use std::path::PathBuf;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tauri::State;
use tokio::sync::Mutex;
use tracing::info;

use nodalync_settle::topic::{
    RevenueSummary, TopicFeeConfig, TopicInfo,
};

use crate::protocol::ProtocolState;

// ─── Persisted Configuration ─────────────────────────────────────────────────

/// HIP-991 configuration persisted to disk.
///
/// Tracks the active fee-bearing topic and Hedera account details needed
/// to interact with it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hip991Config {
    /// Active fee-bearing topic ID (None if not yet created).
    pub active_topic_id: Option<String>,

    /// Topic fee configuration used when creating the topic.
    pub fee_config: TopicFeeConfig,

    /// Hedera operator account ID (e.g., "0.0.7703962").
    pub hedera_account_id: String,

    /// Path to the Hedera private key file.
    pub hedera_key_path: String,

    /// Hedera network ("testnet", "mainnet", "previewnet").
    pub hedera_network: String,

    /// Total messages submitted through this topic.
    pub total_submissions: u64,

    /// Last updated timestamp.
    pub updated_at: String,
}

impl Default for Hip991Config {
    fn default() -> Self {
        Self {
            active_topic_id: None,
            fee_config: TopicFeeConfig::default(),
            hedera_account_id: String::new(),
            hedera_key_path: String::new(),
            hedera_network: "testnet".to_string(),
            total_submissions: 0,
            updated_at: Utc::now().to_rfc3339(),
        }
    }
}

impl Hip991Config {
    /// Load from disk, returning default if missing.
    pub fn load(data_dir: &PathBuf) -> Self {
        let path = Self::config_path(data_dir);
        match std::fs::read_to_string(&path) {
            Ok(json) => serde_json::from_str(&json).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    /// Save to disk.
    pub fn save(&self, data_dir: &PathBuf) -> Result<(), String> {
        let dir = data_dir.join("studio");
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("Failed to create studio dir: {}", e))?;
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize HIP-991 config: {}", e))?;
        std::fs::write(Self::config_path(data_dir), json)
            .map_err(|e| format!("Failed to write HIP-991 config: {}", e))?;
        Ok(())
    }

    fn config_path(data_dir: &PathBuf) -> PathBuf {
        data_dir.join("studio").join("hip991_config.json")
    }

    /// Check if Hedera credentials are configured.
    pub fn has_credentials(&self) -> bool {
        !self.hedera_account_id.is_empty() && !self.hedera_key_path.is_empty()
    }
}

// ─── IPC Response Types ──────────────────────────────────────────────────────

/// Response for get_hip991_status — overview of the HIP-991 fee setup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hip991StatusResponse {
    /// Whether HIP-991 is configured (Hedera credentials present).
    pub configured: bool,

    /// Whether a fee-bearing topic has been created.
    pub topic_active: bool,

    /// Active topic ID (if any).
    pub topic_id: Option<String>,

    /// Fee amount per message (tinybars).
    pub fee_amount: u64,

    /// Fee amount in HBAR.
    pub fee_amount_hbar: f64,

    /// Fee collector account.
    pub fee_collector: String,

    /// Hedera network.
    pub network: String,

    /// Total messages submitted.
    pub total_submissions: u64,

    /// Topic memo.
    pub topic_memo: String,
}

/// Response for create/update operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hip991TopicResponse {
    /// The created/updated topic ID.
    pub topic_id: String,

    /// Fee amount configured.
    pub fee_amount: u64,

    /// Fee amount in HBAR.
    pub fee_amount_hbar: f64,

    /// Fee collector account.
    pub fee_collector: String,

    /// Transaction ID of the creation.
    pub transaction_id: Option<String>,

    /// Topic memo.
    pub memo: String,
}

/// Response for submit_to_topic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hip991SubmitResponse {
    /// Transaction ID.
    pub transaction_id: String,

    /// Topic ID the message was submitted to.
    pub topic_id: String,

    /// Fee charged (tinybars).
    pub fee_charged: u64,

    /// Fee in HBAR.
    pub fee_charged_hbar: f64,

    /// Content hash that was submitted.
    pub content_hash: String,
}

// ─── Tauri IPC Commands ──────────────────────────────────────────────────────

/// Get the current HIP-991 configuration and status.
///
/// Returns whether HIP-991 is configured, the active topic, fee amount, etc.
/// Works even if Hedera is not configured (returns unconfigured status).
#[tauri::command]
pub async fn get_hip991_status(
    protocol: State<'_, Mutex<Option<ProtocolState>>>,
) -> Result<Hip991StatusResponse, String> {
    let data_dir = resolve_data_dir(&protocol).await;
    let config = Hip991Config::load(&data_dir);

    Ok(Hip991StatusResponse {
        configured: config.has_credentials(),
        topic_active: config.active_topic_id.is_some(),
        topic_id: config.active_topic_id,
        fee_amount: config.fee_config.fee_amount,
        fee_amount_hbar: config.fee_config.fee_amount as f64 / 100_000_000.0,
        fee_collector: config.fee_config.fee_collector_account_id.clone(),
        network: config.hedera_network,
        total_submissions: config.total_submissions,
        topic_memo: config.fee_config.topic_memo,
    })
}

/// Configure Hedera credentials for HIP-991.
///
/// Must be called before creating a fee-bearing topic.
/// Credentials are persisted to disk (key path only — the key itself stays on disk).
#[tauri::command]
pub async fn configure_hip991(
    account_id: String,
    key_path: String,
    network: Option<String>,
    protocol: State<'_, Mutex<Option<ProtocolState>>>,
) -> Result<Hip991StatusResponse, String> {
    // Validate the key file exists
    if !std::path::Path::new(&key_path).exists() {
        return Err(format!("Private key file not found: {}", key_path));
    }

    let data_dir = resolve_data_dir(&protocol).await;
    let mut config = Hip991Config::load(&data_dir);

    config.hedera_account_id = account_id;
    config.hedera_key_path = key_path;
    config.hedera_network = network.unwrap_or_else(|| "testnet".to_string());

    // Default fee collector to the operator account if not set
    if config.fee_config.fee_collector_account_id.is_empty() {
        config.fee_config.fee_collector_account_id = config.hedera_account_id.clone();
    }

    config.updated_at = Utc::now().to_rfc3339();
    config.save(&data_dir)?;

    info!(
        account = %config.hedera_account_id,
        network = %config.hedera_network,
        "HIP-991 Hedera credentials configured"
    );

    get_hip991_status(protocol).await
}

/// Create a fee-bearing HCS topic with HIP-991 custom fees.
///
/// This is the core D2 operation. Creates an HCS topic where every message
/// submission automatically charges the configured fee.
///
/// Requires Hedera credentials to be configured first via `configure_hip991`.
#[tauri::command]
pub async fn create_fee_topic(
    fee_amount: Option<u64>,
    topic_memo: Option<String>,
    protocol: State<'_, Mutex<Option<ProtocolState>>>,
) -> Result<Hip991TopicResponse, String> {
    let data_dir = resolve_data_dir(&protocol).await;
    let mut config = Hip991Config::load(&data_dir);

    if !config.has_credentials() {
        return Err("Hedera credentials not configured. Call configure_hip991 first.".to_string());
    }

    // Apply overrides
    if let Some(amount) = fee_amount {
        config.fee_config.fee_amount = amount;
    }
    if let Some(memo) = topic_memo {
        config.fee_config.topic_memo = memo;
    }

    // For non-SDK builds, return a simulated response
    #[cfg(not(feature = "hedera-sdk"))]
    {
        let topic_id = format!("0.0.{}", rand::random::<u32>() % 10_000_000);

        config.active_topic_id = Some(topic_id.clone());
        config.updated_at = Utc::now().to_rfc3339();
        config.save(&data_dir)?;

        info!(topic_id = %topic_id, "Simulated HIP-991 topic creation (no SDK)");

        return Ok(Hip991TopicResponse {
            topic_id,
            fee_amount: config.fee_config.fee_amount,
            fee_amount_hbar: config.fee_config.fee_amount as f64 / 100_000_000.0,
            fee_collector: config.fee_config.fee_collector_account_id,
            transaction_id: None,
            memo: config.fee_config.topic_memo,
        });
    }

    // Real SDK implementation
    #[cfg(feature = "hedera-sdk")]
    {
        use nodalync_settle::topic::TopicFeeManager;

        let hedera_config = build_hedera_config(&config)?;
        let manager = TopicFeeManager::new(&hedera_config)
            .await
            .map_err(|e| format!("Failed to initialize Hedera client: {}", e))?;

        let topic_info = manager
            .create_topic(&config.fee_config)
            .await
            .map_err(|e| format!("Failed to create fee-bearing topic: {}", e))?;

        config.active_topic_id = Some(topic_info.topic_id.clone());
        config.updated_at = Utc::now().to_rfc3339();
        config.save(&data_dir)?;

        Ok(Hip991TopicResponse {
            topic_id: topic_info.topic_id,
            fee_amount: topic_info.fee_amount,
            fee_amount_hbar: topic_info.fee_amount as f64 / 100_000_000.0,
            fee_collector: topic_info.fee_collector_account_id,
            transaction_id: None,
            memo: topic_info.memo,
        })
    }
}

/// Submit a content hash to the fee-bearing topic.
///
/// The submitter is automatically charged the topic's custom fee by Hedera.
/// This is called during the publish flow when content reaches L0.
#[tauri::command]
pub async fn submit_to_topic(
    content_hash: String,
    metadata: Option<String>,
    protocol: State<'_, Mutex<Option<ProtocolState>>>,
) -> Result<Hip991SubmitResponse, String> {
    let data_dir = resolve_data_dir(&protocol).await;
    let mut config = Hip991Config::load(&data_dir);

    let topic_id = config
        .active_topic_id
        .as_ref()
        .ok_or("No active fee-bearing topic. Create one first with create_fee_topic.")?
        .clone();

    if !config.has_credentials() {
        return Err("Hedera credentials not configured.".to_string());
    }

    // Build the message payload: content hash + optional metadata
    let message = if let Some(meta) = &metadata {
        serde_json::json!({
            "content_hash": content_hash,
            "metadata": meta,
            "timestamp": Utc::now().to_rfc3339(),
            "protocol": "nodalync",
            "version": "0.7.1"
        })
        .to_string()
    } else {
        serde_json::json!({
            "content_hash": content_hash,
            "timestamp": Utc::now().to_rfc3339(),
            "protocol": "nodalync",
            "version": "0.7.1"
        })
        .to_string()
    };

    #[cfg(not(feature = "hedera-sdk"))]
    {
        config.total_submissions += 1;
        config.updated_at = Utc::now().to_rfc3339();
        config.save(&data_dir)?;

        return Ok(Hip991SubmitResponse {
            transaction_id: format!("sim-{}", Utc::now().timestamp()),
            topic_id,
            fee_charged: config.fee_config.fee_amount,
            fee_charged_hbar: config.fee_config.fee_amount as f64 / 100_000_000.0,
            content_hash,
        });
    }

    #[cfg(feature = "hedera-sdk")]
    {
        use nodalync_settle::topic::TopicFeeManager;

        let hedera_config = build_hedera_config(&config)?;
        let manager = TopicFeeManager::new(&hedera_config)
            .await
            .map_err(|e| format!("Failed to initialize Hedera client: {}", e))?;

        let tx_id = manager
            .submit_message(&topic_id, message.as_bytes())
            .await
            .map_err(|e| format!("Failed to submit to topic: {}", e))?;

        config.total_submissions += 1;
        config.updated_at = Utc::now().to_rfc3339();
        config.save(&data_dir)?;

        Ok(Hip991SubmitResponse {
            transaction_id: tx_id.as_str().to_string(),
            topic_id,
            fee_charged: config.fee_config.fee_amount,
            fee_charged_hbar: config.fee_config.fee_amount as f64 / 100_000_000.0,
            content_hash,
        })
    }
}

/// Get revenue collected from the fee-bearing topic.
///
/// Queries the Hedera Mirror Node to calculate total fees collected.
#[tauri::command]
pub async fn get_topic_revenue(
    limit: Option<u32>,
    protocol: State<'_, Mutex<Option<ProtocolState>>>,
) -> Result<RevenueSummary, String> {
    let data_dir = resolve_data_dir(&protocol).await;
    let config = Hip991Config::load(&data_dir);

    let topic_id = config
        .active_topic_id
        .as_ref()
        .ok_or("No active fee-bearing topic.")?
        .clone();

    let limit = limit.unwrap_or(100);

    #[cfg(not(feature = "hedera-sdk"))]
    {
        return Ok(RevenueSummary {
            total_revenue: config.total_submissions * config.fee_config.fee_amount,
            total_revenue_hbar: (config.total_submissions * config.fee_config.fee_amount) as f64
                / 100_000_000.0,
            message_count: config.total_submissions,
            avg_fee_per_message: config.fee_config.fee_amount,
            records: Vec::new(),
        });
    }

    #[cfg(feature = "hedera-sdk")]
    {
        use nodalync_settle::topic::TopicFeeManager;

        let hedera_config = build_hedera_config(&config)?;
        let manager = TopicFeeManager::new(&hedera_config)
            .await
            .map_err(|e| format!("Failed to initialize Hedera client: {}", e))?;

        manager
            .get_revenue(&topic_id, limit)
            .await
            .map_err(|e| format!("Failed to query revenue: {}", e))
    }
}

/// Get topic info from the Mirror Node.
#[tauri::command]
pub async fn get_topic_details(
    topic_id: Option<String>,
    protocol: State<'_, Mutex<Option<ProtocolState>>>,
) -> Result<TopicInfo, String> {
    let data_dir = resolve_data_dir(&protocol).await;
    let config = Hip991Config::load(&data_dir);

    let topic_id = topic_id
        .or(config.active_topic_id)
        .ok_or("No topic ID specified and no active topic.")?;

    #[cfg(not(feature = "hedera-sdk"))]
    {
        return Ok(TopicInfo {
            topic_id: topic_id.clone(),
            fee_amount: config.fee_config.fee_amount,
            denominating_token: None,
            fee_collector_account_id: config.fee_config.fee_collector_account_id,
            memo: config.fee_config.topic_memo,
            created_at: config.updated_at,
        });
    }

    #[cfg(feature = "hedera-sdk")]
    {
        use nodalync_settle::topic::TopicFeeManager;

        let hedera_config = build_hedera_config(&config)?;
        let manager = TopicFeeManager::new(&hedera_config)
            .await
            .map_err(|e| format!("Failed to initialize Hedera client: {}", e))?;

        manager
            .get_topic_info(&topic_id)
            .await
            .map_err(|e| format!("Failed to get topic info: {}", e))
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Resolve the data directory from protocol state.
async fn resolve_data_dir(protocol: &State<'_, Mutex<Option<ProtocolState>>>) -> PathBuf {
    let guard = protocol.lock().await;
    match guard.as_ref() {
        Some(state) => state.data_dir.clone(),
        None => ProtocolState::default_data_dir(),
    }
}

/// Build a HederaConfig from the persisted Hip991Config.
#[cfg(feature = "hedera-sdk")]
fn build_hedera_config(
    config: &Hip991Config,
) -> Result<nodalync_settle::HederaConfig, String> {
    use nodalync_settle::HederaNetwork;

    let network = match config.hedera_network.as_str() {
        "mainnet" => HederaNetwork::Mainnet,
        "testnet" => HederaNetwork::Testnet,
        "previewnet" => HederaNetwork::Previewnet,
        other => return Err(format!("Unknown Hedera network: {}", other)),
    };

    Ok(nodalync_settle::HederaConfig {
        network,
        account_id: config.hedera_account_id.clone(),
        private_key_path: PathBuf::from(&config.hedera_key_path),
        contract_id: "0.0.0".to_string(), // Not used for topic operations
        gas: Default::default(),
        retry: Default::default(),
    })
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hip991_config_default() {
        let config = Hip991Config::default();
        assert!(config.active_topic_id.is_none());
        assert!(!config.has_credentials());
        assert_eq!(config.hedera_network, "testnet");
        assert_eq!(config.total_submissions, 0);
    }

    #[test]
    fn test_hip991_config_has_credentials() {
        let mut config = Hip991Config::default();
        assert!(!config.has_credentials());

        config.hedera_account_id = "0.0.7703962".to_string();
        assert!(!config.has_credentials()); // still needs key path

        config.hedera_key_path = "/path/to/key".to_string();
        assert!(config.has_credentials());
    }

    #[test]
    fn test_hip991_config_roundtrip() {
        let dir = std::env::temp_dir().join("nodalync-hip991-test");
        let _ = std::fs::remove_dir_all(&dir);

        let mut config = Hip991Config::default();
        config.hedera_account_id = "0.0.7703962".to_string();
        config.hedera_key_path = "/tmp/key.pem".to_string();
        config.active_topic_id = Some("0.0.99999".to_string());
        config.total_submissions = 42;
        config.save(&dir).unwrap();

        let loaded = Hip991Config::load(&dir);
        assert_eq!(loaded.hedera_account_id, "0.0.7703962");
        assert_eq!(loaded.active_topic_id, Some("0.0.99999".to_string()));
        assert_eq!(loaded.total_submissions, 42);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_hip991_config_load_missing() {
        let dir = std::env::temp_dir().join("nodalync-hip991-missing");
        let _ = std::fs::remove_dir_all(&dir);

        let config = Hip991Config::load(&dir);
        assert!(config.active_topic_id.is_none());
        assert!(!config.has_credentials());
    }
}
