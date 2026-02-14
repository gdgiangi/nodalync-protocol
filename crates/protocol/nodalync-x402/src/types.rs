//! x402 protocol types.
//!
//! Implements the x402 specification types for the Hedera payment scheme.
//! See: https://github.com/coinbase/x402/blob/main/specs/x402-specification.md

use serde::{Deserialize, Serialize};

/// x402 protocol version.
pub const X402_VERSION: u32 = 1;

/// HTTP header name for payment requirements (server → client).
pub const HEADER_PAYMENT_REQUIRED: &str = "X-PAYMENT-REQUIRED";

/// HTTP header name for payment signature (client → server).
pub const HEADER_PAYMENT_SIGNATURE: &str = "X-PAYMENT";

/// HTTP header name for payment response (server → client after settlement).
pub const HEADER_PAYMENT_RESPONSE: &str = "X-PAYMENT-RESPONSE";

/// Hedera network identifiers (CAIP-2 format).
pub const NETWORK_HEDERA_MAINNET: &str = "hedera:mainnet";
pub const NETWORK_HEDERA_TESTNET: &str = "hedera:testnet";

/// The payment scheme used by Nodalync (Hedera exact scheme).
pub const SCHEME_EXACT: &str = "exact";

/// Default maximum timeout for payment validity (seconds).
pub const DEFAULT_MAX_TIMEOUT_SECONDS: u64 = 300; // 5 minutes

// =============================================================================
// Payment Requirements (402 Response)
// =============================================================================

/// Payment requirements returned in the 402 response.
///
/// Sent by the resource server to tell the client how to pay.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaymentRequired {
    /// x402 protocol version.
    pub x402_version: u32,

    /// Description of the resource being accessed.
    pub resource: ResourceInfo,

    /// Payment requirements the client must satisfy.
    pub accepts: Vec<PaymentRequirement>,
}

/// Information about the resource being paid for.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceInfo {
    /// URL of the resource.
    pub url: String,

    /// Human-readable description.
    pub description: String,

    /// MIME type of the resource.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,

    /// Content hash (Nodalync-specific: identifies the content in the network).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
}

/// A single accepted payment method.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaymentRequirement {
    /// Payment scheme (e.g., "exact").
    pub scheme: String,

    /// Network identifier (CAIP-2 format, e.g., "hedera:testnet").
    pub network: String,

    /// Required payment amount (in tinybars for HBAR, or smallest unit for tokens).
    pub amount: String,

    /// Asset identifier.
    /// For native HBAR: "HBAR".
    /// For tokens: the token ID (e.g., "0.0.456858" for USDC on Hedera).
    pub asset: String,

    /// Address to pay to (Hedera account ID, e.g., "0.0.12345").
    pub pay_to: String,

    /// Maximum time in seconds the payment is valid after creation.
    pub max_timeout_seconds: u64,

    /// Nodalync-specific metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra: Option<PaymentExtra>,
}

/// Nodalync-specific extra metadata in payment requirements.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaymentExtra {
    /// Name of the protocol ("Nodalync").
    pub protocol: String,

    /// Protocol version.
    pub protocol_version: String,

    /// Content hash of the requested knowledge.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,

    /// Application fee percentage (e.g., "5" for 5%).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_fee_percent: Option<String>,

    /// Provenance chain depth.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provenance_depth: Option<u32>,
}

// =============================================================================
// Payment Payload (Client → Server)
// =============================================================================

/// Payment payload sent by the client in the X-PAYMENT header.
///
/// For Hedera's exact scheme, this contains a partially-signed transaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaymentPayload {
    /// x402 protocol version.
    pub x402_version: u32,

    /// The payment scheme used.
    pub scheme: String,

    /// Network the payment is for.
    pub network: String,

    /// Payment details specific to the scheme.
    pub payload: HederaPaymentDetails,
}

/// Hedera-specific payment details (exact scheme).
///
/// Uses Hedera's partially-signed transaction model where:
/// 1. Client signs a CryptoTransfer transaction
/// 2. Leaves the fee-payer slot for the facilitator
/// 3. Facilitator adds their signature and submits
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HederaPaymentDetails {
    /// Payer's Hedera account ID (e.g., "0.0.12345").
    pub from: String,

    /// Recipient's Hedera account ID.
    pub to: String,

    /// Payment amount in tinybars (or token smallest unit).
    pub amount: String,

    /// The partially-signed transaction bytes (hex-encoded).
    pub transaction_bytes: String,

    /// Client's signature over the transaction (hex-encoded).
    pub signature: String,

    /// Timestamp after which payment is valid (Unix seconds).
    pub valid_after: String,

    /// Timestamp before which payment is valid (Unix seconds).
    pub valid_before: String,

    /// Unique nonce to prevent replay attacks (hex-encoded 32 bytes).
    pub nonce: String,
}

// =============================================================================
// Facilitator API Types
// =============================================================================

/// Request to the facilitator's /verify endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyRequest {
    /// x402 protocol version.
    pub x402_version: u32,

    /// Base64-encoded payment header from the client.
    pub payment_header: String,

    /// The payment requirements that were sent to the client.
    pub payment_requirements: PaymentRequirement,
}

/// Response from the facilitator's /verify endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyResponse {
    /// Whether the payment is valid.
    pub is_valid: bool,

    /// If invalid, the reason.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invalid_reason: Option<String>,

    /// Payer's address (for audit).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payer: Option<String>,
}

/// Request to the facilitator's /settle endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettleRequest {
    /// x402 protocol version.
    pub x402_version: u32,

    /// Base64-encoded payment header.
    pub payment_header: String,

    /// The payment requirements.
    pub payment_requirements: PaymentRequirement,
}

/// Response from the facilitator's /settle endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettleResponse {
    /// Whether settlement succeeded.
    pub success: bool,

    /// Transaction hash on-chain.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tx_hash: Option<String>,

    /// Network the settlement occurred on.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network: Option<String>,

    /// If failed, the error reason.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Response from the facilitator's /supported endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupportedResponse {
    /// Supported payment schemes/networks.
    pub kinds: Vec<SupportedKind>,
}

/// A supported payment kind.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupportedKind {
    /// Payment scheme (e.g., "exact").
    pub scheme: String,
    /// Network identifier.
    pub network: String,
    /// Supported assets on this network.
    #[serde(default)]
    pub assets: Vec<String>,
}

// =============================================================================
// Payment Response (Server → Client after settlement)
// =============================================================================

/// Payment response included in the X-PAYMENT-RESPONSE header.
///
/// Confirms that payment was received and settled.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaymentResponse {
    /// Whether payment was successful.
    pub success: bool,

    /// Transaction hash on the settlement network.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tx_hash: Option<String>,

    /// Network where settlement occurred.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network: Option<String>,

    /// Nodalync-specific: provenance trail for the accessed content.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provenance: Option<ProvenanceReceipt>,
}

/// Provenance receipt returned after a paid knowledge query.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvenanceReceipt {
    /// Content hash of the accessed knowledge.
    pub content_hash: String,

    /// Owner (creator) of the content.
    pub owner: String,

    /// Root contributors who get paid.
    pub contributors: Vec<ContributorInfo>,

    /// Application fee amount (tinybars).
    pub app_fee: u64,
}

/// Information about a content contributor.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContributorInfo {
    /// Contributor's peer ID.
    pub peer_id: String,

    /// Amount this contributor receives (tinybars).
    pub amount: u64,

    /// Content hash of their source material.
    pub source_hash: String,
}

// =============================================================================
// Configuration
// =============================================================================

/// x402 configuration for a Nodalync node.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct X402Config {
    /// Whether x402 payments are enabled.
    pub enabled: bool,

    /// Hedera network to use ("hedera:testnet" or "hedera:mainnet").
    pub network: String,

    /// Facilitator URL (e.g., "https://api.testnet.blocky402.com/v1").
    pub facilitator_url: String,

    /// Our Hedera account ID for receiving payments.
    pub account_id: String,

    /// Asset to accept ("HBAR" for native, or token ID).
    pub asset: String,

    /// Application fee percentage (0-100, applied on top of content price).
    pub app_fee_percent: u8,

    /// Maximum payment timeout in seconds.
    pub max_timeout_seconds: u64,

    /// Whether to auto-settle payments immediately or batch them.
    pub auto_settle: bool,
}

impl Default for X402Config {
    fn default() -> Self {
        Self {
            enabled: false,
            network: NETWORK_HEDERA_TESTNET.to_string(),
            facilitator_url: "https://api.testnet.blocky402.com/v1".to_string(),
            account_id: String::new(),
            asset: "HBAR".to_string(),
            app_fee_percent: 5,
            max_timeout_seconds: DEFAULT_MAX_TIMEOUT_SECONDS,
            auto_settle: true,
        }
    }
}

impl X402Config {
    /// Create a testnet configuration.
    pub fn testnet(account_id: &str, app_fee_percent: u8) -> Self {
        Self {
            enabled: true,
            network: NETWORK_HEDERA_TESTNET.to_string(),
            facilitator_url: "https://api.testnet.blocky402.com/v1".to_string(),
            account_id: account_id.to_string(),
            asset: "HBAR".to_string(),
            app_fee_percent,
            max_timeout_seconds: DEFAULT_MAX_TIMEOUT_SECONDS,
            auto_settle: true,
        }
    }

    /// Create a mainnet configuration.
    pub fn mainnet(account_id: &str, app_fee_percent: u8) -> Self {
        Self {
            enabled: true,
            network: NETWORK_HEDERA_MAINNET.to_string(),
            facilitator_url: "https://api.blocky402.com/v1".to_string(),
            account_id: account_id.to_string(),
            asset: "HBAR".to_string(),
            app_fee_percent,
            max_timeout_seconds: DEFAULT_MAX_TIMEOUT_SECONDS,
            auto_settle: true,
        }
    }
}

// =============================================================================
// Builder Helpers
// =============================================================================

impl PaymentRequired {
    /// Create a new payment requirement for a Nodalync knowledge resource.
    pub fn for_knowledge(
        resource_url: &str,
        content_hash: &str,
        description: &str,
        price_tinybars: u64,
        config: &X402Config,
    ) -> Self {
        // Apply app fee on top of content price
        let total_price = price_tinybars + (price_tinybars * config.app_fee_percent as u64 / 100);

        Self {
            x402_version: X402_VERSION,
            resource: ResourceInfo {
                url: resource_url.to_string(),
                description: description.to_string(),
                mime_type: Some("application/json".to_string()),
                content_hash: Some(content_hash.to_string()),
            },
            accepts: vec![PaymentRequirement {
                scheme: SCHEME_EXACT.to_string(),
                network: config.network.clone(),
                amount: total_price.to_string(),
                asset: config.asset.clone(),
                pay_to: config.account_id.clone(),
                max_timeout_seconds: config.max_timeout_seconds,
                extra: Some(PaymentExtra {
                    protocol: "Nodalync".to_string(),
                    protocol_version: "0.7.1".to_string(),
                    content_hash: Some(content_hash.to_string()),
                    app_fee_percent: Some(config.app_fee_percent.to_string()),
                    provenance_depth: None,
                }),
            }],
        }
    }
}

impl PaymentPayload {
    /// Decode a payment payload from a base64-encoded header value.
    pub fn from_header(header_value: &str) -> Result<Self, String> {
        use base64::Engine as _;
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(header_value)
            .map_err(|e| format!("base64 decode error: {}", e))?;
        serde_json::from_slice(&decoded).map_err(|e| format!("JSON parse error: {}", e))
    }

    /// Encode this payment payload to a base64 string for the header.
    pub fn to_header(&self) -> Result<String, String> {
        use base64::Engine as _;
        let json = serde_json::to_vec(self).map_err(|e| format!("JSON encode error: {}", e))?;
        Ok(base64::engine::general_purpose::STANDARD.encode(&json))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_payment_required_creation() {
        let config = X402Config::testnet("0.0.12345", 5);
        let pr = PaymentRequired::for_knowledge(
            "nodalync://query/abc123",
            "abc123",
            "Knowledge about Rust programming",
            100,
            &config,
        );

        assert_eq!(pr.x402_version, X402_VERSION);
        assert_eq!(pr.accepts.len(), 1);
        assert_eq!(pr.accepts[0].scheme, SCHEME_EXACT);
        assert_eq!(pr.accepts[0].network, NETWORK_HEDERA_TESTNET);
        // 100 + 5% = 105
        assert_eq!(pr.accepts[0].amount, "105");
        assert_eq!(pr.accepts[0].pay_to, "0.0.12345");
    }

    #[test]
    fn test_payment_required_serialization() {
        let config = X402Config::testnet("0.0.12345", 5);
        let pr = PaymentRequired::for_knowledge(
            "nodalync://query/abc123",
            "abc123",
            "Test knowledge",
            100,
            &config,
        );

        let json = serde_json::to_string(&pr).unwrap();
        let deserialized: PaymentRequired = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.x402_version, pr.x402_version);
        assert_eq!(deserialized.accepts.len(), pr.accepts.len());
        assert_eq!(deserialized.accepts[0].amount, pr.accepts[0].amount);
    }

    #[test]
    fn test_config_defaults() {
        let config = X402Config::default();
        assert!(!config.enabled);
        assert_eq!(config.network, NETWORK_HEDERA_TESTNET);
        assert_eq!(config.app_fee_percent, 5);
        assert_eq!(config.max_timeout_seconds, 300);
    }

    #[test]
    fn test_config_testnet() {
        let config = X402Config::testnet("0.0.12345", 10);
        assert!(config.enabled);
        assert_eq!(config.network, NETWORK_HEDERA_TESTNET);
        assert_eq!(config.app_fee_percent, 10);
    }

    #[test]
    fn test_config_mainnet() {
        let config = X402Config::mainnet("0.0.54321", 3);
        assert!(config.enabled);
        assert_eq!(config.network, NETWORK_HEDERA_MAINNET);
        assert_eq!(config.app_fee_percent, 3);
    }

    #[test]
    fn test_payment_payload_header_roundtrip() {
        let payload = PaymentPayload {
            x402_version: X402_VERSION,
            scheme: SCHEME_EXACT.to_string(),
            network: NETWORK_HEDERA_TESTNET.to_string(),
            payload: HederaPaymentDetails {
                from: "0.0.12345".to_string(),
                to: "0.0.54321".to_string(),
                amount: "100".to_string(),
                transaction_bytes: "deadbeef".to_string(),
                signature: "cafebabe".to_string(),
                valid_after: "1700000000".to_string(),
                valid_before: "1700000300".to_string(),
                nonce: "0123456789abcdef".to_string(),
            },
        };

        let encoded = payload.to_header().unwrap();
        let decoded = PaymentPayload::from_header(&encoded).unwrap();

        assert_eq!(decoded.x402_version, payload.x402_version);
        assert_eq!(decoded.payload.from, "0.0.12345");
        assert_eq!(decoded.payload.to, "0.0.54321");
        assert_eq!(decoded.payload.amount, "100");
    }

    #[test]
    fn test_app_fee_calculation() {
        // 0% fee
        let config = X402Config::testnet("0.0.12345", 0);
        let pr = PaymentRequired::for_knowledge("url", "hash", "desc", 100, &config);
        assert_eq!(pr.accepts[0].amount, "100");

        // 5% fee
        let config = X402Config::testnet("0.0.12345", 5);
        let pr = PaymentRequired::for_knowledge("url", "hash", "desc", 100, &config);
        assert_eq!(pr.accepts[0].amount, "105");

        // 10% fee
        let config = X402Config::testnet("0.0.12345", 10);
        let pr = PaymentRequired::for_knowledge("url", "hash", "desc", 1000, &config);
        assert_eq!(pr.accepts[0].amount, "1100");
    }

    #[test]
    fn test_resource_info() {
        let info = ResourceInfo {
            url: "nodalync://query/abc".to_string(),
            description: "Knowledge about topic".to_string(),
            mime_type: Some("application/json".to_string()),
            content_hash: Some("abc123".to_string()),
        };

        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("nodalync://query/abc"));
        assert!(json.contains("abc123"));
    }

    #[test]
    fn test_payment_extra_serialization() {
        let extra = PaymentExtra {
            protocol: "Nodalync".to_string(),
            protocol_version: "0.7.1".to_string(),
            content_hash: Some("abc123".to_string()),
            app_fee_percent: Some("5".to_string()),
            provenance_depth: Some(3),
        };

        let json = serde_json::to_string(&extra).unwrap();
        let deserialized: PaymentExtra = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.protocol, "Nodalync");
        assert_eq!(deserialized.provenance_depth, Some(3));
    }
}
