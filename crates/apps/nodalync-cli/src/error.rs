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
    #[error("Identity already exists at {0}")]
    IdentityExists(String),

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

    /// Password required but not provided.
    #[error("Password required. Set NODALYNC_PASSWORD env var or run in a terminal for interactive input.")]
    PasswordRequired,

    /// Confirmation or input required from user.
    #[error("{0}")]
    ConfirmationRequired(String),

    /// Invalid input (file, content, or argument validation error).
    #[error("{0}")]
    InvalidInput(String),
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
            | Self::IdentityExists(_)
            | Self::NodeNotRunning
            | Self::NodeAlreadyRunning
            | Self::PasswordRequired
            | Self::ConfirmationRequired(_) => 1,
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
            // Validation/input errors: 11
            Self::InvalidInput(_) => 11,
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

            // Identity/auth errors
            Self::IdentityNotInitialized => ErrorCode::AccessDenied,
            Self::IdentityExists(_) => ErrorCode::AccessDenied,
            Self::PasswordRequired => ErrorCode::AccessDenied,

            // Config/parse errors
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

            // User-facing errors
            Self::User(_) => ErrorCode::AccessDenied,

            // Confirmation required (missing --force, etc.)
            Self::ConfirmationRequired(_) => ErrorCode::AccessDenied,

            // Input validation errors
            Self::InvalidInput(_) => ErrorCode::InvalidManifest,
        }
    }

    /// Get a CLI-appropriate suggestion for recovering from this error.
    pub fn suggestion(&self) -> Option<&str> {
        match self {
            Self::User(_) => None, // message itself is the guidance
            Self::Config(_) => None, // config errors include actionable details
            Self::Ops(_) | Self::Store(_) | Self::Io(_) => None, // too varied
            Self::IdentityNotInitialized => Some("Run 'nodalync init' to create your identity."),
            Self::IdentityExists(_) => Some("Delete the identity directory shown above and run 'nodalync init' again, or use 'nodalync init --wizard' in an interactive terminal."),
            Self::NodeNotRunning => Some("Start the node with 'nodalync start'."),
            Self::NodeAlreadyRunning => Some("Stop the node first with 'nodalync stop'."),
            Self::NotFound(_) => Some("Use 'nodalync list' or 'nodalync search' to find content."),
            Self::FileNotFound(_) => Some("Check that the file path exists and is readable."),
            Self::InvalidHash(_) => Some("Hash must be exactly 64 hex characters (0-9, a-f)."),
            Self::InsufficientBalance { .. } => {
                Some("Deposit more funds with 'nodalync deposit <amount>'.")
            }
            Self::Toml(_) => Some("Check config file syntax at ~/.nodalync/config.toml."),
            Self::Json(_) => Some("Check the JSON input format."),
            Self::Network(_) => Some("Check network connectivity. Run 'nodalync status' to verify."),
            Self::Settlement(_) => Some(
                "Check Hedera configuration (HEDERA_ACCOUNT_ID, HEDERA_PRIVATE_KEY, HEDERA_CONTRACT_ID).",
            ),
            Self::PasswordRequired => Some(
                "Set NODALYNC_PASSWORD env var or run in a terminal for interactive input.",
            ),
            Self::ConfirmationRequired(_) => None, // message itself is the guidance
            Self::InvalidInput(_) => None, // message itself is the guidance
        }
    }
    /// Format this error as a JSON string for machine consumers (Issue #83).
    ///
    /// Returns a compact JSON object with error code, message, optional hint,
    /// and exit code so scripts and MCP integrations can reliably parse errors.
    pub fn to_json(&self) -> String {
        let json = serde_json::json!({
            "error": {
                "code": self.error_code().to_string(),
                "message": self.to_string(),
                "hint": self.suggestion(),
                "exit_code": self.exit_code(),
            }
        });
        serde_json::to_string(&json).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression test for Issue #83: errors should be serializable as JSON
    /// when --format json is used.
    #[test]
    fn test_error_json_format() {
        let err = CliError::FileNotFound("/tmp/missing.txt".to_string());
        let json_str = err.to_json();

        let parsed: serde_json::Value = serde_json::from_str(&json_str)
            .expect("Error JSON should be valid JSON");

        let error_obj = parsed.get("error").expect("Should have 'error' key");
        assert_eq!(
            error_obj.get("code").unwrap().as_str().unwrap(),
            "NOT_FOUND"
        );
        assert!(
            error_obj.get("message").unwrap().as_str().unwrap().contains("missing.txt"),
            "Message should contain the file path"
        );
        assert!(
            error_obj.get("hint").unwrap().as_str().is_some(),
            "FileNotFound should have a hint"
        );
        assert_eq!(error_obj.get("exit_code").unwrap().as_i64().unwrap(), 2);
    }

    /// Regression test for Issue #83: JSON errors with no hint should have null hint.
    #[test]
    fn test_error_json_format_no_hint() {
        let err = CliError::user("Something went wrong");
        let json_str = err.to_json();

        let parsed: serde_json::Value = serde_json::from_str(&json_str)
            .expect("Error JSON should be valid JSON");

        let error_obj = parsed.get("error").expect("Should have 'error' key");
        assert!(
            error_obj.get("hint").unwrap().is_null(),
            "User error should have null hint"
        );
        assert_eq!(
            error_obj.get("code").unwrap().as_str().unwrap(),
            "ACCESS_DENIED"
        );
    }

    #[test]
    fn test_suggestion_identity_not_initialized() {
        let err = CliError::IdentityNotInitialized;
        assert_eq!(
            err.suggestion(),
            Some("Run 'nodalync init' to create your identity.")
        );
    }

    #[test]
    fn test_suggestion_not_found() {
        let err = CliError::NotFound("abc123".to_string());
        assert_eq!(
            err.suggestion(),
            Some("Use 'nodalync list' or 'nodalync search' to find content.")
        );
    }

    #[test]
    fn test_suggestion_file_not_found() {
        let err = CliError::FileNotFound("/tmp/missing.txt".to_string());
        assert_eq!(
            err.suggestion(),
            Some("Check that the file path exists and is readable.")
        );
    }

    #[test]
    fn test_suggestion_invalid_hash() {
        let err = CliError::InvalidHash("aaaa".to_string());
        assert_eq!(
            err.suggestion(),
            Some("Hash must be exactly 64 hex characters (0-9, a-f).")
        );
    }

    #[test]
    fn test_suggestion_insufficient_balance() {
        let err = CliError::InsufficientBalance {
            required: 1000,
            available: 100,
        };
        assert_eq!(
            err.suggestion(),
            Some("Deposit more funds with 'nodalync deposit <amount>'.")
        );
    }

    #[test]
    fn test_suggestion_user_returns_none() {
        let err = CliError::User("some message".to_string());
        assert!(err.suggestion().is_none());
    }

    #[test]
    fn test_suggestion_config_returns_none() {
        let err = CliError::Config("bad config".to_string());
        assert!(err.suggestion().is_none());
    }

    #[test]
    fn test_suggestion_node_not_running() {
        let err = CliError::NodeNotRunning;
        assert_eq!(
            err.suggestion(),
            Some("Start the node with 'nodalync start'.")
        );
    }

    #[test]
    fn test_suggestion_node_already_running() {
        let err = CliError::NodeAlreadyRunning;
        assert_eq!(
            err.suggestion(),
            Some("Stop the node first with 'nodalync stop'.")
        );
    }

    #[test]
    fn test_password_required_error() {
        let err = CliError::PasswordRequired;
        assert_eq!(err.exit_code(), 1);
        assert_eq!(err.error_code(), ErrorCode::AccessDenied);
        assert!(err.suggestion().is_some());
        assert!(err.suggestion().unwrap().contains("NODALYNC_PASSWORD"));
    }
}
