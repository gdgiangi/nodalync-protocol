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

### EVM Address Handling

**Critical for ECDSA accounts**: When interacting with the settlement contract, the EVM address
used by `msg.sender` differs based on account key type:

| Key Type | EVM Address (`msg.sender`) |
|----------|---------------------------|
| **ECDSA** | Derived from public key: `keccak256(uncompressed_pubkey)[12:]` |
| **Ed25519** | Simple padded account number: `0x000...{account_num_hex}` |

For ECDSA accounts, `AccountId::to_solidity_address()` returns the **wrong** address for
contract storage lookups. The contract uses `msg.sender` (the key-derived address) when
storing balances, but queries using `to_solidity_address()` will look up the wrong slot.

**To get the correct EVM address for any account:**
```bash
curl -s "https://testnet.mirrornode.hedera.com/api/v1/accounts/0.0.ACCOUNT_ID" | jq '.evm_address'
```

### Deposit/Withdraw

**Important**: Deposits must call the contract's `deposit()` payable function to update
the internal `balances` mapping. A simple `TransferTransaction` sends HBAR but does NOT
update the contract's balance tracking.

```rust
pub async fn deposit(&self, amount: Amount) -> Result<TransactionId> {
    // CORRECT: Call the contract's deposit() payable function
    let tx = ContractExecuteTransaction::new()
        .contract_id(self.contract_id)
        .gas(100_000)
        .payable_amount(Hbar::from_tinybars(amount as i64))
        .function("deposit")
        .execute(&self.client)
        .await?;

    let receipt = tx.get_receipt(&self.client).await?;
    Ok(receipt.transaction_id)
}

pub async fn withdraw(&self, amount: Amount) -> Result<TransactionId> {
    let tx = ContractExecuteTransaction::new()
        .contract_id(self.contract_id)
        .gas(100_000)
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

---

## Debugging & Verification

### Verify Transactions On-Chain

After any settlement operation, always verify on-chain status:

```bash
# Check recent transactions - should show CONTRACTCALL, not just CRYPTOTRANSFER
curl -s "https://testnet.mirrornode.hedera.com/api/v1/transactions?account.id=0.0.ACCOUNT&limit=5&order=desc" \
  | jq '.transactions[] | {timestamp: .consensus_timestamp, type: .name, result: .result}'

# Check contract calls specifically
curl -s "https://testnet.mirrornode.hedera.com/api/v1/contracts/0.0.7729011/results?limit=5&order=desc" \
  | jq '.results[] | {timestamp, from, result: .error_message}'
```

### Check Contract State

```bash
# View all storage slots
curl -s "https://testnet.mirrornode.hedera.com/api/v1/contracts/0.0.7729011/state" | jq '.state'

# Query balance for an address (balances mapping, selector 0x27e235e3)
# Replace EVM_ADDRESS with 40 hex chars (no 0x prefix)
curl -s -X POST "https://testnet.mirrornode.hedera.com/api/v1/contracts/call" \
  -H "Content-Type: application/json" \
  -d '{
    "block": "latest",
    "data": "0x27e235e3000000000000000000000000EVM_ADDRESS",
    "to": "0xc6b4bFD28AF2F6999B32510557380497487A60dD"
  }' | jq '.result'
```

### Check Event Logs

```bash
# View deposit/withdraw events (shows actual credited address)
curl -s "https://testnet.mirrornode.hedera.com/api/v1/contracts/0.0.7729011/results/logs?order=desc&limit=10" \
  | jq '.logs[] | {timestamp, topics, data}'
```

### Common Issues

| Symptom | Cause | Solution |
|---------|-------|----------|
| Transaction shows `CRYPTOTRANSFER` not `CONTRACTCALL` | Using `TransferTransaction` instead of `ContractExecuteTransaction` | Use `ContractExecuteTransaction` with `payable_amount()` |
| Balance query returns 0 after deposit | Wrong EVM address for ECDSA accounts | Use key-derived `evm_address` from mirror node |
| `CONTRACT_REVERT_EXECUTED` | Contract logic rejected the call | Check function parameters, balances, or channel state |
| CLI shows success but contract reverts | Receipt status not properly checked | Verify via mirror node API |

### Contract Function Selectors

| Function | Selector | Notes |
|----------|----------|-------|
| `deposit()` | `0xd0e30db0` | Payable, no parameters |
| `withdraw(uint256)` | `0x2e1a7d4d` | Amount in tinybars |
| `balances(address)` | `0x27e235e3` | Public mapping getter |
| `openChannel(bytes32,address,uint256,uint256)` | `0xcf027915` | channelId, peer, deposit1, deposit2 |
| `closeChannel(bytes32,uint256,uint256,bytes)` | varies | channelId, bal1, bal2, signatures |
| `settleBatch(bytes32,bytes32,bytes[])` | varies | batchId, merkleRoot, entries |
