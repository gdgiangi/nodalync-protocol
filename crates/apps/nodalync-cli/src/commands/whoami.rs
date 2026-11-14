//! Show identity information command.

use crate::config::CliConfig;
use crate::context::{decrypt_identity, get_libp2p_peer_id, NodeContext};
use crate::error::CliResult;
use crate::output::{OutputFormat, Render, WhoamiOutput};

/// Execute the whoami command.
pub fn whoami(config: CliConfig, format: OutputFormat) -> CliResult<String> {
    let ctx = NodeContext::local(config.clone())?;

    // Get public key
    let public_key = ctx.ops.state.identity.public_key()?;

    // Get libp2p peer ID (requires decrypting the private key)
    let (private_key, _) = decrypt_identity(&ctx.ops.state)?;
    let libp2p_peer_id = get_libp2p_peer_id(&private_key)?;

    let output = WhoamiOutput {
        peer_id: ctx.peer_id().to_string(),
        libp2p_peer_id: libp2p_peer_id.to_string(),
        public_key: format!("0x{}", hex::encode(public_key.0)),
        addresses: vec![], // Addresses populated when network is running
    };

    Ok(output.render(format))
}

// Simple hex encoding helper
mod hex {
    pub fn encode(bytes: impl AsRef<[u8]>) -> String {
        bytes
            .as_ref()
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::init::init;
    use tempfile::TempDir;

    fn setup_config(temp_dir: &TempDir) -> CliConfig {
        let mut config = CliConfig::default();
        config.storage.content_dir = temp_dir.path().join("content");
        config.storage.cache_dir = temp_dir.path().join("cache");
        config.storage.database = temp_dir.path().join("nodalync.db");
        config.identity.keyfile = temp_dir.path().join("identity").join("keypair.key");
        config
    }

    #[test]
    fn test_whoami_after_init() {
        std::env::set_var("NODALYNC_PASSWORD", "test_password");

        let temp_dir = TempDir::new().unwrap();
        let config = setup_config(&temp_dir);

        // Initialize first
        init(config.clone(), OutputFormat::Human, false).unwrap();

        // Then whoami
        let result = whoami(config, OutputFormat::Human);
        assert!(result.is_ok());

        let output = result.unwrap();
        assert!(output.contains("PeerId"));
        assert!(output.contains("Public Key"));
    }

    #[test]
    fn test_whoami_json() {
        std::env::set_var("NODALYNC_PASSWORD", "test_password");

        let temp_dir = TempDir::new().unwrap();
        let config = setup_config(&temp_dir);

        init(config.clone(), OutputFormat::Human, false).unwrap();

        let result = whoami(config, OutputFormat::Json);
        assert!(result.is_ok());

        let output = result.unwrap();
        assert!(output.contains("\"peer_id\""));
    }

    /// Regression test for Issue #46: wrong password should surface as a clear
    /// decryption error, not "Identity not initialized."
    ///
    /// Tests the identity load path directly to avoid env var race conditions
    /// with parallel tests.
    #[test]
    fn test_wrong_password_surfaces_decryption_error_not_identity_missing() {
        let temp_dir = TempDir::new().unwrap();
        let identity_dir = temp_dir.path().join("identity");
        std::fs::create_dir_all(&identity_dir).unwrap();

        let store = nodalync_store::IdentityStore::new(&identity_dir).unwrap();

        // Generate identity with one password
        store.generate("correct_password").unwrap();

        // Try to load with wrong password
        let result = store.load("wrong_password");
        assert!(result.is_err());

        let err = result.unwrap_err();
        let err_msg = err.to_string();

        // The error should surface as Encryption, not IdentityNotFound
        assert!(
            matches!(err, nodalync_store::StoreError::Encryption(_)),
            "Wrong password should give Encryption error, got: {:?}",
            err
        );
        assert!(
            err_msg.contains("password"),
            "Error should mention password, got: {}",
            err_msg
        );

        // Verify our CLI mapping: Encryption errors become CliError::User, not
        // CliError::IdentityNotInitialized
        let cli_err = crate::context::identity_load_error(err);
        assert!(
            !cli_err.to_string().contains("Identity not initialized"),
            "CLI error should NOT say 'Identity not initialized', got: {}",
            cli_err
        );
    }
}
