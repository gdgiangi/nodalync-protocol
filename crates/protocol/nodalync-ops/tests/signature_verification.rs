//! Security integration tests for signature verification.
//!
//! Tests that verify the signature verification security fixes from
//! Phases 2, 3, and 4 of the security hardening plan.

use nodalync_crypto::{
    content_hash, generate_identity, peer_id_from_public_key, sign, Hash, PeerId, PrivateKey,
    PublicKey, Signature,
};
use nodalync_ops::{current_timestamp, DefaultNodeOperations, OpsError};
use nodalync_store::{ChannelStore, NodeStateConfig, PeerInfo, PeerStore};
use nodalync_types::{ChannelState, Metadata, Payment, ProvenanceEntry, Visibility};
use nodalync_wire::{
    ChannelBalances, ChannelClosePayload, ChannelOpenPayload, QueryRequestPayload,
};
use tempfile::TempDir;

// =============================================================================
// Helpers
// =============================================================================

fn create_test_ops() -> (
    DefaultNodeOperations,
    PrivateKey,
    PublicKey,
    PeerId,
    TempDir,
) {
    let temp_dir = TempDir::new().unwrap();
    let config = NodeStateConfig::new(temp_dir.path());
    let state = nodalync_store::NodeState::open(config).unwrap();

    let (private_key, public_key) = generate_identity();
    let peer_id = peer_id_from_public_key(&public_key);

    let ops = DefaultNodeOperations::with_defaults(state, peer_id);
    (ops, private_key, public_key, peer_id, temp_dir)
}

fn register_peer(ops: &mut DefaultNodeOperations, peer_id: &PeerId, public_key: &PublicKey) {
    let peer_info = PeerInfo::new(
        *peer_id,
        *public_key,
        vec!["/ip4/127.0.0.1/tcp/9000".to_string()],
        current_timestamp(),
    );
    ops.state.peers.upsert(&peer_info).unwrap();
}

fn create_test_payment(
    amount: u64,
    recipient: PeerId,
    query_hash: Hash,
    channel_id: Hash,
    provenance: Vec<ProvenanceEntry>,
) -> Payment {
    Payment::new(
        content_hash(b"payment"),
        channel_id,
        amount,
        recipient,
        query_hash,
        provenance,
        current_timestamp(),
        Signature::from_bytes([0u8; 64]),
    )
}

fn create_signed_payment(
    amount: u64,
    recipient: PeerId,
    query_hash: Hash,
    channel_id: Hash,
    provenance: Vec<ProvenanceEntry>,
    private_key: &PrivateKey,
) -> Payment {
    // Build with stub sig first, compute message, then replace
    let mut payment = Payment::new(
        content_hash(b"payment"),
        channel_id,
        amount,
        recipient,
        query_hash,
        provenance,
        current_timestamp(),
        Signature::from_bytes([0u8; 64]),
    );
    let msg = nodalync_valid::construct_payment_message(&payment);
    payment.signature = sign(private_key, &msg);
    payment
}

// =============================================================================
// Phase 2: Message Signature Verification (C2)
// =============================================================================

/// Test that a forged message (signed with wrong key but claiming to be from
/// a registered peer) is rejected by handle_network_event.
///
/// Since InboundRequestId is not constructible outside libp2p, we verify
/// this indirectly: register peer A's key, then try to use peer A's PeerId
/// as a channel close requester but with a signature from key B.
/// The handler should reject the forged signature.
#[tokio::test]
async fn test_message_with_forged_sender_rejected() {
    // Create ops with known identity
    let (mut ops, node_private_key, _node_pubkey, _node_peer_id, _temp) = create_test_ops();

    // Create two identities: A (victim) and B (attacker)
    let (_private_key_a, public_key_a) = generate_identity();
    let peer_a = peer_id_from_public_key(&public_key_a);
    let (private_key_b, _public_key_b) = generate_identity();

    // Register peer A's public key
    register_peer(&mut ops, &peer_a, &public_key_a);

    // Open a channel with peer A
    let channel_id = content_hash(b"forged-test-channel");
    let open_request = ChannelOpenPayload {
        channel_id,
        initial_balance: 200_0000_0000, // 200 HBAR (above 100 HBAR minimum)
        funding_tx: None,
        hedera_account: None,
    };
    ops.handle_channel_open(&peer_a, &open_request)
        .await
        .unwrap();

    // Attacker (B) signs a channel close pretending to be peer A
    let forged_signature = nodalync_valid::sign_channel_close(
        &private_key_b, // Wrong key!
        &channel_id,
        0,
        200_0000_0000,
        200_0000_0000,
    );

    let close_request = ChannelClosePayload {
        channel_id,
        nonce: 0,
        final_balances: ChannelBalances::new(200_0000_0000, 200_0000_0000),
        initiator_signature: forged_signature,
    };

    // This should fail because peer A's key is registered and the signature
    // was made with key B
    let result = ops.handle_channel_close_request(&peer_a, &close_request, &node_private_key);
    assert!(
        result.is_err(),
        "Forged signature should be rejected when peer key is registered"
    );
}

/// Test that a valid message (signed with correct key) from a registered peer
/// is accepted.
#[tokio::test]
async fn test_message_with_valid_signature_accepted() {
    let (mut ops, node_private_key, _node_pubkey, _node_peer_id, _temp) = create_test_ops();

    // Create peer identity
    let (private_key, public_key) = generate_identity();
    let peer_id = peer_id_from_public_key(&public_key);

    // Register peer's public key
    register_peer(&mut ops, &peer_id, &public_key);

    // Open a channel
    let channel_id = content_hash(b"valid-sig-channel");
    let open_request = ChannelOpenPayload {
        channel_id,
        initial_balance: 200_0000_0000, // 200 HBAR (above 100 HBAR minimum)
        funding_tx: None,
        hedera_account: None,
    };
    ops.handle_channel_open(&peer_id, &open_request)
        .await
        .unwrap();

    // Sign close with correct key
    let valid_signature = nodalync_valid::sign_channel_close(
        &private_key,
        &channel_id,
        0,
        200_0000_0000,
        200_0000_0000,
    );

    let close_request = ChannelClosePayload {
        channel_id,
        nonce: 0,
        final_balances: ChannelBalances::new(200_0000_0000, 200_0000_0000),
        initiator_signature: valid_signature,
    };

    let result = ops.handle_channel_close_request(&peer_id, &close_request, &node_private_key);
    assert!(
        result.is_ok(),
        "Valid signature should be accepted: {:?}",
        result.err()
    );
}

/// Test that messages from unknown peers (no registered key) still proceed
/// under the soft-fail policy.
#[tokio::test]
async fn test_message_from_unknown_peer_still_processed() {
    let (mut ops, node_private_key, _node_pubkey, _node_peer_id, _temp) = create_test_ops();

    // Create peer but do NOT register their key
    let (_, public_key) = generate_identity();
    let unknown_peer = peer_id_from_public_key(&public_key);

    // Open a channel (this itself succeeds without key registration)
    let channel_id = content_hash(b"unknown-peer-channel");
    let open_request = ChannelOpenPayload {
        channel_id,
        initial_balance: 200_0000_0000, // 200 HBAR (above 100 HBAR minimum)
        funding_tx: None,
        hedera_account: None,
    };
    ops.handle_channel_open(&unknown_peer, &open_request)
        .await
        .unwrap();

    // Close with a stub signature — should succeed because peer is unknown (soft-fail)
    let close_request = ChannelClosePayload {
        channel_id,
        nonce: 0,
        final_balances: ChannelBalances::new(200_0000_0000, 200_0000_0000),
        initiator_signature: Signature::from_bytes([0u8; 64]),
    };

    let result = ops.handle_channel_close_request(&unknown_peer, &close_request, &node_private_key);
    assert!(
        result.is_ok(),
        "Unknown peer should proceed via soft-fail: {:?}",
        result.err()
    );
}

// =============================================================================
// Phase 3: Payment Signature Fixes (C1, C3)
// =============================================================================

/// Test that a payment with a forged signature is rejected when the peer's
/// public key is registered.
#[tokio::test]
async fn test_forged_payment_signature_rejected() {
    let (mut ops, _mock_net, _mock_settle, _temp) =
        nodalync_test_utils::create_test_ops_with_mocks();

    // Create peer identities
    let (_requester_pk, requester_pubkey) = generate_identity();
    let requester = peer_id_from_public_key(&requester_pubkey);
    let (forger_pk, _forger_pubkey) = generate_identity();

    // Register requester's public key
    register_peer(&mut ops, &requester, &requester_pubkey);

    // Create and publish paid content
    let content = b"Paid content for signature test";
    let meta = Metadata::new("Sig Test", content.len() as u64);
    let hash = ops.create_content(content, meta).unwrap();
    ops.publish_content(&hash, Visibility::Shared, 100)
        .await
        .unwrap();

    // Open channel
    let channel_id = content_hash(b"forged-payment-channel");
    ops.accept_payment_channel(&channel_id, &requester, 500, 1000)
        .unwrap();

    let manifest = ops.get_content_manifest(&hash).unwrap().unwrap();

    // Sign payment with WRONG key (forger, not requester)
    let payment = create_signed_payment(
        100,
        manifest.owner,
        hash,
        channel_id,
        manifest.provenance.root_l0l1.clone(),
        &forger_pk, // Wrong key!
    );

    let request = QueryRequestPayload {
        hash,
        query: None,
        payment,
        version_spec: None,
        payment_nonce: 1,
    };

    let result = ops.handle_query_request(&requester, &request).await;
    assert!(
        matches!(result, Err(OpsError::PaymentValidationFailed(_))),
        "Payment with forged signature should be rejected: {:?}",
        result
    );
}

/// Test that a payment with a valid signature from the registered peer is accepted.
#[tokio::test]
async fn test_valid_signed_payment_accepted() {
    let (mut ops, _mock_net, _mock_settle, _temp) =
        nodalync_test_utils::create_test_ops_with_mocks();

    // Create peer identity
    let (requester_pk, requester_pubkey) = generate_identity();
    let requester = peer_id_from_public_key(&requester_pubkey);

    // Register requester's public key
    register_peer(&mut ops, &requester, &requester_pubkey);

    // Create and publish paid content
    let content = b"Paid content for valid sig";
    let meta = Metadata::new("Valid Sig", content.len() as u64);
    let hash = ops.create_content(content, meta).unwrap();
    ops.publish_content(&hash, Visibility::Shared, 100)
        .await
        .unwrap();

    // Open channel
    let channel_id = content_hash(b"valid-payment-channel");
    ops.accept_payment_channel(&channel_id, &requester, 500, 1000)
        .unwrap();

    let manifest = ops.get_content_manifest(&hash).unwrap().unwrap();

    // Sign payment with CORRECT key
    let payment = create_signed_payment(
        100,
        manifest.owner,
        hash,
        channel_id,
        manifest.provenance.root_l0l1.clone(),
        &requester_pk,
    );

    let request = QueryRequestPayload {
        hash,
        query: None,
        payment,
        version_spec: None,
        payment_nonce: 1,
    };

    let result = ops.handle_query_request(&requester, &request).await;
    assert!(
        result.is_ok(),
        "Payment with valid signature should be accepted: {:?}",
        result.err()
    );
}

// =============================================================================
// Phase 3: Channel Close Signature Fixes (C5)
// =============================================================================

/// Test that a channel close request with a forged signature is rejected
/// when the peer's public key is registered.
#[tokio::test]
async fn test_channel_close_forged_signature_rejected() {
    let (mut ops, node_private_key, _node_pubkey, _node_peer_id, _temp) = create_test_ops();

    // Create two identities
    let (_real_pk, real_pubkey) = generate_identity();
    let real_peer = peer_id_from_public_key(&real_pubkey);
    let (forger_pk, _forger_pubkey) = generate_identity();

    // Register real peer's key
    register_peer(&mut ops, &real_peer, &real_pubkey);

    // Open channel
    let channel_id = content_hash(b"close-forge-channel");
    let open_request = ChannelOpenPayload {
        channel_id,
        initial_balance: 200_0000_0000, // 200 HBAR (above 100 HBAR minimum)
        funding_tx: None,
        hedera_account: None,
    };
    ops.handle_channel_open(&real_peer, &open_request)
        .await
        .unwrap();

    // Sign close with forger's key
    let forged_sig = nodalync_valid::sign_channel_close(
        &forger_pk,
        &channel_id,
        0,
        200_0000_0000,
        200_0000_0000,
    );

    let close_request = ChannelClosePayload {
        channel_id,
        nonce: 0,
        final_balances: ChannelBalances::new(200_0000_0000, 200_0000_0000),
        initiator_signature: forged_sig,
    };

    let result = ops.handle_channel_close_request(&real_peer, &close_request, &node_private_key);
    assert!(
        result.is_err(),
        "Channel close with forged signature should be rejected"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("invalid initiator signature"),
        "Error should mention invalid signature, got: {}",
        err_msg
    );
}

/// Test that channel close from an unknown peer (no registered key) succeeds
/// under the soft-fail policy.
#[tokio::test]
async fn test_channel_close_unknown_peer_soft_fails() {
    let (mut ops, node_private_key, _node_pubkey, _node_peer_id, _temp) = create_test_ops();

    // Create peer but do NOT register their key
    let (_, pubkey) = generate_identity();
    let unknown_peer = peer_id_from_public_key(&pubkey);

    // Open channel
    let channel_id = content_hash(b"close-softfail-channel");
    let open_request = ChannelOpenPayload {
        channel_id,
        initial_balance: 200_0000_0000, // 200 HBAR (above 100 HBAR minimum)
        funding_tx: None,
        hedera_account: None,
    };
    ops.handle_channel_open(&unknown_peer, &open_request)
        .await
        .unwrap();

    // Close with a stub signature — should succeed because peer is unknown
    let close_request = ChannelClosePayload {
        channel_id,
        nonce: 0,
        final_balances: ChannelBalances::new(200_0000_0000, 200_0000_0000),
        initiator_signature: Signature::from_bytes([0u8; 64]),
    };

    let result = ops.handle_channel_close_request(&unknown_peer, &close_request, &node_private_key);
    assert!(
        result.is_ok(),
        "Channel close from unknown peer should soft-fail and proceed: {:?}",
        result.err()
    );

    // Verify channel is closing
    let channel = ops.get_payment_channel(&unknown_peer).unwrap().unwrap();
    assert!(
        channel.state == ChannelState::Closing || channel.state == ChannelState::Closed,
        "Channel should be in Closing or Closed state, got: {:?}",
        channel.state
    );
}

// =============================================================================
// Phase 3: Real Signatures (C3)
// =============================================================================

/// Test that payment records created by the handler have real signatures
/// (not all-zero stubs) when a private key is available.
#[tokio::test]
async fn test_payment_record_has_real_signature() {
    let (mut ops, _mock_net, _mock_settle, _temp) =
        nodalync_test_utils::create_test_ops_with_mocks();

    // Set a private key so real signatures are generated
    let (node_private_key, _) = generate_identity();
    ops.set_private_key(node_private_key);

    // Create peer (unknown — soft-fail allows query processing)
    let (_, pubkey) = generate_identity();
    let requester = peer_id_from_public_key(&pubkey);

    // Create and publish paid content
    let content = b"Content for real signature test";
    let meta = Metadata::new("Real Sig", content.len() as u64);
    let hash = ops.create_content(content, meta).unwrap();
    ops.publish_content(&hash, Visibility::Shared, 100)
        .await
        .unwrap();

    // Open channel
    let channel_id = content_hash(b"real-sig-channel");
    ops.accept_payment_channel(&channel_id, &requester, 500, 1000)
        .unwrap();

    let manifest = ops.get_content_manifest(&hash).unwrap().unwrap();

    let payment = create_test_payment(
        100,
        manifest.owner,
        hash,
        channel_id,
        manifest.provenance.root_l0l1.clone(),
    );

    let request = QueryRequestPayload {
        hash,
        query: None,
        payment,
        version_spec: None,
        payment_nonce: 1,
    };

    let result = ops.handle_query_request(&requester, &request).await;
    assert!(result.is_ok(), "Query should succeed: {:?}", result.err());

    // Check that payment records in the channel store have real signatures
    let payments = ops.state.channels.get_pending_payments(&requester).unwrap();

    // There should be at least one payment recorded
    assert!(
        !payments.is_empty(),
        "Should have recorded at least one payment"
    );

    for payment in &payments {
        assert_ne!(
            payment.signature,
            Signature::from_bytes([0u8; 64]),
            "Payment signature should be real, not a stub"
        );
    }
}

/// Test that query responses (receipts) have real distributor signatures
/// when a private key is available.
#[tokio::test]
async fn test_receipt_has_real_signature() {
    let (mut ops, _mock_net, _mock_settle, _temp) =
        nodalync_test_utils::create_test_ops_with_mocks();

    // Set a private key so real signatures are generated
    let (node_private_key, _) = generate_identity();
    ops.set_private_key(node_private_key);

    // Create peer
    let (_, pubkey) = generate_identity();
    let requester = peer_id_from_public_key(&pubkey);

    // Create and publish paid content
    let content = b"Content for receipt signature test";
    let meta = Metadata::new("Receipt Sig", content.len() as u64);
    let hash = ops.create_content(content, meta).unwrap();
    ops.publish_content(&hash, Visibility::Shared, 100)
        .await
        .unwrap();

    // Open channel
    let channel_id = content_hash(b"receipt-sig-channel");
    ops.accept_payment_channel(&channel_id, &requester, 500, 1000)
        .unwrap();

    let manifest = ops.get_content_manifest(&hash).unwrap().unwrap();

    let payment = create_test_payment(
        100,
        manifest.owner,
        hash,
        channel_id,
        manifest.provenance.root_l0l1.clone(),
    );

    let request = QueryRequestPayload {
        hash,
        query: None,
        payment,
        version_spec: None,
        payment_nonce: 1,
    };

    let result = ops.handle_query_request(&requester, &request).await;
    assert!(result.is_ok(), "Query should succeed: {:?}", result.err());

    let response = result.unwrap();

    // The receipt's distributor_signature should not be all zeros
    assert_ne!(
        response.payment_receipt.distributor_signature,
        Signature::from_bytes([0u8; 64]),
        "Receipt distributor signature should be real, not a stub"
    );
}

// =============================================================================
// Phase 4: Lock Poisoning (H3)
// =============================================================================

/// Test that lock poisoning returns an error instead of panicking.
///
/// This test intentionally poisons a Mutex by panicking inside a thread
/// that holds the lock, then verifies that subsequent operations return
/// Err rather than panicking.
#[test]
fn test_lock_poisoning_returns_error_not_panic() {
    // Create an in-memory database via NodeState, then extract the connection
    let temp_dir = TempDir::new().unwrap();
    let config = NodeStateConfig::new(temp_dir.path());
    let state = nodalync_store::NodeState::open(config).unwrap();
    let conn = state.connection();

    // Poison the mutex by panicking while holding the lock
    let conn_clone = conn.clone();
    let handle = std::thread::spawn(move || {
        let _guard = conn_clone.lock().unwrap();
        panic!("intentional panic to poison the lock");
    });
    // Wait for the thread to finish (it will panic)
    let _ = handle.join();

    // Now the mutex is poisoned. Create a store and try operations.
    let mut peer_store = nodalync_store::SqlitePeerStore::new(conn.clone());

    let (_, public_key) = generate_identity();
    let peer_id = peer_id_from_public_key(&public_key);
    let peer_info = PeerInfo::new(
        peer_id,
        public_key,
        vec!["/ip4/127.0.0.1/tcp/9000".to_string()],
        current_timestamp(),
    );

    // These should return Err, not panic
    let result = peer_store.upsert(&peer_info);
    assert!(result.is_err(), "upsert on poisoned lock should return Err");

    let result = peer_store.get(&peer_id);
    assert!(result.is_err(), "get on poisoned lock should return Err");
}
