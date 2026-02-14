//! Tauri IPC commands for x402 payment integration.
//!
//! Exposes x402 configuration, status, and payment flow to the desktop UI.
//! The PaymentGate is shared state — initialized when the node starts
//! and used by query commands to gate paid content access.

use serde::{Deserialize, Serialize};
use tauri::State;
use tokio::sync::Mutex;
use tracing::info;

use nodalync_x402::{PaymentGate, X402Config, X402Status, TransactionRecord};

use crate::protocol::ProtocolState;

// ─── Shared State ────────────────────────────────────────────────────────────

/// Wrapper for the shared PaymentGate state.
pub type SharedPaymentGate = Mutex<PaymentGate>;

/// Create the initial (disabled) payment gate for Tauri state.
pub fn new_payment_gate() -> SharedPaymentGate {
    Mutex::new(PaymentGate::disabled())
}

// ─── IPC Types ───────────────────────────────────────────────────────────────

/// Input for configuring x402.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct X402ConfigInput {
    /// Whether to enable x402 payments.
    pub enabled: bool,
    /// Hedera network: "hedera:testnet" or "hedera:mainnet".
    pub network: Option<String>,
    /// Facilitator URL.
    pub facilitator_url: Option<String>,
    /// Our Hedera account ID for receiving payments.
    pub account_id: String,
    /// Asset to accept (default: "HBAR").
    pub asset: Option<String>,
    /// Application fee percentage (0-50).
    pub app_fee_percent: Option<u8>,
}

/// Detailed x402 status for the UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct X402StatusResponse {
    /// Core status from the gate.
    #[serde(flatten)]
    pub status: X402Status,
    /// Total volume in HBAR (display-friendly).
    pub total_volume_hbar: f64,
    /// Total fees in HBAR (display-friendly).
    pub total_app_fees_hbar: f64,
    /// Average transaction size in tinybars.
    pub avg_transaction_size: u64,
}

// ─── Tauri IPC Commands ──────────────────────────────────────────────────────

/// Get the current x402 payment status.
///
/// Returns whether x402 is enabled, configuration details,
/// and aggregate transaction statistics.
#[tauri::command]
pub async fn get_x402_status(
    gate: State<'_, SharedPaymentGate>,
) -> Result<X402StatusResponse, String> {
    let gate = gate.lock().await;
    let status = gate.status().await;

    let avg_tx = if status.total_transactions > 0 {
        status.total_volume / status.total_transactions as u64
    } else {
        0
    };

    Ok(X402StatusResponse {
        total_volume_hbar: status.total_volume as f64 / 100_000_000.0,
        total_app_fees_hbar: status.total_app_fees as f64 / 100_000_000.0,
        avg_transaction_size: avg_tx,
        status,
    })
}

/// Configure x402 payments.
///
/// Enables or updates x402 payment settings. Takes effect immediately.
/// Persists config to `{data_dir}/studio/x402_config.json`.
#[tauri::command]
pub async fn configure_x402(
    input: X402ConfigInput,
    gate: State<'_, SharedPaymentGate>,
    protocol: State<'_, Mutex<Option<ProtocolState>>>,
) -> Result<X402StatusResponse, String> {
    // Validate fee rate
    if let Some(fee) = input.app_fee_percent {
        if fee > 50 {
            return Err("App fee percentage must be 0-50".to_string());
        }
    }

    let config = X402Config {
        enabled: input.enabled,
        network: input
            .network
            .unwrap_or_else(|| "hedera:testnet".to_string()),
        facilitator_url: input
            .facilitator_url
            .unwrap_or_else(|| "https://api.testnet.blocky402.com/v1".to_string()),
        account_id: input.account_id.clone(),
        asset: input.asset.unwrap_or_else(|| "HBAR".to_string()),
        app_fee_percent: input.app_fee_percent.unwrap_or(5),
        max_timeout_seconds: 300,
        auto_settle: true,
    };

    // Persist config to disk
    let data_dir = resolve_data_dir(&protocol).await;
    let config_dir = data_dir.join("studio");
    std::fs::create_dir_all(&config_dir)
        .map_err(|e| format!("Failed to create config dir: {}", e))?;
    let config_json = serde_json::to_string_pretty(&config)
        .map_err(|e| format!("Failed to serialize x402 config: {}", e))?;
    std::fs::write(config_dir.join("x402_config.json"), config_json)
        .map_err(|e| format!("Failed to write x402 config: {}", e))?;

    // Create new gate with updated config
    let new_gate = PaymentGate::new(config)
        .map_err(|e| format!("Failed to create payment gate: {}", e))?;

    // Swap the gate
    let mut gate_lock = gate.lock().await;
    *gate_lock = new_gate;

    info!(
        enabled = input.enabled,
        account_id = %input.account_id,
        "x402 configuration updated"
    );

    // Return updated status
    let status = gate_lock.status().await;
    let avg_tx = if status.total_transactions > 0 {
        status.total_volume / status.total_transactions as u64
    } else {
        0
    };

    Ok(X402StatusResponse {
        total_volume_hbar: status.total_volume as f64 / 100_000_000.0,
        total_app_fees_hbar: status.total_app_fees as f64 / 100_000_000.0,
        avg_transaction_size: avg_tx,
        status,
    })
}

/// Get x402 transaction history.
///
/// Returns all x402 payment transactions with settlement details.
#[tauri::command]
pub async fn get_x402_transactions(
    limit: Option<usize>,
    gate: State<'_, SharedPaymentGate>,
) -> Result<Vec<TransactionRecord>, String> {
    let gate = gate.lock().await;
    let mut transactions = gate.get_transactions().await;

    // Newest first
    transactions.reverse();

    // Apply limit
    if let Some(limit) = limit {
        transactions.truncate(limit);
    }

    Ok(transactions)
}

/// Check if x402 is supported by the configured facilitator.
///
/// Queries the facilitator's /supported endpoint to verify network compatibility.
#[tauri::command]
pub async fn check_x402_facilitator(
    gate: State<'_, SharedPaymentGate>,
) -> Result<FacilitatorCheckResponse, String> {
    let gate = gate.lock().await;

    if !gate.is_enabled() {
        return Ok(FacilitatorCheckResponse {
            available: false,
            supports_hedera: false,
            facilitator_url: String::new(),
            supported_networks: Vec::new(),
            error: Some("x402 is not enabled".to_string()),
        });
    }

    let config = gate.config();
    let client = nodalync_x402::FacilitatorClient::from_config(config)
        .map_err(|e| format!("Failed to create facilitator client: {}", e))?;

    match client.get_supported().await {
        Ok(supported) => {
            let networks: Vec<String> = supported
                .kinds
                .iter()
                .map(|k| format!("{}:{}", k.scheme, k.network))
                .collect();

            let supports_hedera = supported
                .kinds
                .iter()
                .any(|k| k.network.contains("hedera"));

            Ok(FacilitatorCheckResponse {
                available: true,
                supports_hedera,
                facilitator_url: config.facilitator_url.clone(),
                supported_networks: networks,
                error: None,
            })
        }
        Err(e) => Ok(FacilitatorCheckResponse {
            available: false,
            supports_hedera: false,
            facilitator_url: config.facilitator_url.clone(),
            supported_networks: Vec::new(),
            error: Some(format!("Facilitator check failed: {}", e)),
        }),
    }
}

/// Response from facilitator availability check.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FacilitatorCheckResponse {
    /// Whether the facilitator is reachable.
    pub available: bool,
    /// Whether the facilitator supports Hedera.
    pub supports_hedera: bool,
    /// Facilitator URL that was checked.
    pub facilitator_url: String,
    /// List of supported scheme:network combinations.
    pub supported_networks: Vec<String>,
    /// Error message if check failed.
    pub error: Option<String>,
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Resolve the data directory from protocol state or default.
async fn resolve_data_dir(
    protocol: &State<'_, Mutex<Option<ProtocolState>>>,
) -> std::path::PathBuf {
    let guard = protocol.lock().await;
    match guard.as_ref() {
        Some(state) => state.data_dir.clone(),
        None => ProtocolState::default_data_dir(),
    }
}

/// Load x402 config from disk (called on app startup).
pub fn load_x402_config(data_dir: &std::path::Path) -> X402Config {
    let config_path = data_dir.join("studio").join("x402_config.json");
    match std::fs::read_to_string(&config_path) {
        Ok(json) => serde_json::from_str(&json).unwrap_or_default(),
        Err(_) => X402Config::default(),
    }
}
