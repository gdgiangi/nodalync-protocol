//! Show node status command.

use nodalync_store::{ManifestFilter, ManifestStore, SettlementQueueStore};
use nodalync_types::Visibility;

use crate::config::CliConfig;
use crate::context::NodeContext;
use crate::error::CliResult;
use crate::output::{OutputFormat, Render, StatusOutput};

/// Execute the status command.
pub async fn status(config: CliConfig, format: OutputFormat) -> CliResult<String> {
    // Try to initialize context
    let ctx = match NodeContext::with_network(config).await {
        Ok(ctx) => ctx,
        Err(_) => {
            // Node not running or can't initialize
            let output = StatusOutput {
                running: false,
                peer_id: "N/A".to_string(),
                uptime_secs: None,
                connected_peers: 0,
                shared_content: 0,
                private_content: 0,
                pending_payments: 0,
                pending_amount: 0,
            };
            return Ok(output.render(format));
        }
    };

    // Count content by visibility
    let shared_count = ctx
        .ops
        .state
        .manifests
        .list(ManifestFilter::default().with_visibility(Visibility::Shared))?
        .len() as u32;

    let private_count = ctx
        .ops
        .state
        .manifests
        .list(ManifestFilter::default().with_visibility(Visibility::Private))?
        .len() as u32;

    // Get pending payments
    let pending = ctx.ops.state.settlement.get_pending()?;
    let pending_amount = ctx.ops.state.settlement.get_pending_total()?;

    // Get connected peers
    let connected_peers = ctx.connected_peers() as u32;

    let output = StatusOutput {
        running: true,
        peer_id: ctx.peer_id().to_string(),
        uptime_secs: None, // Would need to track start time
        connected_peers,
        shared_content: shared_count,
        private_content: private_count,
        pending_payments: pending.len() as u32,
        pending_amount,
    };

    Ok(output.render(format))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_output_running() {
        let output = StatusOutput {
            running: true,
            peer_id: "ndl1abc123".to_string(),
            uptime_secs: Some(3661),
            connected_peers: 12,
            shared_content: 5,
            private_content: 2,
            pending_payments: 3,
            pending_amount: 100_000_000,
        };

        let human = output.render(OutputFormat::Human);
        assert!(human.contains("running"));
        assert!(human.contains("shared"));

        let json = output.render(OutputFormat::Json);
        assert!(json.contains("\"running\": true"));
    }

    #[test]
    fn test_status_output_stopped() {
        let output = StatusOutput {
            running: false,
            peer_id: "N/A".to_string(),
            uptime_secs: None,
            connected_peers: 0,
            shared_content: 0,
            private_content: 0,
            pending_payments: 0,
            pending_amount: 0,
        };

        let human = output.render(OutputFormat::Human);
        assert!(human.contains("stopped"));
    }
}
