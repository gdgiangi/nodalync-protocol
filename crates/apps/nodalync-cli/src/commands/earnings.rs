//! Show earnings breakdown command.

use nodalync_store::{ManifestFilter, ManifestStore};

use crate::config::CliConfig;
use crate::context::NodeContext;
use crate::error::CliResult;
use crate::output::{EarningsOutput, OutputFormat, Render};

/// Execute the earnings command.
pub fn earnings(
    config: CliConfig,
    format: OutputFormat,
    content_filter: Option<String>,
    limit: u32,
) -> CliResult<String> {
    // Initialize context
    let ctx = NodeContext::local(config)?;

    // Get all manifests (no filter to start)
    let filter = ManifestFilter::default();
    let all_manifests = ctx.ops.state.manifests.list(filter)?;

    // Filter to owned content with earnings
    let mut content_earnings: Vec<_> = all_manifests
        .iter()
        .filter(|m| m.owner == ctx.peer_id())
        .filter(|m| {
            if let Some(ref hash_filter) = content_filter {
                m.hash.to_string().starts_with(hash_filter)
            } else {
                true
            }
        })
        .map(|m| crate::output::ContentEarning {
            hash: m.hash.to_string(),
            title: m.metadata.title.clone(),
            queries: m.economics.total_queries,
            total_earned: m.economics.total_revenue,
            price: m.economics.price,
        })
        .filter(|e| e.total_earned > 0 || e.queries > 0)
        .collect();

    // Sort by total earned (descending)
    content_earnings.sort_by(|a, b| b.total_earned.cmp(&a.total_earned));

    // Apply limit
    content_earnings.truncate(limit as usize);

    // Calculate totals
    let total_earned: u64 = content_earnings.iter().map(|e| e.total_earned).sum();
    let total_queries: u64 = content_earnings.iter().map(|e| e.queries).sum();

    let output = EarningsOutput {
        content: content_earnings,
        total_earned,
        total_queries,
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
        config
    }

    #[test]
    fn test_earnings_empty() {
        std::env::set_var("NODALYNC_PASSWORD", "test_password");

        let temp_dir = TempDir::new().unwrap();
        let config = setup_config(&temp_dir);

        // Initialize identity first
        crate::commands::init::init(config.clone(), OutputFormat::Human, false).unwrap();

        let result = earnings(config, OutputFormat::Human, None, 10);
        assert!(result.is_ok());
    }
}
