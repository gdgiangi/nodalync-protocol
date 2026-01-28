//! Spec compliance tests for nodalync-types.
//!
//! These tests verify that the type implementations match the requirements
//! defined in Protocol Specification §4.

use nodalync_crypto::{content_hash, generate_identity, peer_id_from_public_key};
use nodalync_types::*;

// =============================================================================
// §4.1 ContentType Tests
// =============================================================================

#[test]
fn spec_4_1_content_type_values() {
    // ContentType wire format values must match spec
    assert_eq!(ContentType::L0 as u8, 0x00);
    assert_eq!(ContentType::L1 as u8, 0x01);
    assert_eq!(ContentType::L3 as u8, 0x03);
    // Note: L2 is intentionally skipped (internal only)
}

#[test]
fn spec_4_1_content_type_serialization_roundtrip() {
    for ct in [ContentType::L0, ContentType::L1, ContentType::L3] {
        let json = serde_json::to_string(&ct).unwrap();
        let deserialized: ContentType = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, ct);
    }
}

// =============================================================================
// §4.2 Visibility Tests
// =============================================================================

#[test]
fn spec_4_2_visibility_values() {
    assert_eq!(Visibility::Private as u8, 0x00);
    assert_eq!(Visibility::Unlisted as u8, 0x01);
    assert_eq!(Visibility::Shared as u8, 0x02);
}

#[test]
fn spec_4_2_visibility_serialization_roundtrip() {
    for v in [
        Visibility::Private,
        Visibility::Unlisted,
        Visibility::Shared,
    ] {
        let json = serde_json::to_string(&v).unwrap();
        let deserialized: Visibility = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, v);
    }
}

// =============================================================================
// §4.3 Version Tests
// =============================================================================

#[test]
fn spec_4_3_version_v1_constraints() {
    // Constraint: If number == 1, previous MUST be None, root MUST equal content hash
    let content = b"test content";
    let hash = content_hash(content);
    let timestamp = 1234567890u64;

    let version = Version::new_v1(hash, timestamp);

    assert_eq!(version.number, 1);
    assert!(version.previous.is_none(), "v1 previous must be None");
    assert_eq!(version.root, hash, "v1 root must equal content hash");
    assert!(version.is_valid(&hash));
}

#[test]
fn spec_4_3_version_v2_constraints() {
    // Constraint: If number > 1, previous MUST be Some, root MUST equal previous.root
    let hash1 = content_hash(b"content v1");
    let hash2 = content_hash(b"content v2");

    let v1 = Version::new_v1(hash1, 1000);
    let v2 = Version::new_from_previous(&v1, hash1, 2000);

    assert_eq!(v2.number, 2);
    assert_eq!(v2.previous, Some(hash1), "v2 previous must be Some");
    assert_eq!(v2.root, v1.root, "v2 root must equal v1.root");
    assert!(v2.is_valid(&hash2));
}

#[test]
fn spec_4_3_version_chain() {
    // Test a chain of versions maintains root consistency
    let hash1 = content_hash(b"v1");
    let hash2 = content_hash(b"v2");
    let hash3 = content_hash(b"v3");

    let v1 = Version::new_v1(hash1, 1000);
    let v2 = Version::new_from_previous(&v1, hash1, 2000);
    let v3 = Version::new_from_previous(&v2, hash2, 3000);

    // All versions share the same root
    assert_eq!(v1.root, hash1);
    assert_eq!(v2.root, hash1);
    assert_eq!(v3.root, hash1);

    // Version numbers increment
    assert_eq!(v1.number, 1);
    assert_eq!(v2.number, 2);
    assert_eq!(v3.number, 3);

    // All are valid
    assert!(v1.is_valid(&hash1));
    assert!(v2.is_valid(&hash2));
    assert!(v3.is_valid(&hash3));
}

// =============================================================================
// §4.4 Mention Tests
// =============================================================================

#[test]
fn spec_4_4_location_type_values() {
    assert_eq!(LocationType::Paragraph as u8, 0x00);
    assert_eq!(LocationType::Page as u8, 0x01);
    assert_eq!(LocationType::Timestamp as u8, 0x02);
    assert_eq!(LocationType::Line as u8, 0x03);
    assert_eq!(LocationType::Section as u8, 0x04);
}

#[test]
fn spec_4_4_classification_values() {
    assert_eq!(Classification::Claim as u8, 0x00);
    assert_eq!(Classification::Statistic as u8, 0x01);
    assert_eq!(Classification::Definition as u8, 0x02);
    assert_eq!(Classification::Observation as u8, 0x03);
    assert_eq!(Classification::Method as u8, 0x04);
    assert_eq!(Classification::Result as u8, 0x05);
}

#[test]
fn spec_4_4_confidence_values() {
    assert_eq!(Confidence::Explicit as u8, 0x00);
    assert_eq!(Confidence::Inferred as u8, 0x01);
}

#[test]
fn spec_4_4_mention_serialization_roundtrip() {
    let id = content_hash(b"mention content");
    let loc = SourceLocation::with_quote(LocationType::Paragraph, "5", "exact quote");

    let mention = Mention::new(
        id,
        "This is an atomic fact about something important",
        loc,
        Classification::Claim,
        Confidence::Explicit,
    )
    .with_entities(vec!["Entity1".to_string(), "Entity2".to_string()]);

    let json = serde_json::to_string(&mention).unwrap();
    let deserialized: Mention = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.id, mention.id);
    assert_eq!(deserialized.content, mention.content);
    assert_eq!(deserialized.classification, mention.classification);
    assert_eq!(deserialized.confidence, mention.confidence);
    assert_eq!(deserialized.entities, mention.entities);
}

// =============================================================================
// §4.5 Provenance Tests
// =============================================================================

#[test]
fn spec_4_5_provenance_l0_constraints() {
    // Constraint: For L0, root_L0L1 = [self], derived_from = [], depth = 0
    let hash = content_hash(b"L0 content");
    let (_, public_key) = generate_identity();
    let owner = peer_id_from_public_key(&public_key);

    let provenance = Provenance::new_l0(hash, owner);

    assert_eq!(provenance.root_l0l1.len(), 1);
    assert_eq!(provenance.root_l0l1[0].hash, hash);
    assert!(provenance.derived_from.is_empty());
    assert_eq!(provenance.depth, 0);
    assert!(provenance.is_l0());
    assert!(provenance.is_valid(&hash));
}

#[test]
fn spec_4_5_provenance_l3_constraints() {
    // Constraint: For L3, root_L0L1.len() >= 1, derived_from.len() >= 1
    let source1_hash = content_hash(b"source1");
    let source2_hash = content_hash(b"source2");
    let derived_hash = content_hash(b"derived");
    let (_, pk1) = generate_identity();
    let (_, pk2) = generate_identity();
    let owner1 = peer_id_from_public_key(&pk1);
    let owner2 = peer_id_from_public_key(&pk2);

    let entry1 = ProvenanceEntry::new(source1_hash, owner1, Visibility::Shared);
    let entry2 = ProvenanceEntry::new(source2_hash, owner2, Visibility::Shared);

    let provenance =
        Provenance::new_derived(vec![entry1, entry2], vec![source1_hash, source2_hash], 1);

    assert!(!provenance.root_l0l1.is_empty());
    assert!(!provenance.derived_from.is_empty());
    assert!(provenance.depth > 0);
    assert!(provenance.is_derived());
    assert!(provenance.is_valid(&derived_hash));
}

#[test]
fn spec_4_5_provenance_no_self_reference() {
    // Constraint: No self-reference allowed in derived_from
    let hash = content_hash(b"content");
    let (_, pk) = generate_identity();
    let owner = peer_id_from_public_key(&pk);

    let entry = ProvenanceEntry::new(hash, owner, Visibility::Shared);

    // Create provenance with self-reference in derived_from
    let provenance = Provenance::new_derived(vec![entry], vec![hash], 1);

    // Self-reference should make it invalid
    assert!(!provenance.is_valid(&hash));
}

#[test]
fn spec_4_5_provenance_weight_handling() {
    // Constraint: Same source appearing multiple times gets higher weight
    let hash = content_hash(b"source");
    let (_, pk) = generate_identity();
    let owner = peer_id_from_public_key(&pk);

    let entry1 = ProvenanceEntry::with_weight(hash, owner, Visibility::Shared, 1);
    let entry2 = ProvenanceEntry::with_weight(hash, owner, Visibility::Shared, 2);

    let merged = Provenance::merge_entries(vec![entry1, entry2]);

    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].weight, 3);
}

// =============================================================================
// §4.6 AccessControl Tests
// =============================================================================

#[test]
fn spec_4_6_access_control_logic() {
    // Access logic from spec:
    // Access granted if:
    // (allowlist is None OR peer in allowlist) AND
    // (denylist is None OR peer NOT in denylist)

    let (_, pk1) = generate_identity();
    let (_, pk2) = generate_identity();
    let (_, pk3) = generate_identity();
    let peer1 = peer_id_from_public_key(&pk1);
    let peer2 = peer_id_from_public_key(&pk2);
    let peer3 = peer_id_from_public_key(&pk3);

    // No restrictions
    let open = AccessControl::open();
    assert!(open.is_peer_allowed(&peer1));
    assert!(open.is_peer_allowed(&peer2));

    // Allowlist only
    let allowlist = AccessControl::with_allowlist(vec![peer1]);
    assert!(allowlist.is_peer_allowed(&peer1));
    assert!(!allowlist.is_peer_allowed(&peer2));

    // Denylist only
    let denylist = AccessControl::with_denylist(vec![peer2]);
    assert!(denylist.is_peer_allowed(&peer1));
    assert!(!denylist.is_peer_allowed(&peer2));

    // Both allowlist and denylist
    let both = AccessControl {
        allowlist: Some(vec![peer1, peer2]),
        denylist: Some(vec![peer2]),
        ..Default::default()
    };
    assert!(both.is_peer_allowed(&peer1)); // In allowlist, not in denylist
    assert!(!both.is_peer_allowed(&peer2)); // In allowlist but also in denylist
    assert!(!both.is_peer_allowed(&peer3)); // Not in allowlist
}

// =============================================================================
// §4.7 Economics Tests
// =============================================================================

#[test]
fn spec_4_7_currency_values() {
    assert_eq!(Currency::HBAR as u8, 0x00);
}

#[test]
fn spec_4_7_economics_tracking() {
    let mut economics = Economics::with_price(1000);

    assert_eq!(economics.price, 1000);
    assert_eq!(economics.total_queries, 0);
    assert_eq!(economics.total_revenue, 0);

    economics.record_query(1000);
    economics.record_query(1000);
    economics.record_query(1000);

    assert_eq!(economics.total_queries, 3);
    assert_eq!(economics.total_revenue, 3000);
}

// =============================================================================
// §4.8 Manifest Tests
// =============================================================================

#[test]
fn spec_4_8_manifest_l0_creation() {
    let content = b"Test document content";
    let hash = content_hash(content);
    let (_, public_key) = generate_identity();
    let owner = peer_id_from_public_key(&public_key);
    let timestamp = 1234567890u64;

    let metadata = Metadata::new("Test Document", content.len() as u64)
        .with_description("A test document")
        .with_tags(vec!["test".to_string()])
        .with_mime_type("text/plain");

    let manifest = Manifest::new_l0(hash, owner, metadata, timestamp);

    // Verify all required fields
    assert_eq!(manifest.hash, hash);
    assert_eq!(manifest.content_type, ContentType::L0);
    assert_eq!(manifest.owner, owner);
    assert!(manifest.version.is_first_version());
    assert_eq!(manifest.visibility, Visibility::Private);
    assert!(manifest.provenance.is_l0());
    assert_eq!(manifest.created_at, timestamp);
    assert_eq!(manifest.updated_at, timestamp);
}

#[test]
fn spec_4_8_manifest_serialization_roundtrip() {
    let content = b"Content";
    let hash = content_hash(content);
    let (_, pk) = generate_identity();
    let owner = peer_id_from_public_key(&pk);
    let metadata = Metadata::new("Title", content.len() as u64);

    let manifest = Manifest::new_l0(hash, owner, metadata, 1000);

    let json = serde_json::to_string(&manifest).unwrap();
    let deserialized: Manifest = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.hash, manifest.hash);
    assert_eq!(deserialized.owner, manifest.owner);
    assert_eq!(deserialized.content_type, manifest.content_type);
    assert_eq!(deserialized.visibility, manifest.visibility);
}

// =============================================================================
// §4.9 L1Summary Tests
// =============================================================================

#[test]
fn spec_4_9_l1_summary() {
    let l0_hash = content_hash(b"source document");
    let mention_id = content_hash(b"mention");

    let mention = Mention::new(
        mention_id,
        "Key finding from the document",
        SourceLocation::paragraph(1),
        Classification::Result,
        Confidence::Explicit,
    );

    let summary = L1Summary::new(
        l0_hash,
        10,
        vec![mention],
        vec!["Science".to_string(), "Research".to_string()],
        "This document discusses important scientific findings.",
    );

    assert_eq!(summary.l0_hash, l0_hash);
    assert_eq!(summary.mention_count, 10);
    assert_eq!(summary.preview_mentions.len(), 1);
    assert_eq!(summary.primary_topics.len(), 2);
    assert_eq!(summary.hidden_mention_count(), 9);
}

// =============================================================================
// Constants Tests (Appendix B)
// =============================================================================

#[test]
fn appendix_b_protocol_constants() {
    assert_eq!(PROTOCOL_MAGIC, 0x00);
    assert_eq!(PROTOCOL_VERSION, 0x01);
}

#[test]
fn appendix_b_content_limits() {
    assert_eq!(MAX_CONTENT_SIZE, 104_857_600); // 100 MB
    assert_eq!(MAX_MESSAGE_SIZE, 10_485_760); // 10 MB
    assert_eq!(MAX_MENTIONS_PER_L0, 1000);
    assert_eq!(MAX_SOURCES_PER_L3, 100);
    assert_eq!(MAX_PROVENANCE_DEPTH, 100);
}

#[test]
fn appendix_b_metadata_limits() {
    assert_eq!(MAX_TAGS, 20);
    assert_eq!(MAX_TAG_LENGTH, 50);
    assert_eq!(MAX_TITLE_LENGTH, 200);
    assert_eq!(MAX_DESCRIPTION_LENGTH, 2000);
    assert_eq!(MAX_SUMMARY_LENGTH, 500);
    assert_eq!(MAX_MENTION_CONTENT_LENGTH, 1000);
    assert_eq!(MAX_QUOTE_LENGTH, 500);
}

#[test]
fn appendix_b_economics_constants() {
    assert_eq!(MIN_PRICE, 1);
    assert_eq!(MAX_PRICE, 10_000_000_000_000_000);
    assert_eq!(SYNTHESIS_FEE_NUMERATOR, 5);
    assert_eq!(SYNTHESIS_FEE_DENOMINATOR, 100);
    assert_eq!(SETTLEMENT_BATCH_THRESHOLD, 10_000_000_000); // 100 HBAR
    assert_eq!(SETTLEMENT_BATCH_INTERVAL_MS, 3_600_000); // 1 hour
}

#[test]
fn appendix_b_timing_constants() {
    assert_eq!(MESSAGE_TIMEOUT_MS, 30_000);
    assert_eq!(CHANNEL_DISPUTE_PERIOD_MS, 86_400_000); // 24 hours
    assert_eq!(MAX_CLOCK_SKEW_MS, 300_000); // 5 minutes
}

#[test]
fn appendix_b_dht_constants() {
    assert_eq!(DHT_BUCKET_SIZE, 20);
    assert_eq!(DHT_ALPHA, 3);
    assert_eq!(DHT_REPLICATION, 20);
}

// =============================================================================
// Error Code Tests (Appendix C)
// =============================================================================

#[test]
fn appendix_c_query_errors() {
    assert_eq!(ErrorCode::NotFound as u16, 0x0001);
    assert_eq!(ErrorCode::AccessDenied as u16, 0x0002);
    assert_eq!(ErrorCode::PaymentRequired as u16, 0x0003);
    assert_eq!(ErrorCode::PaymentInvalid as u16, 0x0004);
    assert_eq!(ErrorCode::RateLimited as u16, 0x0005);
    assert_eq!(ErrorCode::VersionNotFound as u16, 0x0006);
}

#[test]
fn appendix_c_channel_errors() {
    assert_eq!(ErrorCode::ChannelNotFound as u16, 0x0100);
    assert_eq!(ErrorCode::ChannelClosed as u16, 0x0101);
    assert_eq!(ErrorCode::InsufficientBalance as u16, 0x0102);
    assert_eq!(ErrorCode::InvalidNonce as u16, 0x0103);
    assert_eq!(ErrorCode::InvalidSignature as u16, 0x0104);
}

#[test]
fn appendix_c_validation_errors() {
    assert_eq!(ErrorCode::InvalidHash as u16, 0x0200);
    assert_eq!(ErrorCode::InvalidProvenance as u16, 0x0201);
    assert_eq!(ErrorCode::InvalidVersion as u16, 0x0202);
    assert_eq!(ErrorCode::InvalidManifest as u16, 0x0203);
    assert_eq!(ErrorCode::ContentTooLarge as u16, 0x0204);
}

#[test]
fn appendix_c_network_errors() {
    assert_eq!(ErrorCode::PeerNotFound as u16, 0x0300);
    assert_eq!(ErrorCode::ConnectionFailed as u16, 0x0301);
    assert_eq!(ErrorCode::Timeout as u16, 0x0302);
}

#[test]
fn appendix_c_internal_error() {
    assert_eq!(ErrorCode::InternalError as u16, 0xFFFF);
}

// =============================================================================
// Channel and Settlement Tests
// =============================================================================

#[test]
fn channel_state_values() {
    assert_eq!(ChannelState::Opening as u8, 0x00);
    assert_eq!(ChannelState::Open as u8, 0x01);
    assert_eq!(ChannelState::Closing as u8, 0x02);
    assert_eq!(ChannelState::Closed as u8, 0x03);
    assert_eq!(ChannelState::Disputed as u8, 0x04);
}

#[test]
fn channel_lifecycle() {
    let channel_id = content_hash(b"channel");
    let (_, pk) = generate_identity();
    let peer_id = peer_id_from_public_key(&pk);

    let mut channel = Channel::new(channel_id, peer_id, 1000, 1000);
    assert_eq!(channel.state, ChannelState::Opening);

    channel.mark_open(500, 2000);
    assert_eq!(channel.state, ChannelState::Open);
    assert!(channel.is_open());

    channel.mark_closing(3000);
    assert_eq!(channel.state, ChannelState::Closing);

    channel.mark_closed(4000);
    assert_eq!(channel.state, ChannelState::Closed);
    assert!(channel.is_closed());
}

#[test]
fn settlement_batch_aggregation() {
    let (_, pk1) = generate_identity();
    let (_, pk2) = generate_identity();
    let recipient1 = peer_id_from_public_key(&pk1);
    let recipient2 = peer_id_from_public_key(&pk2);

    let entry1 = SettlementEntry::new(recipient1, 1000, vec![], vec![]);
    let entry2 = SettlementEntry::new(recipient1, 500, vec![], vec![]); // Same recipient
    let entry3 = SettlementEntry::new(recipient2, 200, vec![], vec![]);

    let batch = SettlementBatch::new(
        content_hash(b"batch"),
        vec![entry1, entry2, entry3],
        content_hash(b"merkle"),
    );

    assert_eq!(batch.total_amount(), 1700);
    assert_eq!(batch.entry_count(), 3);
    assert_eq!(batch.unique_recipients().len(), 2);
    assert_eq!(batch.amount_for_recipient(&recipient1), 1500);
    assert_eq!(batch.amount_for_recipient(&recipient2), 200);
}
