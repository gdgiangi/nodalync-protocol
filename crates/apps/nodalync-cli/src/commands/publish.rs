//! Publish content command.

use std::path::Path;

use colored::Colorize;
use nodalync_crypto::content_hash;
use nodalync_types::{Metadata, Visibility};

use crate::config::{ndl_to_units, tinybars_to_hbar, CliConfig};
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

    // Reject directories (Issue #22/#82: clear error with correct INVALID_INPUT code)
    if file.is_dir() {
        return Err(CliError::InvalidInput(
            "Cannot publish a directory. Please specify a file path.".to_string(),
        ));
    }

    // Create spinner for human output
    let spinner = if format == OutputFormat::Human {
        progress::spinner("Reading file...")
    } else {
        progress::hidden()
    };

    // Read file content
    let content = std::fs::read(file)?;

    // Guard: reject empty files (Issue #49: use InvalidInput, not User/AccessDenied)
    if content.is_empty() {
        return Err(CliError::InvalidInput(
            "Cannot publish an empty file. The file has 0 bytes of content.".to_string(),
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

    // Validate price BEFORE writing any content to disk.
    // This prevents ghost content from being stored when price validation fails.
    if price_units > 0 {
        nodalync_econ::validate_price(price_units).map_err(|e| {
            // Issue #81: Show HBAR values instead of raw tinybars for user comprehension
            match &e {
                nodalync_econ::EconError::PriceTooHigh { price, max } => CliError::user(format!(
                    "Invalid price: {} HBAR exceeds maximum {} HBAR",
                    tinybars_to_hbar(*price),
                    tinybars_to_hbar(*max)
                )),
                _ => CliError::user(format!("Invalid price: {}", e)),
            }
        })?;
    }

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
                "  Delete it first with '{}', or create a new version with '{}'.",
                format!("nodalync delete {}", computed_hash).bold(),
                "nodalync update".bold(),
            );
            if crate::prompt::is_interactive() {
                if !crate::prompt::confirm("Re-publish with new metadata?")? {
                    return Ok("Cancelled.".to_string());
                }
            } else {
                return Err(CliError::user(format!(
                    "Content already exists with hash {}. Delete it first with 'nodalync delete {}', or create a new version with 'nodalync update'.",
                    computed_hash, computed_hash
                )));
            }
        } else {
            // In JSON mode, always error on re-publish to avoid silent overwrites
            return Err(CliError::user(format!(
                "Content already exists with hash {}. Delete it first with 'nodalync delete {}', or create a new version with 'nodalync update'.",
                computed_hash, computed_hash
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

    /// Regression test for Issue #16: ghost content on failed publish.
    ///
    /// Publishing with an extreme price should fail early (before writing
    /// content to disk) and leave no content in the store.
    #[tokio::test]
    async fn test_publish_extreme_price_no_ghost_content() {
        std::env::set_var("NODALYNC_PASSWORD", "test_password");

        let temp_dir = TempDir::new().unwrap();
        let config = setup_config(&temp_dir);

        // Initialize identity first
        crate::commands::init::init(config.clone(), OutputFormat::Human, false).unwrap();

        // Create a file to publish
        let file_path = temp_dir.path().join("extreme_price.txt");
        std::fs::write(&file_path, "Content with extreme price").unwrap();

        // Publish with extreme price (well above MAX_PRICE = 10^16)
        let result = publish(
            config.clone(),
            OutputFormat::Json,
            &file_path,
            Some(999_999_999_999_999_999.0), // Extreme price in HBAR
            Visibility::Shared,
            None,
            None,
        )
        .await;

        assert!(result.is_err(), "Publish with extreme price should fail");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("price"),
            "Error should mention price: {}",
            err_msg
        );
        // Issue #81: Error should show HBAR values, not raw tinybars
        assert!(
            err_msg.contains("HBAR"),
            "Price error should show HBAR units, got: {}",
            err_msg
        );

        // Verify no content was stored (no ghost content)
        use nodalync_store::ManifestStore;
        let ctx = crate::context::NodeContext::local(config).unwrap();
        let filter = nodalync_store::ManifestFilter::new();
        let manifests = ctx.ops.state.manifests.list(filter).unwrap();
        assert!(
            manifests.is_empty(),
            "No content should exist after failed publish, found {} items",
            manifests.len()
        );
    }

    /// Regression test for Issue #22/#82: publishing a directory should show a clear error
    /// with InvalidInput variant (not User/ACCESS_DENIED).
    #[tokio::test]
    async fn test_publish_directory_rejected() {
        std::env::set_var("NODALYNC_PASSWORD", "test_password");

        let temp_dir = TempDir::new().unwrap();
        let config = setup_config(&temp_dir);

        // Initialize identity first
        crate::commands::init::init(config.clone(), OutputFormat::Human, false).unwrap();

        // Try to publish a directory
        let result = publish(
            config,
            OutputFormat::Json,
            temp_dir.path(),
            None,
            Visibility::Shared,
            None,
            None,
        )
        .await;

        assert!(result.is_err(), "Publishing a directory should fail");
        let err = result.unwrap_err();

        // Issue #82: Should be InvalidInput, not User (which maps to ACCESS_DENIED)
        assert!(
            matches!(err, CliError::InvalidInput(_)),
            "Directory error should be InvalidInput, got: {:?}",
            err
        );
        assert_ne!(
            err.error_code(),
            nodalync_types::ErrorCode::AccessDenied,
            "Directory error code should NOT be ACCESS_DENIED"
        );
        assert!(
            err.to_string().contains("directory"),
            "Error should mention 'directory', got: {}",
            err
        );
    }

    /// Regression test for Issue #44: duplicate-publish error should reference
    /// valid commands (delete + update), not nonexistent flags (--force, --update).
    #[tokio::test]
    async fn test_duplicate_publish_error_references_valid_commands() {
        std::env::set_var("NODALYNC_PASSWORD", "test_password");

        let temp_dir = TempDir::new().unwrap();
        let config = setup_config(&temp_dir);

        // Initialize identity first
        crate::commands::init::init(config.clone(), OutputFormat::Human, false).unwrap();

        // Create a file to publish
        let file_path = temp_dir.path().join("duplicate.txt");
        std::fs::write(&file_path, "Content for duplicate test").unwrap();

        // First publish should succeed
        let result = publish(
            config.clone(),
            OutputFormat::Json,
            &file_path,
            None,
            Visibility::Shared,
            None,
            None,
        )
        .await;
        assert!(
            result.is_ok(),
            "First publish should succeed: {:?}",
            result.err()
        );

        // Second publish of same file should fail with actionable error
        let result = publish(
            config,
            OutputFormat::Json,
            &file_path,
            None,
            Visibility::Shared,
            None,
            None,
        )
        .await;
        assert!(result.is_err(), "Duplicate publish should fail");
        let err_msg = result.unwrap_err().to_string();

        // Should reference valid commands
        assert!(
            err_msg.contains("nodalync delete"),
            "Error should suggest 'nodalync delete', got: {}",
            err_msg
        );
        assert!(
            err_msg.contains("nodalync update"),
            "Error should suggest 'nodalync update', got: {}",
            err_msg
        );

        // Should NOT reference nonexistent flags
        assert!(
            !err_msg.contains("--force"),
            "Error should not reference nonexistent --force flag, got: {}",
            err_msg
        );
        assert!(
            !err_msg.contains("--update"),
            "Error should not reference nonexistent --update flag, got: {}",
            err_msg
        );
    }

    /// Regression test for Issue #49: empty file error should use InvalidInput,
    /// not User/ACCESS_DENIED error code.
    #[tokio::test]
    async fn test_empty_file_uses_correct_error_code() {
        std::env::set_var("NODALYNC_PASSWORD", "test_password");

        let temp_dir = TempDir::new().unwrap();
        let config = setup_config(&temp_dir);

        crate::commands::init::init(config.clone(), OutputFormat::Human, false).unwrap();

        // Create an empty file
        let empty_file = temp_dir.path().join("empty.txt");
        std::fs::write(&empty_file, b"").unwrap();

        let result = publish(
            config,
            OutputFormat::Json,
            &empty_file,
            None,
            Visibility::Shared,
            None,
            None,
        )
        .await;

        assert!(result.is_err(), "Publishing empty file should fail");
        let err = result.unwrap_err();

        // Should be InvalidInput, not User (which maps to ACCESS_DENIED)
        assert!(
            matches!(err, CliError::InvalidInput(_)),
            "Empty file error should be InvalidInput, got: {:?}",
            err
        );
        assert_ne!(
            err.error_code(),
            nodalync_types::ErrorCode::AccessDenied,
            "Empty file error code should NOT be ACCESS_DENIED"
        );
    }
}
