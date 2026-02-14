//! Payment gate for x402-protected resources.
//!
//! The `PaymentGate` is the core middleware that handles the x402 flow:
//! 1. Check if a resource requires payment
//! 2. Generate 402 Payment Required responses
//! 3. Validate incoming payments
//! 4. Settle payments via the facilitator
//! 5. Record transactions for audit

use std::collections::HashSet;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::sync::RwLock;
use tracing::{debug, info};

use crate::error::{X402Error, X402Result};
use crate::facilitator::FacilitatorClient;
use crate::types::{
    PaymentPayload, PaymentRequired, PaymentResponse, ProvenanceReceipt, SettleResponse, X402Config,
};

/// Transaction record for audit and reporting.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionRecord {
    /// Unique transaction ID (from facilitator).
    pub tx_hash: Option<String>,

    /// Timestamp of the transaction.
    pub timestamp: u64,

    /// Payer's account/address.
    pub payer: String,

    /// Total amount paid (tinybars).
    pub amount: u64,

    /// Content hash that was accessed.
    pub content_hash: String,

    /// Application fee collected (tinybars).
    pub app_fee: u64,

    /// Creator payment (tinybars).
    pub creator_payment: u64,

    /// Settlement network.
    pub network: String,

    /// Whether settlement succeeded.
    pub settled: bool,
}

/// Payment gate that manages x402 payment flow for Nodalync resources.
pub struct PaymentGate {
    /// x402 configuration.
    config: X402Config,

    /// Facilitator client for verification and settlement.
    facilitator: Option<FacilitatorClient>,

    /// Used nonces for replay prevention.
    used_nonces: Arc<RwLock<HashSet<String>>>,

    /// Transaction history for reporting.
    transactions: Arc<RwLock<Vec<TransactionRecord>>>,
}

impl PaymentGate {
    /// Create a new payment gate from configuration.
    pub fn new(config: X402Config) -> X402Result<Self> {
        let facilitator = if config.enabled {
            Some(FacilitatorClient::from_config(&config)?)
        } else {
            None
        };

        Ok(Self {
            config,
            facilitator,
            used_nonces: Arc::new(RwLock::new(HashSet::new())),
            transactions: Arc::new(RwLock::new(Vec::new())),
        })
    }

    /// Create a disabled payment gate (no x402).
    pub fn disabled() -> Self {
        Self {
            config: X402Config::default(),
            facilitator: None,
            used_nonces: Arc::new(RwLock::new(HashSet::new())),
            transactions: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Check if x402 is enabled.
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Get the current configuration.
    pub fn config(&self) -> &X402Config {
        &self.config
    }

    /// Generate a 402 Payment Required response for a knowledge resource.
    ///
    /// Called when a client requests paid content without providing payment.
    pub fn payment_required(
        &self,
        resource_url: &str,
        content_hash: &str,
        description: &str,
        price_tinybars: u64,
    ) -> X402Result<PaymentRequired> {
        if !self.config.enabled {
            return Err(X402Error::NotConfigured);
        }

        Ok(PaymentRequired::for_knowledge(
            resource_url,
            content_hash,
            description,
            price_tinybars,
            &self.config,
        ))
    }

    /// Process an incoming payment for a resource.
    ///
    /// This is the main entry point for handling x402 payments:
    /// 1. Decode the payment header
    /// 2. Validate the payment locally (scheme, network, amount, timing, nonce)
    /// 3. Verify via the facilitator
    /// 4. Settle via the facilitator (if auto_settle is enabled)
    /// 5. Record the transaction
    ///
    /// Returns a `PaymentResponse` on success with settlement details.
    pub async fn process_payment(
        &self,
        payment_header: &str,
        content_hash: &str,
        required_amount: u64,
    ) -> X402Result<PaymentResponse> {
        if !self.config.enabled {
            return Err(X402Error::NotConfigured);
        }

        let facilitator = self.facilitator.as_ref().ok_or(X402Error::NotConfigured)?;

        // Step 1: Decode the payment payload
        let payload = PaymentPayload::from_header(payment_header)
            .map_err(|e| X402Error::MalformedPayload { reason: e })?;

        // Step 2: Local validation
        self.validate_payload(&payload, required_amount)?;

        // Step 3: Check nonce (replay prevention)
        self.check_nonce(&payload.payload.nonce).await?;

        // Step 4: Build the requirement for the facilitator
        let requirement = &self
            .payment_required(
                &format!("nodalync://content/{}", content_hash),
                content_hash,
                "Knowledge query",
                required_amount,
            )?
            .accepts[0];

        // Step 5: Verify and settle via facilitator
        let settle_result = if self.config.auto_settle {
            facilitator
                .verify_and_settle(payment_header, requirement)
                .await?
        } else {
            // Just verify, don't settle yet
            let verify_result = facilitator.verify(payment_header, requirement).await?;
            if !verify_result.is_valid {
                return Err(X402Error::VerificationFailed {
                    reason: verify_result
                        .invalid_reason
                        .unwrap_or_else(|| "unknown".to_string()),
                });
            }
            SettleResponse {
                success: true,
                tx_hash: None,
                network: Some(self.config.network.clone()),
                error: None,
            }
        };

        if !settle_result.success {
            return Err(X402Error::SettlementFailed {
                reason: settle_result
                    .error
                    .unwrap_or_else(|| "unknown settlement failure".to_string()),
            });
        }

        // Step 6: Record nonce as used
        self.mark_nonce_used(&payload.payload.nonce).await;

        // Step 7: Calculate fee split
        let total_amount: u64 = payload.payload.amount.parse().unwrap_or(required_amount);
        let app_fee = total_amount * self.config.app_fee_percent as u64
            / (100 + self.config.app_fee_percent as u64);
        let creator_payment = total_amount - app_fee;

        // Step 8: Record transaction
        let record = TransactionRecord {
            tx_hash: settle_result.tx_hash.clone(),
            timestamp: current_timestamp(),
            payer: payload.payload.from.clone(),
            amount: total_amount,
            content_hash: content_hash.to_string(),
            app_fee,
            creator_payment,
            network: self.config.network.clone(),
            settled: settle_result.success,
        };

        self.transactions.write().await.push(record);

        info!(
            content_hash = %content_hash,
            amount = total_amount,
            app_fee = app_fee,
            tx_hash = ?settle_result.tx_hash,
            "x402 payment processed successfully"
        );

        // Step 9: Build response
        Ok(PaymentResponse {
            success: true,
            tx_hash: settle_result.tx_hash,
            network: settle_result.network,
            provenance: Some(ProvenanceReceipt {
                content_hash: content_hash.to_string(),
                owner: self.config.account_id.clone(),
                contributors: Vec::new(), // Populated by caller with actual provenance
                app_fee,
            }),
        })
    }

    /// Validate a payment payload locally before sending to the facilitator.
    fn validate_payload(&self, payload: &PaymentPayload, required_amount: u64) -> X402Result<()> {
        // Check scheme
        if payload.scheme != "exact" {
            return Err(X402Error::UnsupportedScheme {
                scheme: payload.scheme.clone(),
            });
        }

        // Check network
        if payload.network != self.config.network {
            return Err(X402Error::UnsupportedNetwork {
                network: payload.network.clone(),
            });
        }

        // Check amount
        let amount: u64 =
            payload
                .payload
                .amount
                .parse()
                .map_err(|_| X402Error::MalformedPayload {
                    reason: format!("invalid amount: {}", payload.payload.amount),
                })?;

        // Total required = content price + app fee
        let total_required =
            required_amount + (required_amount * self.config.app_fee_percent as u64 / 100);
        if amount < total_required {
            return Err(X402Error::InsufficientPayment {
                required: total_required,
                received: amount,
            });
        }

        // Check timing
        let now = current_timestamp();

        let valid_after: u64 = payload.payload.valid_after.parse().unwrap_or(0);
        if now < valid_after {
            return Err(X402Error::PaymentNotYetValid { valid_after });
        }

        let valid_before: u64 = payload.payload.valid_before.parse().unwrap_or(u64::MAX);
        if now > valid_before {
            return Err(X402Error::PaymentExpired {
                expired_at: valid_before,
            });
        }

        // Check recipient
        if payload.payload.to != self.config.account_id {
            return Err(X402Error::MalformedPayload {
                reason: format!(
                    "wrong recipient: expected {}, got {}",
                    self.config.account_id, payload.payload.to
                ),
            });
        }

        Ok(())
    }

    /// Check if a nonce has been used before.
    async fn check_nonce(&self, nonce: &str) -> X402Result<()> {
        let nonces = self.used_nonces.read().await;
        if nonces.contains(nonce) {
            return Err(X402Error::NonceReused {
                nonce: nonce.to_string(),
            });
        }
        Ok(())
    }

    /// Mark a nonce as used.
    async fn mark_nonce_used(&self, nonce: &str) {
        let mut nonces = self.used_nonces.write().await;
        nonces.insert(nonce.to_string());

        // Prevent unbounded growth — prune if too many
        // In production, this would use a time-windowed set or bloom filter
        if nonces.len() > 100_000 {
            debug!("Pruning nonce set (exceeded 100k entries)");
            nonces.clear();
        }
    }

    /// Get transaction history.
    pub async fn get_transactions(&self) -> Vec<TransactionRecord> {
        self.transactions.read().await.clone()
    }

    /// Get total revenue collected (app fees).
    pub async fn total_app_fees(&self) -> u64 {
        self.transactions
            .read()
            .await
            .iter()
            .filter(|t| t.settled)
            .map(|t| t.app_fee)
            .sum()
    }

    /// Get total volume processed.
    pub async fn total_volume(&self) -> u64 {
        self.transactions
            .read()
            .await
            .iter()
            .filter(|t| t.settled)
            .map(|t| t.amount)
            .sum()
    }

    /// Get the number of settled transactions.
    pub async fn transaction_count(&self) -> usize {
        self.transactions
            .read()
            .await
            .iter()
            .filter(|t| t.settled)
            .count()
    }

    /// Get x402 status summary.
    pub async fn status(&self) -> X402Status {
        X402Status {
            enabled: self.config.enabled,
            network: self.config.network.clone(),
            facilitator_url: self.config.facilitator_url.clone(),
            account_id: self.config.account_id.clone(),
            app_fee_percent: self.config.app_fee_percent,
            total_transactions: self.transaction_count().await,
            total_volume: self.total_volume().await,
            total_app_fees: self.total_app_fees().await,
        }
    }
}

/// x402 status summary for reporting.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct X402Status {
    pub enabled: bool,
    pub network: String,
    pub facilitator_url: String,
    pub account_id: String,
    pub app_fee_percent: u8,
    pub total_transactions: usize,
    pub total_volume: u64,
    pub total_app_fees: u64,
}

/// Get the current Unix timestamp in seconds.
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_payment_gate_disabled() {
        let gate = PaymentGate::disabled();
        assert!(!gate.is_enabled());
    }

    #[test]
    fn test_payment_gate_enabled() {
        let config = X402Config::testnet("0.0.12345", 5);
        let gate = PaymentGate::new(config).unwrap();
        assert!(gate.is_enabled());
    }

    #[test]
    fn test_payment_required_generation() {
        let config = X402Config::testnet("0.0.12345", 5);
        let gate = PaymentGate::new(config).unwrap();

        let pr = gate
            .payment_required("nodalync://query/abc", "abc123", "Test knowledge", 100)
            .unwrap();

        assert_eq!(pr.x402_version, 1);
        assert_eq!(pr.accepts.len(), 1);
        assert_eq!(pr.accepts[0].amount, "105"); // 100 + 5%
    }

    #[test]
    fn test_payment_required_disabled() {
        let gate = PaymentGate::disabled();
        let result = gate.payment_required("url", "hash", "desc", 100);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_payload_correct() {
        let config = X402Config::testnet("0.0.12345", 5);
        let gate = PaymentGate::new(config).unwrap();

        let now = current_timestamp();
        let payload = PaymentPayload {
            x402_version: 1,
            scheme: "exact".to_string(),
            network: "hedera:testnet".to_string(),
            payload: crate::types::HederaPaymentDetails {
                from: "0.0.99999".to_string(),
                to: "0.0.12345".to_string(),
                amount: "105".to_string(), // 100 + 5%
                transaction_bytes: "deadbeef".to_string(),
                signature: "cafebabe".to_string(),
                valid_after: (now - 60).to_string(),
                valid_before: (now + 300).to_string(),
                nonce: "unique_nonce_1".to_string(),
            },
        };

        let result = gate.validate_payload(&payload, 100);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_payload_wrong_scheme() {
        let config = X402Config::testnet("0.0.12345", 5);
        let gate = PaymentGate::new(config).unwrap();

        let payload = PaymentPayload {
            x402_version: 1,
            scheme: "upto".to_string(),
            network: "hedera:testnet".to_string(),
            payload: crate::types::HederaPaymentDetails {
                from: "0.0.99999".to_string(),
                to: "0.0.12345".to_string(),
                amount: "105".to_string(),
                transaction_bytes: "".to_string(),
                signature: "".to_string(),
                valid_after: "0".to_string(),
                valid_before: "99999999999".to_string(),
                nonce: "nonce".to_string(),
            },
        };

        let result = gate.validate_payload(&payload, 100);
        assert!(matches!(result, Err(X402Error::UnsupportedScheme { .. })));
    }

    #[test]
    fn test_validate_payload_wrong_network() {
        let config = X402Config::testnet("0.0.12345", 5);
        let gate = PaymentGate::new(config).unwrap();

        let payload = PaymentPayload {
            x402_version: 1,
            scheme: "exact".to_string(),
            network: "eip155:8453".to_string(), // Base, not Hedera
            payload: crate::types::HederaPaymentDetails {
                from: "0.0.99999".to_string(),
                to: "0.0.12345".to_string(),
                amount: "105".to_string(),
                transaction_bytes: "".to_string(),
                signature: "".to_string(),
                valid_after: "0".to_string(),
                valid_before: "99999999999".to_string(),
                nonce: "nonce".to_string(),
            },
        };

        let result = gate.validate_payload(&payload, 100);
        assert!(matches!(result, Err(X402Error::UnsupportedNetwork { .. })));
    }

    #[test]
    fn test_validate_payload_insufficient_amount() {
        let config = X402Config::testnet("0.0.12345", 5);
        let gate = PaymentGate::new(config).unwrap();

        let payload = PaymentPayload {
            x402_version: 1,
            scheme: "exact".to_string(),
            network: "hedera:testnet".to_string(),
            payload: crate::types::HederaPaymentDetails {
                from: "0.0.99999".to_string(),
                to: "0.0.12345".to_string(),
                amount: "50".to_string(), // Less than 105 (100 + 5%)
                transaction_bytes: "".to_string(),
                signature: "".to_string(),
                valid_after: "0".to_string(),
                valid_before: "99999999999".to_string(),
                nonce: "nonce".to_string(),
            },
        };

        let result = gate.validate_payload(&payload, 100);
        assert!(matches!(result, Err(X402Error::InsufficientPayment { .. })));
    }

    #[test]
    fn test_validate_payload_wrong_recipient() {
        let config = X402Config::testnet("0.0.12345", 5);
        let gate = PaymentGate::new(config).unwrap();

        let payload = PaymentPayload {
            x402_version: 1,
            scheme: "exact".to_string(),
            network: "hedera:testnet".to_string(),
            payload: crate::types::HederaPaymentDetails {
                from: "0.0.99999".to_string(),
                to: "0.0.99999".to_string(), // Wrong recipient
                amount: "105".to_string(),
                transaction_bytes: "".to_string(),
                signature: "".to_string(),
                valid_after: "0".to_string(),
                valid_before: "99999999999".to_string(),
                nonce: "nonce".to_string(),
            },
        };

        let result = gate.validate_payload(&payload, 100);
        assert!(matches!(result, Err(X402Error::MalformedPayload { .. })));
    }

    #[tokio::test]
    async fn test_nonce_replay_prevention() {
        let config = X402Config::testnet("0.0.12345", 5);
        let gate = PaymentGate::new(config).unwrap();

        // First use should succeed
        let result = gate.check_nonce("nonce_1").await;
        assert!(result.is_ok());

        // Mark as used
        gate.mark_nonce_used("nonce_1").await;

        // Second use should fail
        let result = gate.check_nonce("nonce_1").await;
        assert!(matches!(result, Err(X402Error::NonceReused { .. })));
    }

    #[tokio::test]
    async fn test_transaction_tracking() {
        let gate = PaymentGate::disabled();

        assert_eq!(gate.transaction_count().await, 0);
        assert_eq!(gate.total_volume().await, 0);
        assert_eq!(gate.total_app_fees().await, 0);

        // Manually add a transaction record
        gate.transactions.write().await.push(TransactionRecord {
            tx_hash: Some("0x123".to_string()),
            timestamp: 1700000000,
            payer: "0.0.99999".to_string(),
            amount: 105,
            content_hash: "abc123".to_string(),
            app_fee: 5,
            creator_payment: 100,
            network: "hedera:testnet".to_string(),
            settled: true,
        });

        assert_eq!(gate.transaction_count().await, 1);
        assert_eq!(gate.total_volume().await, 105);
        assert_eq!(gate.total_app_fees().await, 5);
    }

    #[tokio::test]
    async fn test_status_report() {
        let config = X402Config::testnet("0.0.12345", 5);
        let gate = PaymentGate::new(config).unwrap();

        let status = gate.status().await;
        assert!(status.enabled);
        assert_eq!(status.network, "hedera:testnet");
        assert_eq!(status.account_id, "0.0.12345");
        assert_eq!(status.app_fee_percent, 5);
        assert_eq!(status.total_transactions, 0);
    }
}
