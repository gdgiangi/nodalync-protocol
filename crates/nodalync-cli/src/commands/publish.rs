//! Publish content command.

use std::path::Path;

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
        crate::commands::init::init(config.clone(), OutputFormat::Human).unwrap();

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
