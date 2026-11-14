//! Update content command.

use std::path::Path;

use nodalync_store::ManifestStore;
use nodalync_types::Metadata;

use crate::config::{hbar_to_tinybars, CliConfig};
use crate::context::{parse_hash, read_input_file, require_manifest, NodeContext};
use crate::error::CliResult;
use crate::output::{OutputFormat, Render, UpdateOutput};

/// Execute the update command.
pub fn update(
    config: CliConfig,
    format: OutputFormat,
    hash_str: &str,
    file: &Path,
    title: Option<String>,
    price: Option<f64>,
) -> CliResult<String> {
    // Parse hash
    let hash = parse_hash(hash_str)?;

    // Read file content (which must exist)
    let content = read_input_file(file)?;

    // Initialize context and get existing manifest
    let (mut ctx, existing) = open_existing(config, &hash, hash_str)?;

    // Update content
    let metadata = updated_metadata(&existing, title, content.len());
    let new_hash = ctx.ops.update_content(&hash, &content, metadata)?;

    // Carry forward economics from previous version
    let new_manifest = carry_forward_economics(&mut ctx, &new_hash, &existing, price)?;

    let output = UpdateOutput {
        previous_hash: hash.to_string(),
        previous_version: existing.version.number,
        new_hash: new_hash.to_string(),
        new_version: new_manifest.version.number,
        version_root: new_manifest.version.root.to_string(),
    };

    Ok(output.render(format))
}

/// A local context and the manifest of the content to update.
fn open_existing(
    config: CliConfig,
    hash: &nodalync_crypto::Hash,
    hash_str: &str,
) -> CliResult<(NodeContext, nodalync_types::Manifest)> {
    let ctx = NodeContext::local(config)?;
    let existing = require_manifest(&ctx.ops, hash, hash_str)?;
    Ok((ctx, existing))
}

/// Metadata for the new version: the given title or the previous one, preserving
/// description and mime type.
fn updated_metadata(
    existing: &nodalync_types::Manifest,
    title: Option<String>,
    size: usize,
) -> Metadata {
    let new_title = title.unwrap_or_else(|| existing.metadata.title.clone());
    let mut metadata = Metadata::new(&new_title, size as u64);

    // Preserve description and mime type
    if let Some(ref desc) = existing.metadata.description {
        metadata = metadata.with_description(desc);
    }
    if let Some(ref mime) = existing.metadata.mime_type {
        metadata = metadata.with_mime_type(mime);
    }
    metadata
}

/// Give the new version the explicit price (HBAR), or else the previous version's
/// price, and the previous currency. Returns the updated manifest.
fn carry_forward_economics(
    ctx: &mut NodeContext,
    new_hash: &nodalync_crypto::Hash,
    existing: &nodalync_types::Manifest,
    price: Option<f64>,
) -> CliResult<nodalync_types::Manifest> {
    let new_price = match price {
        Some(p) => hbar_to_tinybars(p),   // explicit override
        None => existing.economics.price, // carry forward
    };
    let new_hash_str = new_hash.to_string();
    let mut new_manifest = require_manifest(&ctx.ops, new_hash, &new_hash_str)?;
    new_manifest.economics.price = new_price;
    new_manifest.economics.currency = existing.economics.currency;
    ctx.ops.state.manifests.update(&new_manifest)?;

    // Get the updated manifest for version info output
    require_manifest(&ctx.ops, new_hash, &new_hash_str)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::init::init;
    use std::io::Write;
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
    fn test_update_not_found() {
        std::env::set_var("NODALYNC_PASSWORD", "test_password");

        let temp_dir = TempDir::new().unwrap();
        let config = setup_config(&temp_dir);

        init(config.clone(), OutputFormat::Human, false).unwrap();

        // Create a file to update with
        let file_path = temp_dir.path().join("new_content.txt");
        let mut file = std::fs::File::create(&file_path).unwrap();
        file.write_all(b"new content").unwrap();

        // Try to update non-existent content
        let result = update(
            config,
            OutputFormat::Human,
            "invalidhash123",
            &file_path,
            None,
            None,
        );

        assert!(result.is_err());
    }
}
