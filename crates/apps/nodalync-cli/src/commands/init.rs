//! Initialize identity command.

use crate::config::{default_config_path, CliConfig};
use crate::context::NodeContext;
use crate::error::{CliError, CliResult};
use crate::output::{InitOutput, OutputFormat, Render};
use crate::wizard::run_wizard;

/// Execute the init command.
pub fn init(config: CliConfig, format: OutputFormat, wizard: bool) -> CliResult<String> {
    // Check if identity already exists
    clear_existing_identity(&config.base_dir().join("identity"), wizard)?;

    // Run wizard if requested
    let config = maybe_run_wizard(config, wizard)?;

    // Initialize storage
    let state = NodeContext::for_init(config.clone())?;

    // Generate identity
    let peer_id = generate_identity(&state)?;

    // Save default config
    let config_path = default_config_path();
    config.save(&config_path)?;

    // Create output
    let output = InitOutput {
        peer_id: peer_id.to_string(),
        config_path: config_path.to_string_lossy().to_string(),
    };

    Ok(output.render(format))
}

/// An existing identity is an error, except in interactive wizard mode, which
/// automatically reinitializes by removing it.
fn clear_existing_identity(identity_dir: &std::path::Path, wizard: bool) -> CliResult<()> {
    if !identity_dir.join("keypair.key").exists() {
        return Ok(());
    }
    if !(wizard && crate::prompt::is_interactive()) {
        return Err(CliError::IdentityExists(identity_dir.display().to_string()));
    }
    eprintln!("⚠️  Existing identity found. Reinitializing...");
    std::fs::remove_dir_all(identity_dir)
        .map_err(|e| CliError::User(format!("Failed to remove existing identity: {}", e)))
}

/// Run the setup wizard when requested in an interactive terminal.
fn maybe_run_wizard(config: CliConfig, wizard: bool) -> CliResult<CliConfig> {
    if wizard && crate::prompt::is_interactive() {
        return run_wizard(config);
    }
    Ok(config)
}

/// Generate the identity, encrypted with the identity password.
fn generate_identity(state: &nodalync_store::NodeState) -> CliResult<nodalync_crypto::PeerId> {
    let password = init_password()?;
    Ok(state.identity.generate(&password)?)
}

/// Get password from environment (preferred for scripts/CI) or prompt.
fn init_password() -> CliResult<String> {
    if let Ok(pwd) = std::env::var("NODALYNC_PASSWORD") {
        return Ok(pwd);
    }
    if crate::prompt::is_interactive() {
        return Ok(crate::prompt::password_with_confirm(
            "Enter password to encrypt identity",
        )?);
    }
    Err(CliError::PasswordRequired)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_config(temp_dir: &TempDir) -> CliConfig {
        let mut config = CliConfig::default();
        config.storage.content_dir = temp_dir.path().join("content");
        config.storage.cache_dir = temp_dir.path().join("cache");
        config.storage.database = temp_dir.path().join("nodalync.db");
        config.identity.keyfile = temp_dir.path().join("identity").join("keypair.key");
        config
    }

    #[test]
    fn test_init_creates_identity() {
        // Set password for non-interactive test
        std::env::set_var("NODALYNC_PASSWORD", "test_password");

        let temp_dir = TempDir::new().unwrap();
        let config = test_config(&temp_dir);

        let result = init(config.clone(), OutputFormat::Human, false);
        assert!(result.is_ok());

        let output = result.unwrap();
        assert!(output.contains("Identity created"));
    }

    #[test]
    fn test_init_fails_if_exists() {
        // Set password for non-interactive test
        std::env::set_var("NODALYNC_PASSWORD", "test_password");

        let temp_dir = TempDir::new().unwrap();
        let config = test_config(&temp_dir);

        // First init should succeed
        let result = init(config.clone(), OutputFormat::Human, false);
        assert!(result.is_ok());

        // Second init without wizard should fail
        let result2 = init(config, OutputFormat::Human, false);
        assert!(matches!(result2, Err(CliError::IdentityExists(_))));
    }

    /// Regression test for Issue #17: error message should contain the actual path.
    #[test]
    fn test_init_identity_exists_shows_actual_path() {
        std::env::set_var("NODALYNC_PASSWORD", "test_password");

        let temp_dir = TempDir::new().unwrap();
        let config = test_config(&temp_dir);

        // First init succeeds
        init(config.clone(), OutputFormat::Human, false).unwrap();

        // Second init should fail with the actual identity directory path
        let result = init(config, OutputFormat::Human, false);
        assert!(result.is_err());

        let err = result.unwrap_err();
        let err_msg = err.to_string();

        // Error message should contain the actual temp dir path, not ~/.nodalync
        assert!(
            err_msg.contains(temp_dir.path().to_str().unwrap()),
            "Error should contain actual path, got: {}",
            err_msg
        );
        assert!(
            !err_msg.contains("~/.nodalync"),
            "Error should not contain hardcoded ~/.nodalync path, got: {}",
            err_msg
        );
    }

    // Note: Testing wizard auto-reinit requires interactive mode,
    // which can't be easily tested in unit tests. The wizard flag
    // combined with is_interactive() check ensures safe behavior.
}
