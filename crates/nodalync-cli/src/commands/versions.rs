//! Show content versions command.

use crate::config::CliConfig;
use crate::context::{parse_hash, NodeContext};
use crate::error::{CliError, CliResult};
use crate::output::{OutputFormat, Render, VersionInfo, VersionsOutput};

/// Execute the versions command.
pub fn versions(config: CliConfig, format: OutputFormat, hash_str: &str) -> CliResult<String> {
    // Parse hash
    let hash = parse_hash(hash_str)?;

    // Initialize context
    let ctx = NodeContext::local(config)?;

    // Get content manifest to find version root
    let manifest = ctx
        .ops
        .get_content_manifest(&hash)?
        .ok_or_else(|| CliError::NotFound(hash_str.to_string()))?;

    let version_root = manifest.version.root;

    // Get all versions
    let versions_list = ctx.ops.get_content_versions(&version_root)?;

    if versions_list.is_empty() {
        return Err(CliError::NotFound(hash_str.to_string()));
    }

    // Find the latest version number
    let max_version = versions_list.iter().map(|m| m.number).max().unwrap_or(1);

    // Convert to version info
    let version_infos: Vec<VersionInfo> = versions_list
        .iter()
        .map(|m| VersionInfo {
            version: m.number,
            hash: m.hash.to_string(),
            timestamp: m.timestamp,
            visibility: format!("{:?}", m.visibility),
            is_latest: m.number == max_version,
        })
        .collect();

    let output = VersionsOutput {
        version_root: version_root.to_string(),
        versions: version_infos,
    };

    Ok(output.render(format))
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
    fn test_versions_not_found() {
        std::env::set_var("NODALYNC_PASSWORD", "test_password");

        let temp_dir = TempDir::new().unwrap();
        let config = setup_config(&temp_dir);

        init(config.clone(), OutputFormat::Human, false).unwrap();

        let result = versions(config, OutputFormat::Human, "invalidhash");
        assert!(result.is_err());
    }
}
