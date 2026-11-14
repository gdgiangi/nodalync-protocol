//! Build L2 Entity Graph command.

use crate::config::CliConfig;
use crate::context::{load_entity_graph, parse_hashes, require_manifest, NodeContext};
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
    let sources = parse_l1_sources(source_strs)?;

    // Initialize context
    let mut ctx = NodeContext::local(config)?;

    // Verify sources exist
    require_source_manifests(&ctx, &sources)?;

    // Get title
    let _title = title.unwrap_or_else(|| "Entity Graph".to_string());

    // Store source count before consuming
    let source_count = sources.len();

    // Build L2 with default config
    let l2_config = nodalync_types::L2BuildConfig::default();
    let hash = ctx.ops.build_l2(sources, Some(l2_config))?;

    // Load entity graph from content to count entities and relationships
    let graph = load_entity_graph(&ctx, &hash, "entity graph")?;

    let output = BuildL2Output {
        hash: hash.to_string(),
        entity_count: graph.entities.len(),
        relationship_count: graph.relationships.len(),
        source_count,
    };

    Ok(output.render(format))
}

/// Every source must have a manifest here.
fn require_source_manifests(ctx: &NodeContext, sources: &[nodalync_crypto::Hash]) -> CliResult<()> {
    for source in sources {
        require_manifest(&ctx.ops, source, &source.to_string())?;
    }
    Ok(())
}

/// Parse the L1 source hashes; at least one is required.
fn parse_l1_sources(source_strs: &[String]) -> CliResult<Vec<nodalync_crypto::Hash>> {
    let sources = parse_hashes(source_strs)?;
    if sources.is_empty() {
        return Err(CliError::User(
            "At least one L1 source is required".to_string(),
        ));
    }
    Ok(sources)
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
