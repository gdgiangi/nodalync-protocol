//! Deposit tokens command.

use crate::config::{ndl_to_units, CliConfig};
use crate::context::NodeContext;
use crate::error::CliResult;
use crate::output::{OutputFormat, Render, TransactionOutput};

/// Execute the deposit command.
pub async fn deposit(config: CliConfig, format: OutputFormat, amount_ndl: f64) -> CliResult<String> {
    // Convert to units
    let amount = ndl_to_units(amount_ndl);

    // Initialize context with network
    let ctx = NodeContext::with_network(config).await?;

    // Perform deposit
    let tx_id = ctx.settlement.deposit(amount).await?;

    // Get new balance
    let new_balance = ctx.settlement.get_balance().await?;

    let output = TransactionOutput {
        operation: "Deposit".to_string(),
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
    fn test_deposit_output() {
        let output = TransactionOutput {
            operation: "Deposit".to_string(),
            amount: 50_000_000,
            new_balance: 150_000_000,
            transaction_id: "tx123".to_string(),
        };

        let human = output.render(OutputFormat::Human);
        assert!(human.contains("DEPOSIT"));
        assert!(human.contains("New Balance"));

        let json = output.render(OutputFormat::Json);
        assert!(json.contains("\"operation\""));
    }
}
