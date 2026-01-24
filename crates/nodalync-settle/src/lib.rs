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
//! │ force_settle   │         │   ├─ MockSettlement     │
//! └────────────────┘         │   └─ HederaSettlement   │
//!                            └───────────┬─────────────┘
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
//! ## With Mock (for testing)
//!
//! ```rust
//! use nodalync_settle::{MockSettlement, Settlement};
//! use nodalync_settle::types::AccountId;
//!
//! # async fn example() -> nodalync_settle::SettleResult<()> {
//! let mock = MockSettlement::with_balance(AccountId::simple(12345), 1_000_000);
//!
//! // Deposit more tokens
//! mock.deposit(100_000).await?;
//!
//! // Check balance
//! let balance = mock.get_balance().await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## With Hedera (for production)
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
//! The [`Settlement`] trait provides a common interface for all settlement
//! implementations. This allows code to work with either the mock or real
//! implementation transparently.
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
#[cfg(feature = "hedera-sdk")]
mod hedera;
mod mock;
mod retry;
mod traits;
pub mod types;

// Re-export main types
pub use account_mapping::AccountMapper;
pub use config::{GasConfig, HederaConfig, HederaNetwork, RetryConfig};
pub use error::{SettleError, SettleResult};
#[cfg(feature = "hedera-sdk")]
pub use hedera::HederaSettlement;
pub use mock::{MockSettlement, MockSettlementBuilder};
pub use retry::RetryPolicy;
pub use traits::Settlement;

// Re-export key types from types module
pub use types::{AccountId, Attestation, ChannelId, SettlementStatus, TransactionId};

#[cfg(test)]
mod tests {
    use super::*;
    use nodalync_crypto::{content_hash, generate_identity, peer_id_from_public_key};
    use nodalync_types::{SettlementBatch, SettlementEntry};

    fn test_peer_id() -> nodalync_crypto::PeerId {
        let (_, public_key) = generate_identity();
        peer_id_from_public_key(&public_key)
    }

    #[tokio::test]
    async fn test_mock_settlement_integration() {
        // Create mock with balance
        let peer1 = test_peer_id();
        let peer2 = test_peer_id();

        let mock = MockSettlementBuilder::new()
            .balance(10_000)
            .peer_account(peer1, AccountId::simple(111))
            .peer_account(peer2, AccountId::simple(222))
            .build();

        // Verify initial balance
        assert_eq!(mock.get_balance().await.unwrap(), 10_000);

        // Create attestation
        let hash = content_hash(b"test content");
        let prov_root = content_hash(b"provenance");
        mock.attest(&hash, &prov_root).await.unwrap();

        let att = mock.get_attestation(&hash).await.unwrap();
        assert!(att.is_some());

        // Create and settle a batch
        let batch = SettlementBatch::new(
            content_hash(b"batch"),
            vec![
                SettlementEntry::new(peer1, 3000, vec![], vec![]),
                SettlementEntry::new(peer2, 2000, vec![], vec![]),
            ],
            content_hash(b"merkle"),
        );

        let tx_id = mock.settle_batch(&batch).await.unwrap();
        assert!(!tx_id.as_str().is_empty());

        // Balance should be reduced
        assert_eq!(mock.get_balance().await.unwrap(), 5_000);
    }

    #[tokio::test]
    async fn test_settlement_trait_object() {
        // Verify Settlement can be used as trait object
        let mock: Box<dyn Settlement> =
            Box::new(MockSettlement::with_balance(AccountId::simple(1), 1000));

        let balance = mock.get_balance().await.unwrap();
        assert_eq!(balance, 1000);

        mock.deposit(500).await.unwrap();
        assert_eq!(mock.get_balance().await.unwrap(), 1500);
    }

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
