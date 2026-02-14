//! Error types for x402 payment integration.

use thiserror::Error;

/// Result type for x402 operations.
pub type X402Result<T> = Result<T, X402Error>;

/// Errors that can occur during x402 payment operations.
#[derive(Debug, Error)]
pub enum X402Error {
    /// Payment amount is insufficient for the requested resource.
    #[error("insufficient payment: required {required} tinybars, received {received}")]
    InsufficientPayment {
        /// Amount required
        required: u64,
        /// Amount received
        received: u64,
    },

    /// Payment payload signature is invalid.
    #[error("invalid payment signature")]
    InvalidSignature,

    /// Payment payload is malformed or missing required fields.
    #[error("malformed payment payload: {reason}")]
    MalformedPayload {
        /// Description of what's wrong
        reason: String,
    },

    /// The payment scheme is not supported.
    #[error("unsupported payment scheme: {scheme}")]
    UnsupportedScheme {
        /// The unsupported scheme name
        scheme: String,
    },

    /// The payment network is not supported.
    #[error("unsupported network: {network}")]
    UnsupportedNetwork {
        /// The unsupported network identifier
        network: String,
    },

    /// Payment has expired (validBefore exceeded).
    #[error("payment expired at {expired_at}")]
    PaymentExpired {
        /// When the payment expired (Unix timestamp)
        expired_at: u64,
    },

    /// Payment is not yet valid (validAfter not reached).
    #[error("payment not yet valid until {valid_after}")]
    PaymentNotYetValid {
        /// When the payment becomes valid (Unix timestamp)
        valid_after: u64,
    },

    /// Facilitator verification failed.
    #[error("facilitator verification failed: {reason}")]
    VerificationFailed {
        /// Reason for failure
        reason: String,
    },

    /// Facilitator settlement failed.
    #[error("facilitator settlement failed: {reason}")]
    SettlementFailed {
        /// Reason for failure
        reason: String,
    },

    /// Network/HTTP error communicating with facilitator.
    #[error("facilitator communication error: {0}")]
    FacilitatorNetwork(String),

    /// x402 is not configured or disabled.
    #[error("x402 payments not configured")]
    NotConfigured,

    /// The requested resource is not payable via x402.
    #[error("resource not payable: {resource}")]
    NotPayable {
        /// The resource identifier
        resource: String,
    },

    /// Nonce has already been used (replay attack prevention).
    #[error("nonce already used: {nonce}")]
    NonceReused {
        /// The reused nonce
        nonce: String,
    },

    /// Internal error.
    #[error("internal x402 error: {0}")]
    Internal(String),
}

impl X402Error {
    /// Returns a user-friendly suggestion for recovering from this error.
    pub fn suggestion(&self) -> &str {
        match self {
            Self::InsufficientPayment { .. } => {
                "Increase payment amount to meet the resource price"
            }
            Self::InvalidSignature => "Ensure the payment payload is correctly signed",
            Self::MalformedPayload { .. } => "Check the payment payload format against x402 spec",
            Self::UnsupportedScheme { .. } => "Use the 'exact' payment scheme for Hedera",
            Self::UnsupportedNetwork { .. } => "Use 'hedera:testnet' or 'hedera:mainnet' network",
            Self::PaymentExpired { .. } => "Create a new payment with a later validBefore",
            Self::PaymentNotYetValid { .. } => "Wait until the validAfter timestamp",
            Self::VerificationFailed { .. } => "Check payment details and retry",
            Self::SettlementFailed { .. } => "Retry settlement or check facilitator status",
            Self::FacilitatorNetwork(_) => "Check network connectivity to the facilitator",
            Self::NotConfigured => "Enable x402 in the node configuration",
            Self::NotPayable { .. } => "This resource does not require payment",
            Self::NonceReused { .. } => "Use a fresh nonce for each payment",
            Self::Internal(_) => "This is an internal error; please report it",
        }
    }

    /// Returns true if this error is transient and the operation may succeed on retry.
    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            Self::FacilitatorNetwork(_) | Self::SettlementFailed { .. }
        )
    }

    /// Returns the HTTP status code appropriate for this error.
    pub fn http_status(&self) -> u16 {
        match self {
            Self::InsufficientPayment { .. }
            | Self::InvalidSignature
            | Self::MalformedPayload { .. }
            | Self::PaymentExpired { .. }
            | Self::PaymentNotYetValid { .. }
            | Self::NonceReused { .. } => 402,
            Self::UnsupportedScheme { .. } | Self::UnsupportedNetwork { .. } => 400,
            Self::NotPayable { .. } => 404,
            Self::NotConfigured => 501,
            _ => 500,
        }
    }
}

impl From<reqwest::Error> for X402Error {
    fn from(e: reqwest::Error) -> Self {
        Self::FacilitatorNetwork(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_suggestions() {
        let err = X402Error::InsufficientPayment {
            required: 100,
            received: 50,
        };
        assert!(!err.suggestion().is_empty());
    }

    #[test]
    fn test_error_transient() {
        assert!(X402Error::FacilitatorNetwork("timeout".into()).is_transient());
        assert!(!X402Error::InvalidSignature.is_transient());
    }

    #[test]
    fn test_error_http_status() {
        assert_eq!(
            X402Error::InsufficientPayment {
                required: 100,
                received: 50
            }
            .http_status(),
            402
        );
        assert_eq!(
            X402Error::UnsupportedScheme {
                scheme: "foo".into()
            }
            .http_status(),
            400
        );
        assert_eq!(X402Error::NotConfigured.http_status(), 501);
    }
}
