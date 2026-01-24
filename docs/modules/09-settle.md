# Module: nodalync-settle

**Source:** Protocol Specification §12

## Overview

Blockchain settlement on Hedera Hashgraph. Handles deposits, withdrawals, channel management, and batch settlement.

**Key Design Decision:** The settlement contract distributes payments to ALL recipients directly.
When a settlement batch is submitted, the contract pays:
- Content owners (5% synthesis fee + any root shares they have)
- All root contributors (their proportional shares)

This ensures trustless distribution — content owners cannot withhold payments from upstream contributors.
All recipients must have Hedera accounts to receive payments.

## Dependencies

- `nodalync-types` — Settlement types
- `nodalync-econ` — Batch creation
- `hedera-sdk` — Hedera integration

---

## §12.1 Chain Selection

**Primary chain:** Hedera Hashgraph

**Rationale:**
- Fast finality (3-5 seconds)
- Low cost (~$0.0001/tx)
- High throughput (10,000+ TPS)
- Enterprise backing (helps with non-crypto user trust)

---

## §12.2 On-Chain Data

### Contract State

```solidity
// Simplified representation of on-chain state

contract NodalyncSettlement {
    // Token balances
    mapping(address => uint256) public balances;
    
    // Payment channels
    struct Channel {
        address participant1;
        address participant2;
        uint256 balance1;
        uint256 balance2;
        uint64 nonce;
        ChannelStatus status;
    }
    mapping(bytes32 => Channel) public channels;
    
    // Content attestations
    struct Attestation {
        bytes32 contentHash;
        address owner;
        uint64 timestamp;
        bytes32 provenanceRoot;
    }
    mapping(bytes32 => Attestation) public attestations;
}
```

---

## §12.3 Contract Operations

### Deposit/Withdraw

```rust
pub async fn deposit(&self, amount: Amount) -> Result<TransactionId> {
    let tx = TransferTransaction::new()
        .hbar_transfer(self.account_id, Hbar::from_tinybars(-(amount as i64)))
        .hbar_transfer(self.contract_id, Hbar::from_tinybars(amount as i64))
        .execute(&self.client)
        .await?;
    
    let receipt = tx.get_receipt(&self.client).await?;
    Ok(receipt.transaction_id)
}

pub async fn withdraw(&self, amount: Amount) -> Result<TransactionId> {
    let tx = ContractExecuteTransaction::new()
        .contract_id(self.contract_id)
        .function("withdraw")
        .function_parameters(ContractFunctionParameters::new().add_uint256(amount))
        .execute(&self.client)
        .await?;
    
    Ok(tx.transaction_id)
}
```

### Content Attestation

```rust
pub async fn attest(
    &self,
    content_hash: &Hash,
    provenance_root: &Hash,
) -> Result<TransactionId> {
    let tx = ContractExecuteTransaction::new()
        .contract_id(self.contract_id)
        .function("attest")
        .function_parameters(
            ContractFunctionParameters::new()
                .add_bytes32(&content_hash.0)
                .add_bytes32(&provenance_root.0)
        )
        .execute(&self.client)
        .await?;
    
    Ok(tx.transaction_id)
}
```

### Channel Operations

```rust
pub async fn open_channel(
    &self,
    peer: &AccountId,
    my_deposit: Amount,
    peer_deposit: Amount,
) -> Result<(ChannelId, TransactionId)> {
    let channel_id = compute_channel_id(&self.account_id, peer);
    
    let tx = ContractExecuteTransaction::new()
        .contract_id(self.contract_id)
        .function("openChannel")
        .function_parameters(
            ContractFunctionParameters::new()
                .add_bytes32(&channel_id.0)
                .add_address(peer)
                .add_uint256(my_deposit)
                .add_uint256(peer_deposit)
        )
        .execute(&self.client)
        .await?;
    
    Ok((channel_id, tx.transaction_id))
}

pub async fn close_channel(
    &self,
    channel_id: &ChannelId,
    final_balances: ChannelBalances,
    signatures: [Signature; 2],
) -> Result<TransactionId> {
    // NOTE: The spec's ChannelClosePayload.settlement_tx is the encoded
    // bytes of this on-chain call. Both parties must agree on final_balances
    // and sign before submitting.
    let tx = ContractExecuteTransaction::new()
        .contract_id(self.contract_id)
        .function("closeChannel")
        .function_parameters(
            ContractFunctionParameters::new()
                .add_bytes32(&channel_id.0)
                .add_uint256(final_balances.initiator)
                .add_uint256(final_balances.responder)
                .add_bytes(&signatures[0].0)
                .add_bytes(&signatures[1].0)
        )
        .execute(&self.client)
        .await?;
    
    Ok(tx.transaction_id)
}

/// Create settlement_tx bytes for ChannelClosePayload
pub fn create_close_tx_bytes(
    &self,
    channel_id: &ChannelId,
    final_balances: &ChannelBalances,
) -> Vec<u8> {
    // Encode the proposed close transaction for P2P negotiation
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&channel_id.0);
    bytes.extend_from_slice(&final_balances.initiator.to_be_bytes());
    bytes.extend_from_slice(&final_balances.responder.to_be_bytes());
    bytes
}

pub async fn dispute_channel(
    &self,
    channel_id: &ChannelId,
    claimed_state: &ChannelUpdatePayload,
) -> Result<TransactionId> {
    let tx = ContractExecuteTransaction::new()
        .contract_id(self.contract_id)
        .function("disputeChannel")
        .function_parameters(
            ContractFunctionParameters::new()
                .add_bytes32(&channel_id.0)
                .add_uint64(claimed_state.nonce)
                .add_uint256(claimed_state.balances.initiator)
                .add_uint256(claimed_state.balances.responder)
                .add_bytes(&claimed_state.signature.0)
        )
        .execute(&self.client)
        .await?;
    
    Ok(tx.transaction_id)
}

/// Resolve a dispute after the dispute period (24 hours).
/// The contract will use the highest-nonce state submitted during the dispute period.
pub async fn resolve_dispute(
    &self,
    channel_id: &ChannelId,
) -> Result<TransactionId> {
    // After CHANNEL_DISPUTE_PERIOD_MS (24 hours), anyone can call resolve
    let tx = ContractExecuteTransaction::new()
        .contract_id(self.contract_id)
        .function("resolveDispute")
        .function_parameters(
            ContractFunctionParameters::new()
                .add_bytes32(&channel_id.0)
        )
        .execute(&self.client)
        .await?;
    
    Ok(tx.transaction_id)
}

/// Submit a counter-claim during dispute period with a higher nonce state
pub async fn counter_dispute(
    &self,
    channel_id: &ChannelId,
    better_state: &ChannelUpdatePayload,
) -> Result<TransactionId> {
    // If we have a state with higher nonce, submit it to win the dispute
    let tx = ContractExecuteTransaction::new()
        .contract_id(self.contract_id)
        .function("counterDispute")
        .function_parameters(
            ContractFunctionParameters::new()
                .add_bytes32(&channel_id.0)
                .add_uint64(better_state.nonce)
                .add_uint256(better_state.balances.initiator)
                .add_uint256(better_state.balances.responder)
                .add_bytes(&better_state.signature.0)
        )
        .execute(&self.client)
        .await?;
    
    Ok(tx.transaction_id)
}
```

### Batch Settlement

```rust
pub async fn settle_batch(&self, batch: SettlementBatch) -> Result<TransactionId> {
    // Encode batch entries
    let entries_encoded: Vec<Vec<u8>> = batch.entries
        .iter()
        .map(|e| encode_settlement_entry(e))
        .collect();
    
    let tx = ContractExecuteTransaction::new()
        .contract_id(self.contract_id)
        .function("settleBatch")
        .function_parameters(
            ContractFunctionParameters::new()
                .add_bytes32(&batch.batch_id.0)
                .add_bytes32(&batch.merkle_root.0)
                .add_bytes_array(&entries_encoded)
        )
        .execute(&self.client)
        .await?;
    
    Ok(tx.transaction_id)
}
```

---

## Settlement Trait

```rust
#[async_trait]
pub trait Settlement {
    // Balance management
    async fn deposit(&self, amount: Amount) -> Result<TransactionId>;
    async fn withdraw(&self, amount: Amount) -> Result<TransactionId>;
    async fn get_balance(&self) -> Result<Amount>;
    
    // Attestations
    async fn attest(&self, content_hash: &Hash, provenance_root: &Hash) -> Result<TransactionId>;
    async fn get_attestation(&self, content_hash: &Hash) -> Result<Option<Attestation>>;
    
    // Channels
    async fn open_channel(&self, peer: &AccountId, deposit: Amount) -> Result<ChannelId>;
    async fn close_channel(&self, channel_id: &ChannelId, final_state: ChannelBalances, signatures: [Signature; 2]) -> Result<TransactionId>;
    async fn dispute_channel(&self, channel_id: &ChannelId, state: &ChannelUpdatePayload) -> Result<TransactionId>;
    async fn counter_dispute(&self, channel_id: &ChannelId, better_state: &ChannelUpdatePayload) -> Result<TransactionId>;
    async fn resolve_dispute(&self, channel_id: &ChannelId) -> Result<TransactionId>;
    
    // Batch settlement - distributes to ALL recipients in the batch
    async fn settle_batch(&self, batch: SettlementBatch) -> Result<TransactionId>;
    async fn verify_settlement(&self, tx_id: &TransactionId) -> Result<SettlementStatus>;
}

pub enum SettlementStatus {
    Pending,
    Confirmed { block: u64, timestamp: Timestamp },
    Failed { reason: String },
}
```

---

## Configuration

```toml
[settlement]
# Hedera network: mainnet, testnet, previewnet
network = "testnet"

# Account ID (format: 0.0.12345)
account_id = "0.0.12345"

# Private key (or path to file)
private_key_path = "~/.nodalync/hedera.key"

# Contract ID
contract_id = "0.0.67890"

# Gas limits
max_gas_attest = 100000
max_gas_settle = 500000
```

---

## Test Cases (Testnet)

1. **Deposit**: Deposit tokens → balance increases
2. **Withdraw**: Withdraw tokens → balance decreases
3. **Attest**: Create attestation → retrievable on-chain
4. **Channel lifecycle**: Open → update → close
5. **Dispute initiation**: Submit dispute → channel enters Disputed state
6. **Counter dispute**: Submit higher-nonce state → wins dispute
7. **Dispute resolution**: After 24h → resolve settles to highest nonce
8. **Batch settlement**: Multiple recipients settled in one tx
9. **Batch distribution**: All root contributors receive correct amounts
10. **Merkle verification**: Prove inclusion in batch
