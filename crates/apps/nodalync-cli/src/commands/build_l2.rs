//! Build L2 Entity Graph command.

use nodalync_store::ContentStore;

use crate::config::CliConfig;
use crate::context::{parse_hash, NodeContext};
use crate::error::{CliError, CliResult};
use crate::output::{BuildL2Output, OutputFormat, Render};

/// Execute the build-l2 command.
pub fn build_l2(
    config: CliConfig,
    format: OutputFormat,
    source_strs: &[String],
    title: Option<String>,
) -> CliResult<String> {
    // Parse source hashes
    let sources: Vec<_> = source_strs
        .iter()
        .map(|s| parse_hash(s))
        .collect::<Result<Vec<_>, _>>()?;

    if sources.is_empty() {
        return Err(CliError::User(
            "At least one L1 source is required".to_string(),
        ));
    }

    // Initialize context
    let mut ctx = NodeContext::local(config)?;

    // Verify sources exist
    for source in &sources {
        ctx.ops
            .get_content_manifest(source)?
            .ok_or_else(|| CliError::NotFound(source.to_string()))?;
    }

    // Get title
    let _title = title.unwrap_or_else(|| "Entity Graph".to_string());

    // Store source count before consuming
    let source_count = sources.len();

    // Build L2 with default config
    let l2_config = nodalync_types::L2BuildConfig::default();
    let hash = ctx.ops.build_l2(sources, Some(l2_config))?;

    // Get the entity graph to count entities and relationships
    let _manifest = ctx.ops.get_content_manifest(&hash)?.unwrap();

    // Load entity graph from content
    let content = ctx
        .ops
        .state
        .content
        .load(&hash)?
        .ok_or_else(|| CliError::NotFound(hash.to_string()))?;

    // Deserialize to count
    let graph: nodalync_types::L2EntityGraph = ciborium::from_reader(&content[..])
        .map_err(|e| CliError::User(format!("Failed to parse entity graph: {}", e)))?;

    let output = BuildL2Output {
        hash: hash.to_string(),
        entity_count: graph.entities.len(),
        relationship_count: graph.relationships.len(),
        source_count,
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
    fn test_build_l2_no_sources() {
        std::env::set_var("NODALYNC_PASSWORD", "test_password");

        let temp_dir = TempDir::new().unwrap();
        let config = setup_config(&temp_dir);

        init(config.clone(), OutputFormat::Human, false).unwrap();

        let sources: Vec<String> = vec![];
        let result = build_l2(config, OutputFormat::Human, &sources, None);

        assert!(result.is_err());
    }

    #[test]
    fn test_build_l2_source_not_found() {
        std::env::set_var("NODALYNC_PASSWORD", "test_password");

        let temp_dir = TempDir::new().unwrap();
        let config = setup_config(&temp_dir);

        init(config.clone(), OutputFormat::Human, false).unwrap();

        let sources = vec!["invalidhash".to_string()];
        let result = build_l2(config, OutputFormat::Human, &sources, None);

        assert!(result.is_err());
    }
}
