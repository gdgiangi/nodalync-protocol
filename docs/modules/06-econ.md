# Module: nodalync-econ

**Source:** Protocol Specification §10

## Overview

Revenue distribution calculations. Pure functions, no I/O.

**Key Design Decision:** The settlement contract distributes payments to ALL root contributors directly. When Bob queries Alice's L3 (which derives from Carol's L0), the settlement contract pays:
- Alice: 5% synthesis fee + her root shares
- Carol: her root shares
- Any other root contributors: their shares

This ensures trustless distribution — Alice cannot withhold payment from Carol.

## Dependencies

- `nodalync-types` — ProvenanceEntry, Distribution, Amount

---

## §10.1 Revenue Distribution

### Constants

```rust
/// Synthesis fee: 5%
pub const SYNTHESIS_FEE_NUMERATOR: u64 = 5;
pub const SYNTHESIS_FEE_DENOMINATOR: u64 = 100;

/// Root pool: 95%
pub const ROOT_POOL_NUMERATOR: u64 = 95;
pub const ROOT_POOL_DENOMINATOR: u64 = 100;

/// Settlement threshold: 100 NDL (in smallest units)
pub const SETTLEMENT_BATCH_THRESHOLD: Amount = 10_000_000_000;

/// Settlement interval: 1 hour
pub const SETTLEMENT_BATCH_INTERVAL_MS: u64 = 3_600_000;
```

### Distribution Function

```rust
/// Distribute payment revenue to owner and root contributors.
/// 
/// # Arguments
/// * `payment_amount` - Total payment received
/// * `owner` - Content owner (receives synthesis fee)
/// * `provenance` - All root L0+L1 sources with weights
/// 
/// # Returns
/// Vec of distributions to each recipient
pub fn distribute_revenue(
    payment_amount: Amount,
    owner: &PeerId,
    provenance: &[ProvenanceEntry],
) -> Vec<Distribution> {
    let mut distributions = Vec::new();
    
    // Calculate shares
    let owner_share = payment_amount * SYNTHESIS_FEE_NUMERATOR / SYNTHESIS_FEE_DENOMINATOR;
    let root_pool = payment_amount * ROOT_POOL_NUMERATOR / ROOT_POOL_DENOMINATOR;
    
    // Total weight across all roots
    let total_weight: u64 = provenance.iter().map(|e| e.weight as u64).sum();
    
    if total_weight == 0 {
        // Edge case: no roots (shouldn't happen for valid L3)
        distributions.push(Distribution {
            recipient: owner.clone(),
            amount: payment_amount,
            source_hash: Hash::default(), // Owner's own content
        });
        return distributions;
    }
    
    // Per-weight share (integer division, remainder goes to owner)
    let per_weight = root_pool / total_weight;
    let mut distributed: Amount = 0;
    
    // Group by owner to aggregate payments
    let mut owner_amounts: HashMap<PeerId, Amount> = HashMap::new();
    
    for entry in provenance {
        let amount = per_weight * (entry.weight as u64);
        distributed += amount;
        
        *owner_amounts.entry(entry.owner.clone()).or_default() += amount;
    }
    
    // Add synthesis fee to owner (may already have root shares)
    let remainder = root_pool - distributed; // Rounding dust
    *owner_amounts.entry(owner.clone()).or_default() += owner_share + remainder;
    
    // Convert to distributions
    for (recipient, amount) in owner_amounts {
        if amount > 0 {
            distributions.push(Distribution {
                recipient,
                amount,
                source_hash: Hash::default(), // Aggregated
            });
        }
    }
    
    distributions
}
```

### Example (from spec)

```
Scenario:
    Bob's L3 derives from:
        - Alice's L0 (weight: 2)
        - Carol's L0 (weight: 1)
        - Bob's L0 (weight: 2)
    Total weight: 5
    
    Query payment: 100 NDL

Distribution:
    owner_share = 100 * 5/100 = 5 NDL (Bob's synthesis fee)
    root_pool = 100 * 95/100 = 95 NDL
    per_weight = 95 / 5 = 19 NDL

    Alice: 2 * 19 = 38 NDL
    Carol: 1 * 19 = 19 NDL
    Bob (roots): 2 * 19 = 38 NDL
    Bob (synthesis): 5 NDL
    Bob total: 43 NDL
    
Final:
    Alice: 38 NDL (38%)
    Carol: 19 NDL (19%)
    Bob: 43 NDL (43%)
```

---

## §10.3 Price Constraints

```rust
pub const MIN_PRICE: Amount = 1;
pub const MAX_PRICE: Amount = 10_000_000_000_000_000; // 10^16

pub fn validate_price(price: Amount) -> Result<(), EconError> {
    if price < MIN_PRICE {
        return Err(EconError::PriceTooLow);
    }
    if price > MAX_PRICE {
        return Err(EconError::PriceTooHigh);
    }
    Ok(())
}
```

---

## §10.4 Settlement Batching

```rust
/// Aggregate payments into settlement batch.
/// 
/// Combines all pending payments, aggregating by recipient.
pub fn create_settlement_batch(
    payments: &[Payment],
) -> SettlementBatch {
    let mut by_recipient: HashMap<PeerId, (Amount, Vec<Hash>, Vec<Hash>)> = HashMap::new();
    
    for payment in payments {
        // Distribute this payment
        let distributions = distribute_revenue(
            payment.amount,
            &payment.recipient,
            &payment.provenance,
        );
        
        for dist in distributions {
            let entry = by_recipient.entry(dist.recipient.clone()).or_default();
            entry.0 += dist.amount;
            if !entry.1.contains(&dist.source_hash) {
                entry.1.push(dist.source_hash);
            }
            if !entry.2.contains(&payment.id) {
                entry.2.push(payment.id.clone());
            }
        }
    }
    
    let entries: Vec<SettlementEntry> = by_recipient
        .into_iter()
        .map(|(recipient, (amount, provenance_hashes, payment_ids))| {
            SettlementEntry {
                recipient,
                amount,
                provenance_hashes,
                payment_ids,
            }
        })
        .collect();
    
    let batch_id = compute_batch_id(&entries);
    let merkle_root = compute_merkle_root(&entries);
    
    SettlementBatch {
        batch_id,
        entries,
        merkle_root,
    }
}

/// Check if settlement should be triggered.
pub fn should_settle(
    pending_total: Amount,
    last_settlement: Timestamp,
    now: Timestamp,
) -> bool {
    // Threshold reached
    if pending_total >= SETTLEMENT_BATCH_THRESHOLD {
        return true;
    }
    
    // Interval elapsed
    if now - last_settlement >= SETTLEMENT_BATCH_INTERVAL_MS {
        return true;
    }
    
    false
}
```

---

## Merkle Root Computation

```rust
/// Compute merkle root of settlement entries.
/// Allows any recipient to verify their inclusion.
pub fn compute_merkle_root(entries: &[SettlementEntry]) -> Hash {
    if entries.is_empty() {
        return Hash::default();
    }
    
    // Leaf hashes
    let mut hashes: Vec<Hash> = entries
        .iter()
        .map(|e| hash_settlement_entry(e))
        .collect();
    
    // Build tree
    while hashes.len() > 1 {
        let mut next_level = Vec::new();
        for chunk in hashes.chunks(2) {
            if chunk.len() == 2 {
                next_level.push(hash_pair(&chunk[0], &chunk[1]));
            } else {
                next_level.push(chunk[0].clone());
            }
        }
        hashes = next_level;
    }
    
    hashes.pop().unwrap()
}

fn hash_settlement_entry(entry: &SettlementEntry) -> Hash {
    let mut hasher = Sha256::new();
    hasher.update(&entry.recipient.0);
    hasher.update(&entry.amount.to_be_bytes());
    // ... hash other fields
    Hash(hasher.finalize().into())
}

fn hash_pair(a: &Hash, b: &Hash) -> Hash {
    let mut hasher = Sha256::new();
    hasher.update(&a.0);
    hasher.update(&b.0);
    Hash(hasher.finalize().into())
}
```

---

## Public API

```rust
// Distribution
pub fn distribute_revenue(
    payment_amount: Amount,
    owner: &PeerId,
    provenance: &[ProvenanceEntry],
) -> Vec<Distribution>;

// Batching
pub fn create_settlement_batch(payments: &[Payment]) -> SettlementBatch;
pub fn should_settle(pending_total: Amount, last_settlement: Timestamp, now: Timestamp) -> bool;

// Validation
pub fn validate_price(price: Amount) -> Result<(), EconError>;

// Merkle proofs
pub fn compute_merkle_root(entries: &[SettlementEntry]) -> Hash;
pub fn create_merkle_proof(entries: &[SettlementEntry], index: usize) -> MerkleProof;
pub fn verify_merkle_proof(root: &Hash, entry: &SettlementEntry, proof: &MerkleProof) -> bool;
```

---

## Test Cases

1. **Basic distribution**: 100 tokens, single root → 95 to root, 5 to owner
2. **Multiple roots**: Verify equal per-weight distribution
3. **Owner is root**: Owner gets synthesis fee + root share
4. **Rounding**: Integer division remainder goes to owner
5. **Zero payment**: Handle gracefully
6. **Empty provenance**: All to owner
7. **Batch aggregation**: Multiple payments to same recipient aggregate
8. **Merkle proof**: Create proof, verify proof
9. **Settlement trigger**: Threshold triggers, interval triggers
