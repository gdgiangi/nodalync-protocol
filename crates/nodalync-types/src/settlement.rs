//! Settlement types for on-chain batch processing.
//!
//! This module defines the structures used for settling payments
//! on-chain in batches, as specified in docs/modules/02-types.md.

use nodalync_crypto::{Hash, PeerId};
use serde::{Deserialize, Serialize};

use crate::Amount;

/// A single distribution to a content contributor.
///
/// Created during revenue distribution calculation to track
/// how much each source contributor should receive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Distribution {
    /// Recipient's peer ID
    pub recipient: PeerId,
    /// Amount to distribute
    pub amount: Amount,
    /// Hash of the source content this is for
    pub source_hash: Hash,
}

impl Distribution {
    /// Create a new distribution.
    pub fn new(recipient: PeerId, amount: Amount, source_hash: Hash) -> Self {
        Self {
            recipient,
            amount,
            source_hash,
        }
    }
}

/// An entry in a settlement batch.
///
/// Aggregates multiple distributions to a single recipient
/// for efficient on-chain settlement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SettlementEntry {
    /// Recipient's peer ID
    pub recipient: PeerId,
    /// Total amount to settle
    pub amount: Amount,
    /// Content hashes for audit trail
    pub provenance_hashes: Vec<Hash>,
    /// Payment IDs included in this settlement
    pub payment_ids: Vec<Hash>,
}

impl SettlementEntry {
    /// Create a new settlement entry.
    pub fn new(
        recipient: PeerId,
        amount: Amount,
        provenance_hashes: Vec<Hash>,
        payment_ids: Vec<Hash>,
    ) -> Self {
        Self {
            recipient,
            amount,
            provenance_hashes,
            payment_ids,
        }
    }

    /// Create an entry from multiple distributions.
    pub fn from_distributions(
        recipient: PeerId,
        distributions: Vec<Distribution>,
        payment_ids: Vec<Hash>,
    ) -> Self {
        let amount = distributions.iter().map(|d| d.amount).sum();
        let provenance_hashes = distributions.iter().map(|d| d.source_hash).collect();

        Self {
            recipient,
            amount,
            provenance_hashes,
            payment_ids,
        }
    }

    /// Get the number of provenance sources.
    pub fn source_count(&self) -> usize {
        self.provenance_hashes.len()
    }

    /// Get the number of payments included.
    pub fn payment_count(&self) -> usize {
        self.payment_ids.len()
    }
}

/// A batch of settlements to be processed on-chain.
///
/// Batches aggregate multiple settlement entries for efficient
/// on-chain processing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SettlementBatch {
    /// Unique batch identifier
    pub batch_id: Hash,
    /// Settlement entries in this batch
    pub entries: Vec<SettlementEntry>,
    /// Merkle root of entries for verification
    pub merkle_root: Hash,
}

impl SettlementBatch {
    /// Create a new settlement batch.
    ///
    /// Note: The merkle_root should be computed by the caller.
    pub fn new(batch_id: Hash, entries: Vec<SettlementEntry>, merkle_root: Hash) -> Self {
        Self {
            batch_id,
            entries,
            merkle_root,
        }
    }

    /// Get the total amount in this batch.
    pub fn total_amount(&self) -> Amount {
        self.entries.iter().map(|e| e.amount).sum()
    }

    /// Get the number of entries.
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    /// Get the total number of payments included.
    pub fn total_payment_count(&self) -> usize {
        self.entries.iter().map(|e| e.payment_count()).sum()
    }

    /// Get all unique recipients in this batch.
    pub fn unique_recipients(&self) -> Vec<PeerId> {
        let mut recipients: Vec<PeerId> = self.entries.iter().map(|e| e.recipient).collect();
        recipients.sort_by(|a, b| a.0.cmp(&b.0));
        recipients.dedup();
        recipients
    }

    /// Check if this batch is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Check if a recipient is included in this batch.
    pub fn contains_recipient(&self, recipient: &PeerId) -> bool {
        self.entries.iter().any(|e| e.recipient == *recipient)
    }

    /// Get the amount for a specific recipient.
    pub fn amount_for_recipient(&self, recipient: &PeerId) -> Amount {
        self.entries
            .iter()
            .filter(|e| e.recipient == *recipient)
            .map(|e| e.amount)
            .sum()
    }
}

impl Default for SettlementBatch {
    fn default() -> Self {
        Self {
            batch_id: Hash([0u8; 32]),
            entries: Vec::new(),
            merkle_root: Hash([0u8; 32]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nodalync_crypto::{content_hash, generate_identity, peer_id_from_public_key};

    fn test_hash(data: &[u8]) -> Hash {
        content_hash(data)
    }

    fn test_peer_id() -> PeerId {
        let (_, public_key) = generate_identity();
        peer_id_from_public_key(&public_key)
    }

    #[test]
    fn test_distribution_new() {
        let recipient = test_peer_id();
        let source_hash = test_hash(b"source");

        let dist = Distribution::new(recipient, 100, source_hash);

        assert_eq!(dist.recipient, recipient);
        assert_eq!(dist.amount, 100);
        assert_eq!(dist.source_hash, source_hash);
    }

    #[test]
    fn test_settlement_entry_new() {
        let recipient = test_peer_id();
        let hash1 = test_hash(b"prov1");
        let hash2 = test_hash(b"prov2");
        let payment_id = test_hash(b"payment");

        let entry = SettlementEntry::new(recipient, 500, vec![hash1, hash2], vec![payment_id]);

        assert_eq!(entry.recipient, recipient);
        assert_eq!(entry.amount, 500);
        assert_eq!(entry.source_count(), 2);
        assert_eq!(entry.payment_count(), 1);
    }

    #[test]
    fn test_settlement_entry_from_distributions() {
        let recipient = test_peer_id();
        let dist1 = Distribution::new(recipient, 100, test_hash(b"src1"));
        let dist2 = Distribution::new(recipient, 200, test_hash(b"src2"));
        let payment_id = test_hash(b"payment");

        let entry =
            SettlementEntry::from_distributions(recipient, vec![dist1, dist2], vec![payment_id]);

        assert_eq!(entry.amount, 300);
        assert_eq!(entry.source_count(), 2);
    }

    #[test]
    fn test_settlement_batch_new() {
        let recipient = test_peer_id();
        let entry = SettlementEntry::new(
            recipient,
            1000,
            vec![test_hash(b"prov")],
            vec![test_hash(b"payment")],
        );
        let batch_id = test_hash(b"batch");
        let merkle_root = test_hash(b"merkle");

        let batch = SettlementBatch::new(batch_id, vec![entry], merkle_root);

        assert_eq!(batch.batch_id, batch_id);
        assert_eq!(batch.merkle_root, merkle_root);
        assert_eq!(batch.entry_count(), 1);
        assert_eq!(batch.total_amount(), 1000);
    }

    #[test]
    fn test_settlement_batch_total_amount() {
        let recipient1 = test_peer_id();
        let recipient2 = test_peer_id();

        let entry1 = SettlementEntry::new(recipient1, 1000, vec![], vec![]);
        let entry2 = SettlementEntry::new(recipient2, 500, vec![], vec![]);

        let batch = SettlementBatch::new(
            test_hash(b"batch"),
            vec![entry1, entry2],
            test_hash(b"merkle"),
        );

        assert_eq!(batch.total_amount(), 1500);
        assert_eq!(batch.entry_count(), 2);
    }

    #[test]
    fn test_settlement_batch_unique_recipients() {
        let recipient1 = test_peer_id();
        let recipient2 = test_peer_id();

        let entry1 = SettlementEntry::new(recipient1, 1000, vec![], vec![]);
        let entry2 = SettlementEntry::new(recipient1, 500, vec![], vec![]); // Same recipient
        let entry3 = SettlementEntry::new(recipient2, 200, vec![], vec![]);

        let batch = SettlementBatch::new(
            test_hash(b"batch"),
            vec![entry1, entry2, entry3],
            test_hash(b"merkle"),
        );

        let recipients = batch.unique_recipients();
        assert_eq!(recipients.len(), 2);
    }

    #[test]
    fn test_settlement_batch_contains_recipient() {
        let recipient1 = test_peer_id();
        let recipient2 = test_peer_id();
        let not_included = test_peer_id();

        let entry1 = SettlementEntry::new(recipient1, 1000, vec![], vec![]);
        let entry2 = SettlementEntry::new(recipient2, 500, vec![], vec![]);

        let batch = SettlementBatch::new(
            test_hash(b"batch"),
            vec![entry1, entry2],
            test_hash(b"merkle"),
        );

        assert!(batch.contains_recipient(&recipient1));
        assert!(batch.contains_recipient(&recipient2));
        assert!(!batch.contains_recipient(&not_included));
    }

    #[test]
    fn test_settlement_batch_amount_for_recipient() {
        let recipient1 = test_peer_id();
        let recipient2 = test_peer_id();

        let entry1 = SettlementEntry::new(recipient1, 1000, vec![], vec![]);
        let entry2 = SettlementEntry::new(recipient1, 500, vec![], vec![]); // Same recipient
        let entry3 = SettlementEntry::new(recipient2, 200, vec![], vec![]);

        let batch = SettlementBatch::new(
            test_hash(b"batch"),
            vec![entry1, entry2, entry3],
            test_hash(b"merkle"),
        );

        assert_eq!(batch.amount_for_recipient(&recipient1), 1500);
        assert_eq!(batch.amount_for_recipient(&recipient2), 200);
    }

    #[test]
    fn test_settlement_batch_is_empty() {
        let empty_batch = SettlementBatch::default();
        assert!(empty_batch.is_empty());

        let entry = SettlementEntry::new(test_peer_id(), 100, vec![], vec![]);
        let non_empty =
            SettlementBatch::new(test_hash(b"batch"), vec![entry], test_hash(b"merkle"));
        assert!(!non_empty.is_empty());
    }

    #[test]
    fn test_settlement_batch_total_payment_count() {
        let entry1 = SettlementEntry::new(
            test_peer_id(),
            1000,
            vec![],
            vec![test_hash(b"p1"), test_hash(b"p2")],
        );
        let entry2 = SettlementEntry::new(test_peer_id(), 500, vec![], vec![test_hash(b"p3")]);

        let batch = SettlementBatch::new(
            test_hash(b"batch"),
            vec![entry1, entry2],
            test_hash(b"merkle"),
        );

        assert_eq!(batch.total_payment_count(), 3);
    }

    #[test]
    fn test_distribution_serialization() {
        let dist = Distribution::new(test_peer_id(), 100, test_hash(b"src"));

        let json = serde_json::to_string(&dist).unwrap();
        let deserialized: Distribution = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.amount, dist.amount);
    }

    #[test]
    fn test_settlement_batch_serialization() {
        let entry = SettlementEntry::new(test_peer_id(), 1000, vec![], vec![]);
        let batch = SettlementBatch::new(test_hash(b"batch"), vec![entry], test_hash(b"merkle"));

        let json = serde_json::to_string(&batch).unwrap();
        let deserialized: SettlementBatch = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.total_amount(), batch.total_amount());
        assert_eq!(deserialized.entry_count(), batch.entry_count());
    }
}
