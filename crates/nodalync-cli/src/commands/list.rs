//! List local content command.

use nodalync_store::{ManifestFilter, ManifestStore};
use nodalync_types::{ContentType, Visibility};

use crate::config::CliConfig;
use crate::context::NodeContext;
use crate::error::CliResult;
use crate::output::{manifest_to_summary, ListOutput, ManifestSummary, OutputFormat, Render};

/// Execute the list command.
pub async fn list(
    config: CliConfig,
    format: OutputFormat,
    visibility_filter: Option<Visibility>,
    content_type_filter: Option<ContentType>,
    limit: u32,
    network: bool,
) -> CliResult<String> {
    if network {
        list_network(config, format, content_type_filter, limit).await
    } else {
        list_local(
            config,
            format,
            visibility_filter,
            content_type_filter,
            limit,
        )
    }
}

/// List local content only.
fn list_local(
    config: CliConfig,
    format: OutputFormat,
    visibility_filter: Option<Visibility>,
    content_type_filter: Option<ContentType>,
    limit: u32,
) -> CliResult<String> {
    let ctx = NodeContext::local(config)?;

    // Build filter
    let mut filter = ManifestFilter::default();

    if let Some(vis) = visibility_filter {
        filter = filter.with_visibility(vis);
    }

    if let Some(ct) = content_type_filter {
        filter = filter.with_content_type(ct);
    }

    filter = filter.limit(limit);

    // Query manifests
    let manifests = ctx.ops.state.manifests.list(filter)?;

    // Convert to summaries
    let summaries: Vec<_> = manifests.iter().map(manifest_to_summary).collect();
    let total = summaries.len();

    let output = ListOutput {
        manifests: summaries,
        total,
    };

    Ok(output.render(format))
}

/// List content including network announcements.
async fn list_network(
    config: CliConfig,
    format: OutputFormat,
    content_type_filter: Option<ContentType>,
    limit: u32,
) -> CliResult<String> {
    use crate::progress::{hidden, spinner};

    // Only show spinner for human output
    let pb = if format == OutputFormat::Human {
        spinner("Connecting to network...")
    } else {
        hidden()
    };

    // Initialize context with networking enabled
    let mut ctx = NodeContext::with_network(config).await?;

    pb.set_message("Bootstrapping...");
    ctx.bootstrap().await?;

    pb.set_message("Fetching content from network...");

    // Use search_network with empty query to list all available content
    let results = ctx
        .ops
        .search_network("", content_type_filter, limit)
        .await?;

    pb.finish_and_clear();

    // Convert to ManifestSummary format
    let summaries: Vec<ManifestSummary> = results
        .iter()
        .map(|r| ManifestSummary {
            hash: r.hash.to_string(),
            title: r.title.clone(),
            version: 1, // Unknown version for network results
            visibility: "Shared".to_string(),
            price: r.price,
            queries: r.total_queries,
            content_type: format!("{:?}", r.content_type),
        })
        .collect();

    let total = summaries.len();

    let output = ListOutput {
        manifests: summaries,
        total,
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

    #[tokio::test]
    async fn test_list_empty() {
        std::env::set_var("NODALYNC_PASSWORD", "test_password");

        let temp_dir = TempDir::new().unwrap();
        let config = setup_config(&temp_dir);

        init(config.clone(), OutputFormat::Human, false).unwrap();

        let result = list(config, OutputFormat::Human, None, None, 50, false).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        assert!(output.contains("No content found"));
    }

    #[tokio::test]
    async fn test_list_json() {
        std::env::set_var("NODALYNC_PASSWORD", "test_password");

        let temp_dir = TempDir::new().unwrap();
        let config = setup_config(&temp_dir);

        init(config.clone(), OutputFormat::Human, false).unwrap();

        let result = list(config, OutputFormat::Json, None, None, 50, false).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        assert!(output.contains("\"manifests\""));
        assert!(output.contains("\"total\""));
    }
}
