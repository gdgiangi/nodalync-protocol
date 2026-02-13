//! Integration tests for nodalync-graph L2 Graph DB
//!
//! Tests DB operations, subgraph queries, and context retrieval end-to-end
//! using an in-memory SQLite database.

use chrono::Utc;
use nodalync_graph::{Entity, L2GraphDB, Relationship};

/// Helper: create a fresh in-memory graph DB
fn test_db() -> L2GraphDB {
    L2GraphDB::new(":memory:").expect("in-memory DB should open")
}

/// Helper: insert a test entity
fn make_entity(db: &L2GraphDB, label: &str, etype: &str, desc: &str) -> String {
    let id = db.next_entity_id().unwrap();
    let now = Utc::now();
    let entity = Entity {
        id: id.clone(),
        canonical_label: label.to_string(),
        entity_type: etype.to_string(),
        description: Some(desc.to_string()),
        confidence: 1.0,
        first_seen: now,
        last_updated: now,
        source_count: 1,
        metadata_json: None,
        aliases: vec![],
    };
    db.upsert_entity(&entity).unwrap();
    id
}

/// Helper: insert a relationship
fn make_rel(db: &L2GraphDB, subject: &str, predicate: &str, object: &str) -> String {
    let id = db.next_relationship_id().unwrap();
    let rel = Relationship {
        id: id.clone(),
        subject_id: subject.to_string(),
        predicate: predicate.to_string(),
        object_type: "entity".to_string(),
        object_value: object.to_string(),
        confidence: 0.95,
        extracted_at: Utc::now(),
        metadata_json: None,
    };
    db.add_relationship(&rel).unwrap();
    id
}

// ─── DB Operations ──────────────────────────────────────────────────────────

#[test]
fn test_entity_upsert_and_find() {
    let db = test_db();
    let id = make_entity(
        &db,
        "Nodalync Protocol",
        "Product",
        "A decentralized knowledge protocol",
    );

    let found = db.find_entity("Nodalync Protocol").unwrap();
    assert!(found.is_some());
    let e = found.unwrap();
    assert_eq!(e.id, id);
    assert_eq!(e.entity_type, "Product");
}

#[test]
fn test_entity_find_case_insensitive() {
    let db = test_db();
    make_entity(&db, "Gabriel Giangi", "Person", "Founder");

    let found = db.find_entity("gabriel giangi").unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().canonical_label, "Gabriel Giangi");
}

#[test]
fn test_entity_find_by_alias() {
    let db = test_db();
    let id = db.next_entity_id().unwrap();
    let now = Utc::now();
    let entity = Entity {
        id: id.clone(),
        canonical_label: "Nodalync".to_string(),
        entity_type: "Organization".to_string(),
        description: Some("Protocol company".to_string()),
        confidence: 1.0,
        first_seen: now,
        last_updated: now,
        source_count: 1,
        metadata_json: None,
        aliases: vec!["Exo".to_string(), "nodalync-protocol".to_string()],
    };
    db.upsert_entity(&entity).unwrap();

    // Should find by alias
    let found = db.find_entity("Exo").unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().id, id);
}

#[test]
fn test_entity_not_found() {
    let db = test_db();
    let found = db.find_entity("nonexistent").unwrap();
    assert!(found.is_none());
}

#[test]
fn test_id_counter_increments() {
    let db = test_db();
    let e1 = db.next_entity_id().unwrap();
    let e2 = db.next_entity_id().unwrap();
    assert_ne!(e1, e2);
    assert!(e1.starts_with('e'));
    assert!(e2.starts_with('e'));

    let r1 = db.next_relationship_id().unwrap();
    let r2 = db.next_relationship_id().unwrap();
    assert_ne!(r1, r2);
    assert!(r1.starts_with('r'));
}

#[test]
fn test_relationship_dedup() {
    let db = test_db();
    let e1 = make_entity(&db, "A", "Thing", "First");
    let e2 = make_entity(&db, "B", "Thing", "Second");

    let rel = Relationship {
        id: db.next_relationship_id().unwrap(),
        subject_id: e1.clone(),
        predicate: "relatedTo".to_string(),
        object_type: "entity".to_string(),
        object_value: e2.clone(),
        confidence: 0.9,
        extracted_at: Utc::now(),
        metadata_json: None,
    };

    let first = db.add_relationship(&rel).unwrap();
    assert!(first); // new

    // Same subject/predicate/object should be skipped
    let dup_rel = Relationship {
        id: db.next_relationship_id().unwrap(),
        ..rel.clone()
    };
    let second = db.add_relationship(&dup_rel).unwrap();
    assert!(!second); // duplicate
}

#[test]
fn test_content_registry() {
    let db = test_db();
    let cid = db.register_content("abc123hash", "markdown").unwrap();
    assert!(!cid.is_empty());

    let found = db.content_hash_exists("abc123hash").unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap(), cid);

    let not_found = db.content_hash_exists("nope").unwrap();
    assert!(not_found.is_none());
}

#[test]
fn test_entity_source_link() {
    let db = test_db();
    let eid = make_entity(&db, "Test", "Thing", "desc");
    let cid = db.register_content("hash1", "md").unwrap();
    db.link_entity_source(&eid, &cid).unwrap();

    let sources = db.get_entity_sources(&eid).unwrap();
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].content_id, cid);
}

#[test]
fn test_list_by_type() {
    let db = test_db();
    make_entity(&db, "Alice", "Person", "Engineer");
    make_entity(&db, "Bob", "Person", "Designer");
    make_entity(&db, "Nodalync", "Product", "Protocol");

    let people = db.list_entities_by_type("Person", 10).unwrap();
    assert_eq!(people.len(), 2);

    let products = db.list_entities_by_type("Product", 10).unwrap();
    assert_eq!(products.len(), 1);
}

#[test]
fn test_clear_all() {
    let db = test_db();
    make_entity(&db, "Temp", "Thing", "will be cleared");
    let stats = db.get_stats().unwrap();
    assert_eq!(*stats.get("entities").unwrap(), 1);

    db.clear_all().unwrap();
    let stats = db.get_stats().unwrap();
    assert_eq!(*stats.get("entities").unwrap(), 0);
}

// ─── Search ─────────────────────────────────────────────────────────────────

#[test]
fn test_search_single_word() {
    let db = test_db();
    make_entity(
        &db,
        "Nodalync Protocol",
        "Product",
        "Knowledge economics protocol",
    );
    make_entity(&db, "Unrelated Thing", "Thing", "No match here");

    let results = db.search_entities("nodalync", 10).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].canonical_label, "Nodalync Protocol");
}

#[test]
fn test_search_multi_word() {
    let db = test_db();
    make_entity(&db, "Nodalync Protocol", "Product", "Knowledge economics");
    make_entity(
        &db,
        "Revenue Strategy",
        "Decision",
        "First revenue in 90 days",
    );
    make_entity(
        &db,
        "Nodalync Revenue Plan",
        "Asset",
        "Revenue plan for protocol",
    );

    let results = db.search_entities("nodalync revenue", 10).unwrap();
    // "Nodalync Revenue Plan" matches both words → should rank highest
    assert!(!results.is_empty());
    assert_eq!(results[0].canonical_label, "Nodalync Revenue Plan");
}

#[test]
fn test_search_by_description() {
    let db = test_db();
    make_entity(
        &db,
        "Funding Decision",
        "Decision",
        "Bootstrap with grants before venture capital",
    );

    let results = db.search_entities("bootstrap grants", 10).unwrap();
    assert_eq!(results.len(), 1);
}

#[test]
fn test_search_empty_query() {
    let db = test_db();
    make_entity(&db, "Anything", "Thing", "desc");
    let results = db.search_entities("", 10).unwrap();
    assert!(results.is_empty());
}

// ─── Subgraph ───────────────────────────────────────────────────────────────

#[test]
fn test_subgraph_basic() {
    let db = test_db();
    let e1 = make_entity(&db, "Center", "Thing", "The center node");
    let e2 = make_entity(&db, "Neighbor1", "Thing", "One hop away");
    let e3 = make_entity(&db, "Neighbor2", "Thing", "One hop away");
    let e4 = make_entity(&db, "TwoHop", "Thing", "Two hops away");

    make_rel(&db, &e1, "relatedTo", &e2);
    make_rel(&db, &e1, "mentions", &e3);
    make_rel(&db, &e2, "relatedTo", &e4);

    let sg = db.get_subgraph(&e1, 1, 50).unwrap();
    assert_eq!(sg.center_entity.canonical_label, "Center");
    assert_eq!(sg.connected_entities.len(), 2); // e2, e3
                                                // e4 is 2 hops away, should not be included at max_hops=1

    let sg2 = db.get_subgraph(&e1, 2, 50).unwrap();
    assert_eq!(sg2.connected_entities.len(), 3); // e2, e3, e4
}

#[test]
fn test_subgraph_max_results() {
    let db = test_db();
    let center = make_entity(&db, "Hub", "Thing", "Central node");
    for i in 0..10 {
        let n = make_entity(&db, &format!("Spoke{}", i), "Thing", "spoke");
        make_rel(&db, &center, "relatedTo", &n);
    }

    let sg = db.get_subgraph(&center, 1, 5).unwrap();
    assert!(sg.connected_entities.len() <= 5);
}

// ─── Context ────────────────────────────────────────────────────────────────

#[test]
fn test_context_filters_relationships() {
    let db = test_db();
    let e1 = make_entity(&db, "Nodalync Protocol", "Product", "Knowledge protocol");
    let e2 = make_entity(&db, "Nodalync Revenue", "Goal", "Revenue target");
    let e3 = make_entity(&db, "Unrelated", "Thing", "Not in search results");

    // Relationship between two matched entities
    make_rel(&db, &e1, "relatedTo", &e2);
    // Relationship to an entity NOT in the result set
    make_rel(&db, &e1, "mentions", &e3);

    let ctx = db.get_context("nodalync", 10).unwrap();
    assert_eq!(ctx.relevant_entities.len(), 2); // e1, e2
                                                // Should only include the e1↔e2 relationship, not e1→e3
    assert_eq!(ctx.relevant_relationships.len(), 1);
    assert_eq!(ctx.relevant_relationships[0].object_value, e2);
}

#[test]
fn test_context_confidence_scoring() {
    let db = test_db();
    make_entity(
        &db,
        "Nodalync Revenue Plan",
        "Asset",
        "Revenue plan for the protocol",
    );

    let ctx = db.get_context("nodalync revenue", 10).unwrap();
    // Both words match → confidence should be 1.0
    assert!(
        ctx.confidence_score > 0.9,
        "confidence was {}",
        ctx.confidence_score
    );

    let ctx2 = db.get_context("nodalync xyzbogus", 10).unwrap();
    // Only one of two words matches → confidence ~0.5
    assert!(
        ctx2.confidence_score < 0.8,
        "confidence was {}",
        ctx2.confidence_score
    );
}

#[test]
fn test_context_empty_query() {
    let db = test_db();
    make_entity(&db, "Test", "Thing", "desc");
    let ctx = db.get_context("", 10).unwrap();
    assert!(ctx.relevant_entities.is_empty());
    assert_eq!(ctx.confidence_score, 0.0);
}

// ─── Stats ──────────────────────────────────────────────────────────────────

#[test]
fn test_stats() {
    let db = test_db();
    let e1 = make_entity(&db, "A", "Person", "desc");
    let e2 = make_entity(&db, "B", "Person", "desc");
    make_entity(&db, "C", "Product", "desc");
    make_rel(&db, &e1, "knows", &e2);
    db.register_content("hash1", "md").unwrap();

    let stats = db.get_stats().unwrap();
    assert_eq!(*stats.get("entities").unwrap(), 3);
    assert_eq!(*stats.get("relationships").unwrap(), 1);
    assert_eq!(*stats.get("content_items").unwrap(), 1);
    assert_eq!(*stats.get("  type:Person").unwrap(), 2);
    assert_eq!(*stats.get("  type:Product").unwrap(), 1);
}

// ─── Increment source count ─────────────────────────────────────────────────

#[test]
fn test_increment_source_count() {
    let db = test_db();
    let eid = make_entity(&db, "Counter", "Thing", "desc");

    let before = db.find_entity("Counter").unwrap().unwrap();
    assert_eq!(before.source_count, 1);

    db.increment_source_count(&eid).unwrap();

    let after = db.find_entity("Counter").unwrap().unwrap();
    assert_eq!(after.source_count, 2);
}

// ── Multi-byte safety ────────────────────────────────────────────────────────

#[test]
fn test_context_multibyte_content_no_panic() {
    // Regression test: context query should not panic when source content
    // contains multi-byte characters near the 300-byte truncation boundary.
    let db = test_db();

    // Create an entity whose metadata points to a source file that doesn't
    // exist (so no file read will happen), then verify context query itself
    // doesn't panic on entities with multi-byte descriptions.
    let id = db.next_entity_id().unwrap();
    let now = Utc::now();
    // Description with multi-byte chars (box-drawing, emoji, CJK)
    let desc = "┌─────────────────────────────────────────────────────────────────┐ \
                日本語テスト 🎯 Ünïcödé chàracters → àcross bøundaries │ \
                More text to push past the typical truncation point so we exercise the safe_truncate path \
                in search display functions which used to panic on byte boundaries.";
    let entity = Entity {
        id: id.clone(),
        canonical_label: "MultiByteTest".to_string(),
        entity_type: "Test".to_string(),
        description: Some(desc.to_string()),
        confidence: 1.0,
        first_seen: now,
        last_updated: now,
        source_count: 1,
        metadata_json: None,
        aliases: vec![],
    };
    db.upsert_entity(&entity).unwrap();

    // This must not panic
    let result = db.get_context("multibyte", 10);
    assert!(result.is_ok());
    let ctx = result.unwrap();
    assert!(!ctx.relevant_entities.is_empty());
}
