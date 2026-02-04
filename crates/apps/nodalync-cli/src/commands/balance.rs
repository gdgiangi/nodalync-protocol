//! Show balance command.

use nodalync_store::SettlementQueueStore;

use crate::config::CliConfig;
use crate::context::NodeContext;
use crate::error::CliResult;
use crate::output::{BalanceOutput, OutputFormat, Render};

/// Execute the balance command.
pub async fn balance(config: CliConfig, format: OutputFormat) -> CliResult<String> {
    // Initialize context with network (for settlement)
    let ctx = NodeContext::with_network(config).await?;

    // Get pending earnings from settlement queue
    let pending_earnings = ctx.ops.state.settlement.get_pending_total()?;

    // Count pending payments
    let pending_distributions = ctx.ops.state.settlement.get_pending()?;
    let pending_payments = pending_distributions.len() as u32;

    // Get protocol balance from settlement (or fall back to offline)
    let (protocol_balance, offline) = match ctx.settlement.as_ref() {
        Some(settlement) => (settlement.get_balance().await?, false),
        None => (0, true),
    };

    let output = BalanceOutput {
        protocol_balance,
        pending_earnings,
        pending_payments,
        offline,
    };

    Ok(output.render(format))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_balance_output() {
        let output = BalanceOutput {
            protocol_balance: 100_000_000,
            pending_earnings: 5_000_000,
            pending_payments: 3,
            offline: false,
        };

        let human = output.render(OutputFormat::Human);
        assert!(human.contains("Protocol Balance"));
        assert!(human.contains("Pending Earnings"));
        assert!(!human.contains("offline"));

        let json = output.render(OutputFormat::Json);
        assert!(json.contains("\"protocol_balance\""));
    }

    #[tokio::test]
    async fn test_balance_output_offline() {
        let output = BalanceOutput {
            protocol_balance: 0,
            pending_earnings: 5_000_000,
            pending_payments: 3,
            offline: true,
        };

        let human = output.render(OutputFormat::Human);
        assert!(human.contains("Protocol Balance"));
        assert!(human.contains("offline"));
        assert!(human.contains("Hedera not configured"));
    }
}
