//! Delete local content command.

use nodalync_store::{ContentStore, ManifestStore};
use nodalync_types::Visibility;

use crate::config::CliConfig;
use crate::context::{parse_hash, NodeContext};
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

    // Initialize context
    let mut ctx = NodeContext::local(config)?;

    // Verify content exists
    let manifest = ctx
        .ops
        .get_content_manifest(&hash)?
        .ok_or_else(|| CliError::NotFound(hash_str.to_string()))?;

    // Check ownership
    if manifest.owner != ctx.peer_id() {
        return Err(CliError::User("You don't own this content".to_string()));
    }

    // Prompt for confirmation if not forcing
    if !force {
        if !crate::prompt::is_interactive() {
            return Err(CliError::ConfirmationRequired(
                "Delete requires confirmation. Use --force to skip, or run in a terminal for interactive confirmation.".into(),
            ));
        }
        let prompt = format!(
            "Delete \"{}\"? This cannot be undone.",
            manifest.metadata.title
        );
        if !crate::prompt::confirm(&prompt)? {
            return Ok("Cancelled.".to_string());
        }
    }

    // Take content offline (manifest preserved for provenance)
    ctx.ops.state.content.delete(&hash)?;
    let mut updated = manifest.clone();
    updated.visibility = Visibility::Offline;
    ctx.ops.state.manifests.update(&updated)?;

    let output = DeleteOutput {
        hash: hash.to_string(),
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
