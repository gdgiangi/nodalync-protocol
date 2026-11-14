//! Query content command.

use std::path::PathBuf;

use crate::config::CliConfig;
use crate::context::{parse_hash, NodeContext};
use crate::error::{CliError, CliResult};
use crate::output::{OutputFormat, QueryOutput, Render};
use crate::progress;

/// Execute the query command.
pub async fn query(
    config: CliConfig,
    format: OutputFormat,
    hash_str: &str,
    output_path: Option<PathBuf>,
) -> CliResult<String> {
    // Parse hash
    let hash = parse_hash(hash_str)?;

    // Create spinner for human output
    let spinner = progress::for_format(format, "Connecting to network...");

    // Initialize context with network and bootstrap to find peers
    let mut ctx = NodeContext::connected(config.clone()).await?;

    spinner.set_message("Fetching content metadata...");

    // Get manifest first to know price
    let manifest = find_manifest(&mut ctx, &hash, hash_str).await?;
    let price = manifest.economics.price;

    // Query content
    spinner.set_message("Querying content...");
    let response = ctx.ops.query_content(&hash, price, None).await?;
    spinner.set_message("Saving content...");

    let save_path = save_content(&config, output_path, hash_str, &response.content)?;
    spinner.finish_and_clear();

    let output = QueryOutput {
        hash: hash.to_string(),
        title: manifest.metadata.title,
        price_paid: price,
        saved_to: save_path.display().to_string(),
    };

    Ok(output.render(format))
}

/// The manifest for `hash`: the local one, else from a preview of remote content
/// (which does a DHT lookup).
async fn find_manifest(
    ctx: &mut NodeContext,
    hash: &nodalync_crypto::Hash,
    hash_str: &str,
) -> CliResult<nodalync_types::Manifest> {
    if let Some(manifest) = ctx.ops.get_content_manifest(hash)? {
        return Ok(manifest);
    }
    ctx.ops
        .preview_content(hash)
        .await
        .map(|p| p.manifest)
        .map_err(|_| CliError::NotFound(hash_str.to_string()))
}

/// Save queried content to `output_path`, or by default to the cache directory
/// under its hash. Returns where it was saved.
fn save_content(
    config: &CliConfig,
    output_path: Option<PathBuf>,
    hash_str: &str,
    content: &[u8],
) -> CliResult<PathBuf> {
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
    std::fs::write(&save_path, content)?;
    Ok(save_path)
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
        std::env::set_var("NODALYNC_PASSWORD", "test_password");

        let temp_dir = TempDir::new().unwrap();
        let config = setup_config(&temp_dir);

        init(config.clone(), OutputFormat::Human, false).unwrap();

        let result = query(config, OutputFormat::Human, "invalidhash", None).await;
        assert!(result.is_err());
    }
}
