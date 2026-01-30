//! Revenue distribution and economic calculations for the Nodalync protocol.
//!
//! This crate implements the economic rules from Protocol Specification §10:
//!
//! - **Revenue Distribution** (§10.1): Split payments between owner and root contributors
//! - **Price Validation** (§10.3): Validate prices against protocol constraints
//! - **Settlement Batching** (§10.4): Create batches for on-chain settlement
//! - **Merkle Proofs**: Allow recipients to verify their inclusion in batches
//!
//! # Key Design Decision
//!
//! The settlement contract distributes payments to ALL root contributors directly.
//! When Bob queries Alice's L3 (which derives from Carol's L0), the settlement
//! contract pays:
//! - Alice: 5% synthesis fee + her root shares
//! - Carol: her root shares
//! - Any other root contributors: their shares
//!
//! This ensures trustless distribution — Alice cannot withhold payment from Carol.
//!
//! # Example
//!
//! ```
//! use nodalync_econ::{distribute_revenue, validate_price, should_settle};
//! use nodalync_crypto::{content_hash, generate_identity, peer_id_from_public_key};
//! use nodalync_types::{ProvenanceEntry, Visibility};
//!
//! // Validate a price
//! assert!(validate_price(100).is_ok());
//! assert!(validate_price(0).is_err());
//!
//! // Distribute revenue
//! let (_, pk) = generate_identity();
//! let owner = peer_id_from_public_key(&pk);
//! let hash = content_hash(b"content");
//! let entry = ProvenanceEntry::with_weight(hash, owner, Visibility::Shared, 1);
//!
//! let distributions = distribute_revenue(100, &owner, &[entry]);
//! assert!(!distributions.is_empty());
//!
//! // Check settlement triggers
//! let should = should_settle(0, 0, 3_600_000); // 1 hour elapsed
//! assert!(should);
//! ```
//!
//! # Distribution Algorithm
//!
//! Revenue distribution follows these rules:
//!
//! 1. **Synthesis fee**: Owner receives 5% of payment
//! 2. **Root pool**: Remaining 95% distributed to root contributors by weight
//! 3. **Per-weight share**: `root_pool / total_weight`
//! 4. **Rounding**: Any remainder goes to the owner
//!
//! When the owner is also a root contributor, they receive both the synthesis
//! fee and their proportional root share.

pub mod distribution;
pub mod distributor;
pub mod error;
pub mod merkle;
pub mod price;
pub mod settlement;

// Re-export main types and functions
pub use error::{EconError, EconResult};

// Distribution functions
pub use distribution::{calculate_root_pool, calculate_synthesis_fee, distribute_revenue};

// Price validation
pub use price::{is_valid_price, validate_price};

// Settlement functions
pub use settlement::{calculate_pending_total, create_settlement_batch, should_settle};

// Merkle functions
pub use merkle::{
    compute_batch_id, compute_merkle_root, create_merkle_proof, hash_settlement_entry,
    verify_merkle_proof, MerkleProof,
};

// Distributor trait and implementations
pub use distributor::{DefaultDistributor, Distributor};

#[cfg(test)]
mod tests {
    use super::*;
    use nodalync_crypto::{
        content_hash, generate_identity, peer_id_from_public_key, Hash, PeerId, Signature,
    };
    use nodalync_types::{Payment, ProvenanceEntry, Visibility};

    fn test_peer_id() -> PeerId {
        let (_, public_key) = generate_identity();
        peer_id_from_public_key(&public_key)
    }

    fn test_hash(data: &[u8]) -> Hash {
        content_hash(data)
    }

    fn test_signature() -> Signature {
        Signature([0u8; 64])
    }

    /// Integration test: Full distribution flow
    #[test]
    fn test_full_distribution_flow() {
        // Setup: Bob owns content derived from Alice and Carol
        let bob = test_peer_id();
        let alice = test_peer_id();
        let carol = test_peer_id();

        // Bob's L3 has provenance from Alice (weight 2) and Carol (weight 1)
        let alice_entry =
            ProvenanceEntry::with_weight(test_hash(b"alice"), alice, Visibility::Shared, 2);
        let carol_entry =
            ProvenanceEntry::with_weight(test_hash(b"carol"), carol, Visibility::Shared, 1);

        // Payment of 100 tokens
        let distributions = distribute_revenue(100, &bob, &[alice_entry, carol_entry]);

        // Verify distributions
        let bob_amount = distributions
            .iter()
            .find(|d| d.recipient == bob)
            .map(|d| d.amount)
            .unwrap_or(0);
        let alice_amount = distributions
            .iter()
            .find(|d| d.recipient == alice)
            .map(|d| d.amount)
            .unwrap_or(0);
        let carol_amount = distributions
            .iter()
            .find(|d| d.recipient == carol)
            .map(|d| d.amount)
            .unwrap_or(0);

        // Synthesis fee: 5
        // Root pool: 95
        // Per weight: 95 / 3 = 31
        // Alice: 2 * 31 = 62
        // Carol: 1 * 31 = 31
        // Remainder: 95 - 93 = 2
        // Bob (synthesis + remainder): 5 + 2 = 7
        assert_eq!(bob_amount, 7);
        assert_eq!(alice_amount, 62);
        assert_eq!(carol_amount, 31);

        // Total equals payment
        assert_eq!(bob_amount + alice_amount + carol_amount, 100);
    }

    /// Integration test: Settlement batch creation
    #[test]
    fn test_settlement_batch_creation() {
        let owner = test_peer_id();
        let root = test_peer_id();

        let entry = ProvenanceEntry::with_weight(test_hash(b"src"), root, Visibility::Shared, 1);

        let payment = Payment::new(
            test_hash(b"payment"),
            test_hash(b"channel"),
            100,
            owner,
            test_hash(b"query"),
            vec![entry],
            1234567890,
            test_signature(),
        );

        // Create batch
        let batch = create_settlement_batch(&[payment]);

        // Verify batch
        assert!(!batch.is_empty());
        assert_eq!(batch.total_amount(), 100);

        // Verify merkle proofs work
        for (i, entry) in batch.entries.iter().enumerate() {
            let proof = create_merkle_proof(&batch.entries, i).unwrap();
            assert!(verify_merkle_proof(&batch.merkle_root, entry, &proof));
        }
    }

    /// Integration test: Price validation
    #[test]
    fn test_price_validation_integration() {
        // Valid prices
        assert!(validate_price(1).is_ok());
        assert!(validate_price(1_000_000_000).is_ok()); // 10 HBAR

        // Invalid prices
        assert!(validate_price(0).is_err());

        // Check helper function
        assert!(is_valid_price(100));
        assert!(!is_valid_price(0));
    }

    /// Integration test: Settlement trigger conditions
    #[test]
    fn test_settlement_triggers() {
        use nodalync_types::{SETTLEMENT_BATCH_INTERVAL_MS, SETTLEMENT_BATCH_THRESHOLD};

        // Threshold trigger
        assert!(should_settle(SETTLEMENT_BATCH_THRESHOLD, 0, 0));

        // Interval trigger
        assert!(should_settle(0, 0, SETTLEMENT_BATCH_INTERVAL_MS));

        // Neither
        assert!(!should_settle(0, 0, 0));
    }

    /// Integration test: Distributor trait usage
    #[test]
    fn test_distributor_trait() {
        let distributor = DefaultDistributor::new();
        let owner = test_peer_id();
        let root = test_peer_id();

        let entry = ProvenanceEntry::with_weight(test_hash(b"src"), root, Visibility::Shared, 1);

        let payment = Payment::new(
            test_hash(b"payment"),
            test_hash(b"channel"),
            100,
            owner,
            test_hash(b"query"),
            vec![entry],
            1234567890,
            test_signature(),
        );

        // Use trait methods
        let distributions = distributor.distribute(&payment, None);
        assert!(!distributions.is_empty());

        let batch = distributor.calculate_batch(&[payment]);
        assert!(!batch.is_empty());
    }

    /// Integration test: Merkle proof roundtrip
    #[test]
    fn test_merkle_proof_roundtrip() {
        use nodalync_types::SettlementEntry;

        let entries: Vec<SettlementEntry> = (0..5)
            .map(|i| SettlementEntry::new(test_peer_id(), 100 * (i + 1), vec![], vec![]))
            .collect();

        let root = compute_merkle_root(&entries);

        // Verify all entries
        for (i, entry) in entries.iter().enumerate() {
            let proof = create_merkle_proof(&entries, i).unwrap();
            assert!(
                verify_merkle_proof(&root, entry, &proof),
                "Failed to verify entry {}",
                i
            );
        }
    }

    /// Integration test: Helper functions
    #[test]
    fn test_helper_functions() {
        // Synthesis fee calculation
        assert_eq!(calculate_synthesis_fee(100), 5);
        assert_eq!(calculate_synthesis_fee(1000), 50);

        // Root pool calculation
        assert_eq!(calculate_root_pool(100), 95);
        assert_eq!(calculate_root_pool(1000), 950);

        // Pending total calculation
        let owner = test_peer_id();
        let payment1 = Payment::new(
            test_hash(b"p1"),
            test_hash(b"ch"),
            100,
            owner,
            test_hash(b"q"),
            vec![],
            1234567890,
            test_signature(),
        );
        let payment2 = Payment::new(
            test_hash(b"p2"),
            test_hash(b"ch"),
            50,
            owner,
            test_hash(b"q"),
            vec![],
            1234567891,
            test_signature(),
        );

        assert_eq!(calculate_pending_total(&[payment1, payment2]), 150);
    }
}
