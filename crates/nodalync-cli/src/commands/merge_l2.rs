//! Merge L2 Entity Graphs command.

use nodalync_store::ContentStore;

use crate::config::CliConfig;
use crate::context::{parse_hash, NodeContext};
use crate::error::{CliError, CliResult};
use crate::output::{MergeL2Output, OutputFormat, Render};

/// Execute the merge-l2 command.
pub fn merge_l2(
    config: CliConfig,
    format: OutputFormat,
    graph_strs: &[String],
    _title: Option<String>,
) -> CliResult<String> {
    // Parse graph hashes
    let graphs: Vec<_> = graph_strs
        .iter()
        .map(|s| parse_hash(s))
        .collect::<Result<Vec<_>, _>>()?;

    if graphs.len() < 2 {
        return Err(CliError::User(
            "At least two L2 graphs are required for merging".to_string(),
        ));
    }

    // Initialize context
    let mut ctx = NodeContext::local(config)?;

    // Verify graphs exist and are L2
    for graph in &graphs {
        let manifest = ctx
            .ops
            .get_content_manifest(graph)?
            .ok_or_else(|| CliError::NotFound(graph.to_string()))?;

        if manifest.content_type != nodalync_types::ContentType::L2 {
            return Err(CliError::User(format!(
                "Content {} is not an L2 Entity Graph",
                graph
            )));
        }

        // Check ownership
        if manifest.owner != ctx.peer_id() {
            return Err(CliError::User(format!("You don't own L2 graph {}", graph)));
        }
    }

    // Store graph count before consuming
    let merged_count = graphs.len();

    // Merge with default config
    let merge_config = nodalync_types::L2MergeConfig::default();
    let hash = ctx.ops.merge_l2(graphs, Some(merge_config))?;

    // Get merged graph to count
    let content = ctx
        .ops
        .state
        .content
        .load(&hash)?
        .ok_or_else(|| CliError::NotFound(hash.to_string()))?;

    let graph: nodalync_types::L2EntityGraph = ciborium::from_reader(&content[..])
        .map_err(|e| CliError::User(format!("Failed to parse merged graph: {}", e)))?;

    let output = MergeL2Output {
        hash: hash.to_string(),
        merged_count,
        entity_count: graph.entities.len(),
        relationship_count: graph.relationships.len(),
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
    fn test_merge_l2_not_enough_graphs() {
        std::env::set_var("NODALYNC_PASSWORD", "test_password");

        let temp_dir = TempDir::new().unwrap();
        let config = setup_config(&temp_dir);

        init(config.clone(), OutputFormat::Human, false).unwrap();

        let graphs = vec!["hash1".to_string()];
        let result = merge_l2(config, OutputFormat::Human, &graphs, None);

        assert!(result.is_err());
    }
}
