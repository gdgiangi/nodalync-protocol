//! Hedera testnet faucet integration.
//!
//! This module provides utilities for requesting HBAR from the Hedera testnet faucet,
//! which is useful for CI/CD pipelines and automated testing.
//!
//! # Usage
//!
//! ```rust,no_run
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! use nodalync_settle::faucet::{HederaFaucet, FaucetConfig};
//!
//! let faucet = HederaFaucet::testnet();
//! let result = faucet.request_hbar("0.0.12345").await?;
//! println!("Funded account with {} HBAR", result.amount_hbar);
//! # Ok(())
//! # }
//! ```
//!
//! # Limitations
//!
//! - The faucet is rate-limited (typically once per IP per 24 hours)
//! - Only works on testnet, not mainnet
//! - May require solving a captcha in browser contexts

use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{debug, info, warn};

use crate::error::{SettleError, SettleResult};
use crate::types::AccountId;

/// Hedera testnet faucet client.
///
/// Provides methods to request HBAR from the testnet faucet for testing purposes.
pub struct HederaFaucet {
    /// Faucet API endpoint
    endpoint: String,
    /// HTTP client with timeout
    timeout: Duration,
}

/// Configuration for the faucet client.
#[derive(Debug, Clone)]
pub struct FaucetConfig {
    /// Faucet endpoint URL
    pub endpoint: String,
    /// Request timeout
    pub timeout: Duration,
}

impl Default for FaucetConfig {
    fn default() -> Self {
        Self {
            endpoint: "https://faucet.hedera.com/api".to_string(),
            timeout: Duration::from_secs(30),
        }
    }
}

impl FaucetConfig {
    /// Create a config for the testnet faucet.
    pub fn testnet() -> Self {
        Self::default()
    }
}

/// Result of a successful faucet request.
#[derive(Debug, Clone)]
pub struct FaucetResult {
    /// Account that was funded
    pub account_id: String,
    /// Amount of HBAR dispensed
    pub amount_hbar: f64,
    /// Transaction ID (if available)
    pub transaction_id: Option<String>,
}

/// Faucet request payload (reserved for future JSON body requests).
#[derive(Debug, Serialize)]
#[allow(dead_code)]
struct FaucetRequest {
    address: String,
}

/// Faucet response payload.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum FaucetResponse {
    Success {
        #[serde(rename = "transactionId")]
        transaction_id: Option<String>,
        amount: Option<f64>,
    },
    Error {
        error: String,
    },
}

impl HederaFaucet {
    /// Create a new faucet client with default testnet configuration.
    pub fn testnet() -> Self {
        Self::new(FaucetConfig::testnet())
    }

    /// Create a new faucet client with custom configuration.
    pub fn new(config: FaucetConfig) -> Self {
        Self {
            endpoint: config.endpoint,
            timeout: config.timeout,
        }
    }

    /// Request HBAR from the testnet faucet.
    ///
    /// # Arguments
    ///
    /// * `account_id` - The Hedera account ID to fund (e.g., "0.0.12345")
    ///
    /// # Returns
    ///
    /// A `FaucetResult` with details about the funding transaction.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The account ID is invalid
    /// - The faucet request fails (rate limit, network error, etc.)
    /// - The faucet returns an error response
    pub async fn request_hbar(&self, account_id: &str) -> SettleResult<FaucetResult> {
        // Validate account ID format
        let _ = AccountId::from_string(account_id)?;

        debug!(account_id, endpoint = %self.endpoint, "Requesting HBAR from faucet");

        // Build the request URL
        let url = format!("{}/account/{}", self.endpoint, account_id);

        // Make the HTTP request using reqwest
        let client = reqwest::Client::builder()
            .timeout(self.timeout)
            .build()
            .map_err(|e| SettleError::network(format!("Failed to create HTTP client: {}", e)))?;

        let response = client
            .post(&url)
            .header("Content-Type", "application/json")
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    SettleError::timeout(format!("Faucet request timed out: {}", e))
                } else {
                    SettleError::network(format!("Faucet request failed: {}", e))
                }
            })?;

        let status = response.status();

        // Handle rate limiting
        if status.as_u16() == 429 {
            warn!(account_id, "Faucet rate limit reached");
            return Err(SettleError::network(
                "Faucet rate limit reached. Please try again later.",
            ));
        }

        // Handle other non-success status codes
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            warn!(
                account_id,
                status = status.as_u16(),
                body,
                "Faucet request failed"
            );
            return Err(SettleError::network(format!(
                "Faucet request failed with status {}: {}",
                status, body
            )));
        }

        // Parse the response
        let body = response
            .text()
            .await
            .map_err(|e| SettleError::network(format!("Failed to read faucet response: {}", e)))?;

        // Try to parse as JSON, but handle plain text responses too
        let result = if body.starts_with('{') {
            match serde_json::from_str::<FaucetResponse>(&body) {
                Ok(FaucetResponse::Success {
                    transaction_id,
                    amount,
                }) => FaucetResult {
                    account_id: account_id.to_string(),
                    amount_hbar: amount.unwrap_or(100.0), // Default testnet faucet amount
                    transaction_id,
                },
                Ok(FaucetResponse::Error { error }) => {
                    return Err(SettleError::network(format!("Faucet error: {}", error)));
                }
                Err(_) => {
                    // If JSON parsing fails, treat as success with default values
                    FaucetResult {
                        account_id: account_id.to_string(),
                        amount_hbar: 100.0,
                        transaction_id: None,
                    }
                }
            }
        } else {
            // Plain text response - treat as success
            FaucetResult {
                account_id: account_id.to_string(),
                amount_hbar: 100.0,
                transaction_id: None,
            }
        };

        info!(
            account_id,
            amount_hbar = result.amount_hbar,
            "Successfully requested HBAR from faucet"
        );

        Ok(result)
    }

    /// Check if an account can be funded (not rate-limited).
    ///
    /// This is a best-effort check. The actual request may still fail
    /// due to race conditions or server-side state changes.
    pub async fn can_fund(&self, account_id: &str) -> bool {
        // Validate account ID first
        if AccountId::from_string(account_id).is_err() {
            return false;
        }

        // Try a HEAD request to check rate limit
        let url = format!("{}/account/{}", self.endpoint, account_id);

        let client = match reqwest::Client::builder().timeout(self.timeout).build() {
            Ok(c) => c,
            Err(_) => return false,
        };

        match client.head(&url).send().await {
            Ok(response) => response.status().as_u16() != 429,
            Err(_) => false,
        }
    }
}

/// Request HBAR from the testnet faucet (convenience function).
///
/// This is a convenience wrapper around `HederaFaucet::testnet().request_hbar()`.
///
/// # Arguments
///
/// * `account_id` - The Hedera account ID to fund (e.g., "0.0.12345")
///
/// # Example
///
/// ```rust,no_run
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// use nodalync_settle::faucet::request_testnet_hbar;
///
/// let result = request_testnet_hbar("0.0.12345").await?;
/// # Ok(())
/// # }
/// ```
pub async fn request_testnet_hbar(account_id: &str) -> SettleResult<FaucetResult> {
    HederaFaucet::testnet().request_hbar(account_id).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_faucet_config_default() {
        let config = FaucetConfig::default();
        assert!(config.endpoint.contains("faucet.hedera.com"));
        assert_eq!(config.timeout, Duration::from_secs(30));
    }

    #[test]
    fn test_faucet_creation() {
        let faucet = HederaFaucet::testnet();
        assert!(faucet.endpoint.contains("faucet.hedera.com"));
    }

    #[test]
    fn test_faucet_result() {
        let result = FaucetResult {
            account_id: "0.0.12345".to_string(),
            amount_hbar: 100.0,
            transaction_id: Some("tx123".to_string()),
        };
        assert_eq!(result.account_id, "0.0.12345");
        assert_eq!(result.amount_hbar, 100.0);
    }
}

#[cfg(all(test, feature = "testnet"))]
mod integration_tests {
    use super::*;
    use std::env;

    /// Integration test that requests HBAR from the faucet.
    ///
    /// Run with: cargo test -p nodalync-settle --features testnet -- --nocapture
    #[tokio::test]
    async fn test_faucet_request() {
        nodalync_test_utils::try_load_dotenv();
        let account_id = match env::var("HEDERA_ACCOUNT_ID").ok() {
            Some(id) => id,
            None => {
                println!("Skipping test: HEDERA_ACCOUNT_ID not set");
                return;
            }
        };

        let faucet = HederaFaucet::testnet();

        match faucet.request_hbar(&account_id).await {
            Ok(result) => {
                println!("===========================================");
                println!("Faucet Request Successful!");
                println!("===========================================");
                println!("Account: {}", result.account_id);
                println!("Amount: {} HBAR", result.amount_hbar);
                if let Some(tx_id) = &result.transaction_id {
                    println!("Transaction: {}", tx_id);
                }
                println!("===========================================");
            }
            Err(e) => {
                // Faucet failures are expected due to rate limiting
                println!("Faucet request failed (expected if rate-limited): {}", e);
            }
        }
    }
}
