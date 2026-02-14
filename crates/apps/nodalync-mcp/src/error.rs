//! Error types for the MCP server.

use nodalync_ops::OpsError;
use nodalync_types::{Amount, ErrorCode};
use thiserror::Error;

/// Result type for MCP operations.
pub type McpResult<T> = Result<T, McpError>;

/// Error types for MCP server operations.
#[derive(Debug, Error)]
pub enum McpError {
    /// Budget exceeded - query would cost more than remaining budget.
    #[error("budget exceeded: query costs {cost} tinybars but only {remaining} remaining")]
    BudgetExceeded {
        /// Cost of the requested query.
        cost: Amount,
        /// Remaining budget.
        remaining: Amount,
    },

    /// Content not found.
    #[error("content not found: {0}")]
    NotFound(String),

    /// Invalid content hash format.
    #[error("invalid hash format: {0}")]
    InvalidHash(String),

    /// Operations error from nodalync-ops.
    #[error("operation failed: {0}")]
    Ops(#[from] OpsError),

    /// JSON serialization error.
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Empty content — publish/synthesize/update with no content.
    #[error("content cannot be empty")]
    EmptyContent,

    /// Content exceeds maximum allowed size.
    #[error("content too large: {size} bytes exceeds maximum of {max} bytes")]
    ContentTooLarge {
        /// Actual content size.
        size: usize,
        /// Maximum allowed size.
        max: usize,
    },

    /// Content with this hash already exists.
    #[error("content already exists: {0}")]
    ContentAlreadyExists(String),

    /// Internal server error.
    #[error("internal error: {0}")]
    Internal(String),

    /// x402 payment required — content needs payment via x402 protocol.
    #[error("payment required: content {content_hash} costs {price_tinybars} tinybars")]
    X402PaymentRequired {
        /// Content hash requiring payment.
        content_hash: String,
        /// Price in tinybars.
        price_tinybars: Amount,
    },

    /// x402 payment failed — payment was provided but invalid.
    #[error("x402 payment failed: {reason}")]
    X402PaymentFailed {
        /// Reason the payment failed.
        reason: String,
    },
}

impl McpError {
    /// Create a new internal error.
    pub fn internal(msg: impl Into<String>) -> Self {
        Self::Internal(msg.into())
    }

    /// Get the protocol error code for this error.
    ///
    /// Maps MCP errors to the appropriate `ErrorCode` from spec Appendix C.
    pub fn error_code(&self) -> ErrorCode {
        match self {
            Self::BudgetExceeded { .. } => ErrorCode::InsufficientBalance,
            Self::NotFound(_) => ErrorCode::NotFound,
            Self::InvalidHash(_) => ErrorCode::InvalidHash,
            Self::Ops(e) => e.error_code(),
            Self::Serialization(_) => ErrorCode::InvalidManifest,
            Self::EmptyContent => ErrorCode::InvalidManifest,
            Self::ContentTooLarge { .. } => ErrorCode::InvalidManifest,
            Self::ContentAlreadyExists(_) => ErrorCode::InvalidManifest,
            Self::Internal(_) => ErrorCode::InternalError,
            Self::X402PaymentRequired { .. } => ErrorCode::PaymentRequired,
            Self::X402PaymentFailed { .. } => ErrorCode::PaymentInvalid,
        }
    }
}
