//! Update content command.

use std::path::Path;

use nodalync_types::Metadata;

use crate::config::CliConfig;
use crate::context::{parse_hash, NodeContext};
use crate::error::{CliError, CliResult};
use crate::output::{OutputFormat, Render, UpdateOutput};

/// Execute the update command.
pub fn update(
    config: CliConfig,
    format: OutputFormat,
    hash_str: &str,
    file: &Path,
    title: Option<String>,
) -> CliResult<String> {
    // Parse hash
    let hash = parse_hash(hash_str)?;

    // Validate file exists
    if !file.exists() {
        return Err(CliError::FileNotFound(file.display().to_string()));
    }

    // Read file content
    let content = std::fs::read(file)?;

    // Initialize context
    let mut ctx = NodeContext::local(config)?;

    // Get existing manifest
    let existing = ctx
        .ops
        .get_content_manifest(&hash)?
        .ok_or_else(|| CliError::NotFound(hash_str.to_string()))?;

    // Create new metadata
    let new_title = title.unwrap_or_else(|| existing.metadata.title.clone());
    let mut metadata = Metadata::new(&new_title, content.len() as u64);

    // Preserve description and mime type
    if let Some(ref desc) = existing.metadata.description {
        metadata = metadata.with_description(desc);
    }
    if let Some(ref mime) = existing.metadata.mime_type {
        metadata = metadata.with_mime_type(mime);
    }

    // Update content
    let new_hash = ctx.ops.update_content(&hash, &content, metadata)?;

    // Get the new manifest to extract version info
    let new_manifest = ctx
        .ops
        .get_content_manifest(&new_hash)?
        .ok_or_else(|| CliError::NotFound(new_hash.to_string()))?;

    let output = UpdateOutput {
        previous_hash: hash.to_string(),
        previous_version: existing.version.number,
        new_hash: new_hash.to_string(),
        new_version: new_manifest.version.number,
        version_root: new_manifest.version.root.to_string(),
    };

    Ok(output.render(format))
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
        );

        assert!(result.is_err());
    }
}
