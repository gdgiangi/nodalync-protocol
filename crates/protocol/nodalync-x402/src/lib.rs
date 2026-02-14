//! x402 Payment Required protocol integration for Nodalync.
//!
//! This crate implements the [x402 payment protocol](https://www.x402.org/) for
//! Nodalync knowledge access. x402 enables HTTP-native micropayments where AI
//! agents and applications can pay for knowledge access programmatically.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────┐     GET /knowledge     ┌──────────────┐
//! │  AI Agent    │ ──────────────────────→│  Nodalync    │
//! │  (Client)    │ ←────────────────────  │  Node        │
//! │              │  402 Payment Required  │  (Server)    │
//! │              │                        │              │
//! │              │  GET + X-PAYMENT hdr   │              │
//! │              │ ──────────────────────→│              │
//! │              │                        │     ┌────────┤
//! │              │                        │     │Payment │
//! │              │                        │     │Gate    │
//! │              │                        │     └───┬────┤
//! │              │                        │         │    │
//! │              │                        │    ┌────▼───┐│
//! │              │                        │    │Blocky  ││
//! │              │                        │    │402     ││
//! │              │  200 OK + content      │    │(verify ││
//! │              │ ←────────────────────  │    │+settle)││
//! │              │  + X-PAYMENT-RESPONSE  │    └────────┘│
//! └─────────────┘                        └──────────────┘
//! ```
//!
//! # Components
//!
//! - **[`types`]**: x402 protocol message types (PaymentRequired, PaymentPayload, etc.)
//! - **[`facilitator`]**: Client for Blocky402-compatible facilitators
//! - **[`gate`]**: Payment gate middleware for protecting resources
//! - **[`error`]**: Error types with recovery suggestions
//!
//! # Usage
//!
//! ## As a Resource Server (receiving payments)
//!
//! ```rust,no_run
//! use nodalync_x402::{PaymentGate, X402Config};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Configure x402 for testnet
//! let config = X402Config::testnet("0.0.12345", 5); // 5% app fee
//! let gate = PaymentGate::new(config)?;
//!
//! // When a client requests paid content without payment:
//! let payment_req = gate.payment_required(
//!     "nodalync://query/abc123",
//!     "abc123",
//!     "Knowledge about Rust programming",
//!     100, // 100 tinybars base price
//! )?;
//! // → Return 402 with payment_req as JSON in PAYMENT-REQUIRED header
//!
//! // When a client retries with payment:
//! let payment_header = "base64-encoded-payment...";
//! let response = gate.process_payment(payment_header, "abc123", 100).await?;
//! // → response.success == true, deliver the content
//! # Ok(())
//! # }
//! ```
//!
//! ## Checking Status
//!
//! ```rust
//! use nodalync_x402::{PaymentGate, X402Config};
//!
//! # async fn example() {
//! let config = X402Config::testnet("0.0.12345", 5);
//! let gate = PaymentGate::new(config).unwrap();
//!
//! let status = gate.status().await;
//! println!("Enabled: {}", status.enabled);
//! println!("Transactions: {}", status.total_transactions);
//! println!("Volume: {} tinybars", status.total_volume);
//! println!("Fees collected: {} tinybars", status.total_app_fees);
//! # }
//! ```
//!
//! # Compatibility
//!
//! This implementation is compatible with:
//! - [Blocky402](https://blocky402.com/) facilitator (Hedera testnet V1)
//! - [Coinbase CDP](https://docs.cdp.coinbase.com/x402/) facilitator (EVM chains)
//! - Any x402-compliant facilitator implementing the verify/settle endpoints
//!
//! # Hedera Payment Scheme
//!
//! Nodalync uses the "exact" payment scheme on Hedera:
//! 1. Client creates a CryptoTransfer transaction paying the resource server
//! 2. Client partially signs the transaction (leaving fee-payer slot)
//! 3. Facilitator verifies, adds gas signature, and submits to Hedera
//! 4. Settlement is confirmed on-chain

pub mod error;
pub mod facilitator;
pub mod gate;
pub mod types;

// Re-export main types
pub use error::{X402Error, X402Result};
pub use facilitator::FacilitatorClient;
pub use gate::{PaymentGate, TransactionRecord, X402Status};
pub use types::{
    PaymentPayload, PaymentRequired, PaymentResponse, X402Config, HEADER_PAYMENT_REQUIRED,
    HEADER_PAYMENT_RESPONSE, HEADER_PAYMENT_SIGNATURE, NETWORK_HEDERA_MAINNET,
    NETWORK_HEDERA_TESTNET, SCHEME_EXACT, X402_VERSION,
};
