//! Preview content command.

use crate::config::CliConfig;
use crate::context::{parse_hash, NodeContext};
use crate::error::{CliError, CliResult};
use crate::output::{l1_to_preview, OutputFormat, PreviewOutput, Render};

/// Execute the preview command.
pub async fn preview(config: CliConfig, format: OutputFormat, hash_str: &str) -> CliResult<String> {
    // Parse hash
    let hash = parse_hash(hash_str)?;

    // Initialize context (try network first for remote content)
    let mut ctx = NodeContext::with_network(config).await?;

    // Bootstrap to find peers
    ctx.bootstrap().await?;

    // Try to preview
    let preview_response = ctx
        .ops
        .preview_content(&hash)
        .await
        .map_err(|_| CliError::NotFound(hash_str.to_string()))?;

    let manifest = &preview_response.manifest;

    // Get L1 mentions
    let mentions = Some(l1_to_preview(&preview_response.l1_summary));

    let output = PreviewOutput {
        hash: manifest.hash.to_string(),
        title: manifest.metadata.title.clone(),
        owner: manifest.owner.to_string(),
        price: manifest.economics.price,
        queries: manifest.economics.total_queries,
        content_type: format!("{:?}", manifest.content_type),
        visibility: format!("{:?}", manifest.visibility),
        size: manifest.metadata.content_size,
        mentions,
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
        config.network.enabled = false;
        config
    }

    #[tokio::test]
    async fn test_preview_not_found() {
        std::env::set_var("NODALYNC_PASSWORD", "test_password");

        let temp_dir = TempDir::new().unwrap();
        let config = setup_config(&temp_dir);

        init(config.clone(), OutputFormat::Human, false).unwrap();

        let result = preview(config, OutputFormat::Human, "invalidhash").await;
        assert!(result.is_err());
    }
}
