//! Error types for the MCP server.

use nodalync_ops::OpsError;
use nodalync_types::Amount;
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

    /// Internal server error.
    #[error("internal error: {0}")]
    Internal(String),
}

impl McpError {
    /// Create a new internal error.
    pub fn internal(msg: impl Into<String>) -> Self {
        Self::Internal(msg.into())
    }
}
