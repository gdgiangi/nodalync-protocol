//! Publish content command.

use std::path::Path;

use colored::Colorize;
use nodalync_crypto::content_hash;
use nodalync_types::{Metadata, Visibility};

use crate::config::{ndl_to_units, CliConfig};
use crate::context::NodeContext;
use crate::error::{CliError, CliResult};
use crate::output::{OutputFormat, PublishOutput, Render};
use crate::progress;

/// Execute the publish command.
pub async fn publish(
    config: CliConfig,
    format: OutputFormat,
    file: &Path,
    price: Option<f64>,
    visibility: Visibility,
    title: Option<String>,
    description: Option<String>,
) -> CliResult<String> {
    // Validate file exists
    if !file.exists() {
        return Err(CliError::FileNotFound(file.display().to_string()));
    }

    // Create spinner for human output
    let spinner = if format == OutputFormat::Human {
        progress::spinner("Reading file...")
    } else {
        progress::hidden()
    };

    // Read file content
    let content = std::fs::read(file)?;

    // Guard: reject empty files
    if content.is_empty() {
        return Err(CliError::user(
            "Cannot publish an empty file. The file has 0 bytes of content.",
        ));
    }

    // Guard: warn about binary content
    // Check first 8KB for null bytes as a heuristic for binary data
    let check_len = content.len().min(8192);
    let has_null_bytes = content[..check_len].contains(&0);
    if has_null_bytes && format == OutputFormat::Human {
        eprintln!(
            "{}: File appears to be binary. Binary content cannot be meaningfully indexed or queried.",
            "Warning".yellow().bold()
        );
        if crate::prompt::is_interactive() && !crate::prompt::confirm("Publish anyway?")? {
            return Ok("Cancelled.".to_string());
        }
    }

    spinner.set_message("Hashing content...");

    // Get title from filename if not provided
    let title = title.unwrap_or_else(|| {
        file.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Untitled")
            .to_string()
    });

    // Convert price to units
    let price_units = price
        .map(ndl_to_units)
        .unwrap_or_else(|| config.economics.default_price_units());

    // Initialize context with network
    spinner.set_message("Connecting to network...");
    let mut ctx = NodeContext::with_network(config).await?;

    // Bootstrap the network to find peers
    ctx.bootstrap().await?;

    // Subscribe to announcements (required for GossipSub mesh formation)
    if let Some(ref network) = ctx.network {
        network.subscribe_announcements().await?;
    }

    // Create metadata
    let mut metadata = Metadata::new(&title, content.len() as u64);
    if let Some(desc) = description {
        metadata = metadata.with_description(&desc);
    }

    // Detect mime type from extension
    if let Some(ext) = file.extension().and_then(|e| e.to_str()) {
        let mime_type = match ext.to_lowercase().as_str() {
            "txt" => "text/plain",
            "md" => "text/markdown",
            "html" | "htm" => "text/html",
            "json" => "application/json",
            "pdf" => "application/pdf",
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            _ => "application/octet-stream",
        };
        metadata = metadata.with_mime_type(mime_type);
    }

    // Check if this content already exists (re-publish detection)
    let computed_hash = content_hash(&content);
    if let Ok(Some(existing)) = ctx.ops.get_content_manifest(&computed_hash) {
        if format == OutputFormat::Human {
            eprintln!(
                "{}: Content with this hash already exists (title: \"{}\", price: {} units).",
                "Warning".yellow().bold(),
                existing.metadata.title,
                existing.economics.price,
            );
            eprintln!(
                "  Use '{}' to update metadata, or '{}' to create a new version.",
                "nodalync publish --update".bold(),
                "nodalync update".bold(),
            );
            if crate::prompt::is_interactive() {
                if !crate::prompt::confirm("Overwrite existing metadata?")? {
                    return Ok("Cancelled.".to_string());
                }
            } else {
                return Err(CliError::user(
                    "Content already exists. Use --force to overwrite, or 'nodalync update' to create a new version.",
                ));
            }
        } else {
            // In JSON mode, always error on re-publish to avoid silent overwrites
            return Err(CliError::user(format!(
                "Content already exists with hash {}. Use 'nodalync update' to create a new version.",
                computed_hash
            )));
        }
    }

    // Create content
    let hash = ctx.ops.create_content(&content, metadata.clone())?;

    // Extract L1 mentions (if L0 content)
    spinner.set_message("Extracting mentions...");
    let mentions = match ctx.ops.extract_l1_summary(&hash) {
        Ok(summary) => Some(summary.mention_count as usize),
        Err(_) => None,
    };

    // Publish content
    spinner.set_message("Publishing to network...");
    ctx.ops
        .publish_content(&hash, visibility, price_units)
        .await?;

    // Wait for GossipSub propagation (needs time for mesh to form)
    spinner.set_message("Propagating to network...");
    let wait_secs = ctx.config.network.gossipsub_propagation_wait;
    tokio::time::sleep(std::time::Duration::from_secs(wait_secs)).await;
    spinner.finish_and_clear();

    // Create output
    let output = PublishOutput {
        hash: hash.to_string(),
        title,
        size: content.len() as u64,
        price: price_units,
        visibility: format!("{:?}", visibility),
        mentions,
    };

    Ok(output.render(format))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_config(temp_dir: &TempDir) -> CliConfig {
        let mut config = CliConfig::default();
        config.storage.content_dir = temp_dir.path().join("content");
        config.storage.cache_dir = temp_dir.path().join("cache");
        config.storage.database = temp_dir.path().join("nodalync.db");
        config.identity.keyfile = temp_dir.path().join("identity").join("keypair.key");
        config.network.enabled = false; // Disable network for tests
        config
    }

    #[tokio::test]
    async fn test_publish_file_not_found() {
        std::env::set_var("NODALYNC_PASSWORD", "test_password");

        let temp_dir = TempDir::new().unwrap();
        let config = setup_config(&temp_dir);

        // Initialize identity first
        crate::commands::init::init(config.clone(), OutputFormat::Human, false).unwrap();

        let result = publish(
            config,
            OutputFormat::Human,
            Path::new("/nonexistent/file.txt"),
            None,
            Visibility::Shared,
            None,
            None,
        )
        .await;

        assert!(matches!(result, Err(CliError::FileNotFound(_))));
    }
}
