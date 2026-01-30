//! CLI error types.

use nodalync_types::ErrorCode;
use thiserror::Error;

/// CLI result type.
pub type CliResult<T> = Result<T, CliError>;

/// CLI error enum wrapping all crate errors.
#[derive(Debug, Error)]
pub enum CliError {
    /// Configuration error.
    #[error("Configuration error: {0}")]
    Config(String),

    /// Operations error.
    #[error("{0}")]
    Ops(#[from] nodalync_ops::OpsError),

    /// Network error.
    #[error("{0}")]
    Network(#[from] nodalync_net::NetworkError),

    /// Settlement error.
    #[error("{0}")]
    Settlement(#[from] nodalync_settle::SettleError),

    /// Store error.
    #[error("{0}")]
    Store(#[from] nodalync_store::StoreError),

    /// IO error.
    #[error("{0}")]
    Io(#[from] std::io::Error),

    /// JSON serialization error.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// TOML parsing error.
    #[error("TOML error: {0}")]
    Toml(#[from] toml::de::Error),

    /// User-facing error with actionable message.
    #[error("{0}")]
    User(String),

    /// Content not found.
    #[error("Content not found: {0}")]
    NotFound(String),

    /// Insufficient balance.
    #[error("Insufficient balance: need {required}, have {available}")]
    InsufficientBalance { required: u64, available: u64 },

    /// Identity not initialized.
    #[error("Identity not initialized. Run 'nodalync init' first.")]
    IdentityNotInitialized,

    /// Identity already exists.
    #[error("Identity already exists. Delete ~/.nodalync/identity to reinitialize.")]
    IdentityExists,

    /// Node not running.
    #[error("Node is not running. Start with 'nodalync start'.")]
    NodeNotRunning,

    /// Node already running.
    #[error("Node is already running.")]
    NodeAlreadyRunning,

    /// Invalid hash format.
    #[error("Invalid hash format: {0}")]
    InvalidHash(String),

    /// File not found.
    #[error("File not found: {0}")]
    FileNotFound(String),
}

impl CliError {
    /// Create a configuration error.
    pub fn config(msg: impl Into<String>) -> Self {
        Self::Config(msg.into())
    }

    /// Create a user-facing error.
    pub fn user(msg: impl Into<String>) -> Self {
        Self::User(msg.into())
    }

    /// Get the exit code for this error.
    pub fn exit_code(&self) -> i32 {
        match self {
            // User errors: 1
            Self::User(_)
            | Self::IdentityNotInitialized
            | Self::IdentityExists
            | Self::NodeNotRunning
            | Self::NodeAlreadyRunning => 1,
            // Not found: 2
            Self::NotFound(_) | Self::FileNotFound(_) => 2,
            // Config errors: 3
            Self::Config(_) | Self::Toml(_) => 3,
            // Balance/payment errors: 4
            Self::InsufficientBalance { .. } => 4,
            // Network errors: 5
            Self::Network(_) => 5,
            // Store errors: 6
            Self::Store(_) => 6,
            // Settlement errors: 7
            Self::Settlement(_) => 7,
            // Operations errors: 8
            Self::Ops(_) => 8,
            // IO errors: 9
            Self::Io(_) => 9,
            // JSON/format errors: 10
            Self::Json(_) | Self::InvalidHash(_) => 10,
        }
    }

    /// Get the protocol error code for this error.
    ///
    /// Maps CLI errors to the appropriate `ErrorCode` from spec Appendix C.
    pub fn error_code(&self) -> ErrorCode {
        match self {
            // Content errors
            Self::NotFound(_) | Self::FileNotFound(_) => ErrorCode::NotFound,
            Self::InvalidHash(_) => ErrorCode::InvalidHash,

            // Payment/channel errors
            Self::InsufficientBalance { .. } => ErrorCode::InsufficientBalance,

            // Identity/config errors
            Self::IdentityNotInitialized | Self::IdentityExists => ErrorCode::InvalidManifest,
            Self::Config(_) | Self::Toml(_) | Self::Json(_) => ErrorCode::InvalidManifest,

            // Node state errors
            Self::NodeNotRunning | Self::NodeAlreadyRunning => ErrorCode::ConnectionFailed,

            // Network errors
            Self::Network(_) => ErrorCode::ConnectionFailed,

            // Delegated errors
            Self::Ops(e) => e.error_code(),
            Self::Settlement(_) => ErrorCode::InternalError,
            Self::Store(_) => ErrorCode::InternalError,
            Self::Io(_) => ErrorCode::InternalError,

            // User-facing errors are generic
            Self::User(_) => ErrorCode::InternalError,
        }
    }
}
