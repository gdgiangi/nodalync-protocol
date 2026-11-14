//! Create L3 synthesis command.

use std::path::Path;

use nodalync_types::{Metadata, Visibility};

use crate::config::{ndl_to_units, CliConfig};
use crate::context::{parse_hashes, require_file, require_manifest, NodeContext};
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
    let (sources, content) = load_synthesis_inputs(output_file, source_strs)?;

    // Initialize context
    let mut ctx = NodeContext::with_network(config.clone()).await?;

    // Verify sources exist and are owned/queried
    check_sources_usable(&ctx, &sources)?;

    // Derive L3 content
    let title = synthesis_title(title, output_file);
    let (hash, provenance_roots) = derive_l3(&mut ctx, &sources, &content, &title)?;

    // Optionally publish
    let price_units = price
        .map(ndl_to_units)
        .unwrap_or_else(|| config.economics.default_price_units());
    let final_price = publish_if_requested(&mut ctx, &hash, publish, price_units).await?;

    let output = SynthesizeOutput {
        hash: hash.to_string(),
        source_count: sources.len(),
        provenance_roots,
        published: publish,
        price: final_price,
    };

    Ok(output.render(format))
}

/// The source hashes (at least one) and the synthesis content, checking the file
/// exists before parsing the sources.
fn load_synthesis_inputs(
    output_file: &Path,
    source_strs: &[String],
) -> CliResult<(Vec<nodalync_crypto::Hash>, Vec<u8>)> {
    // Validate output file exists
    require_file(output_file)?;

    // Parse source hashes
    let sources = parse_hashes(source_strs)?;
    if sources.is_empty() {
        return Err(CliError::User(
            "At least one source is required".to_string(),
        ));
    }

    // Read synthesis content
    Ok((sources, std::fs::read(output_file)?))
}

/// Every source must exist and be owned or previously queried.
fn check_sources_usable(ctx: &NodeContext, sources: &[nodalync_crypto::Hash]) -> CliResult<()> {
    for source in sources {
        check_source_usable(ctx, source)?;
    }
    Ok(())
}

/// A source must exist and be owned or in cache (queried).
fn check_source_usable(ctx: &NodeContext, source: &nodalync_crypto::Hash) -> CliResult<()> {
    use nodalync_store::CacheStore;

    let manifest = require_manifest(&ctx.ops, source, &source.to_string())?;
    let is_owned = manifest.owner == ctx.peer_id();
    if !is_owned && !ctx.ops.state.cache.is_cached(source) {
        return Err(CliError::User(format!(
            "Source {} must be owned or previously queried",
            source
        )));
    }
    Ok(())
}

/// The given title, else the output file's name without extension, else "Synthesis".
fn synthesis_title(title: Option<String>, output_file: &Path) -> String {
    title.unwrap_or_else(|| {
        output_file
            .file_stem()
            .and_then(|n| n.to_str())
            .unwrap_or("Synthesis")
            .to_string()
    })
}

/// Derive L3 content; returns its hash and the number of sources it derives from.
fn derive_l3(
    ctx: &mut NodeContext,
    sources: &[nodalync_crypto::Hash],
    content: &[u8],
    title: &str,
) -> CliResult<(nodalync_crypto::Hash, usize)> {
    let metadata = Metadata::new(title, content.len() as u64);
    let hash = ctx.ops.derive_content(sources, content, metadata)?;

    // Get provenance info
    let manifest = ctx.ops.get_content_manifest(&hash)?.unwrap();
    Ok((hash, manifest.provenance.derived_from.len()))
}

/// Publish as shared at `price_units` when requested; returns the price when published.
async fn publish_if_requested(
    ctx: &mut NodeContext,
    hash: &nodalync_crypto::Hash,
    publish: bool,
    price_units: u64,
) -> CliResult<Option<u64>> {
    if !publish {
        return Ok(None);
    }
    ctx.ops
        .publish_content(hash, Visibility::Shared, price_units)
        .await?;
    Ok(Some(price_units))
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
