//! Create synthetic local-only notes for manually testing the native application.
//! Usage: cargo run --example seed_dev_profile -- /tmp/nodalync-studio-dev-profile
//! Refuses to replace an existing identity. No networking or settlement is started.

#[allow(dead_code)]
#[path = "../rust-src/protocol.rs"]
mod protocol;

use anyhow::{bail, Context, Result};
use chrono::Utc;
use nodalync_graph::{Entity, L2GraphDB, Relationship};
use nodalync_types::Metadata;
use std::collections::HashMap;
use std::path::PathBuf;

const PASSWORD: &str = "local-studio-test-2026";

fn main() -> Result<()> {
    let data_dir = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .context("Pass an explicit throwaway profile directory")?;
    if protocol::ProtocolState::identity_exists(&data_dir) {
        bail!(
            "Refusing to replace existing identity at {}",
            data_dir.display()
        );
    }
    std::fs::create_dir_all(data_dir.join("studio"))?;
    let graph = L2GraphDB::new(data_dir.join("studio/knowledge.db"))?;
    if graph.get_stats()?.get("entities").copied().unwrap_or(0) != 0 {
        bail!("Refusing to seed a graph that already contains entities");
    }
    let mut state = protocol::ProtocolState::init_with_name(
        &data_dir,
        PASSWORD,
        Some("Studio Test Workspace".into()),
    )?;

    let notes = [
        ("Atlas project kickoff", "# Atlas project kickoff\n\nMaya Chen and Rowan Park are exploring Atlas, a local-first research workspace built with Nodalync. The first milestone is a calm place to collect notes, find related ideas, and inspect each source.\n\nWe will validate the library, native Markdown imports, and the knowledge graph before trying any network collaboration. All notes stay private on this device.\n", vec!["Atlas", "Maya Chen", "Rowan Park", "Nodalync", "Local-first knowledge"]),
        ("A practical provenance checklist", "# A practical provenance checklist\n\nRowan Park proposed a provenance checklist for Atlas. Every imported source should retain its content hash. A note should remain readable without a network connection, and extracted ideas should link back to the original text.\n\nMaya Chen will test how people move from a graph entity to its supporting source. Clear source links matter more than adding visual complexity.\n", vec!["Atlas", "Rowan Park", "Maya Chen", "Provenance"]),
        ("Research workspace review", "# Research workspace review\n\nThe Atlas review focused on three questions: Can a new user unlock the workspace? Can they import a Markdown note without losing the original text? Can they find the same idea in both the library and the graph?\n\nNodalync provides local content storage and provenance. The next experiment connects Local-first knowledge with source-backed retrieval. This is synthetic development content for testing the desktop app.\n", vec!["Atlas", "Nodalync", "Local-first knowledge", "Provenance"]),
    ];

    let entities = [
        (
            "Atlas",
            "Project",
            "A fictional local-first research workspace.",
        ),
        (
            "Maya Chen",
            "Person",
            "A fictional designer studying source-backed workflows.",
        ),
        (
            "Rowan Park",
            "Person",
            "A fictional researcher testing provenance and retrieval.",
        ),
        (
            "Nodalync",
            "Technology",
            "The protocol backing this local development workspace.",
        ),
        (
            "Local-first knowledge",
            "Concept",
            "Knowledge that remains useful on its owner's device.",
        ),
        (
            "Provenance",
            "Concept",
            "Links between extracted ideas and their original source notes.",
        ),
    ];
    let mut ids = HashMap::new();
    for (label, kind, description) in entities {
        let id = graph.next_entity_id()?;
        graph.upsert_entity(&Entity {
            id: id.clone(),
            canonical_label: label.into(),
            entity_type: kind.into(),
            description: Some(description.into()),
            confidence: 1.0,
            first_seen: Utc::now(),
            last_updated: Utc::now(),
            source_count: 0,
            metadata_json: Some(r#"{"synthetic_development_fixture":true}"#.into()),
            aliases: vec![],
        })?;
        ids.insert(label, id);
    }

    for (title, text, labels) in notes {
        let metadata = Metadata::new(title, text.len() as u64).with_mime_type("text/markdown");
        let hash = state.ops.create_content(text.as_bytes(), metadata)?;
        state.ops.extract_l1_summary(&hash)?;
        let content_id = graph.register_content(&hash.to_string(), "L0")?;
        for label in labels {
            graph.link_entity_source(&ids[label], &content_id)?;
            graph.increment_source_count(&ids[label])?;
        }
        println!("Stored note: {title} ({hash})");
    }

    for (source, predicate, target) in [
        ("Maya Chen", "works_on", "Atlas"),
        ("Rowan Park", "works_on", "Atlas"),
        ("Atlas", "built_with", "Nodalync"),
        ("Atlas", "explores", "Local-first knowledge"),
        ("Atlas", "tracks", "Provenance"),
        ("Provenance", "supports", "Local-first knowledge"),
    ] {
        graph.add_relationship(&Relationship {
            id: graph.next_relationship_id()?,
            subject_id: ids[source].clone(),
            predicate: predicate.into(),
            object_type: "entity".into(),
            object_value: ids[target].clone(),
            confidence: 1.0,
            extracted_at: Utc::now(),
            metadata_json: Some(r#"{"synthetic_development_fixture":true}"#.into()),
        })?;
    }
    println!("Profile: {}", data_dir.display());
    println!("Synthetic test-only password: {PASSWORD}");
    println!("3 private notes, 6 entities, 6 relationships; networking remains off.");
    Ok(())
}
