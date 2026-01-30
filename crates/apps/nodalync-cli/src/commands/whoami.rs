//! Show identity information command.

use crate::config::CliConfig;
use crate::context::{get_libp2p_peer_id, NodeContext};
use crate::error::CliResult;
use crate::output::{OutputFormat, Render, WhoamiOutput};
use crate::prompt::get_identity_password;

/// Execute the whoami command.
pub fn whoami(config: CliConfig, format: OutputFormat) -> CliResult<String> {
    let ctx = NodeContext::local(config.clone())?;

    // Get public key
    let public_key = ctx.ops.state.identity.public_key()?;

    // Get libp2p peer ID (requires decrypting the private key)
    let password = get_identity_password()?;
    let (private_key, _) = ctx.ops.state.identity.load(&password)?;
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
}
