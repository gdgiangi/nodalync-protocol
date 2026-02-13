//! x402 facilitator client.
//!
//! Communicates with a Blocky402-compatible facilitator for payment verification
//! and settlement. The facilitator handles:
//! - Verifying payment signatures and validity
//! - Submitting transactions to the blockchain
//! - Paying gas fees on behalf of the client

use std::time::Duration;

use reqwest::Client;
use tracing::{debug, info, warn};

use crate::error::{X402Error, X402Result};
use crate::types::{
    PaymentRequirement, SettleRequest, SettleResponse, SupportedResponse,
    VerifyRequest, VerifyResponse, X402Config, X402_VERSION,
};

/// Default HTTP timeout for facilitator requests.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// Client for communicating with an x402 facilitator (e.g., Blocky402).
#[derive(Clone)]
pub struct FacilitatorClient {
    /// HTTP client
    client: Client,
    /// Base URL of the facilitator
    base_url: String,
}

impl FacilitatorClient {
    /// Create a new facilitator client.
    pub fn new(facilitator_url: &str) -> X402Result<Self> {
        let client = Client::builder()
            .timeout(DEFAULT_TIMEOUT)
            .build()
            .map_err(|e| X402Error::FacilitatorNetwork(format!("failed to create HTTP client: {}", e)))?;

        Ok(Self {
            client,
            base_url: facilitator_url.trim_end_matches('/').to_string(),
        })
    }

    /// Create from an x402 config.
    pub fn from_config(config: &X402Config) -> X402Result<Self> {
        Self::new(&config.facilitator_url)
    }

    /// Check which networks/schemes the facilitator supports.
    pub async fn get_supported(&self) -> X402Result<SupportedResponse> {
        let url = format!("{}/supported", self.base_url);
        debug!(url = %url, "Querying facilitator supported networks");

        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(X402Error::FacilitatorNetwork(format!(
                "facilitator /supported returned {}: {}",
                status, body
            )));
        }

        let supported: SupportedResponse = response.json().await.map_err(|e| {
            X402Error::FacilitatorNetwork(format!("failed to parse supported response: {}", e))
        })?;

        debug!(kinds = supported.kinds.len(), "Facilitator supports {} networks", supported.kinds.len());
        Ok(supported)
    }

    /// Verify a payment payload against the requirements.
    ///
    /// The facilitator checks:
    /// - Payment signature is valid
    /// - Amount meets requirements
    /// - Funds are available
    /// - Transaction is well-formed
    pub async fn verify(
        &self,
        payment_header: &str,
        requirements: &PaymentRequirement,
    ) -> X402Result<VerifyResponse> {
        let url = format!("{}/verify", self.base_url);
        debug!(url = %url, "Verifying payment with facilitator");

        let request = VerifyRequest {
            x402_version: X402_VERSION,
            payment_header: payment_header.to_string(),
            payment_requirements: requirements.clone(),
        };

        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(X402Error::VerificationFailed {
                reason: format!("facilitator returned {}: {}", status, body),
            });
        }

        let verify_response: VerifyResponse = response.json().await.map_err(|e| {
            X402Error::VerificationFailed {
                reason: format!("failed to parse verify response: {}", e),
            }
        })?;

        if verify_response.is_valid {
            debug!(payer = ?verify_response.payer, "Payment verified successfully");
        } else {
            warn!(
                reason = ?verify_response.invalid_reason,
                "Payment verification failed"
            );
        }

        Ok(verify_response)
    }

    /// Settle a verified payment on-chain.
    ///
    /// The facilitator:
    /// 1. Takes the partially-signed transaction
    /// 2. Adds its own signature (paying gas)
    /// 3. Submits to the blockchain
    /// 4. Returns the transaction hash
    pub async fn settle(
        &self,
        payment_header: &str,
        requirements: &PaymentRequirement,
    ) -> X402Result<SettleResponse> {
        let url = format!("{}/settle", self.base_url);
        debug!(url = %url, "Settling payment with facilitator");

        let request = SettleRequest {
            x402_version: X402_VERSION,
            payment_header: payment_header.to_string(),
            payment_requirements: requirements.clone(),
        };

        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(X402Error::SettlementFailed {
                reason: format!("facilitator returned {}: {}", status, body),
            });
        }

        let settle_response: SettleResponse = response.json().await.map_err(|e| {
            X402Error::SettlementFailed {
                reason: format!("failed to parse settle response: {}", e),
            }
        })?;

        if settle_response.success {
            info!(
                tx_hash = ?settle_response.tx_hash,
                network = ?settle_response.network,
                "Payment settled successfully"
            );
        } else {
            warn!(
                error = ?settle_response.error,
                "Payment settlement failed"
            );
        }

        Ok(settle_response)
    }

    /// Verify and settle a payment in one operation.
    ///
    /// Convenience method that first verifies, then settles if valid.
    pub async fn verify_and_settle(
        &self,
        payment_header: &str,
        requirements: &PaymentRequirement,
    ) -> X402Result<SettleResponse> {
        // Step 1: Verify
        let verify_result = self.verify(payment_header, requirements).await?;

        if !verify_result.is_valid {
            return Err(X402Error::VerificationFailed {
                reason: verify_result
                    .invalid_reason
                    .unwrap_or_else(|| "unknown verification failure".to_string()),
            });
        }

        // Step 2: Settle
        self.settle(payment_header, requirements).await
    }

    /// Check if the facilitator supports a specific network.
    pub async fn supports_network(&self, network: &str) -> X402Result<bool> {
        let supported = self.get_supported().await?;
        Ok(supported
            .kinds
            .iter()
            .any(|k| k.network == network))
    }

    /// Get the facilitator's base URL.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}

impl std::fmt::Debug for FacilitatorClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FacilitatorClient")
            .field("base_url", &self.base_url)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_creation() {
        let client = FacilitatorClient::new("https://api.testnet.blocky402.com/v1");
        assert!(client.is_ok());

        let client = client.unwrap();
        assert_eq!(client.base_url(), "https://api.testnet.blocky402.com/v1");
    }

    #[test]
    fn test_client_url_normalization() {
        let client = FacilitatorClient::new("https://api.testnet.blocky402.com/v1/").unwrap();
        assert_eq!(client.base_url(), "https://api.testnet.blocky402.com/v1");
    }

    #[test]
    fn test_client_from_config() {
        let config = X402Config::testnet("0.0.12345", 5);
        let client = FacilitatorClient::from_config(&config);
        assert!(client.is_ok());
    }

    #[test]
    fn test_client_debug() {
        let client = FacilitatorClient::new("https://example.com/v1").unwrap();
        let debug = format!("{:?}", client);
        assert!(debug.contains("example.com"));
    }
}
