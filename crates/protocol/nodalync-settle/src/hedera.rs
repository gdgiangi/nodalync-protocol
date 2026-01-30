//! Hedera settlement implementation.
//!
//! This module is only available when the `hedera-sdk` feature is enabled.
//! It requires `protoc` to be installed for compilation.

use std::str::FromStr;
use std::sync::RwLock;

use async_trait::async_trait;
use hedera::{
    AccountBalanceQuery, AccountId as HederaAccountId, Client, ContractExecuteTransaction,
    ContractFunctionParameters, ContractId, Hbar, PrivateKey, TransactionId as HederaTransactionId,
    TransactionReceiptQuery, TransferTransaction,
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
            contract = %config.contract_id,
            "Hedera settlement initialized"
        );

        Ok(Self {
            client,
            operator_id,
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
            .unwrap()
            .as_millis() as Timestamp
    }

    /// Encode a settlement entry for the contract call.
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

    /// Wait for a transaction receipt.
    async fn wait_for_receipt(
        &self,
        tx_id: &HederaTransactionId,
    ) -> SettleResult<hedera::TransactionReceipt> {
        self.retry_policy
            .execute(|| async {
                TransactionReceiptQuery::new()
                    .transaction_id(*tx_id)
                    .execute(&self.client)
                    .await
                    .map_err(|e| SettleError::hedera_sdk(e.to_string()))
            })
            .await
    }
}

#[async_trait]
impl Settlement for HederaSettlement {
    async fn deposit(&self, amount: u64) -> SettleResult<TransactionId> {
        debug!(amount, "Depositing to settlement contract");

        let tx = self
            .retry_policy
            .execute(|| async {
                TransferTransaction::new()
                    .hbar_transfer(self.operator_id, Hbar::from_tinybars(-(amount as i64)))
                    .hbar_transfer(
                        HederaAccountId::new(
                            self.contract_id.shard,
                            self.contract_id.realm,
                            self.contract_id.num,
                        ),
                        Hbar::from_tinybars(amount as i64),
                    )
                    .execute(&self.client)
                    .await
                    .map_err(|e| SettleError::hedera_sdk(e.to_string()))
            })
            .await?;

        let receipt = self.wait_for_receipt(&tx.transaction_id).await?;

        if receipt.status != hedera::Status::Success {
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
                    .map_err(|e| SettleError::hedera_sdk(e.to_string()))
            })
            .await?;

        let receipt = self.wait_for_receipt(&tx.transaction_id).await?;

        if receipt.status != hedera::Status::Success {
            return Err(SettleError::transaction_failed(format!(
                "withdraw failed: {:?}",
                receipt.status
            )));
        }

        info!(amount, tx_id = %tx.transaction_id, "Withdrawal successful");
        Ok(Self::from_hedera_tx_id(&tx.transaction_id))
    }

    async fn get_balance(&self) -> SettleResult<u64> {
        let balance = self
            .retry_policy
            .execute(|| async {
                AccountBalanceQuery::new()
                    .account_id(self.operator_id)
                    .execute(&self.client)
                    .await
                    .map_err(|e| SettleError::hedera_sdk(e.to_string()))
            })
            .await?;

        Ok(balance.hbars.to_tinybars() as u64)
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
                    .map_err(|e| SettleError::hedera_sdk(e.to_string()))
            })
            .await?;

        let receipt = self.wait_for_receipt(&tx.transaction_id).await?;

        if receipt.status != hedera::Status::Success {
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

    async fn get_attestation(&self, _content_hash: &Hash) -> SettleResult<Option<Attestation>> {
        // Query contract state to get attestation
        // This would require a ContractCallQuery in the real implementation
        warn!("get_attestation not yet implemented for Hedera");
        Ok(None)
    }

    async fn open_channel(&self, peer: &PeerId, deposit: u64) -> SettleResult<ChannelId> {
        let peer_account = self.account_mapper.read().unwrap().require_account(peer)?;

        let hedera_peer = self.to_hedera_account(&peer_account);

        // Generate channel ID from participants
        let channel_hash = nodalync_crypto::content_hash(
            &[
                &self.operator_id.num.to_be_bytes()[..],
                &peer_account.num.to_be_bytes()[..],
                &self.current_timestamp().to_be_bytes()[..],
            ]
            .concat(),
        );
        let channel_id = ChannelId::new(channel_hash);

        debug!(
            peer = %peer,
            deposit,
            channel_id = %channel_id,
            "Opening payment channel"
        );

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
                            .add_address(&hedera_peer.to_solidity_address().unwrap())
                            .add_uint256(deposit.into())
                            .add_uint256(0u64.into()), // peer deposit (0 initially)
                    )
                    .execute(&self.client)
                    .await
                    .map_err(|e| SettleError::hedera_sdk(e.to_string()))
            })
            .await?;

        let receipt = self.wait_for_receipt(&tx.transaction_id).await?;

        if receipt.status != hedera::Status::Success {
            return Err(SettleError::transaction_failed(format!(
                "open channel failed: {:?}",
                receipt.status
            )));
        }

        info!(channel_id = %channel_id, "Channel opened");
        Ok(channel_id)
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
                    .map_err(|e| SettleError::hedera_sdk(e.to_string()))
            })
            .await?;

        let receipt = self.wait_for_receipt(&tx.transaction_id).await?;

        if receipt.status != hedera::Status::Success {
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
                    .map_err(|e| SettleError::hedera_sdk(e.to_string()))
            })
            .await?;

        let receipt = self.wait_for_receipt(&tx.transaction_id).await?;

        if receipt.status != hedera::Status::Success {
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
                    .map_err(|e| SettleError::hedera_sdk(e.to_string()))
            })
            .await?;

        let receipt = self.wait_for_receipt(&tx.transaction_id).await?;

        if receipt.status != hedera::Status::Success {
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
                    .map_err(|e| SettleError::hedera_sdk(e.to_string()))
            })
            .await?;

        let receipt = self.wait_for_receipt(&tx.transaction_id).await?;

        if receipt.status != hedera::Status::Success {
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

        // Encode entries (scope the lock to release it before async operations)
        let encoded_entries: Vec<Vec<u8>> = {
            let mapper = self.account_mapper.read().unwrap();
            let mut entries = Vec::new();
            for entry in &batch.entries {
                let account = mapper.require_account(&entry.recipient)?;
                entries.push(self.encode_settlement_entry(
                    &account,
                    entry.amount,
                    &entry.provenance_hashes,
                ));
            }
            entries
        };

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
                    .map_err(|e| SettleError::hedera_sdk(e.to_string()))
            })
            .await?;

        let receipt = self.wait_for_receipt(&tx.transaction_id).await?;

        if receipt.status != hedera::Status::Success {
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
                if receipt.status == hedera::Status::Success {
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

    fn get_account_for_peer(&self, peer: &PeerId) -> Option<AccountId> {
        self.account_mapper.read().unwrap().get_account(peer)
    }

    fn register_peer_account(&mut self, peer: &PeerId, account: AccountId) {
        self.account_mapper.write().unwrap().register(peer, account);
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
    //! Run with: cargo test --features testnet -- --ignored

    use super::*;
    use std::env;
    use tempfile::NamedTempFile;

    fn get_test_credentials() -> Option<(String, String, String, NamedTempFile)> {
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
    #[ignore]
    async fn test_hedera_get_balance() {
        let (account_id, contract_id, _key, temp_file) =
            get_test_credentials().expect("Missing testnet config");

        let config =
            HederaConfig::testnet(&account_id, temp_file.path().to_path_buf(), &contract_id);

        let settlement = HederaSettlement::new(config).await.unwrap();

        let balance = settlement.get_balance().await.unwrap();
        println!("Balance: {} tinybars", balance);
        assert!(balance > 0, "Account should have some balance");
    }

    #[tokio::test]
    #[ignore]
    async fn test_hedera_verify_settlement() {
        let (account_id, contract_id, _key, temp_file) =
            get_test_credentials().expect("Missing testnet config");

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
