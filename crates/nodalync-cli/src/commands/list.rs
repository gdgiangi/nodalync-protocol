//! List local content command.

use nodalync_store::{ManifestFilter, ManifestStore};
use nodalync_types::{ContentType, Visibility};

use crate::config::CliConfig;
use crate::context::NodeContext;
use crate::error::CliResult;
use crate::output::{manifest_to_summary, ListOutput, OutputFormat, Render};

/// Execute the list command.
pub fn list(
    config: CliConfig,
    format: OutputFormat,
    visibility_filter: Option<Visibility>,
    content_type_filter: Option<ContentType>,
    limit: u32,
) -> CliResult<String> {
    let ctx = NodeContext::local(config)?;

    // Build filter
    let mut filter = ManifestFilter::default();

    if let Some(vis) = visibility_filter {
        filter = filter.with_visibility(vis);
    }

    if let Some(ct) = content_type_filter {
        filter = filter.with_content_type(ct);
    }

    filter = filter.limit(limit);

    // Query manifests
    let manifests = ctx.ops.state.manifests.list(filter)?;

    // Convert to summaries
    let summaries: Vec<_> = manifests.iter().map(manifest_to_summary).collect();
    let total = summaries.len();

    let output = ListOutput {
        manifests: summaries,
        total,
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
    fn test_list_empty() {
        let temp_dir = TempDir::new().unwrap();
        let config = setup_config(&temp_dir);

        init(config.clone(), OutputFormat::Human).unwrap();

        let result = list(config, OutputFormat::Human, None, None, 50);
        assert!(result.is_ok());

        let output = result.unwrap();
        assert!(output.contains("No content found"));
    }

    #[test]
    fn test_list_json() {
        let temp_dir = TempDir::new().unwrap();
        let config = setup_config(&temp_dir);

        init(config.clone(), OutputFormat::Human).unwrap();

        let result = list(config, OutputFormat::Json, None, None, 50);
        assert!(result.is_ok());

        let output = result.unwrap();
        assert!(output.contains("\"manifests\""));
        assert!(output.contains("\"total\""));
    }
}
