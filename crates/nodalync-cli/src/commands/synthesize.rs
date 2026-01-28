//! Create L3 synthesis command.

use std::path::Path;

use nodalync_types::{Metadata, Visibility};

use crate::config::{ndl_to_units, CliConfig};
use crate::context::{parse_hash, NodeContext};
use crate::error::{CliError, CliResult};
use crate::output::{OutputFormat, Render, SynthesizeOutput};

/// Execute the synthesize command.
pub async fn synthesize(
    config: CliConfig,
    format: OutputFormat,
    source_strs: &[String],
    output_file: &Path,
    title: Option<String>,
    price: Option<f64>,
    publish: bool,
) -> CliResult<String> {
    // Validate output file exists
    if !output_file.exists() {
        return Err(CliError::FileNotFound(output_file.display().to_string()));
    }

    // Parse source hashes
    let sources: Vec<_> = source_strs
        .iter()
        .map(|s| parse_hash(s))
        .collect::<Result<Vec<_>, _>>()?;

    if sources.is_empty() {
        return Err(CliError::User(
            "At least one source is required".to_string(),
        ));
    }

    // Read synthesis content
    let content = std::fs::read(output_file)?;

    // Initialize context
    let mut ctx = NodeContext::with_network(config.clone()).await?;

    // Verify sources exist and are owned/queried
    for source in &sources {
        let manifest = ctx
            .ops
            .get_content_manifest(source)?
            .ok_or_else(|| CliError::NotFound(source.to_string()))?;

        // Check if owned or in cache (queried)
        use nodalync_store::CacheStore;
        let is_owned = manifest.owner == ctx.peer_id();
        let is_cached = ctx.ops.state.cache.is_cached(source);

        if !is_owned && !is_cached {
            return Err(CliError::User(format!(
                "Source {} must be owned or previously queried",
                source
            )));
        }
    }

    // Get title
    let title = title.unwrap_or_else(|| {
        output_file
            .file_stem()
            .and_then(|n| n.to_str())
            .unwrap_or("Synthesis")
            .to_string()
    });

    // Create metadata
    let metadata = Metadata::new(&title, content.len() as u64);

    // Derive L3 content
    let hash = ctx.ops.derive_content(&sources, &content, metadata)?;

    // Get provenance info
    let manifest = ctx.ops.get_content_manifest(&hash)?.unwrap();
    let provenance_roots = manifest.provenance.derived_from.len();

    // Optionally publish
    let (published, final_price) = if publish {
        let price_units = price
            .map(ndl_to_units)
            .unwrap_or_else(|| config.economics.default_price_units());
        ctx.ops
            .publish_content(&hash, Visibility::Shared, price_units)
            .await?;
        (true, Some(price_units))
    } else {
        (false, None)
    };

    let output = SynthesizeOutput {
        hash: hash.to_string(),
        source_count: sources.len(),
        provenance_roots,
        published,
        price: final_price,
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
    async fn test_synthesize_file_not_found() {
        std::env::set_var("NODALYNC_PASSWORD", "test_password");

        let temp_dir = TempDir::new().unwrap();
        let config = setup_config(&temp_dir);

        init(config.clone(), OutputFormat::Human, false).unwrap();

        let sources = vec!["hash1".to_string()];
        let result = synthesize(
            config,
            OutputFormat::Human,
            &sources,
            Path::new("/nonexistent.txt"),
            None,
            None,
            false,
        )
        .await;

        assert!(matches!(result, Err(CliError::FileNotFound(_))));
    }

    #[tokio::test]
    async fn test_synthesize_no_sources() {
        std::env::set_var("NODALYNC_PASSWORD", "test_password");

        let temp_dir = TempDir::new().unwrap();
        let config = setup_config(&temp_dir);

        init(config.clone(), OutputFormat::Human, false).unwrap();

        // Create output file
        let output_path = temp_dir.path().join("synthesis.txt");
        std::fs::write(&output_path, b"synthesis content").unwrap();

        let sources: Vec<String> = vec![];
        let result = synthesize(
            config,
            OutputFormat::Human,
            &sources,
            &output_path,
            None,
            None,
            false,
        )
        .await;

        assert!(result.is_err());
    }
}
