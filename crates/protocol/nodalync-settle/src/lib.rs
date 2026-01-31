//! On-chain settlement for the Nodalync protocol.
//!
//! This crate provides the blockchain settlement layer for Nodalync,
//! connecting the off-chain settlement queue to Hedera Hashgraph for
//! trustless payment distribution.
//!
//! # Overview
//!
//! The settlement module handles:
//! - **Deposits/Withdrawals**: Managing tokens in the settlement contract
//! - **Content Attestation**: Creating on-chain proofs of content ownership
//! - **Payment Channels**: Opening, updating, and closing payment channels
//! - **Batch Settlement**: Distributing payments to ALL recipients in a batch
//!
//! # Architecture
//!
//! ```text
//! nodalync-ops                nodalync-settle
//! ┌────────────────┐         ┌─────────────────────────┐
//! │ trigger_settle │ ──────► │ Settlement (trait)      │
//! │ force_settle   │         │   └─ HederaSettlement   │
//! └────────────────┘         └───────────┬─────────────┘
//!                                        │
//!                                        ▼
//!                            ┌─────────────────────────┐
//!                            │ Hedera Hashgraph        │
//!                            │ (Smart Contract)        │
//!                            └─────────────────────────┘
//! ```
//!
//! # Usage
//!
//! Requires the `hedera-sdk` feature and `protoc` installed:
//!
//! ```toml
//! nodalync-settle = { path = "...", features = ["hedera-sdk"] }
//! ```
//!
//! ```rust,ignore
//! use nodalync_settle::{HederaSettlement, HederaConfig, Settlement};
//! use std::path::PathBuf;
//!
//! # async fn example() -> nodalync_settle::SettleResult<()> {
//! let config = HederaConfig::testnet(
//!     "0.0.12345",
//!     PathBuf::from("~/.nodalync/hedera.key"),
//!     "0.0.67890",
//! );
//!
//! let settlement = HederaSettlement::new(config).await?;
//! let balance = settlement.get_balance().await?;
//! # Ok(())
//! # }
//! ```
//!
//! # Settlement Trait
//!
//! The [`Settlement`] trait provides a common interface for settlement
//! implementations.
//!
//! Key methods:
//! - `deposit()` / `withdraw()` / `get_balance()` - Balance management
//! - `attest()` / `get_attestation()` - Content attestation
//! - `open_channel()` / `close_channel()` - Payment channel lifecycle
//! - `settle_batch()` - Core batch settlement operation
//!
//! # Account Mapping
//!
//! The module maintains a mapping between Nodalync PeerIds (off-chain)
//! and Hedera AccountIds (on-chain). All recipients in a settlement
//! batch must have registered accounts to receive payments.

mod account_mapping;
mod config;
mod error;
pub mod faucet;
#[cfg(feature = "hedera-sdk")]
mod hedera;
mod retry;
mod traits;
pub mod types;

// Re-export main types
pub use account_mapping::AccountMapper;
pub use config::{GasConfig, HederaConfig, HederaNetwork, RetryConfig};
pub use error::{SettleError, SettleResult};
pub use faucet::{request_testnet_hbar, FaucetConfig, FaucetResult, HederaFaucet};
#[cfg(feature = "hedera-sdk")]
pub use hedera::HederaSettlement;
pub use retry::RetryPolicy;
pub use traits::Settlement;

// Re-export key types from types module
pub use types::{AccountId, Attestation, ChannelId, SettlementStatus, TransactionId};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_account_id_roundtrip() {
        let account = AccountId::from_string("0.0.12345").unwrap();
        assert_eq!(account.to_string(), "0.0.12345");

        let account2 = AccountId::new(1, 2, 34567);
        assert_eq!(account2.to_string(), "1.2.34567");
    }

    #[test]
    fn test_transaction_id() {
        let tx_id = TransactionId::new("0.0.12345@1234567890.123");
        assert_eq!(tx_id.as_str(), "0.0.12345@1234567890.123");
        assert_eq!(format!("{}", tx_id), "0.0.12345@1234567890.123");
    }

    #[test]
    fn test_settlement_status() {
        let pending = SettlementStatus::Pending;
        assert!(pending.is_pending());

        let confirmed = SettlementStatus::confirmed(100, 1234567890);
        assert!(confirmed.is_confirmed());

        let failed = SettlementStatus::failed("out of gas");
        assert!(failed.is_failed());
    }
}
