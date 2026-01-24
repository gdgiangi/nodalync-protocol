//! Reference L3 as L0 command.

use crate::config::CliConfig;
use crate::context::{parse_hash, NodeContext};
use crate::error::CliResult;
use crate::output::{OutputFormat, ReferenceOutput, Render};

/// Execute the reference command.
///
/// Creates an L0 reference from an existing L3 content, allowing
/// the L3 synthesis to be used as a primary source for future derivations.
pub fn reference(
    config: CliConfig,
    format: OutputFormat,
    l3_hash_str: &str,
) -> CliResult<String> {
    // Parse hash
    let l3_hash = parse_hash(l3_hash_str)?;

    // Initialize context
    let mut ctx = NodeContext::local(config)?;

    // Create L0 reference from L3
    let l0_hash = ctx.ops.reference_l3_as_l0(&l3_hash)?;

    let output = ReferenceOutput {
        l3_hash: l3_hash.to_string(),
        l0_hash: l0_hash.to_string(),
    };

    Ok(output.render(format))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_config(temp_dir: &TempDir) -> CliConfig {
        let mut config = CliConfig::default();
        config.storage.content_dir = temp_dir.path().join("content");
        config.storage.cache_dir = temp_dir.path().join("cache");
        config.storage.database = temp_dir.path().join("nodalync.db");
        config.identity.keyfile = temp_dir.path().join("identity").join("keypair.key");
        config.network.enabled = false;
        config
    }

    #[test]
    fn test_reference_not_found() {
        std::env::set_var("NODALYNC_PASSWORD", "test_password");

        let temp_dir = TempDir::new().unwrap();
        let config = setup_config(&temp_dir);

        // Initialize identity first
        crate::commands::init::init(config.clone(), OutputFormat::Human).unwrap();

        let result = reference(config, OutputFormat::Human, "invalidhash");
        assert!(result.is_err());
    }
}
