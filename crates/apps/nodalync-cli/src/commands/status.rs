//! Show node status command.

use nodalync_store::{ManifestFilter, ManifestStore, SettlementQueueStore};
use nodalync_types::Visibility;

use crate::config::CliConfig;
use crate::context::NodeContext;
use crate::error::CliResult;
use crate::node_runner::{
    calculate_uptime, check_existing_node, pid_file_path, read_start_time, read_status_file,
    status_file_path,
};
use crate::output::{OutputFormat, Render, StatusOutput};

/// Execute the status command.
pub async fn status(config: CliConfig, format: OutputFormat) -> CliResult<String> {
    let base_dir = config.base_dir();

    // Check if a node is running via PID file
    let running_pid = check_existing_node(&base_dir);

    // Try to initialize local context (for content stats)
    let ctx = NodeContext::local(config.clone()).ok();

    // If no node is running, show stopped status with local stats
    let Some(pid) = running_pid else {
        return Ok(stopped_status(ctx.as_ref()).render(format));
    };

    // Calculate uptime from PID file start time
    let uptime_secs = read_start_time(&pid_file_path(&base_dir)).map(calculate_uptime);

    // Node is running, get full status
    let output = match ctx {
        Some(ctx) => running_status(&ctx, &base_dir, uptime_secs)?,
        // Can't get local context, show minimal running status
        None => StatusOutput {
            running: true,
            peer_id: format!("PID {}", pid),
            uptime_secs,
            connected_peers: 0,
            shared_content: 0,
            private_content: 0,
            pending_payments: 0,
            pending_amount: 0,
        },
    };

    Ok(output.render(format))
}

/// Status of a stopped node, with whatever local stats can be read.
fn stopped_status(ctx: Option<&NodeContext>) -> StatusOutput {
    let (shared, private, pending_payments, pending_amount, peer_id) = match ctx {
        Some(ctx) => (
            count_with_visibility(ctx, Visibility::Shared).unwrap_or(0),
            count_with_visibility(ctx, Visibility::Private).unwrap_or(0),
            ctx.ops
                .state
                .settlement
                .get_pending()
                .unwrap_or_default()
                .len() as u32,
            ctx.ops.state.settlement.get_pending_total().unwrap_or(0),
            ctx.peer_id().to_string(),
        ),
        None => (0, 0, 0, 0, "N/A".to_string()),
    };

    StatusOutput {
        running: false,
        peer_id,
        uptime_secs: None,
        connected_peers: 0,
        shared_content: shared,
        private_content: private,
        pending_payments,
        pending_amount,
    }
}

/// Full status of a running node.
fn running_status(
    ctx: &NodeContext,
    base_dir: &std::path::Path,
    uptime_secs: Option<u64>,
) -> CliResult<StatusOutput> {
    // Count content by visibility
    let shared_count = count_with_visibility(ctx, Visibility::Shared)?;
    let private_count = count_with_visibility(ctx, Visibility::Private)?;

    // Get pending payments
    let pending = ctx.ops.state.settlement.get_pending()?;
    let pending_amount = ctx.ops.state.settlement.get_pending_total()?;

    // Get connected peers from the status file (written by the running node)
    let connected_peers = read_status_file(&status_file_path(base_dir))
        .map(|s| s.connected_peers)
        .unwrap_or(0);

    Ok(StatusOutput {
        running: true,
        peer_id: ctx.peer_id().to_string(),
        uptime_secs,
        connected_peers,
        shared_content: shared_count,
        private_content: private_count,
        pending_payments: pending.len() as u32,
        pending_amount,
    })
}

/// How many of our manifests have this visibility.
fn count_with_visibility(
    ctx: &NodeContext,
    visibility: Visibility,
) -> Result<u32, nodalync_store::StoreError> {
    Ok(ctx
        .ops
        .state
        .manifests
        .list(ManifestFilter::default().with_visibility(visibility))?
        .len() as u32)
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
