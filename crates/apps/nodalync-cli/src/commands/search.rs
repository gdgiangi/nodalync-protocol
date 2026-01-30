//! Search command.

use nodalync_store::{ManifestFilter, ManifestStore};
use nodalync_types::ContentType;

use crate::config::CliConfig;
use crate::context::NodeContext;
use crate::error::CliResult;
use crate::output::{OutputFormat, Render, SearchOutput, SearchResult};

/// Execute the search command.
pub async fn search(
    config: CliConfig,
    format: OutputFormat,
    query: &str,
    content_type: Option<ContentType>,
    limit: u32,
    all: bool,
) -> CliResult<String> {
    if all {
        // Network search: local + cached announcements + peer queries
        search_network(config, format, query, content_type, limit).await
    } else {
        // Local-only search
        search_local(config, format, query, content_type, limit)
    }
}

/// Search local manifests only.
fn search_local(
    config: CliConfig,
    format: OutputFormat,
    query: &str,
    content_type: Option<ContentType>,
    limit: u32,
) -> CliResult<String> {
    // Initialize context (local only, no network needed)
    let state = NodeContext::for_init(config)?;

    // Build filter with text query
    let mut filter = ManifestFilter::new().with_text_query(query).limit(limit);

    if let Some(ct) = content_type {
        filter = filter.with_content_type(ct);
    }

    // Search local manifests
    let manifests = state.manifests.list(filter)?;

    // Convert to search results
    let results: Vec<SearchResult> = manifests
        .iter()
        .map(|m| SearchResult {
            hash: m.hash.to_string(),
            title: m.metadata.title.clone(),
            content_type: format!("{:?}", m.content_type),
            price: m.economics.price,
            owner: m.owner.to_string(),
            description: m.metadata.description.clone(),
            source: Some("local".to_string()),
        })
        .collect();

    let total = results.len();

    let output = SearchOutput {
        query: query.to_string(),
        results,
        total,
        sources: Some("local".to_string()),
    };

    Ok(output.render(format))
}

/// Search across network: local + cached announcements + connected peers.
async fn search_network(
    config: CliConfig,
    format: OutputFormat,
    query: &str,
    content_type: Option<ContentType>,
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

    pb.set_message("Searching network...");
    let results = ctx.ops.search_network(query, content_type, limit).await?;

    pb.finish_and_clear();

    // Convert to output search results
    let output_results: Vec<SearchResult> = results
        .iter()
        .map(|r| SearchResult {
            hash: r.hash.to_string(),
            title: r.title.clone(),
            content_type: format!("{:?}", r.content_type),
            price: r.price,
            owner: r.owner.to_string(),
            description: None,
            source: Some(r.source.to_string()),
        })
        .collect();

    let total = output_results.len();

    let output = SearchOutput {
        query: query.to_string(),
        results: output_results,
        total,
        sources: Some("local + network".to_string()),
    };

    Ok(output.render(format))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_config(temp_dir: &TempDir) -> CliConfig {
        let mut config = CliConfig::default();
        config.storage.content_dir = temp_dir.path().join("content");
        config.storage.cache_dir = temp_dir.path().join("cache");
        config.storage.database = temp_dir.path().join("nodalync.db");
        config.identity.keyfile = temp_dir.path().join("identity").join("keypair.key");
        config
    }

    #[tokio::test]
    async fn test_search_no_results() {
        let temp_dir = TempDir::new().unwrap();
        let config = test_config(&temp_dir);

        let result = search(config, OutputFormat::Human, "nonexistent", None, 20, false).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        assert!(output.contains("No results found"));
    }

    #[test]
    fn test_search_output_json() {
        let output = SearchOutput {
            query: "test".to_string(),
            results: vec![SearchResult {
                hash: "abc123".to_string(),
                title: "Test Title".to_string(),
                content_type: "L0".to_string(),
                price: 1000,
                owner: "peer123".to_string(),
                description: Some("A test description".to_string()),
                source: Some("local".to_string()),
            }],
            total: 1,
            sources: Some("local".to_string()),
        };

        let json = output.render(OutputFormat::Json);
        assert!(json.contains("\"query\": \"test\""));
        assert!(json.contains("\"title\": \"Test Title\""));
        assert!(json.contains("\"source\": \"local\""));
    }
}
