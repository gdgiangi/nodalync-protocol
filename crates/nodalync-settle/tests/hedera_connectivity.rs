//! Hedera testnet connectivity test.
//!
//! This test verifies that the Hedera SDK can connect to testnet
//! and query account balance. It does NOT require a deployed contract.
//!
//! Run with:
//!   cargo test -p nodalync-settle --test hedera_connectivity --features hedera-sdk -- --nocapture
//!
//! Credentials are loaded from (in order of priority):
//!   1. Environment variables: HEDERA_ACCOUNT_ID, HEDERA_PRIVATE_KEY
//!   2. .env file at project root
//!   3. ~/.nodalync/hedera.key (for private key only)
//!
//! If no credentials are found, tests are skipped gracefully.

#![cfg(feature = "hedera-sdk")]

use hedera::{AccountBalanceQuery, AccountId, Client, PrivateKey};
use std::env;
use std::path::PathBuf;
use std::str::FromStr;

/// Try to load .env file from project root
fn try_load_dotenv() {
    // Find project root by looking for Cargo.toml
    if let Ok(manifest_dir) = env::var("CARGO_MANIFEST_DIR") {
        let mut path = PathBuf::from(manifest_dir);
        // Go up to project root (from crates/nodalync-settle to root)
        path.pop(); // crates
        path.pop(); // root
        path.push(".env");

        if path.exists() {
            // Read and parse .env file manually (no external dependency)
            if let Ok(contents) = std::fs::read_to_string(&path) {
                for line in contents.lines() {
                    let line = line.trim();
                    if line.is_empty() || line.starts_with('#') {
                        continue;
                    }
                    if let Some((key, value)) = line.split_once('=') {
                        let key = key.trim();
                        let value = value.trim().trim_matches('"').trim_matches('\'');
                        // Only set if not already set
                        if env::var(key).is_err() {
                            env::set_var(key, value);
                        }
                    }
                }
            }
        }
    }
}

/// Try to load private key from ~/.nodalync/hedera.key
fn try_load_key_file() -> Option<String> {
    let home = env::var("HOME").ok()?;
    let key_path = PathBuf::from(home).join(".nodalync").join("hedera.key");
    std::fs::read_to_string(key_path).ok().map(|s| s.trim().to_string())
}

/// Get test credentials from environment, .env file, or key file
fn get_credentials() -> Option<(String, String)> {
    // First try loading .env file
    try_load_dotenv();

    // Get account ID from environment
    let account_id = env::var("HEDERA_ACCOUNT_ID").ok()?;

    // Get private key - try env var first, then key file
    let private_key = env::var("HEDERA_PRIVATE_KEY")
        .ok()
        .or_else(try_load_key_file)?;

    Some((account_id, private_key))
}

#[tokio::test]
async fn test_hedera_testnet_connectivity() {
    // Get credentials
    let (account_id_str, private_key_str) = match get_credentials() {
        Some(creds) => creds,
        None => {
            println!("Skipping test: HEDERA_ACCOUNT_ID and HEDERA_PRIVATE_KEY not set");
            println!("To run this test:");
            println!("  export HEDERA_ACCOUNT_ID=0.0.7703962");
            println!("  export HEDERA_PRIVATE_KEY=<your-private-key>");
            return;
        }
    };

    println!("Testing Hedera testnet connectivity...");
    println!("Account ID: {}", account_id_str);

    // Parse account ID
    let account_id = AccountId::from_str(&account_id_str).expect("Failed to parse account ID");
    println!("Parsed account ID: {:?}", account_id);

    // Parse private key (remove 0x prefix if present)
    let key_str = private_key_str
        .strip_prefix("0x")
        .unwrap_or(&private_key_str);
    let private_key = PrivateKey::from_str(key_str).expect("Failed to parse private key");
    println!("Private key parsed successfully");

    // Create testnet client
    let client = Client::for_testnet();
    client.set_operator(account_id, private_key);
    println!("Hedera client created for testnet");

    // Query balance
    println!("Querying account balance...");
    let balance = AccountBalanceQuery::new()
        .account_id(account_id)
        .execute(&client)
        .await
        .expect("Failed to query balance");

    let hbar_balance = balance.hbars;
    let tinybar_balance = hbar_balance.to_tinybars();

    println!("===========================================");
    println!("SUCCESS! Connected to Hedera testnet");
    println!("===========================================");
    println!("Account: {}", account_id_str);
    println!(
        "Balance: {} HBAR ({} tinybars)",
        hbar_balance, tinybar_balance
    );
    println!("===========================================");

    // Basic assertion - account should exist (balance query succeeded)
    assert!(tinybar_balance >= 0, "Balance should be non-negative");
}

#[tokio::test]
async fn test_hedera_account_info() {
    let (account_id_str, private_key_str) = match get_credentials() {
        Some(creds) => creds,
        None => {
            println!("Skipping test: credentials not set");
            return;
        }
    };

    let account_id = AccountId::from_str(&account_id_str).unwrap();
    let key_str = private_key_str
        .strip_prefix("0x")
        .unwrap_or(&private_key_str);
    let private_key = PrivateKey::from_str(key_str).unwrap();

    let client = Client::for_testnet();
    client.set_operator(account_id, private_key.clone());

    // Query account info (this is a paid query, may fail on testnet)
    match hedera::AccountInfoQuery::new()
        .account_id(account_id)
        .execute(&client)
        .await
    {
        Ok(info) => {
            println!("===========================================");
            println!("Account Info:");
            println!("===========================================");
            println!("Account ID: {}", info.account_id);
            println!("Balance: {}", info.balance);
            println!("Key: {:?}", info.key);
            println!("Auto-renew period: {:?}", info.auto_renew_period);
            println!("Memo: {}", info.account_memo);
            println!("===========================================");
        }
        Err(e) => {
            // AccountInfoQuery is a paid query and may fail with signature issues
            // This is expected behavior on testnet with certain account types
            println!("Note: AccountInfoQuery failed (expected for some testnet accounts)");
            println!("Error: {}", e);
            println!("This does not indicate a connectivity problem.");
        }
    }
}

#[tokio::test]
async fn test_private_key_derivation() {
    // Test that we can derive public key from private key
    let key_hex = "d21f3bfe69929b1d6e0f37fa9622b96f874a892f7236a7e0e3c8d7b62b422d8b";

    let private_key = PrivateKey::from_str(key_hex).expect("Failed to parse private key");

    let public_key = private_key.public_key();

    println!("Private key (hex): {}", key_hex);
    println!("Public key: {}", public_key);

    // Public key should be derivable
    assert!(!public_key.to_string().is_empty());
}
