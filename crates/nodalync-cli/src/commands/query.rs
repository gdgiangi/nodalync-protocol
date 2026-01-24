//! Query content command.

use std::path::PathBuf;

use crate::config::CliConfig;
use crate::context::{parse_hash, NodeContext};
use crate::error::{CliError, CliResult};
use crate::output::{OutputFormat, QueryOutput, Render};

/// Execute the query command.
pub async fn query(
    config: CliConfig,
    format: OutputFormat,
    hash_str: &str,
    output_path: Option<PathBuf>,
) -> CliResult<String> {
    // Parse hash
    let hash = parse_hash(hash_str)?;

    // Initialize context with network
    let mut ctx = NodeContext::with_network(config.clone()).await?;

    // Get manifest first to know price
    let manifest = ctx
        .ops
        .get_content_manifest(&hash)?
        .or_else(|| {
            // Try preview for remote content
            ctx.ops.preview_content(&hash).ok().map(|p| p.manifest)
        })
        .ok_or_else(|| CliError::NotFound(hash_str.to_string()))?;

    let price = manifest.economics.price;
    let title = manifest.metadata.title.clone();

    // Query content
    let response = ctx.ops.query_content(&hash, price, None).await?;

    // Determine output path
    let save_path = output_path.unwrap_or_else(|| {
        let cache_dir = config.storage.cache_dir.clone();
        std::fs::create_dir_all(&cache_dir).ok();
        cache_dir.join(hash_str)
    });

    // Save content to file
    if let Some(parent) = save_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&save_path, &response.content)?;

    let output = QueryOutput {
        hash: hash.to_string(),
        title,
        price_paid: price,
        saved_to: save_path.display().to_string(),
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
    async fn test_query_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let config = setup_config(&temp_dir);

        init(config.clone(), OutputFormat::Human).unwrap();

        let result = query(config, OutputFormat::Human, "invalidhash", None).await;
        assert!(result.is_err());
    }
}
