//! Tauri IPC commands for application-level fee management.
//!
//! This module implements Studio's business model: a configurable app fee
//! applied on top of protocol-level content prices when queries flow through
//! the desktop app.
//!
//! **Key distinction**: The protocol's 5% synthesis fee is baked into
//! `nodalync-econ` and goes to content creators. The app fee here is
//! *application-level* — it's what makes Studio a business. Other apps
//! building on the Nodalync protocol can set their own fee rates.
//!
//! Data is persisted to `{data_dir}/studio/fee_config.json` and
//! `{data_dir}/studio/transactions.json`.

use std::path::PathBuf;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tauri::State;
use tokio::sync::Mutex;
use tracing::info;

use crate::protocol::ProtocolState;

// ─── Constants ───────────────────────────────────────────────────────────────

/// Default app fee rate (5%).
const DEFAULT_FEE_RATE: f64 = 0.05;

/// Minimum allowed fee rate (0% — free).
const MIN_FEE_RATE: f64 = 0.0;

/// Maximum allowed fee rate (50% — sanity cap).
const MAX_FEE_RATE: f64 = 0.50;

/// Maximum transactions to return in a single query.
const MAX_TRANSACTION_LIMIT: u32 = 500;

// ─── Persisted Types ─────────────────────────────────────────────────────────

/// Application fee configuration, persisted to disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeeConfig {
    /// Fee rate as a fraction (0.05 = 5%).
    pub rate: f64,
    /// Total app fees collected (tinybars).
    pub total_collected: u64,
    /// Number of transactions processed.
    pub transaction_count: u64,
    /// Last modified timestamp.
    pub updated_at: String,
}

impl Default for FeeConfig {
    fn default() -> Self {
        Self {
            rate: DEFAULT_FEE_RATE,
            total_collected: 0,
            transaction_count: 0,
            updated_at: Utc::now().to_rfc3339(),
        }
    }
}

impl FeeConfig {
    /// Load from disk, returning default if missing or corrupt.
    pub fn load(data_dir: &PathBuf) -> Self {
        let path = Self::config_path(data_dir);
        match std::fs::read_to_string(&path) {
            Ok(json) => serde_json::from_str(&json).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    /// Save to disk.
    fn save(&self, data_dir: &PathBuf) -> Result<(), String> {
        let dir = data_dir.join("studio");
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("Failed to create studio dir: {}", e))?;
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize fee config: {}", e))?;
        std::fs::write(Self::config_path(data_dir), json)
            .map_err(|e| format!("Failed to write fee config: {}", e))?;
        Ok(())
    }

    fn config_path(data_dir: &PathBuf) -> PathBuf {
        data_dir.join("studio").join("fee_config.json")
    }
}

/// A single transaction record with fee breakdown.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionRecord {
    /// Unique transaction ID (UUID v4).
    pub id: String,
    /// Content hash that was queried.
    pub content_hash: String,
    /// Content title (for display).
    pub content_title: String,
    /// Base content price (tinybars) — what the creator set.
    pub content_cost: u64,
    /// App fee amount (tinybars) — Studio's cut.
    pub app_fee: u64,
    /// Total paid by the user (content_cost + app_fee).
    pub total: u64,
    /// Fee rate at time of transaction.
    pub fee_rate: f64,
    /// Who received the content payment (peer ID).
    pub recipient: String,
    /// Transaction status.
    pub status: TransactionStatus,
    /// ISO-8601 timestamp.
    pub created_at: String,
}

/// Transaction status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TransactionStatus {
    /// Payment sent, awaiting settlement.
    Pending,
    /// Settled on-chain.
    Settled,
    /// Failed (content unavailable, network error, etc.).
    Failed,
    /// Free query (no payment required — own content or price=0).
    Free,
}

/// Transaction log, persisted to disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct TransactionLog {
    transactions: Vec<TransactionRecord>,
}

impl Default for TransactionLog {
    fn default() -> Self {
        Self {
            transactions: Vec::new(),
        }
    }
}

impl TransactionLog {
    fn load(data_dir: &PathBuf) -> Self {
        let path = Self::log_path(data_dir);
        match std::fs::read_to_string(&path) {
            Ok(json) => serde_json::from_str(&json).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    fn save(&self, data_dir: &PathBuf) -> Result<(), String> {
        let dir = data_dir.join("studio");
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("Failed to create studio dir: {}", e))?;
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize transaction log: {}", e))?;
        std::fs::write(Self::log_path(data_dir), json)
            .map_err(|e| format!("Failed to write transaction log: {}", e))?;
        Ok(())
    }

    fn log_path(data_dir: &PathBuf) -> PathBuf {
        data_dir.join("studio").join("transactions.json")
    }
}

// ─── Fee Calculation ─────────────────────────────────────────────────────────

/// Calculate the app fee for a given content price.
///
/// Returns (app_fee, total) in tinybars.
pub fn calculate_app_fee(content_price: u64, fee_rate: f64) -> (u64, u64) {
    if content_price == 0 || fee_rate <= 0.0 {
        return (0, content_price);
    }
    let fee = (content_price as f64 * fee_rate).round() as u64;
    (fee, content_price + fee)
}

// ─── Public API: Record Transaction ──────────────────────────────────────────

/// Record a transaction (called internally from query_content).
///
/// This is NOT a Tauri command — it's used by the query flow.
pub fn record_transaction(
    data_dir: &PathBuf,
    content_hash: &str,
    content_title: &str,
    content_cost: u64,
    app_fee: u64,
    recipient: &str,
    status: TransactionStatus,
) -> Result<TransactionRecord, String> {
    let record = TransactionRecord {
        id: uuid_v4(),
        content_hash: content_hash.to_string(),
        content_title: content_title.to_string(),
        content_cost,
        app_fee,
        total: content_cost + app_fee,
        fee_rate: if content_cost > 0 {
            app_fee as f64 / content_cost as f64
        } else {
            0.0
        },
        recipient: recipient.to_string(),
        status: status.clone(),
        created_at: Utc::now().to_rfc3339(),
    };

    // Append to log
    let mut log = TransactionLog::load(data_dir);
    log.transactions.push(record.clone());
    log.save(data_dir)?;

    // Update fee config totals
    let mut config = FeeConfig::load(data_dir);
    config.total_collected += app_fee;
    config.transaction_count += 1;
    config.updated_at = Utc::now().to_rfc3339();
    config.save(data_dir)?;

    info!(
        "Recorded transaction {}: content_cost={}, app_fee={}, total={}, status={:?}",
        record.id, content_cost, app_fee, record.total, status
    );

    Ok(record)
}

// ─── Tauri IPC Commands ──────────────────────────────────────────────────────

/// Response type for get_fee_config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeeConfigResponse {
    /// Current fee rate as percentage (e.g. 5.0 for 5%).
    pub rate_percent: f64,
    /// Current fee rate as fraction (e.g. 0.05 for 5%).
    pub rate: f64,
    /// Total app fees collected (tinybars).
    pub total_collected: u64,
    /// Total app fees collected in HBAR (display-friendly).
    pub total_collected_hbar: f64,
    /// Number of transactions processed.
    pub transaction_count: u64,
    /// Average fee per transaction (tinybars).
    pub avg_fee_per_transaction: u64,
    /// Last updated timestamp.
    pub updated_at: String,
}

/// Get the current fee configuration and summary stats.
///
/// Returns the fee rate, total collected, transaction count, and averages.
/// Works even if node is not initialized (reads from disk).
#[tauri::command]
pub async fn get_fee_config(
    protocol: State<'_, Mutex<Option<ProtocolState>>>,
) -> Result<FeeConfigResponse, String> {
    let data_dir = resolve_data_dir(&protocol).await;
    let config = FeeConfig::load(&data_dir);

    let avg_fee = if config.transaction_count > 0 {
        config.total_collected / config.transaction_count
    } else {
        0
    };

    Ok(FeeConfigResponse {
        rate_percent: config.rate * 100.0,
        rate: config.rate,
        total_collected: config.total_collected,
        total_collected_hbar: config.total_collected as f64 / 100_000_000.0,
        transaction_count: config.transaction_count,
        avg_fee_per_transaction: avg_fee,
        updated_at: config.updated_at,
    })
}

/// Set the application fee rate.
///
/// Rate is specified as a percentage (e.g. 5.0 for 5%).
/// Valid range: 0% to 50%.
#[tauri::command]
pub async fn set_fee_rate(
    rate_percent: f64,
    protocol: State<'_, Mutex<Option<ProtocolState>>>,
) -> Result<FeeConfigResponse, String> {
    // Validate
    let rate = rate_percent / 100.0;
    if rate < MIN_FEE_RATE || rate > MAX_FEE_RATE {
        return Err(format!(
            "Fee rate must be between {}% and {}%. Got: {}%",
            MIN_FEE_RATE * 100.0,
            MAX_FEE_RATE * 100.0,
            rate_percent
        ));
    }

    let data_dir = resolve_data_dir(&protocol).await;
    let mut config = FeeConfig::load(&data_dir);
    config.rate = rate;
    config.updated_at = Utc::now().to_rfc3339();
    config.save(&data_dir)?;

    info!("Fee rate updated to {}%", rate_percent);

    // Return updated config
    get_fee_config(protocol).await
}

/// Response type for transaction history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionHistoryResponse {
    /// Transactions (newest first).
    pub transactions: Vec<TransactionRecord>,
    /// Total number of transactions.
    pub total_count: usize,
    /// Summary: total content costs.
    pub total_content_cost: u64,
    /// Summary: total app fees.
    pub total_app_fees: u64,
    /// Summary: total amount (content + fees).
    pub total_amount: u64,
    /// Summary: total in HBAR.
    pub total_amount_hbar: f64,
    /// Summary: total app fees in HBAR.
    pub total_app_fees_hbar: f64,
}

/// Get transaction history with fee breakdown.
///
/// Returns transactions newest-first with summary statistics.
/// Supports pagination via `limit` and `offset`.
#[tauri::command]
pub async fn get_transaction_history(
    limit: Option<u32>,
    offset: Option<u32>,
    protocol: State<'_, Mutex<Option<ProtocolState>>>,
) -> Result<TransactionHistoryResponse, String> {
    let data_dir = resolve_data_dir(&protocol).await;
    let log = TransactionLog::load(&data_dir);

    let total_count = log.transactions.len();

    // Calculate summaries over ALL transactions
    let total_content_cost: u64 = log.transactions.iter().map(|t| t.content_cost).sum();
    let total_app_fees: u64 = log.transactions.iter().map(|t| t.app_fee).sum();
    let total_amount: u64 = log.transactions.iter().map(|t| t.total).sum();

    // Apply pagination (newest first)
    let limit = limit.unwrap_or(50).min(MAX_TRANSACTION_LIMIT) as usize;
    let offset = offset.unwrap_or(0) as usize;

    let mut transactions: Vec<TransactionRecord> = log.transactions;
    transactions.reverse(); // newest first
    let transactions: Vec<TransactionRecord> = transactions
        .into_iter()
        .skip(offset)
        .take(limit)
        .collect();

    Ok(TransactionHistoryResponse {
        transactions,
        total_count,
        total_content_cost,
        total_app_fees,
        total_amount,
        total_amount_hbar: total_amount as f64 / 100_000_000.0,
        total_app_fees_hbar: total_app_fees as f64 / 100_000_000.0,
    })
}

/// Get a fee quote for a content query before executing it.
///
/// Shows the user: content cost + app fee = total, before they commit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeeQuote {
    /// Base content price (tinybars).
    pub content_cost: u64,
    /// App fee (tinybars).
    pub app_fee: u64,
    /// Total the user will pay (tinybars).
    pub total: u64,
    /// Fee rate applied (percentage).
    pub fee_rate_percent: f64,
    /// Content cost in HBAR.
    pub content_cost_hbar: f64,
    /// App fee in HBAR.
    pub app_fee_hbar: f64,
    /// Total in HBAR.
    pub total_hbar: f64,
}

/// Get a fee quote for querying content at a given price.
///
/// Call this before `query_content` to show the user the full cost breakdown.
#[tauri::command]
pub async fn get_fee_quote(
    content_price: u64,
    protocol: State<'_, Mutex<Option<ProtocolState>>>,
) -> Result<FeeQuote, String> {
    let data_dir = resolve_data_dir(&protocol).await;
    let config = FeeConfig::load(&data_dir);

    let (app_fee, total) = calculate_app_fee(content_price, config.rate);

    Ok(FeeQuote {
        content_cost: content_price,
        app_fee,
        total,
        fee_rate_percent: config.rate * 100.0,
        content_cost_hbar: content_price as f64 / 100_000_000.0,
        app_fee_hbar: app_fee as f64 / 100_000_000.0,
        total_hbar: total as f64 / 100_000_000.0,
    })
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Resolve the data directory from protocol state or default.
async fn resolve_data_dir(protocol: &State<'_, Mutex<Option<ProtocolState>>>) -> PathBuf {
    let guard = protocol.lock().await;
    match guard.as_ref() {
        Some(state) => state.data_dir.clone(),
        None => ProtocolState::default_data_dir(),
    }
}

/// Generate a UUID v4 using rand (no extra dependency needed).
fn uuid_v4() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let mut bytes = [0u8; 16];
    rng.fill(&mut bytes);
    // Set version (4) and variant (RFC 4122)
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3],
        bytes[4], bytes[5],
        bytes[6], bytes[7],
        bytes[8], bytes[9],
        bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15]
    )
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_app_fee_default_rate() {
        // 5% fee on 1000 tinybars
        let (fee, total) = calculate_app_fee(1000, 0.05);
        assert_eq!(fee, 50);
        assert_eq!(total, 1050);
    }

    #[test]
    fn test_calculate_app_fee_zero_price() {
        let (fee, total) = calculate_app_fee(0, 0.05);
        assert_eq!(fee, 0);
        assert_eq!(total, 0);
    }

    #[test]
    fn test_calculate_app_fee_zero_rate() {
        let (fee, total) = calculate_app_fee(1000, 0.0);
        assert_eq!(fee, 0);
        assert_eq!(total, 1000);
    }

    #[test]
    fn test_calculate_app_fee_large_amount() {
        // 5% on 10 HBAR (1_000_000_000 tinybars)
        let (fee, total) = calculate_app_fee(1_000_000_000, 0.05);
        assert_eq!(fee, 50_000_000);
        assert_eq!(total, 1_050_000_000);
    }

    #[test]
    fn test_calculate_app_fee_rounding() {
        // 5% of 33 = 1.65, rounds to 2
        let (fee, total) = calculate_app_fee(33, 0.05);
        assert_eq!(fee, 2);
        assert_eq!(total, 35);
    }

    #[test]
    fn test_calculate_app_fee_custom_rate() {
        // 10% fee
        let (fee, total) = calculate_app_fee(1000, 0.10);
        assert_eq!(fee, 100);
        assert_eq!(total, 1100);
    }

    #[test]
    fn test_uuid_v4_format() {
        let id = uuid_v4();
        assert_eq!(id.len(), 36);
        assert_eq!(&id[8..9], "-");
        assert_eq!(&id[13..14], "-");
        assert_eq!(&id[18..19], "-");
        assert_eq!(&id[23..24], "-");
        // Version nibble should be '4'
        assert_eq!(&id[14..15], "4");
    }

    #[test]
    fn test_uuid_v4_unique() {
        let a = uuid_v4();
        let b = uuid_v4();
        assert_ne!(a, b);
    }

    #[test]
    fn test_fee_config_default() {
        let config = FeeConfig::default();
        assert!((config.rate - 0.05).abs() < f64::EPSILON);
        assert_eq!(config.total_collected, 0);
        assert_eq!(config.transaction_count, 0);
    }

    #[test]
    fn test_fee_config_roundtrip() {
        let dir = std::env::temp_dir().join("nodalync-fee-test");
        let _ = std::fs::remove_dir_all(&dir);

        let mut config = FeeConfig::default();
        config.rate = 0.07;
        config.total_collected = 12345;
        config.transaction_count = 3;
        config.save(&dir).unwrap();

        let loaded = FeeConfig::load(&dir);
        assert!((loaded.rate - 0.07).abs() < f64::EPSILON);
        assert_eq!(loaded.total_collected, 12345);
        assert_eq!(loaded.transaction_count, 3);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_transaction_log_roundtrip() {
        let dir = std::env::temp_dir().join("nodalync-txlog-test");
        let _ = std::fs::remove_dir_all(&dir);

        // Initialize fee config first
        let config = FeeConfig::default();
        config.save(&dir).unwrap();

        let result = record_transaction(
            &dir,
            "abc123",
            "Test Content",
            1000,
            50,
            "peer123",
            TransactionStatus::Settled,
        );
        assert!(result.is_ok());

        let log = TransactionLog::load(&dir);
        assert_eq!(log.transactions.len(), 1);
        assert_eq!(log.transactions[0].content_cost, 1000);
        assert_eq!(log.transactions[0].app_fee, 50);
        assert_eq!(log.transactions[0].total, 1050);

        // Config should reflect the transaction
        let config = FeeConfig::load(&dir);
        assert_eq!(config.total_collected, 50);
        assert_eq!(config.transaction_count, 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_record_free_transaction() {
        let dir = std::env::temp_dir().join("nodalync-free-tx-test");
        let _ = std::fs::remove_dir_all(&dir);

        let config = FeeConfig::default();
        config.save(&dir).unwrap();

        let result = record_transaction(
            &dir,
            "abc",
            "Free Content",
            0,
            0,
            "peer",
            TransactionStatus::Free,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap().status, TransactionStatus::Free);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
