//! Show identity information command.

use crate::config::CliConfig;
use crate::context::NodeContext;
use crate::error::CliResult;
use crate::output::{OutputFormat, Render, WhoamiOutput};

/// Execute the whoami command.
pub fn whoami(config: CliConfig, format: OutputFormat) -> CliResult<String> {
    let ctx = NodeContext::local(config)?;

    // Get public key
    let public_key = ctx.ops.state.identity.public_key()?;

    let output = WhoamiOutput {
        peer_id: ctx.peer_id().to_string(),
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
        let temp_dir = TempDir::new().unwrap();
        let config = setup_config(&temp_dir);

        // Initialize first
        init(config.clone(), OutputFormat::Human).unwrap();

        // Then whoami
        let result = whoami(config, OutputFormat::Human);
        assert!(result.is_ok());

        let output = result.unwrap();
        assert!(output.contains("PeerId"));
        assert!(output.contains("Public Key"));
    }

    #[test]
    fn test_whoami_json() {
        let temp_dir = TempDir::new().unwrap();
        let config = setup_config(&temp_dir);

        init(config.clone(), OutputFormat::Human).unwrap();

        let result = whoami(config, OutputFormat::Json);
        assert!(result.is_ok());

        let output = result.unwrap();
        assert!(output.contains("\"peer_id\""));
    }
}
