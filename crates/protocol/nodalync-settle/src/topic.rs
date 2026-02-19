//! HIP-991 Topic Fee Management for Nodalync.
//!
//! This module implements Hedera's HIP-991 "Permissionless Revenue-Generating
//! Topic IDs for Topic Operators" for the Nodalync protocol. It enables
//! native on-chain fee collection when content is published to HCS topics.
//!
//! # Architecture
//!
//! ```text
//! Creator publishes content
//!        │
//!        ▼
//! ┌──────────────────┐     ┌─────────────────────────────┐
//! │ TopicFeeManager  │────►│ HCS Topic (with custom fee) │
//! │                  │     │  - fee_schedule_key          │
//! │  create_topic()  │     │  - custom_fees: [FixedFee]  │
//! │  submit()        │     │  - fee_exempt_keys           │
//! │  get_revenue()   │     └─────────────────────────────┘
//! └──────────────────┘                   │
//!                                        ▼
//!                              Hedera auto-collects fee
//!                              from submitter → collector
//! ```
//!
//! # Fee Flow
//!
//! 1. Studio creates an HCS topic with a `custom_fee` via [`TopicFeeManager::create_topic`]
//! 2. When a user publishes knowledge, [`TopicFeeManager::submit_message`] sends the
//!    content hash to the topic — Hedera natively charges the custom fee
//! 3. Revenue is queried via Mirror Node in [`TopicFeeManager::get_revenue`]

use serde::{Deserialize, Serialize};
#[cfg(feature = "hedera-sdk")]
use tracing::{debug, info};

use crate::error::SettleResult;
use crate::types::TransactionId;

// ─── Configuration ───────────────────────────────────────────────────────────

/// Configuration for a fee-bearing HCS topic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicFeeConfig {
    /// Fee amount in tinybars (or token units if denominating_token is set).
    /// Default: 1_000_000 (0.01 HBAR) for testnet demo.
    pub fee_amount: u64,

    /// Optional HTS token ID for fee denomination. If None, fees are in HBAR.
    pub denominating_token: Option<String>,

    /// Account ID that collects the fees (the Studio operator).
    pub fee_collector_account_id: String,

    /// Topic memo (publicly visible).
    pub topic_memo: String,
}

impl Default for TopicFeeConfig {
    fn default() -> Self {
        Self {
            fee_amount: 1_000_000, // 0.01 HBAR
            denominating_token: None,
            topic_memo: "Nodalync Studio Knowledge Topic".to_string(),
            fee_collector_account_id: String::new(),
        }
    }
}

/// Information about a created fee-bearing topic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicInfo {
    /// The topic ID (e.g., "0.0.12345").
    pub topic_id: String,

    /// The fee amount configured on the topic.
    pub fee_amount: u64,

    /// Fee denomination (None = HBAR).
    pub denominating_token: Option<String>,

    /// Fee collector account.
    pub fee_collector_account_id: String,

    /// Topic memo.
    pub memo: String,

    /// Creation timestamp (ISO-8601).
    pub created_at: String,
}

/// A revenue record from Mirror Node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevenueRecord {
    /// Transaction ID.
    pub transaction_id: String,

    /// Amount collected (tinybars or token units).
    pub amount: u64,

    /// Payer account ID.
    pub payer_account_id: String,

    /// Consensus timestamp.
    pub consensus_timestamp: String,
}

/// Revenue summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevenueSummary {
    /// Total revenue collected.
    pub total_revenue: u64,

    /// Total in HBAR (if denominated in HBAR).
    pub total_revenue_hbar: f64,

    /// Number of messages submitted (= number of fee collections).
    pub message_count: u64,

    /// Average fee per message.
    pub avg_fee_per_message: u64,

    /// Individual records (most recent first).
    pub records: Vec<RevenueRecord>,
}

// ─── Topic Fee Manager (Hedera SDK implementation) ───────────────────────────

#[cfg(feature = "hedera-sdk")]
mod manager {
    use std::str::FromStr;

    use hiero_sdk::{
        AccountId as HederaAccountId, Client, CustomFixedFee, PrivateKey,
        TopicCreateTransaction, TopicId, TopicMessageSubmitTransaction,
    };

    use super::*;
    use crate::config::{HederaConfig, HederaNetwork};
    use crate::error::SettleError;
    use crate::retry::RetryPolicy;

    /// Manages HCS topics with HIP-991 native fees.
    ///
    /// This is the core D2 deliverable: on-chain fee collection for knowledge
    /// publication, powered entirely by Hedera's native topic fee mechanism.
    pub struct TopicFeeManager {
        /// Hedera SDK client.
        client: Client,

        /// Operator account ID.
        operator_id: HederaAccountId,

        /// Operator private key (needed for signing topic creation).
        operator_key: PrivateKey,

        /// Mirror Node base URL.
        mirror_node_url: String,

        /// Retry policy.
        retry_policy: RetryPolicy,
    }

    impl TopicFeeManager {
        /// Create a new TopicFeeManager from Hedera config.
        pub async fn new(config: &HederaConfig) -> SettleResult<Self> {
            let key_bytes = std::fs::read_to_string(&config.private_key_path)?;
            let private_key = PrivateKey::from_str(key_bytes.trim())
                .map_err(|e| SettleError::config(format!("invalid private key: {}", e)))?;

            let operator_id = HederaAccountId::from_str(&config.account_id).map_err(|e| {
                SettleError::InvalidAccountId(format!("{}: {}", config.account_id, e))
            })?;

            let client = match config.network {
                HederaNetwork::Mainnet => Client::for_mainnet(),
                HederaNetwork::Testnet => Client::for_testnet(),
                HederaNetwork::Previewnet => Client::for_previewnet(),
            };

            client.set_operator(operator_id, private_key.clone());

            let mirror_node_url = config.network.mirror_node_url().to_string();

            info!(
                network = %config.network,
                operator = %config.account_id,
                "TopicFeeManager initialized"
            );

            Ok(Self {
                client,
                operator_id,
                operator_key: private_key,
                mirror_node_url,
                retry_policy: RetryPolicy::from_config(&config.retry),
            })
        }

        /// Create an HCS topic with HIP-991 custom fees.
        ///
        /// This is the core operation: creates a topic where every message
        /// submission automatically charges the configured fee, transferred
        /// natively by Hedera from submitter → fee collector.
        ///
        /// # Arguments
        /// * `fee_config` - Fee configuration (amount, collector, memo)
        ///
        /// # Returns
        /// * `TopicInfo` with the created topic ID and configuration
        pub async fn create_topic(&self, fee_config: &TopicFeeConfig) -> SettleResult<TopicInfo> {
            let fee_collector = HederaAccountId::from_str(&fee_config.fee_collector_account_id)
                .map_err(|e| {
                    SettleError::InvalidAccountId(format!(
                        "fee collector {}: {}",
                        fee_config.fee_collector_account_id, e
                    ))
                })?;

            // Build the custom fee — HBAR denomination (None token = HBAR)
            let custom_fee = CustomFixedFee::new(
                fee_config.fee_amount,
                None, // None = HBAR denomination
                Some(fee_collector),
            );

            // The operator's public key serves as:
            // - admin_key: can update/delete the topic
            // - fee_schedule_key: can update fees later
            // - fee_exempt_key: Studio itself doesn't pay fees on internal ops
            let operator_public_key = self.operator_key.public_key();

            info!(
                fee_amount = fee_config.fee_amount,
                fee_collector = %fee_config.fee_collector_account_id,
                memo = %fee_config.topic_memo,
                "Creating HIP-991 fee-bearing topic"
            );

            let tx = self
                .retry_policy
                .execute(|| async {
                    let mut create_tx = TopicCreateTransaction::new();
                    create_tx
                        .topic_memo(&fee_config.topic_memo)
                        .admin_key(operator_public_key)
                        .fee_schedule_key(operator_public_key)
                        .add_fee_exempt_key(operator_public_key)
                        .add_custom_fee(custom_fee.clone())
                        .execute(&self.client)
                        .await
                        .map_err(crate::error::classify_sdk_error)
                })
                .await?;

            // Wait for receipt to get the topic ID
            let receipt = hiero_sdk::TransactionReceiptQuery::new()
                .transaction_id(tx.transaction_id)
                .execute(&self.client)
                .await
                .map_err(crate::error::classify_sdk_error)?;

            if receipt.status != hiero_sdk::Status::Success {
                return Err(SettleError::transaction_failed(format!(
                    "topic creation failed: {:?}",
                    receipt.status
                )));
            }

            let topic_id = receipt.topic_id.ok_or_else(|| {
                SettleError::internal("topic creation succeeded but no topic ID in receipt")
            })?;

            let now = chrono::Utc::now().to_rfc3339();

            let info = TopicInfo {
                topic_id: topic_id.to_string(),
                fee_amount: fee_config.fee_amount,
                denominating_token: fee_config.denominating_token.clone(),
                fee_collector_account_id: fee_config.fee_collector_account_id.clone(),
                memo: fee_config.topic_memo.clone(),
                created_at: now,
            };

            info!(
                topic_id = %info.topic_id,
                fee_amount = info.fee_amount,
                "HIP-991 fee-bearing topic created successfully"
            );

            Ok(info)
        }

        /// Submit a message to a fee-bearing topic.
        ///
        /// The submitter is automatically charged the topic's custom fee by
        /// Hedera. The fee is transferred from the transaction payer to the
        /// fee collector account — no custom settlement code needed.
        ///
        /// # Arguments
        /// * `topic_id` - The HCS topic ID (e.g., "0.0.12345")
        /// * `message` - The message bytes (typically a content hash or JSON metadata)
        ///
        /// # Returns
        /// * `TransactionId` of the submission
        pub async fn submit_message(
            &self,
            topic_id: &str,
            message: &[u8],
        ) -> SettleResult<TransactionId> {
            let topic = TopicId::from_str(topic_id).map_err(|e| {
                SettleError::config(format!("invalid topic ID {}: {}", topic_id, e))
            })?;

            debug!(
                topic_id = %topic_id,
                message_len = message.len(),
                "Submitting message to fee-bearing topic"
            );

            let tx = self
                .retry_policy
                .execute(|| async {
                    TopicMessageSubmitTransaction::new()
                        .topic_id(topic)
                        .message(message.to_vec())
                        .execute(&self.client)
                        .await
                        .map_err(crate::error::classify_sdk_error)
                })
                .await?;

            // Wait for receipt to confirm submission
            let receipt = hiero_sdk::TransactionReceiptQuery::new()
                .transaction_id(tx.transaction_id)
                .execute(&self.client)
                .await
                .map_err(crate::error::classify_sdk_error)?;

            if receipt.status != hiero_sdk::Status::Success {
                return Err(SettleError::transaction_failed(format!(
                    "message submission failed: {:?}",
                    receipt.status
                )));
            }

            let tx_id_str = tx.transaction_id.to_string();

            info!(
                topic_id = %topic_id,
                tx_id = %tx_id_str,
                sequence = ?receipt.topic_sequence_number,
                "Message submitted to fee-bearing topic"
            );

            Ok(TransactionId::new(tx_id_str))
        }

        /// Query topic info from the Mirror Node.
        ///
        /// Returns the topic's configuration including custom fees.
        pub async fn get_topic_info(&self, topic_id: &str) -> SettleResult<TopicInfo> {
            let url = format!("{}/api/v1/topics/{}", self.mirror_node_url, topic_id);

            let response = reqwest::get(&url)
                .await
                .map_err(|e| SettleError::network(format!("Mirror Node request failed: {}", e)))?;

            if !response.status().is_success() {
                return Err(SettleError::network(format!(
                    "Mirror Node returned status {} for topic {}",
                    response.status(),
                    topic_id
                )));
            }

            let body: serde_json::Value = response.json().await.map_err(|e| {
                SettleError::network(format!("Mirror Node response parse error: {}", e))
            })?;

            // Extract fee info from the Mirror Node response
            let memo = body["memo"].as_str().unwrap_or("").to_string();

            let mut fee_amount: u64 = 0;
            let mut fee_collector = String::new();
            let mut denominating_token = None;

            if let Some(custom_fees) = body["custom_fees"].as_array() {
                if let Some(first_fee) = custom_fees.first() {
                    fee_amount = first_fee["fixed_fee"]["amount"].as_u64().unwrap_or(0);

                    if let Some(token) = first_fee["fixed_fee"]["denominating_token_id"].as_str() {
                        if !token.is_empty() {
                            denominating_token = Some(token.to_string());
                        }
                    }

                    if let Some(collector) = first_fee["fee_collector_account_id"].as_str() {
                        fee_collector = collector.to_string();
                    }
                }
            }

            let created_at = body["created_timestamp"].as_str().unwrap_or("").to_string();

            Ok(TopicInfo {
                topic_id: topic_id.to_string(),
                fee_amount,
                denominating_token,
                fee_collector_account_id: fee_collector,
                memo,
                created_at,
            })
        }

        /// Query revenue collected from a fee-bearing topic via Mirror Node.
        ///
        /// Fetches messages submitted to the topic and calculates total
        /// revenue based on the configured fee amount.
        ///
        /// # Arguments
        /// * `topic_id` - The HCS topic ID
        /// * `limit` - Maximum number of records to return
        pub async fn get_revenue(
            &self,
            topic_id: &str,
            limit: u32,
        ) -> SettleResult<RevenueSummary> {
            // Query messages on the topic from Mirror Node
            let url = format!(
                "{}/api/v1/topics/{}/messages?order=desc&limit={}",
                self.mirror_node_url, topic_id, limit
            );

            let response = reqwest::get(&url)
                .await
                .map_err(|e| SettleError::network(format!("Mirror Node request failed: {}", e)))?;

            if !response.status().is_success() {
                return Err(SettleError::network(format!(
                    "Mirror Node returned status {} for topic {} messages",
                    response.status(),
                    topic_id
                )));
            }

            let body: serde_json::Value = response.json().await.map_err(|e| {
                SettleError::network(format!("Mirror Node response parse error: {}", e))
            })?;

            // Get the topic's fee amount for revenue calculation
            let topic_info = self.get_topic_info(topic_id).await?;
            let fee_per_message = topic_info.fee_amount;

            let mut records = Vec::new();
            let mut message_count: u64 = 0;

            if let Some(messages) = body["messages"].as_array() {
                for msg in messages {
                    message_count += 1;

                    let consensus_timestamp = msg["consensus_timestamp"]
                        .as_str()
                        .unwrap_or("")
                        .to_string();

                    let payer = msg["payer_account_id"].as_str().unwrap_or("").to_string();

                    records.push(RevenueRecord {
                        transaction_id: format!("{}@{}", payer, consensus_timestamp),
                        amount: fee_per_message,
                        payer_account_id: payer,
                        consensus_timestamp,
                    });
                }
            }

            let total_revenue = message_count * fee_per_message;
            let avg_fee = if message_count > 0 {
                total_revenue / message_count
            } else {
                0
            };

            Ok(RevenueSummary {
                total_revenue,
                total_revenue_hbar: total_revenue as f64 / 100_000_000.0,
                message_count,
                avg_fee_per_message: avg_fee,
                records,
            })
        }

        /// Get the operator account ID string.
        pub fn operator_account(&self) -> String {
            self.operator_id.to_string()
        }
    }
}

#[cfg(feature = "hedera-sdk")]
pub use manager::TopicFeeManager;

// ─── Mock implementation (no hedera-sdk) ─────────────────────────────────────

/// Mock topic fee manager for testing without Hedera SDK.
///
/// Records operations in memory for verification in tests.
#[cfg(not(feature = "hedera-sdk"))]
pub struct TopicFeeManager {
    operations: std::sync::Mutex<Vec<String>>,
}

#[cfg(not(feature = "hedera-sdk"))]
impl TopicFeeManager {
    /// Create a mock manager.
    pub fn new_mock() -> Self {
        Self {
            operations: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// Mock topic creation — returns a fake topic ID.
    pub async fn create_topic(&self, fee_config: &TopicFeeConfig) -> SettleResult<TopicInfo> {
        let mut ops = self.operations.lock().unwrap();
        ops.push(format!("create_topic(fee={})", fee_config.fee_amount));

        Ok(TopicInfo {
            topic_id: "0.0.99999".to_string(),
            fee_amount: fee_config.fee_amount,
            denominating_token: fee_config.denominating_token.clone(),
            fee_collector_account_id: fee_config.fee_collector_account_id.clone(),
            memo: fee_config.topic_memo.clone(),
            created_at: "2026-02-18T00:00:00Z".to_string(),
        })
    }

    /// Mock message submission.
    pub async fn submit_message(
        &self,
        topic_id: &str,
        message: &[u8],
    ) -> SettleResult<TransactionId> {
        let mut ops = self.operations.lock().unwrap();
        ops.push(format!(
            "submit_message(topic={}, len={})",
            topic_id,
            message.len()
        ));
        Ok(TransactionId::new(format!(
            "0.0.12345@{}.000",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
        )))
    }

    /// Mock revenue query.
    pub async fn get_revenue(&self, topic_id: &str, _limit: u32) -> SettleResult<RevenueSummary> {
        let mut ops = self.operations.lock().unwrap();
        ops.push(format!("get_revenue(topic={})", topic_id));

        Ok(RevenueSummary {
            total_revenue: 0,
            total_revenue_hbar: 0.0,
            message_count: 0,
            avg_fee_per_message: 0,
            records: Vec::new(),
        })
    }

    /// Get recorded operations (for test assertions).
    pub fn operations(&self) -> Vec<String> {
        self.operations.lock().unwrap().clone()
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_topic_fee_config_default() {
        let config = TopicFeeConfig::default();
        assert_eq!(config.fee_amount, 1_000_000); // 0.01 HBAR
        assert!(config.denominating_token.is_none());
        assert!(config.fee_collector_account_id.is_empty());
    }

    #[test]
    fn test_topic_fee_config_serialization() {
        let config = TopicFeeConfig {
            fee_amount: 5_000_000,
            denominating_token: None,
            fee_collector_account_id: "0.0.7703962".to_string(),
            topic_memo: "Test Topic".to_string(),
        };

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: TopicFeeConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.fee_amount, 5_000_000);
        assert_eq!(deserialized.fee_collector_account_id, "0.0.7703962");
    }

    #[test]
    fn test_topic_info_serialization() {
        let info = TopicInfo {
            topic_id: "0.0.12345".to_string(),
            fee_amount: 1_000_000,
            denominating_token: None,
            fee_collector_account_id: "0.0.7703962".to_string(),
            memo: "Nodalync Knowledge Topic".to_string(),
            created_at: "2026-02-18T20:00:00Z".to_string(),
        };

        let json = serde_json::to_string_pretty(&info).unwrap();
        assert!(json.contains("0.0.12345"));
        assert!(json.contains("1000000"));
    }

    #[test]
    fn test_revenue_summary_empty() {
        let summary = RevenueSummary {
            total_revenue: 0,
            total_revenue_hbar: 0.0,
            message_count: 0,
            avg_fee_per_message: 0,
            records: Vec::new(),
        };
        assert_eq!(summary.message_count, 0);
        assert_eq!(summary.total_revenue, 0);
    }

    #[test]
    fn test_revenue_summary_with_records() {
        let records = vec![
            RevenueRecord {
                transaction_id: "0.0.100@1234567890.000".to_string(),
                amount: 1_000_000,
                payer_account_id: "0.0.100".to_string(),
                consensus_timestamp: "1234567890.000000000".to_string(),
            },
            RevenueRecord {
                transaction_id: "0.0.200@1234567891.000".to_string(),
                amount: 1_000_000,
                payer_account_id: "0.0.200".to_string(),
                consensus_timestamp: "1234567891.000000000".to_string(),
            },
        ];

        let summary = RevenueSummary {
            total_revenue: 2_000_000,
            total_revenue_hbar: 0.02,
            message_count: 2,
            avg_fee_per_message: 1_000_000,
            records,
        };

        assert_eq!(summary.message_count, 2);
        assert_eq!(summary.total_revenue, 2_000_000);
        assert!((summary.total_revenue_hbar - 0.02).abs() < f64::EPSILON);
    }

    #[cfg(not(feature = "hedera-sdk"))]
    #[tokio::test]
    async fn test_mock_topic_creation() {
        let manager = TopicFeeManager::new_mock();
        let config = TopicFeeConfig {
            fee_amount: 1_000_000,
            denominating_token: None,
            fee_collector_account_id: "0.0.7703962".to_string(),
            topic_memo: "Test".to_string(),
        };

        let info = manager.create_topic(&config).await.unwrap();
        assert_eq!(info.topic_id, "0.0.99999");
        assert_eq!(info.fee_amount, 1_000_000);

        let ops = manager.operations();
        assert_eq!(ops.len(), 1);
        assert!(ops[0].contains("create_topic"));
    }

    #[cfg(not(feature = "hedera-sdk"))]
    #[tokio::test]
    async fn test_mock_submit_message() {
        let manager = TopicFeeManager::new_mock();
        let tx_id = manager
            .submit_message("0.0.99999", b"content-hash-abc")
            .await
            .unwrap();

        assert!(!tx_id.as_str().is_empty());

        let ops = manager.operations();
        assert_eq!(ops.len(), 1);
        assert!(ops[0].contains("submit_message"));
    }

    #[cfg(not(feature = "hedera-sdk"))]
    #[tokio::test]
    async fn test_mock_get_revenue() {
        let manager = TopicFeeManager::new_mock();
        let summary = manager.get_revenue("0.0.99999", 50).await.unwrap();
        assert_eq!(summary.message_count, 0);
        assert_eq!(summary.total_revenue, 0);
    }
}
