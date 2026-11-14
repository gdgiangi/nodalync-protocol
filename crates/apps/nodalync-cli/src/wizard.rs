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

    configure_network(&mut config)?;
    println!();

    configure_settlement(&mut config)?;
    println!();

    configure_pricing(&mut config)?;
    println!();

    print_summary(&config);
    confirm_save()?;

    Ok(config)
}

/// A prompt that was cancelled or failed.
fn cancelled(e: dialoguer::Error) -> CliError {
    CliError::User(format!("Wizard cancelled: {}", e))
}

/// Print a step heading with an underline.
fn print_heading(title: &str) {
    println!("{}", title);
    println!("{}", "-".repeat(40));
}

/// Step 1: Network configuration.
fn configure_network(config: &mut CliConfig) -> CliResult<()> {
    print_heading("Step 1: Network Configuration");

    let network_options = [NetworkOption::Enabled, NetworkOption::Disabled];
    let network_selection = Select::new()
        .with_prompt("Enable network connectivity?")
        .items(&network_options)
        .default(0)
        .interact()
        .map_err(cancelled)?;

    config.network.enabled = network_options[network_selection] == NetworkOption::Enabled;
    if !config.network.enabled {
        return Ok(());
    }

    let use_default_bootstrap = Confirm::new()
        .with_prompt("Use default bootstrap nodes?")
        .default(true)
        .interact()
        .map_err(cancelled)?;

    if !use_default_bootstrap {
        let custom_bootstrap: String = Input::new()
            .with_prompt("Enter custom bootstrap address (multiaddr)")
            .interact_text()
            .map_err(cancelled)?;

        config.network.bootstrap_nodes = vec![custom_bootstrap];
    }
    Ok(())
}

/// Step 2: Settlement configuration.
fn configure_settlement(config: &mut CliConfig) -> CliResult<()> {
    print_heading("Step 2: Settlement Configuration");

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
        .map_err(cancelled)?;

    match settlement_options[settlement_selection] {
        SettlementOption::Mock => {
            config.settlement.network = "mock".to_string();
            Ok(())
        }
        SettlementOption::Testnet => {
            config.settlement.network = "hedera-testnet".to_string();
            configure_hedera_credentials(config, "testnet")
        }
        SettlementOption::Mainnet => configure_mainnet(config),
    }
}

/// Mainnet uses real HBAR: confirm it, or switch to testnet for safety.
fn configure_mainnet(config: &mut CliConfig) -> CliResult<()> {
    config.settlement.network = "hedera-mainnet".to_string();

    let confirmed = Confirm::new()
        .with_prompt("WARNING: Mainnet uses real HBAR. Are you sure?")
        .default(false)
        .interact()
        .map_err(cancelled)?;

    if confirmed {
        return configure_hedera_credentials(config, "mainnet");
    }
    config.settlement.network = "hedera-testnet".to_string();
    println!("Switched to testnet for safety.");
    configure_hedera_credentials(config, "testnet")
}

/// Step 3: Economics configuration.
fn configure_pricing(config: &mut CliConfig) -> CliResult<()> {
    print_heading("Step 3: Default Pricing");

    config.economics.default_price = Input::new()
        .with_prompt("Default price per query (HBAR)")
        .default(0.001)
        .interact_text()
        .map_err(cancelled)?;
    Ok(())
}

/// Preview the configuration.
fn print_summary(config: &CliConfig) {
    println!("Configuration Summary");
    println!("{}", "=".repeat(40));
    let network = if config.network.enabled {
        "Enabled"
    } else {
        "Disabled"
    };
    println!("  Network:     {}", network);
    println!("  Settlement:  {}", config.settlement.network);
    println!("  Default price: {} HBAR", config.economics.default_price);
    println!();
}

/// Ask to save the configuration; declining cancels the wizard.
fn confirm_save() -> CliResult<()> {
    let confirmed = Confirm::new()
        .with_prompt("Save this configuration?")
        .default(true)
        .interact()
        .map_err(cancelled)?;

    if !confirmed {
        return Err(CliError::User("Configuration cancelled by user".into()));
    }
    Ok(())
}

/// Configure Hedera credentials for testnet or mainnet.
fn configure_hedera_credentials(config: &mut CliConfig, network: &str) -> CliResult<()> {
    println!();
    print_heading(&format!("Hedera {} Credentials", network));

    // Check if user wants to configure now or later
    let configure_now = Confirm::new()
        .with_prompt("Configure Hedera credentials now? (required for payments)")
        .default(true)
        .interact()
        .map_err(cancelled)?;

    if !configure_now {
        println!(
            "Skipped. You can configure later by editing config.toml or re-running init --wizard"
        );
        return Ok(());
    }

    let (account_id, private_key) = prompt_hedera_credentials()?;
    config.settlement.account_id = Some(account_id);

    let key_path = save_hedera_key(&private_key)?;
    config.settlement.key_path = Some(key_path.clone());

    // Contract ID - use default for testnet
    prompt_contract_id(config, network)?;

    println!("✓ Hedera credentials configured");
    println!("  Key saved to: {}", key_path.display());

    Ok(())
}

/// Ask for the Hedera account ID and private key (the key is not echoed).
fn prompt_hedera_credentials() -> CliResult<(String, String)> {
    // Account ID
    let account_id: String = Input::new()
        .with_prompt("Hedera Account ID (e.g., 0.0.1234567)")
        .interact_text()
        .map_err(cancelled)?;

    // Private key - use Password for security (doesn't echo)
    let private_key: String = Password::new()
        .with_prompt("Hedera Private Key (hex, starts with 0x)")
        .interact()
        .map_err(cancelled)?;
    Ok((account_id, private_key))
}

/// Write the Hedera private key to `hedera.key` in the shared default data directory,
/// readable by the owner only (Unix). Returns its path.
fn save_hedera_key(private_key: &str) -> CliResult<std::path::PathBuf> {
    let key_path = nodalync_store::default_data_dir().join("hedera.key");

    // Create directory if needed
    if let Some(parent) = key_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| CliError::config(format!("Failed to create directory: {}", e)))?;
    }

    // Write key file with secure permissions
    std::fs::write(&key_path, private_key.trim())
        .map_err(|e| CliError::config(format!("Failed to write key file: {}", e)))?;

    restrict_to_owner(&key_path)?;
    Ok(key_path)
}

/// Set file permissions to owner-only (Unix).
#[cfg(unix)]
fn restrict_to_owner(path: &std::path::Path) -> CliResult<()> {
    use std::os::unix::fs::PermissionsExt;
    let perms = std::fs::Permissions::from_mode(0o600);
    std::fs::set_permissions(path, perms)
        .map_err(|e| CliError::config(format!("Failed to set key file permissions: {}", e)))
}

#[cfg(not(unix))]
fn restrict_to_owner(_path: &std::path::Path) -> CliResult<()> {
    Ok(())
}

/// Ask for the settlement contract ID (defaulting to the testnet contract on testnet);
/// an empty answer leaves it unset.
fn prompt_contract_id(config: &mut CliConfig, network: &str) -> CliResult<()> {
    let default_contract = if network == "testnet" {
        "0.0.7729011"
    } else {
        ""
    };

    let contract_id: String = Input::new()
        .with_prompt("Settlement Contract ID")
        .default(default_contract.to_string())
        .interact_text()
        .map_err(cancelled)?;

    if !contract_id.is_empty() {
        config.settlement.contract_id = Some(contract_id);
    }
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
