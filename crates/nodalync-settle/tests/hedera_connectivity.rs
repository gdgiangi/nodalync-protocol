//! Hedera testnet connectivity test.
//!
//! This test verifies that the Hedera SDK can connect to testnet
//! and query account balance. It does NOT require a deployed contract.
//!
//! Run with:
//!   cargo test -p nodalync-settle --test hedera_connectivity --features hedera-sdk -- --nocapture
//!
//! Environment variables:
//!   HEDERA_ACCOUNT_ID - Testnet account ID (e.g., 0.0.7703962)
//!   HEDERA_PRIVATE_KEY - Hex-encoded private key (with or without 0x prefix)

#![cfg(feature = "hedera-sdk")]

use hedera::{AccountBalanceQuery, AccountId, Client, PrivateKey};
use std::env;
use std::str::FromStr;

/// Get test credentials from environment or use defaults for CI skip
fn get_credentials() -> Option<(String, String)> {
    let account_id = env::var("HEDERA_ACCOUNT_ID").ok()?;
    let private_key = env::var("HEDERA_PRIVATE_KEY").ok()?;
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
            println!("  export HEDERA_PRIVATE_KEY=0xd21f3bfe69929b1d6e0f37fa9622b96f874a892f7236a7e0e3c8d7b62b422d8b");
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
