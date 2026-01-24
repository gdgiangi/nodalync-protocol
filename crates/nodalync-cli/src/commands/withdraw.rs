//! Withdraw tokens command.

use crate::config::{ndl_to_units, CliConfig};
use crate::context::NodeContext;
use crate::error::{CliError, CliResult};
use crate::output::{OutputFormat, Render, TransactionOutput};

/// Execute the withdraw command.
pub async fn withdraw(
    config: CliConfig,
    format: OutputFormat,
    amount_ndl: f64,
) -> CliResult<String> {
    // Convert to units
    let amount = ndl_to_units(amount_ndl);

    // Initialize context with network
    let ctx = NodeContext::with_network(config).await?;

    // Check balance first
    let current_balance = ctx.settlement.get_balance().await?;
    if amount > current_balance {
        return Err(CliError::InsufficientBalance {
            required: amount,
            available: current_balance,
        });
    }

    // Perform withdrawal
    let tx_id = ctx.settlement.withdraw(amount).await?;

    // Get new balance
    let new_balance = ctx.settlement.get_balance().await?;

    let output = TransactionOutput {
        operation: "Withdraw".to_string(),
        amount,
        new_balance,
        transaction_id: tx_id.to_string(),
    };

    Ok(output.render(format))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_withdraw_output() {
        let output = TransactionOutput {
            operation: "Withdraw".to_string(),
            amount: 50_000_000,
            new_balance: 50_000_000,
            transaction_id: "tx456".to_string(),
        };

        let human = output.render(OutputFormat::Human);
        assert!(human.contains("WITHDRAW"));
        assert!(human.contains("New Balance"));

        let json = output.render(OutputFormat::Json);
        assert!(json.contains("\"transaction_id\""));
    }
}
