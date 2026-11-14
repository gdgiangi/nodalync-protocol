//! Merge L2 Entity Graphs command.

use crate::config::CliConfig;
use crate::context::{load_entity_graph, parse_hashes, require_manifest, NodeContext};
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
    let graphs = parse_graphs_to_merge(graph_strs)?;

    // Initialize context
    let mut ctx = NodeContext::local(config)?;

    // Verify graphs exist, are L2 and are ours
    check_owned_l2_graphs(&ctx, &graphs)?;

    // Store graph count before consuming
    let merged_count = graphs.len();

    // Merge with default config
    let merge_config = nodalync_types::L2MergeConfig::default();
    let hash = ctx.ops.merge_l2(graphs, Some(merge_config))?;

    // Get merged graph to count
    let graph = load_entity_graph(&ctx, &hash, "merged graph")?;

    let output = MergeL2Output {
        hash: hash.to_string(),
        merged_count,
        entity_count: graph.entities.len(),
        relationship_count: graph.relationships.len(),
    };

    Ok(output.render(format))
}

/// Parse the graph hashes; at least two are required.
fn parse_graphs_to_merge(graph_strs: &[String]) -> CliResult<Vec<nodalync_crypto::Hash>> {
    let graphs = parse_hashes(graph_strs)?;
    if graphs.len() < 2 {
        return Err(CliError::User(
            "At least two L2 graphs are required for merging".to_string(),
        ));
    }
    Ok(graphs)
}

/// Every graph must exist, be an L2 entity graph, and be ours.
fn check_owned_l2_graphs(ctx: &NodeContext, graphs: &[nodalync_crypto::Hash]) -> CliResult<()> {
    for graph in graphs {
        check_owned_l2_graph(ctx, graph)?;
    }
    Ok(())
}

/// The graph must exist, be an L2 entity graph, and be ours.
fn check_owned_l2_graph(ctx: &NodeContext, graph: &nodalync_crypto::Hash) -> CliResult<()> {
    let manifest = require_manifest(&ctx.ops, graph, &graph.to_string())?;

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
    Ok(())
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
