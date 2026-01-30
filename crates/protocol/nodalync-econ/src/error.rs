//! Economic error types for the Nodalync protocol.
//!
//! This module defines the `EconError` enum used by all economic
//! functions in this crate as specified in Protocol Specification §10.

use nodalync_types::Amount;
use thiserror::Error;

/// Errors that can occur during economic calculations.
///
/// Each variant includes descriptive information explaining the error.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum EconError {
    // =========================================================================
    // Price Validation Errors (§10.3)
    // =========================================================================
    /// Price is below the minimum allowed
    #[error("price {price} is below minimum {min}")]
    PriceTooLow {
        /// The submitted price
        price: Amount,
        /// Minimum allowed price
        min: Amount,
    },

    /// Price exceeds the maximum allowed
    #[error("price {price} exceeds maximum {max}")]
    PriceTooHigh {
        /// The submitted price
        price: Amount,
        /// Maximum allowed price
        max: Amount,
    },

    // =========================================================================
    // Distribution Errors (§10.1)
    // =========================================================================
    /// Provenance chain is empty (should have at least one entry for valid L3)
    #[error("empty provenance chain")]
    EmptyProvenance,

    /// Payment amount is zero
    #[error("zero payment amount")]
    ZeroPayment,

    // =========================================================================
    // Merkle Errors (§10.4)
    // =========================================================================
    /// Merkle proof verification failed
    #[error("invalid merkle proof")]
    InvalidMerkleProof,

    /// Entry index is out of bounds
    #[error("entry index {index} out of bounds (len: {len})")]
    IndexOutOfBounds {
        /// The requested index
        index: usize,
        /// Length of the entries array
        len: usize,
    },

    /// Cannot create proof for empty entries
    #[error("cannot create merkle proof for empty entries")]
    EmptyEntries,
}

/// Result type for economic operations.
pub type EconResult<T> = std::result::Result<T, EconError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = EconError::PriceTooLow { price: 0, min: 1 };
        assert!(err.to_string().contains("below minimum"));

        let err = EconError::PriceTooHigh {
            price: 100,
            max: 50,
        };
        assert!(err.to_string().contains("exceeds maximum"));

        let err = EconError::EmptyProvenance;
        assert!(err.to_string().contains("empty provenance"));

        let err = EconError::IndexOutOfBounds { index: 5, len: 3 };
        assert!(err.to_string().contains("5"));
        assert!(err.to_string().contains("3"));
    }

    #[test]
    fn test_error_clone() {
        let err = EconError::ZeroPayment;
        let cloned = err.clone();
        assert_eq!(err, cloned);
    }

    #[test]
    fn test_error_debug() {
        let err = EconError::InvalidMerkleProof;
        let debug = format!("{:?}", err);
        assert!(debug.contains("InvalidMerkleProof"));
    }
}
