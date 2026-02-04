//! Interactive setup wizard for CLI configuration.

use dialoguer::{Confirm, Input, Password, Select};

use crate::config::CliConfig;
use crate::error::{CliError, CliResult};

/// Network configuration options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkOption {
    Enabled,
    Disabled,
}

impl std::fmt::Display for NetworkOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Enabled => write!(f, "Enabled - Connect to the Nodalync network"),
            Self::Disabled => write!(f, "Disabled - Local only (offline mode)"),
        }
    }
}

/// Settlement mode options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettlementOption {
    Mock,
    Testnet,
    Mainnet,
}

impl std::fmt::Display for SettlementOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Mock => write!(f, "Mock - No real payments (development)"),
            Self::Testnet => write!(f, "Testnet - Test HBAR (recommended for testing)"),
            Self::Mainnet => write!(f, "Mainnet - Real HBAR payments"),
        }
    }
}

/// Run the interactive setup wizard.
///
/// Prompts the user for configuration options and returns a configured
/// `CliConfig` ready for initialization.
pub fn run_wizard(mut config: CliConfig) -> CliResult<CliConfig> {
    println!("\nNodalync Setup Wizard");
    println!("{}\n", "=".repeat(40));

    // Step 1: Network configuration
    println!("Step 1: Network Configuration");
    println!("{}", "-".repeat(40));

    let network_options = [NetworkOption::Enabled, NetworkOption::Disabled];
    let network_selection = Select::new()
        .with_prompt("Enable network connectivity?")
        .items(&network_options)
        .default(0)
        .interact()
        .map_err(|e| CliError::User(format!("Wizard cancelled: {}", e)))?;

    config.network.enabled = network_options[network_selection] == NetworkOption::Enabled;

    if config.network.enabled {
        let use_default_bootstrap = Confirm::new()
            .with_prompt("Use default bootstrap nodes?")
            .default(true)
            .interact()
            .map_err(|e| CliError::User(format!("Wizard cancelled: {}", e)))?;

        if !use_default_bootstrap {
            let custom_bootstrap: String = Input::new()
                .with_prompt("Enter custom bootstrap address (multiaddr)")
                .interact_text()
                .map_err(|e| CliError::User(format!("Wizard cancelled: {}", e)))?;

            config.network.bootstrap_nodes = vec![custom_bootstrap];
        }
    }

    println!();

    // Step 2: Settlement configuration
    println!("Step 2: Settlement Configuration");
    println!("{}", "-".repeat(40));

    let settlement_options = [
        SettlementOption::Mock,
        SettlementOption::Testnet,
        SettlementOption::Mainnet,
    ];
    let settlement_selection = Select::new()
        .with_prompt("Select settlement mode")
        .items(&settlement_options)
        .default(1)
        .interact()
        .map_err(|e| CliError::User(format!("Wizard cancelled: {}", e)))?;

    match settlement_options[settlement_selection] {
        SettlementOption::Mock => {
            config.settlement.network = "mock".to_string();
        }
        SettlementOption::Testnet => {
            config.settlement.network = "hedera-testnet".to_string();
            configure_hedera_credentials(&mut config, "testnet")?;
        }
        SettlementOption::Mainnet => {
            config.settlement.network = "hedera-mainnet".to_string();

            let confirmed = Confirm::new()
                .with_prompt("WARNING: Mainnet uses real HBAR. Are you sure?")
                .default(false)
                .interact()
                .map_err(|e| CliError::User(format!("Wizard cancelled: {}", e)))?;

            if !confirmed {
                config.settlement.network = "hedera-testnet".to_string();
                println!("Switched to testnet for safety.");
                configure_hedera_credentials(&mut config, "testnet")?;
            } else {
                configure_hedera_credentials(&mut config, "mainnet")?;
            }
        }
    }

    println!();

    // Step 3: Economics configuration
    println!("Step 3: Default Pricing");
    println!("{}", "-".repeat(40));

    let default_price: f64 = Input::new()
        .with_prompt("Default price per query (HBAR)")
        .default(0.001)
        .interact_text()
        .map_err(|e| CliError::User(format!("Wizard cancelled: {}", e)))?;

    config.economics.default_price = default_price;

    println!();

    // Preview and confirm
    println!("Configuration Summary");
    println!("{}", "=".repeat(40));
    println!(
        "  Network:     {}",
        if config.network.enabled {
            "Enabled"
        } else {
            "Disabled"
        }
    );
    println!("  Settlement:  {}", config.settlement.network);
    println!("  Default price: {} HBAR", config.economics.default_price);
    println!();

    let confirmed = Confirm::new()
        .with_prompt("Save this configuration?")
        .default(true)
        .interact()
        .map_err(|e| CliError::User(format!("Wizard cancelled: {}", e)))?;

    if !confirmed {
        return Err(CliError::User("Configuration cancelled by user".into()));
    }

    Ok(config)
}

/// Configure Hedera credentials for testnet or mainnet.
fn configure_hedera_credentials(config: &mut CliConfig, network: &str) -> CliResult<()> {
    println!();
    println!("Hedera {} Credentials", network);
    println!("{}", "-".repeat(40));

    // Check if user wants to configure now or later
    let configure_now = Confirm::new()
        .with_prompt("Configure Hedera credentials now? (required for payments)")
        .default(true)
        .interact()
        .map_err(|e| CliError::user(format!("Wizard cancelled: {}", e)))?;

    if !configure_now {
        println!(
            "Skipped. You can configure later by editing config.toml or re-running init --wizard"
        );
        return Ok(());
    }

    // Account ID
    let account_id: String = Input::new()
        .with_prompt("Hedera Account ID (e.g., 0.0.1234567)")
        .interact_text()
        .map_err(|e| CliError::user(format!("Wizard cancelled: {}", e)))?;

    config.settlement.account_id = Some(account_id);

    // Private key - use Password for security (doesn't echo)
    let private_key: String = Password::new()
        .with_prompt("Hedera Private Key (hex, starts with 0x)")
        .interact()
        .map_err(|e| CliError::user(format!("Wizard cancelled: {}", e)))?;

    // Determine key file path using the shared default data directory
    let data_dir = nodalync_store::default_data_dir();
    let key_path = data_dir.join("hedera.key");

    // Create directory if needed
    if let Some(parent) = key_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| CliError::config(format!("Failed to create directory: {}", e)))?;
    }

    // Write key file with secure permissions
    std::fs::write(&key_path, private_key.trim())
        .map_err(|e| CliError::config(format!("Failed to write key file: {}", e)))?;

    // Set file permissions to owner-only (Unix)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(&key_path, perms)
            .map_err(|e| CliError::config(format!("Failed to set key file permissions: {}", e)))?;
    }

    config.settlement.key_path = Some(key_path.clone());

    // Contract ID - use default for testnet
    let default_contract = if network == "testnet" {
        "0.0.7729011"
    } else {
        ""
    };

    let contract_id: String = Input::new()
        .with_prompt("Settlement Contract ID")
        .default(default_contract.to_string())
        .interact_text()
        .map_err(|e| CliError::user(format!("Wizard cancelled: {}", e)))?;

    if !contract_id.is_empty() {
        config.settlement.contract_id = Some(contract_id);
    }

    println!("✓ Hedera credentials configured");
    println!("  Key saved to: {}", key_path.display());

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_network_option_display() {
        assert!(NetworkOption::Enabled.to_string().contains("Enabled"));
        assert!(NetworkOption::Disabled.to_string().contains("Disabled"));
    }

    #[test]
    fn test_settlement_option_display() {
        assert!(SettlementOption::Mock.to_string().contains("Mock"));
        assert!(SettlementOption::Testnet.to_string().contains("Testnet"));
        assert!(SettlementOption::Mainnet.to_string().contains("Mainnet"));
    }

    #[test]
    fn test_network_option_equality() {
        assert_ne!(NetworkOption::Enabled, NetworkOption::Disabled);
        assert_eq!(NetworkOption::Enabled, NetworkOption::Enabled);
        assert_eq!(NetworkOption::Disabled, NetworkOption::Disabled);
    }

    #[test]
    fn test_settlement_option_equality() {
        // All three variants should be distinct
        assert_ne!(SettlementOption::Mock, SettlementOption::Testnet);
        assert_ne!(SettlementOption::Mock, SettlementOption::Mainnet);
        assert_ne!(SettlementOption::Testnet, SettlementOption::Mainnet);

        // Each variant should equal itself
        assert_eq!(SettlementOption::Mock, SettlementOption::Mock);
        assert_eq!(SettlementOption::Testnet, SettlementOption::Testnet);
        assert_eq!(SettlementOption::Mainnet, SettlementOption::Mainnet);
    }
}
