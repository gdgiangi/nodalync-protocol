//! Delete local content command.

use nodalync_store::{ContentStore, ManifestStore};
use nodalync_types::Visibility;

use crate::config::CliConfig;
use crate::context::{parse_hash, require_owned_manifest, NodeContext};
use crate::error::{CliError, CliResult};
use crate::output::{DeleteOutput, OutputFormat, Render};

/// Execute the delete command.
pub fn delete(
    config: CliConfig,
    format: OutputFormat,
    hash_str: &str,
    force: bool,
) -> CliResult<String> {
    // Parse hash
    let hash = parse_hash(hash_str)?;

    // Initialize context and verify content exists and we own it
    let (mut ctx, manifest) = open_owned_content(config, &hash, hash_str)?;

    // Prompt for confirmation if not forcing
    if !confirm_delete(force, &manifest.metadata.title)? {
        return Ok("Cancelled.".to_string());
    }

    // Take content offline (manifest preserved for provenance)
    take_offline(&mut ctx, &hash, manifest)?;

    let output = DeleteOutput {
        hash: hash.to_string(),
    };

    Ok(output.render(format))
}

/// A local context and the manifest of content we own.
fn open_owned_content(
    config: CliConfig,
    hash: &nodalync_crypto::Hash,
    hash_str: &str,
) -> CliResult<(NodeContext, nodalync_types::Manifest)> {
    let ctx = NodeContext::local(config)?;
    let manifest = require_owned_manifest(&ctx, hash, hash_str)?;
    Ok((ctx, manifest))
}

/// Whether to go ahead with the delete: always with `force`, otherwise only after the
/// user confirms in an interactive terminal.
fn confirm_delete(force: bool, title: &str) -> CliResult<bool> {
    if force {
        return Ok(true);
    }
    if !crate::prompt::is_interactive() {
        return Err(CliError::ConfirmationRequired(
            "Delete requires confirmation. Use --force to skip, or run in a terminal for interactive confirmation.".into(),
        ));
    }
    let prompt = format!("Delete \"{}\"? This cannot be undone.", title);
    Ok(crate::prompt::confirm(&prompt)?)
}

/// Delete the content bytes and mark the manifest Offline (preserved for provenance).
fn take_offline(
    ctx: &mut NodeContext,
    hash: &nodalync_crypto::Hash,
    mut manifest: nodalync_types::Manifest,
) -> CliResult<()> {
    ctx.ops.state.content.delete(hash)?;
    manifest.visibility = Visibility::Offline;
    ctx.ops.state.manifests.update(&manifest)?;
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
    fn test_delete_not_found() {
        std::env::set_var("NODALYNC_PASSWORD", "test_password");

        let temp_dir = TempDir::new().unwrap();
        let config = setup_config(&temp_dir);

        init(config.clone(), OutputFormat::Human, false).unwrap();

        let result = delete(config, OutputFormat::Human, "invalidhash", true);
        assert!(result.is_err());
    }
}
