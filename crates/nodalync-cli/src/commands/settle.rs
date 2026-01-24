//! Force settlement command.

use std::collections::HashSet;

use nodalync_store::SettlementQueueStore;

use crate::config::CliConfig;
use crate::context::NodeContext;
use crate::error::CliResult;
use crate::output::{OutputFormat, Render, SettleOutput};

/// Execute the settle command.
pub async fn settle(config: CliConfig, format: OutputFormat) -> CliResult<String> {
    // Initialize context with network
    let mut ctx = NodeContext::with_network(config).await?;

    // Get pending info before settlement
    let pending = ctx.ops.state.settlement.get_pending()?;
    let pending_total = ctx.ops.state.settlement.get_pending_total()?;

    // Count unique recipients
    let unique_recipients: HashSet<_> = pending.iter().map(|d| d.recipient).collect();

    // Force settlement
    let batch_id = ctx.ops.force_settlement().await?;

    let output = SettleOutput {
        batch_id: batch_id.map(|h| h.to_string()),
        payments_settled: pending.len() as u32,
        amount_settled: pending_total,
        recipients: unique_recipients.len() as u32,
    };

    Ok(output.render(format))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_settle_output_no_pending() {
        let output = SettleOutput {
            batch_id: None,
            payments_settled: 0,
            amount_settled: 0,
            recipients: 0,
        };

        let human = output.render(OutputFormat::Human);
        assert!(human.contains("No pending payments"));
    }

    #[test]
    fn test_settle_output_with_pending() {
        let output = SettleOutput {
            batch_id: Some("batch123".to_string()),
            payments_settled: 5,
            amount_settled: 100_000_000,
            recipients: 3,
        };

        let human = output.render(OutputFormat::Human);
        assert!(human.contains("Settlement complete"));
        assert!(human.contains("batch123"));
    }
}
