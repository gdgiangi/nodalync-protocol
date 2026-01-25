//! Show node status command.

use nodalync_store::{ManifestFilter, ManifestStore, SettlementQueueStore};
use nodalync_types::Visibility;

use crate::config::CliConfig;
use crate::context::NodeContext;
use crate::error::CliResult;
use crate::node_runner::{calculate_uptime, check_existing_node, pid_file_path, read_start_time};
use crate::output::{OutputFormat, Render, StatusOutput};

/// Execute the status command.
pub async fn status(config: CliConfig, format: OutputFormat) -> CliResult<String> {
    let base_dir = config.base_dir();

    // Check if a node is running via PID file
    let running_pid = check_existing_node(&base_dir);

    // Try to initialize local context (for content stats)
    let ctx = NodeContext::local(config.clone()).ok();

    // If no node is running, show stopped status with local stats
    if running_pid.is_none() {
        let (shared, private, pending_payments, pending_amount, peer_id) =
            if let Some(ref ctx) = ctx {
                let shared = ctx
                    .ops
                    .state
                    .manifests
                    .list(ManifestFilter::default().with_visibility(Visibility::Shared))
                    .map(|v| v.len() as u32)
                    .unwrap_or(0);

                let private = ctx
                    .ops
                    .state
                    .manifests
                    .list(ManifestFilter::default().with_visibility(Visibility::Private))
                    .map(|v| v.len() as u32)
                    .unwrap_or(0);

                let pending = ctx.ops.state.settlement.get_pending().unwrap_or_default();
                let pending_amount = ctx.ops.state.settlement.get_pending_total().unwrap_or(0);

                (
                    shared,
                    private,
                    pending.len() as u32,
                    pending_amount,
                    ctx.peer_id().to_string(),
                )
            } else {
                (0, 0, 0, 0, "N/A".to_string())
            };

        let output = StatusOutput {
            running: false,
            peer_id,
            uptime_secs: None,
            connected_peers: 0,
            shared_content: shared,
            private_content: private,
            pending_payments,
            pending_amount,
        };
        return Ok(output.render(format));
    }

    // Calculate uptime from PID file start time
    let uptime_secs = read_start_time(&pid_file_path(&base_dir)).map(calculate_uptime);

    // Node is running, get full status
    let ctx = match ctx {
        Some(c) => c,
        None => {
            // Can't get local context, show minimal running status
            let output = StatusOutput {
                running: true,
                peer_id: format!("PID {}", running_pid.unwrap()),
                uptime_secs,
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
        uptime_secs,
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
