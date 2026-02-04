//! Hedera settlement implementation.
//!
//! This module is only available when the `hedera-sdk` feature is enabled.
//! It requires `protoc` to be installed for compilation.

use std::str::FromStr;
use std::sync::RwLock;

use async_trait::async_trait;
use hiero_sdk::{
    AccountBalanceQuery, AccountId as HederaAccountId, Client, ContractCallQuery,
    ContractExecuteTransaction, ContractFunctionParameters, ContractId, Hbar, PrivateKey,
    TransactionId as HederaTransactionId, TransactionReceiptQuery,
};
use nodalync_crypto::{Hash, PeerId, Signature, Timestamp};
use nodalync_types::SettlementBatch;
use nodalync_wire::{ChannelBalances, ChannelUpdatePayload};
use tracing::{debug, info, warn};

use crate::account_mapping::AccountMapper;
use crate::config::HederaConfig;
use crate::error::{SettleError, SettleResult};
use crate::retry::RetryPolicy;
use crate::traits::Settlement;
use crate::types::{AccountId, Attestation, ChannelId, SettlementStatus, TransactionId};

/// Hedera settlement implementation.
///
/// Connects to the Hedera network for on-chain settlement operations.
pub struct HederaSettlement {
    /// Hedera SDK client
    client: Client,
    /// Operator account ID
    operator_id: HederaAccountId,
    /// Operator's EVM address (derived from ECDSA key, used as msg.sender in contracts)
    operator_evm_address: String,
    /// Settlement contract ID
    contract_id: ContractId,
    /// Account mapping (PeerId -> AccountId)
    account_mapper: RwLock<AccountMapper>,
    /// Retry policy for transient failures
    retry_policy: RetryPolicy,
    /// Gas configuration
    config: HederaConfig,
}

impl HederaSettlement {
    /// Create a new Hedera settlement instance.
    ///
    /// Loads credentials from the config and initializes the Hedera client.
    pub async fn new(config: HederaConfig) -> SettleResult<Self> {
        // Read private key from file
        let key_bytes = std::fs::read_to_string(&config.private_key_path)?;
        let private_key = PrivateKey::from_str(key_bytes.trim())
            .map_err(|e| SettleError::config(format!("invalid private key: {}", e)))?;

        // Derive EVM address from the ECDSA public key (keccak256(pubkey)[12:])
        // This is the address that will be msg.sender in contract calls
        let operator_evm_address = private_key
            .public_key()
            .to_evm_address()
            .map(|addr| format!("{}", addr))
            .ok_or_else(|| {
                SettleError::config(
                    "private key must be ECDSA to derive EVM address for contract calls",
                )
            })?;

        // Parse account and contract IDs
        let operator_id = HederaAccountId::from_str(&config.account_id)
            .map_err(|e| SettleError::InvalidAccountId(format!("{}: {}", config.account_id, e)))?;

        let contract_id = ContractId::from_str(&config.contract_id)
            .map_err(|e| SettleError::config(format!("invalid contract ID: {}", e)))?;

        // Create client for the appropriate network
        let client = match config.network {
            crate::config::HederaNetwork::Mainnet => Client::for_mainnet(),
            crate::config::HederaNetwork::Testnet => Client::for_testnet(),
            crate::config::HederaNetwork::Previewnet => Client::for_previewnet(),
        };

        // Set operator credentials
        client.set_operator(operator_id, private_key);

        info!(
            network = %config.network,
            operator = %config.account_id,
            evm_address = %operator_evm_address,
            contract = %config.contract_id,
            "Hedera settlement initialized"
        );

        Ok(Self {
            client,
            operator_id,
            operator_evm_address,
            contract_id,
            account_mapper: RwLock::new(AccountMapper::new()),
            retry_policy: RetryPolicy::from_config(&config.retry),
            config,
        })
    }

    /// Convert our AccountId to Hedera's AccountId.
    fn to_hedera_account(&self, account: &AccountId) -> HederaAccountId {
        HederaAccountId::new(account.shard, account.realm, account.num)
    }

    /// Convert Hedera's TransactionId to our TransactionId.
    fn from_hedera_tx_id(tx_id: &HederaTransactionId) -> TransactionId {
        TransactionId::new(tx_id.to_string())
    }

    /// Get the current timestamp in milliseconds.
    fn current_timestamp(&self) -> Timestamp {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("Time went backwards")
            .as_millis() as Timestamp
    }

    /// Encode a settlement entry for the contract call (legacy, uses raw AccountId bytes).
    #[allow(dead_code)]
    fn encode_settlement_entry(
        &self,
        recipient: &AccountId,
        amount: u64,
        provenance_hashes: &[Hash],
    ) -> Vec<u8> {
        let mut bytes = Vec::new();

        // Recipient account (24 bytes: shard.realm.num)
        bytes.extend_from_slice(&recipient.shard.to_be_bytes());
        bytes.extend_from_slice(&recipient.realm.to_be_bytes());
        bytes.extend_from_slice(&recipient.num.to_be_bytes());

        // Amount (8 bytes)
        bytes.extend_from_slice(&amount.to_be_bytes());

        // Number of provenance hashes (4 bytes)
        bytes.extend_from_slice(&(provenance_hashes.len() as u32).to_be_bytes());

        // Provenance hashes
        for hash in provenance_hashes {
            bytes.extend_from_slice(&hash.0);
        }

        bytes
    }

    /// Encode a settlement entry using the resolved EVM address.
    ///
    /// The contract tracks balances by EVM address (20 bytes), not by AccountId.
    /// This method encodes entries with the correct EVM address so `settleBatch`
    /// credits the right on-chain identity.
    fn encode_settlement_entry_evm(
        &self,
        evm_address: &str,
        amount: u64,
        provenance_hashes: &[Hash],
    ) -> Vec<u8> {
        let mut bytes = Vec::new();

        // Recipient EVM address (20 bytes, decoded from 40-char hex string)
        let address_bytes: Vec<u8> = (0..evm_address.len())
            .step_by(2)
            .filter_map(|i| u8::from_str_radix(&evm_address[i..i + 2], 16).ok())
            .collect();
        // Pad or truncate to exactly 20 bytes
        let mut address_20 = [0u8; 20];
        let len = address_bytes.len().min(20);
        address_20[20 - len..].copy_from_slice(&address_bytes[..len]);
        bytes.extend_from_slice(&address_20);

        // Amount (8 bytes)
        bytes.extend_from_slice(&amount.to_be_bytes());

        // Number of provenance hashes (4 bytes)
        bytes.extend_from_slice(&(provenance_hashes.len() as u32).to_be_bytes());

        // Provenance hashes
        for hash in provenance_hashes {
            bytes.extend_from_slice(&hash.0);
        }

        bytes
    }

    /// Wait for a transaction receipt.
    async fn wait_for_receipt(
        &self,
        tx_id: &HederaTransactionId,
    ) -> SettleResult<hiero_sdk::TransactionReceipt> {
        self.retry_policy
            .execute(|| async {
                TransactionReceiptQuery::new()
                    .transaction_id(*tx_id)
                    .execute(&self.client)
                    .await
                    .map_err(crate::error::classify_sdk_error)
            })
            .await
    }

    /// Resolve the EVM address for a Hedera account via the Mirror Node REST API.
    ///
    /// For ECDSA accounts, `AccountId::to_solidity_address()` returns the account-number-derived
    /// address, but the contract's `msg.sender` uses the key-derived EVM address. This method
    /// fetches the correct EVM address from the Mirror Node and caches it.
    async fn resolve_evm_address(&self, account: &AccountId) -> SettleResult<String> {
        // Check cache first
        {
            let mapper = self
                .account_mapper
                .read()
                .map_err(|_| SettleError::internal("account mapper lock poisoned"))?;
            if let Some(cached) = mapper.get_evm_address(account) {
                return Ok(cached.to_string());
            }
        }

        // Fetch from Mirror Node
        let hedera_account = self.to_hedera_account(account);
        let url = format!(
            "{}/api/v1/accounts/{}",
            self.config.network.mirror_node_url(),
            hedera_account
        );

        let response = reqwest::get(&url)
            .await
            .map_err(|e| SettleError::network(format!("Mirror Node request failed: {}", e)))?;

        if !response.status().is_success() {
            return Err(SettleError::network(format!(
                "Mirror Node returned status {} for account {}",
                response.status(),
                hedera_account
            )));
        }

        let body: serde_json::Value = response.json().await.map_err(|e| {
            SettleError::network(format!("Mirror Node response parse error: {}", e))
        })?;

        let raw_address = body["evm_address"].as_str().ok_or_else(|| {
            SettleError::hedera_sdk(format!(
                "Mirror Node response missing evm_address for account {}",
                hedera_account
            ))
        })?;
        let evm_address = raw_address
            .strip_prefix("0x")
            .unwrap_or(raw_address)
            .to_string();

        // Cache the result
        {
            let mut mapper = self
                .account_mapper
                .write()
                .map_err(|_| SettleError::internal("account mapper lock poisoned"))?;
            mapper.set_evm_address(*account, evm_address.clone());
        }

        debug!(account = %hedera_account, evm_address = %evm_address, "Resolved EVM address");
        Ok(evm_address)
    }
}

#[async_trait]
impl Settlement for HederaSettlement {
    async fn deposit(&self, amount: u64) -> SettleResult<TransactionId> {
        debug!(amount, "Depositing to settlement contract");

        // Call the contract's deposit() payable function
        let tx = self
            .retry_policy
            .execute(|| async {
                ContractExecuteTransaction::new()
                    .contract_id(self.contract_id)
                    .gas(self.config.gas.max_gas_deposit)
                    .payable_amount(Hbar::from_tinybars(amount as i64))
                    .function("deposit")
                    .execute(&self.client)
                    .await
                    .map_err(crate::error::classify_sdk_error)
            })
            .await?;

        let receipt = self.wait_for_receipt(&tx.transaction_id).await?;

        if receipt.status != hiero_sdk::Status::Success {
            return Err(SettleError::transaction_failed(format!(
                "deposit failed: {:?}",
                receipt.status
            )));
        }

        info!(amount, tx_id = %tx.transaction_id, "Deposit successful");
        Ok(Self::from_hedera_tx_id(&tx.transaction_id))
    }

    async fn withdraw(&self, amount: u64) -> SettleResult<TransactionId> {
        debug!(amount, "Withdrawing from settlement contract");

        let tx = self
            .retry_policy
            .execute(|| async {
                ContractExecuteTransaction::new()
                    .contract_id(self.contract_id)
                    .gas(self.config.gas.max_gas_withdraw)
                    .function_with_parameters(
                        "withdraw",
                        ContractFunctionParameters::new().add_uint256(amount.into()),
                    )
                    .execute(&self.client)
                    .await
                    .map_err(crate::error::classify_sdk_error)
            })
            .await?;

        let receipt = self.wait_for_receipt(&tx.transaction_id).await?;

        if receipt.status != hiero_sdk::Status::Success {
            return Err(SettleError::transaction_failed(format!(
                "withdraw failed: {:?}",
                receipt.status
            )));
        }

        info!(amount, tx_id = %tx.transaction_id, "Withdrawal successful");
        Ok(Self::from_hedera_tx_id(&tx.transaction_id))
    }

    async fn get_balance(&self) -> SettleResult<u64> {
        // Query the contract's balances(address) mapping for the operator's deposited balance
        // Use the EVM address derived from the ECDSA key (this is what msg.sender is in contracts)
        let result = self
            .retry_policy
            .execute(|| async {
                ContractCallQuery::new()
                    .contract_id(self.contract_id)
                    .gas(100_000)
                    .function_with_parameters(
                        "balances",
                        ContractFunctionParameters::new().add_address(&self.operator_evm_address),
                    )
                    .execute(&self.client)
                    .await
                    .map_err(crate::error::classify_sdk_error)
            })
            .await?;

        // The balances mapping returns a uint256, extract it
        let balance = result
            .get_u256(0)
            .ok_or_else(|| SettleError::hedera_sdk("failed to decode balance from contract"))?
            .try_into()
            .map_err(|_| SettleError::hedera_sdk("balance overflow"))?;

        Ok(balance)
    }

    async fn get_account_balance(&self) -> SettleResult<u64> {
        // Query the actual Hedera account balance (not the contract deposit)
        let balance = self
            .retry_policy
            .execute(|| async {
                AccountBalanceQuery::new()
                    .account_id(self.operator_id)
                    .execute(&self.client)
                    .await
                    .map_err(crate::error::classify_sdk_error)
            })
            .await?;

        // Convert Hbar to tinybars (u64)
        let tinybars = balance.hbars.to_tinybars();
        Ok(tinybars as u64)
    }

    async fn attest(
        &self,
        content_hash: &Hash,
        provenance_root: &Hash,
    ) -> SettleResult<TransactionId> {
        debug!(
            content_hash = %content_hash,
            provenance_root = %provenance_root,
            "Creating attestation"
        );

        let tx = self
            .retry_policy
            .execute(|| async {
                ContractExecuteTransaction::new()
                    .contract_id(self.contract_id)
                    .gas(self.config.gas.max_gas_attest)
                    .function_with_parameters(
                        "attest",
                        ContractFunctionParameters::new()
                            .add_bytes32(&content_hash.0)
                            .add_bytes32(&provenance_root.0),
                    )
                    .execute(&self.client)
                    .await
                    .map_err(crate::error::classify_sdk_error)
            })
            .await?;

        let receipt = self.wait_for_receipt(&tx.transaction_id).await?;

        if receipt.status != hiero_sdk::Status::Success {
            return Err(SettleError::transaction_failed(format!(
                "attest failed: {:?}",
                receipt.status
            )));
        }

        info!(
            content_hash = %content_hash,
            tx_id = %tx.transaction_id,
            "Attestation created"
        );
        Ok(Self::from_hedera_tx_id(&tx.transaction_id))
    }

    async fn get_attestation(&self, content_hash: &Hash) -> SettleResult<Option<Attestation>> {
        // Query contract logs via Mirror Node REST API for attestation events
        let content_hash_hex: String = content_hash
            .0
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect();
        let url = format!(
            "{}/api/v1/contracts/{}/results/logs?order=desc&limit=25",
            self.config.network.mirror_node_url(),
            self.config.contract_id,
        );

        let response = reqwest::get(&url)
            .await
            .map_err(|e| SettleError::network(format!("Mirror Node request failed: {}", e)))?;

        if !response.status().is_success() {
            debug!(
                status = %response.status(),
                "Mirror Node returned non-success for attestation query"
            );
            return Ok(None);
        }

        let body: serde_json::Value = response.json().await.map_err(|e| {
            SettleError::network(format!("Mirror Node response parse error: {}", e))
        })?;

        // Search logs for matching content hash in event topics/data.
        // Ethereum log data is ABI-encoded: content_hash is zero-padded to 32 bytes.
        let padded_hash = format!("{:0>64}", content_hash_hex);
        if let Some(logs) = body["logs"].as_array() {
            for log in logs {
                // Check topics (indexed params) and data (non-indexed)
                let matches_topic = log["topics"]
                    .as_array()
                    .map(|topics| {
                        topics.iter().any(|t| {
                            t.as_str()
                                .map(|s| s.contains(&padded_hash))
                                .unwrap_or(false)
                        })
                    })
                    .unwrap_or(false);
                let matches_data = log["data"]
                    .as_str()
                    .map(|d| d.contains(&padded_hash))
                    .unwrap_or(false);

                if matches_topic || matches_data {
                    // Parse timestamp — skip this log entry if timestamp is missing
                    let timestamp = match log["timestamp"]
                        .as_str()
                        .and_then(|t| t.split('.').next())
                        .and_then(|t| t.parse::<u64>().ok())
                    {
                        Some(ts) => ts,
                        None => continue,
                    };

                    return Ok(Some(Attestation::new(
                        *content_hash,
                        self.get_own_account(),
                        timestamp,
                        *content_hash, // provenance root (simplified — full extraction would require ABI decoding)
                    )));
                }
            }
        }

        Ok(None)
    }

    async fn open_channel(
        &self,
        channel_id: &ChannelId,
        peer: &PeerId,
        deposit: u64,
    ) -> SettleResult<TransactionId> {
        let peer_account = self
            .account_mapper
            .read()
            .map_err(|_| SettleError::internal("account mapper lock poisoned"))?
            .require_account(peer)?;

        // Resolve the peer's EVM address via Mirror Node (not to_solidity_address which
        // returns the wrong address for ECDSA accounts)
        let peer_evm_address = self.resolve_evm_address(&peer_account).await?;

        debug!(
            peer = %peer,
            deposit,
            channel_id = %channel_id,
            "Opening payment channel"
        );

        let channel_hash = channel_id.0;

        let tx = self
            .retry_policy
            .execute(|| async {
                ContractExecuteTransaction::new()
                    .contract_id(self.contract_id)
                    .gas(self.config.gas.max_gas_channel_open)
                    .function_with_parameters(
                        "openChannel",
                        ContractFunctionParameters::new()
                            .add_bytes32(&channel_hash.0)
                            .add_address(&peer_evm_address)
                            .add_uint256(deposit.into())
                            .add_uint256(0u64.into()), // peer deposit (0 initially)
                    )
                    .execute(&self.client)
                    .await
                    .map_err(crate::error::classify_sdk_error)
            })
            .await?;

        let receipt = self.wait_for_receipt(&tx.transaction_id).await?;

        if receipt.status != hiero_sdk::Status::Success {
            return Err(SettleError::transaction_failed(format!(
                "open channel failed: {:?}",
                receipt.status
            )));
        }

        info!(channel_id = %channel_id, "Channel opened");
        Ok(Self::from_hedera_tx_id(&tx.transaction_id))
    }

    async fn close_channel(
        &self,
        channel_id: &ChannelId,
        final_state: &ChannelBalances,
        signatures: &[Signature],
    ) -> SettleResult<TransactionId> {
        debug!(channel_id = %channel_id, "Closing payment channel");

        // Concatenate signatures
        let mut sig_bytes = Vec::new();
        for sig in signatures {
            sig_bytes.extend_from_slice(&sig.0);
        }

        let tx = self
            .retry_policy
            .execute(|| async {
                ContractExecuteTransaction::new()
                    .contract_id(self.contract_id)
                    .gas(self.config.gas.max_gas_channel_close)
                    .function_with_parameters(
                        "closeChannel",
                        ContractFunctionParameters::new()
                            .add_bytes32(&channel_id.0 .0)
                            .add_uint256(final_state.initiator.into())
                            .add_uint256(final_state.responder.into())
                            .add_bytes(&sig_bytes),
                    )
                    .execute(&self.client)
                    .await
                    .map_err(crate::error::classify_sdk_error)
            })
            .await?;

        let receipt = self.wait_for_receipt(&tx.transaction_id).await?;

        if receipt.status != hiero_sdk::Status::Success {
            return Err(SettleError::transaction_failed(format!(
                "close channel failed: {:?}",
                receipt.status
            )));
        }

        info!(channel_id = %channel_id, "Channel closed");
        Ok(Self::from_hedera_tx_id(&tx.transaction_id))
    }

    async fn dispute_channel(
        &self,
        channel_id: &ChannelId,
        state: &ChannelUpdatePayload,
    ) -> SettleResult<TransactionId> {
        debug!(
            channel_id = %channel_id,
            nonce = state.nonce,
            "Initiating channel dispute"
        );

        let tx = self
            .retry_policy
            .execute(|| async {
                ContractExecuteTransaction::new()
                    .contract_id(self.contract_id)
                    .gas(self.config.gas.max_gas_dispute)
                    .function_with_parameters(
                        "disputeChannel",
                        ContractFunctionParameters::new()
                            .add_bytes32(&channel_id.0 .0)
                            .add_uint64(state.nonce)
                            .add_uint256(state.balances.initiator.into())
                            .add_uint256(state.balances.responder.into())
                            .add_bytes(&state.signature.0),
                    )
                    .execute(&self.client)
                    .await
                    .map_err(crate::error::classify_sdk_error)
            })
            .await?;

        let receipt = self.wait_for_receipt(&tx.transaction_id).await?;

        if receipt.status != hiero_sdk::Status::Success {
            return Err(SettleError::transaction_failed(format!(
                "dispute failed: {:?}",
                receipt.status
            )));
        }

        info!(channel_id = %channel_id, "Dispute initiated");
        Ok(Self::from_hedera_tx_id(&tx.transaction_id))
    }

    async fn counter_dispute(
        &self,
        channel_id: &ChannelId,
        better_state: &ChannelUpdatePayload,
    ) -> SettleResult<TransactionId> {
        debug!(
            channel_id = %channel_id,
            nonce = better_state.nonce,
            "Submitting counter-dispute"
        );

        let tx = self
            .retry_policy
            .execute(|| async {
                ContractExecuteTransaction::new()
                    .contract_id(self.contract_id)
                    .gas(self.config.gas.max_gas_dispute)
                    .function_with_parameters(
                        "counterDispute",
                        ContractFunctionParameters::new()
                            .add_bytes32(&channel_id.0 .0)
                            .add_uint64(better_state.nonce)
                            .add_uint256(better_state.balances.initiator.into())
                            .add_uint256(better_state.balances.responder.into())
                            .add_bytes(&better_state.signature.0),
                    )
                    .execute(&self.client)
                    .await
                    .map_err(crate::error::classify_sdk_error)
            })
            .await?;

        let receipt = self.wait_for_receipt(&tx.transaction_id).await?;

        if receipt.status != hiero_sdk::Status::Success {
            return Err(SettleError::transaction_failed(format!(
                "counter-dispute failed: {:?}",
                receipt.status
            )));
        }

        info!(channel_id = %channel_id, "Counter-dispute submitted");
        Ok(Self::from_hedera_tx_id(&tx.transaction_id))
    }

    async fn resolve_dispute(&self, channel_id: &ChannelId) -> SettleResult<TransactionId> {
        debug!(channel_id = %channel_id, "Resolving dispute");

        let tx = self
            .retry_policy
            .execute(|| async {
                ContractExecuteTransaction::new()
                    .contract_id(self.contract_id)
                    .gas(self.config.gas.max_gas_channel_close)
                    .function_with_parameters(
                        "resolveDispute",
                        ContractFunctionParameters::new().add_bytes32(&channel_id.0 .0),
                    )
                    .execute(&self.client)
                    .await
                    .map_err(crate::error::classify_sdk_error)
            })
            .await?;

        let receipt = self.wait_for_receipt(&tx.transaction_id).await?;

        if receipt.status != hiero_sdk::Status::Success {
            return Err(SettleError::transaction_failed(format!(
                "resolve dispute failed: {:?}",
                receipt.status
            )));
        }

        info!(channel_id = %channel_id, "Dispute resolved");
        Ok(Self::from_hedera_tx_id(&tx.transaction_id))
    }

    async fn settle_batch(&self, batch: &SettlementBatch) -> SettleResult<TransactionId> {
        if batch.is_empty() {
            return Err(SettleError::EmptyBatch);
        }

        info!(
            batch_id = %batch.batch_id,
            entries = batch.entry_count(),
            total_amount = batch.total_amount(),
            "Settling batch"
        );

        // 1. Collect recipient accounts (scoped lock)
        let recipient_accounts: Vec<(AccountId, u64, Vec<Hash>)> = {
            let mapper = self
                .account_mapper
                .read()
                .map_err(|_| SettleError::internal("account mapper lock poisoned"))?;
            batch
                .entries
                .iter()
                .map(|e| {
                    let account = mapper.require_account(&e.recipient)?;
                    Ok((account, e.amount, e.provenance_hashes.clone()))
                })
                .collect::<SettleResult<Vec<_>>>()?
        };

        // 2. Resolve EVM addresses for each recipient (async, outside lock)
        let mut resolved: Vec<(String, u64, Vec<Hash>)> = Vec::new();
        for (account, amount, prov_hashes) in &recipient_accounts {
            let evm_address = self.resolve_evm_address(account).await?;
            resolved.push((evm_address, *amount, prov_hashes.clone()));
        }

        // 3. Encode entries using resolved EVM addresses
        let encoded_entries: Vec<Vec<u8>> = resolved
            .iter()
            .map(|(evm_address, amount, prov_hashes)| {
                self.encode_settlement_entry_evm(evm_address, *amount, prov_hashes)
            })
            .collect();

        // Convert to slice of slices for the Hedera API
        let entries_refs: Vec<&[u8]> = encoded_entries.iter().map(|e| e.as_slice()).collect();

        // Clone values needed for the async closure
        let batch_id = batch.batch_id.0;
        let merkle_root = batch.merkle_root.0;

        let tx = self
            .retry_policy
            .execute(|| async {
                ContractExecuteTransaction::new()
                    .contract_id(self.contract_id)
                    .gas(self.config.gas.max_gas_settle)
                    .function_with_parameters(
                        "settleBatch",
                        ContractFunctionParameters::new()
                            .add_bytes32(&batch_id)
                            .add_bytes32(&merkle_root)
                            .add_bytes_array(&entries_refs),
                    )
                    .execute(&self.client)
                    .await
                    .map_err(crate::error::classify_sdk_error)
            })
            .await?;

        let receipt = self.wait_for_receipt(&tx.transaction_id).await?;

        if receipt.status != hiero_sdk::Status::Success {
            return Err(SettleError::transaction_failed(format!(
                "settle batch failed: {:?}",
                receipt.status
            )));
        }

        info!(
            batch_id = %batch.batch_id,
            tx_id = %tx.transaction_id,
            "Batch settled successfully"
        );
        Ok(Self::from_hedera_tx_id(&tx.transaction_id))
    }

    async fn verify_settlement(&self, tx_id: &TransactionId) -> SettleResult<SettlementStatus> {
        // Parse the transaction ID
        let hedera_tx_id = HederaTransactionId::from_str(tx_id.as_str())
            .map_err(|e| SettleError::InvalidTransactionId(format!("{}: {}", tx_id, e)))?;

        // Query the receipt
        match TransactionReceiptQuery::new()
            .transaction_id(hedera_tx_id)
            .execute(&self.client)
            .await
        {
            Ok(receipt) => {
                if receipt.status == hiero_sdk::Status::Success {
                    Ok(SettlementStatus::confirmed(
                        0, // Hedera doesn't have block numbers
                        self.current_timestamp(),
                    ))
                } else {
                    Ok(SettlementStatus::failed(format!("{:?}", receipt.status)))
                }
            }
            Err(e) => {
                // If we can't get the receipt, it might still be pending
                warn!(tx_id = %tx_id, error = %e, "Could not get receipt");
                Ok(SettlementStatus::Pending)
            }
        }
    }

    fn get_own_account(&self) -> AccountId {
        AccountId::new(
            self.operator_id.shard,
            self.operator_id.realm,
            self.operator_id.num,
        )
    }

    fn get_account_for_peer(&self, peer: &PeerId) -> Option<AccountId> {
        self.account_mapper
            .read()
            .map_err(|_| SettleError::internal("account mapper lock poisoned"))
            .ok()
            .and_then(|mapper| mapper.get_account(peer))
    }

    fn register_peer_account(&self, peer: &PeerId, account: AccountId) {
        let _ = self
            .account_mapper
            .write()
            .map_err(|_| SettleError::internal("account mapper lock poisoned"))
            .map(|mut mapper| mapper.register(peer, account));
    }
}

#[cfg(all(test, feature = "testnet"))]
mod tests {
    //! Integration tests for Hedera testnet.
    //!
    //! These tests require environment variables:
    //! - HEDERA_ACCOUNT_ID: Testnet account ID (e.g., 0.0.12345)
    //! - HEDERA_PRIVATE_KEY: Private key for the account
    //! - HEDERA_CONTRACT_ID: Settlement contract ID
    //!
    //! Run with: cargo test --features testnet -- --nocapture

    use super::*;
    use std::env;
    use tempfile::NamedTempFile;

    fn get_test_credentials() -> Option<(String, String, String, NamedTempFile)> {
        nodalync_test_utils::try_load_dotenv();
        let account_id = env::var("HEDERA_ACCOUNT_ID").ok()?;
        let private_key = env::var("HEDERA_PRIVATE_KEY").ok()?;
        let contract_id = env::var("HEDERA_CONTRACT_ID").ok()?;

        // Write private key to temp file (strip 0x prefix if present)
        let key_str = private_key.strip_prefix("0x").unwrap_or(&private_key);
        let mut temp_file = NamedTempFile::new().ok()?;
        std::io::Write::write_all(&mut temp_file, key_str.as_bytes()).ok()?;

        Some((account_id, contract_id, private_key, temp_file))
    }

    #[tokio::test]
    async fn test_hedera_get_balance() {
        let (account_id, contract_id, _key, temp_file) = match get_test_credentials() {
            Some(creds) => creds,
            None => {
                println!("Skipping test: Hedera testnet credentials not set");
                return;
            }
        };

        let config =
            HederaConfig::testnet(&account_id, temp_file.path().to_path_buf(), &contract_id);

        let settlement = HederaSettlement::new(config).await.unwrap();

        let balance = settlement.get_balance().await.unwrap();
        println!("Balance: {} tinybars", balance);
        assert!(balance > 0, "Account should have some balance");
    }

    #[tokio::test]
    async fn test_hedera_verify_settlement() {
        let (account_id, contract_id, _key, temp_file) = match get_test_credentials() {
            Some(creds) => creds,
            None => {
                println!("Skipping test: Hedera testnet credentials not set");
                return;
            }
        };

        let config =
            HederaConfig::testnet(&account_id, temp_file.path().to_path_buf(), &contract_id);

        let settlement = HederaSettlement::new(config).await.unwrap();

        // Test with an invalid transaction ID
        let tx_id = TransactionId::new("0.0.12345@1234567890.123456789");
        let status = settlement.verify_settlement(&tx_id).await.unwrap();

        // Should be pending or failed (not a real tx)
        assert!(!status.is_confirmed());
    }
}
