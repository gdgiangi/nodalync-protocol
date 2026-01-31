//! Initialize identity command.

use crate::config::{default_config_path, CliConfig};
use crate::context::NodeContext;
use crate::error::{CliError, CliResult};
use crate::output::{InitOutput, OutputFormat, Render};
use crate::wizard::run_wizard;

/// Execute the init command.
pub fn init(config: CliConfig, format: OutputFormat, wizard: bool) -> CliResult<String> {
    // Check if identity already exists
    let base_dir = config.base_dir();
    let identity_dir = base_dir.join("identity");

    if identity_dir.join("keypair.key").exists() {
        if wizard && crate::prompt::is_interactive() {
            // In wizard mode, automatically reinitialize by removing existing identity
            eprintln!("⚠️  Existing identity found. Reinitializing...");
            if let Err(e) = std::fs::remove_dir_all(&identity_dir) {
                return Err(CliError::User(format!(
                    "Failed to remove existing identity: {}",
                    e
                )));
            }
        } else {
            return Err(CliError::IdentityExists);
        }
    }

    // Run wizard if requested
    let config = if wizard && crate::prompt::is_interactive() {
        run_wizard(config)?
    } else {
        config
    };

    // Initialize storage
    let state = NodeContext::for_init(config.clone())?;

    // Get password from environment (preferred for scripts/CI) or prompt
    let password = if let Ok(pwd) = std::env::var("NODALYNC_PASSWORD") {
        pwd
    } else if crate::prompt::is_interactive() {
        crate::prompt::password_with_confirm("Enter password to encrypt identity")?
    } else {
        return Err(CliError::User(
            "Set NODALYNC_PASSWORD or run interactively".into(),
        ));
    };

    // Generate identity
    let peer_id = state.identity.generate(&password)?;

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
        assert!(matches!(result2, Err(CliError::IdentityExists)));
    }

    // Note: Testing wizard auto-reinit requires interactive mode,
    // which can't be easily tested in unit tests. The wizard flag
    // combined with is_interactive() check ensures safe behavior.
}
