//! Integration tests for the Settlement trait using MockSettlement.
//!
//! These tests verify the Settlement trait contract via the MockSettlement
//! implementation from nodalync-test-utils. They cover balance management,
//! channel lifecycle, batch settlement, attestations, peer accounts,
//! dispute handling, and settlement verification.

use nodalync_crypto::{content_hash, PeerId, Signature};
use nodalync_settle::{AccountId, Settlement};
use nodalync_test_utils::MockSettlement;
use nodalync_types::settlement::{SettlementBatch, SettlementEntry};
use nodalync_wire::{ChannelBalances, ChannelUpdatePayload};

/// Helper to create a dummy 64-byte signature for testing.
fn dummy_signature() -> Signature {
    Signature([0xABu8; 64])
}

// =============================================================================
// Balance Management
// =============================================================================

#[tokio::test]
async fn test_deposit_withdraw_cycle() {
    let settle = MockSettlement::new();

    // Deposit 5000
    let tx = settle.deposit(5000).await.unwrap();
    assert!(!tx.as_str().is_empty());

    // Withdraw 2000
    settle.withdraw(2000).await.unwrap();

    // Verify final balance is 3000
    let balance = settle.get_balance().await.unwrap();
    assert_eq!(balance, 3000);
    assert_eq!(settle.current_balance(), 3000);

    // Verify operation records
    assert_eq!(settle.deposits(), vec![5000]);
    assert_eq!(settle.withdrawals(), vec![2000]);
}

#[tokio::test]
async fn test_deposit_increases_balance() {
    let settle = MockSettlement::new();

    settle.deposit(1000).await.unwrap();
    assert_eq!(settle.get_balance().await.unwrap(), 1000);

    settle.deposit(2500).await.unwrap();
    assert_eq!(settle.get_balance().await.unwrap(), 3500);

    settle.deposit(500).await.unwrap();
    assert_eq!(settle.get_balance().await.unwrap(), 4000);

    // All deposits recorded in order
    assert_eq!(settle.deposits(), vec![1000, 2500, 500]);
}

#[tokio::test]
async fn test_withdraw_decreases_balance() {
    let settle = MockSettlement::new().with_balance(10_000);

    settle.withdraw(3000).await.unwrap();
    assert_eq!(settle.get_balance().await.unwrap(), 7000);

    settle.withdraw(5000).await.unwrap();
    assert_eq!(settle.get_balance().await.unwrap(), 2000);

    assert_eq!(settle.withdrawals(), vec![3000, 5000]);

    // Withdrawing more than available should fail
    let result = settle.withdraw(3000).await;
    assert!(result.is_err());
}

// =============================================================================
// Channel Lifecycle
// =============================================================================

#[tokio::test]
async fn test_open_close_channel_lifecycle() {
    let settle = MockSettlement::new().with_balance(20_000);
    let peer = PeerId([5u8; 20]);

    // Open channel with deposit
    let channel_id =
        nodalync_settle::ChannelId::new(nodalync_crypto::content_hash(b"test-channel-open-close"));
    settle.open_channel(&channel_id, &peer, 8000).await.unwrap();
    assert_eq!(settle.channel_count(), 1);
    // Opening a channel deducts from balance
    assert_eq!(settle.get_balance().await.unwrap(), 12_000);

    // Close the channel cooperatively
    let final_balances = ChannelBalances::new(4000, 4000);
    let tx = settle
        .close_channel(&channel_id, &final_balances, &[])
        .await
        .unwrap();
    assert!(!tx.as_str().is_empty());
    assert_eq!(settle.channel_count(), 0);
}

// =============================================================================
// Batch Settlement
// =============================================================================

#[tokio::test]
async fn test_settle_batch_records_entries() {
    let settle = MockSettlement::new();
    let recipient = PeerId([7u8; 20]);

    let entry = SettlementEntry::new(
        recipient,
        5000,
        vec![content_hash(b"provenance-1")],
        vec![content_hash(b"payment-1")],
    );

    let batch = SettlementBatch::new(
        content_hash(b"batch-id"),
        vec![entry],
        content_hash(b"merkle-root"),
    );

    let tx = settle.settle_batch(&batch).await.unwrap();
    assert!(!tx.as_str().is_empty());

    let batches = settle.settled_batches();
    assert_eq!(batches.len(), 1);
    assert_eq!(batches[0].total_amount(), 5000);
    assert_eq!(batches[0].entry_count(), 1);
    assert!(batches[0].contains_recipient(&recipient));
}

// =============================================================================
// Attestation
// =============================================================================

#[tokio::test]
async fn test_attestation_store_and_retrieve() {
    let settle = MockSettlement::new();
    let content = content_hash(b"my-content");
    let provenance = content_hash(b"my-provenance-root");

    // Initially no attestation exists
    let none = settle.get_attestation(&content).await.unwrap();
    assert!(none.is_none());

    // Attest
    settle.attest(&content, &provenance).await.unwrap();
    assert_eq!(settle.attestation_count(), 1);

    // Retrieve
    let att = settle.get_attestation(&content).await.unwrap();
    assert!(att.is_some());
    let att = att.unwrap();
    assert_eq!(att.content_hash, content);
    assert_eq!(att.provenance_root, provenance);
    assert_eq!(att.owner, AccountId::simple(99999)); // default account
}

// =============================================================================
// Peer Account Registration
// =============================================================================

#[tokio::test]
async fn test_peer_account_registration() {
    let settle = MockSettlement::new();
    let peer = PeerId([9u8; 20]);
    let account = AccountId::simple(54321);

    // Not registered yet
    assert!(settle.get_account_for_peer(&peer).is_none());

    // Register
    settle.register_peer_account(&peer, account);
    assert_eq!(settle.get_account_for_peer(&peer), Some(account));

    // Own account is independent
    assert_eq!(settle.get_own_account(), AccountId::simple(99999));
}

// =============================================================================
// Dispute Flow
// =============================================================================

#[tokio::test]
async fn test_dispute_counter_dispute_resolve() {
    let settle = MockSettlement::new().with_balance(50_000);
    let peer = PeerId([11u8; 20]);

    // Open a channel first
    let channel_id =
        nodalync_settle::ChannelId::new(nodalync_crypto::content_hash(b"test-channel-dispute"));
    settle
        .open_channel(&channel_id, &peer, 20_000)
        .await
        .unwrap();
    assert_eq!(settle.channel_count(), 1);

    // Initiate a dispute
    let claimed_state = ChannelUpdatePayload {
        channel_id: channel_id.0,
        nonce: 5,
        balances: ChannelBalances::new(12_000, 8_000),
        payments: vec![],
        signature: dummy_signature(),
    };
    let tx = settle
        .dispute_channel(&channel_id, &claimed_state)
        .await
        .unwrap();
    assert!(!tx.as_str().is_empty());
    // Channel is still tracked (dispute does not remove it)
    assert_eq!(settle.channel_count(), 1);

    // Submit a counter-dispute with higher nonce
    let better_state = ChannelUpdatePayload {
        channel_id: channel_id.0,
        nonce: 10,
        balances: ChannelBalances::new(10_000, 10_000),
        payments: vec![],
        signature: dummy_signature(),
    };
    let tx2 = settle
        .counter_dispute(&channel_id, &better_state)
        .await
        .unwrap();
    assert!(!tx2.as_str().is_empty());
    assert_eq!(settle.channel_count(), 1);

    // Resolve the dispute (removes the channel in mock)
    let tx3 = settle.resolve_dispute(&channel_id).await.unwrap();
    assert!(!tx3.as_str().is_empty());
    assert_eq!(settle.channel_count(), 0);
}

// =============================================================================
// Settlement Verification
// =============================================================================

#[tokio::test]
async fn test_verify_settlement_status() {
    let settle = MockSettlement::new();
    let tx_id = nodalync_settle::TransactionId::new("0.0.99999@mock.test");

    let status = settle.verify_settlement(&tx_id).await.unwrap();
    assert!(status.is_confirmed());
    assert!(!status.is_pending());
    assert!(!status.is_failed());
}
