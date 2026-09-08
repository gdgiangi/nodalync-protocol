//! Bounded, read-only relationship browsing. Queries return one page of entities
//! or direct edges; they never expand a graph or load original content bodies.

use anyhow::{bail, Context, Result};
use nodalync_graph::L2GraphDB;
use nodalync_ops::DefaultNodeOperations;
use nodalync_store::ContentStore;
use serde::Serialize;
use std::sync::{Arc, Mutex as StdMutex};
use tauri::State;
use tokio::sync::Mutex;

use crate::protocol::ProtocolState;
use crate::publish_commands::parse_hash;

// Include only relationships that can be displayed with two real endpoints.
// UNION counts a self-relationship once in an entity's incident degree.
const EDGE_CTE: &str = "WITH valid_edges AS (
    SELECT r.id, r.subject_id, r.object_value, r.predicate, r.confidence FROM relationships r
    JOIN entities source ON source.id = r.subject_id
    JOIN entities target ON target.id = r.object_value
    WHERE r.object_type = 'entity'
)";
const DEGREE_CTE: &str = ", incident AS (
    SELECT id, subject_id AS entity_id FROM valid_edges
    UNION SELECT id, object_value AS entity_id FROM valid_edges
), degrees AS (
    SELECT entity_id, COUNT(*) AS relationship_count FROM incident GROUP BY entity_id
)";
const IS_NOTE: &str = "(substr(e.id, 1, 6) = 'vault:' OR
    CASE WHEN json_valid(e.metadata_json)
    THEN COALESCE(json_extract(e.metadata_json, '$.extraction') = 'vault_import', 0) ELSE 0 END)";
const NAME_MATCH: &str = r"(e.canonical_label LIKE ?2 ESCAPE '\' OR EXISTS (
    SELECT 1 FROM entity_aliases a WHERE a.entity_id = e.id AND a.alias LIKE ?2 ESCAPE '\'
))";
const DIRECTION_MATCH: &str = "(r.subject_id = ?1 OR r.object_value = ?1)
    AND (?2 = 'all' OR (?2 = 'incoming' AND r.object_value = ?1)
        OR (?2 = 'outgoing' AND r.subject_id = ?1))";

#[derive(Debug, Clone, Serialize)]
pub struct RelationshipEntity {
    pub id: String,
    pub label: String,
    pub entity_type: String,
    pub description: Option<String>,
    pub source_count: u64,
    pub relationship_count: u64,
}

#[derive(Debug, Serialize)]
pub struct RelationshipCounts {
    pub notes: u64,
    pub concepts: u64,
    pub relationships: u64,
}

#[derive(Debug, Serialize)]
pub struct RelationshipTypeCount {
    pub entity_type: String,
    pub count: u64,
}

#[derive(Debug, Serialize)]
pub struct RelationshipEntityPage {
    pub items: Vec<RelationshipEntity>,
    pub total: u64,
    pub offset: u32,
    pub limit: u32,
    pub has_more: bool,
    pub counts: RelationshipCounts,
    pub types: Vec<RelationshipTypeCount>,
}

#[derive(Debug, Serialize)]
pub struct RelationshipEdge {
    pub id: String,
    pub source: RelationshipEntity,
    pub target: RelationshipEntity,
    pub predicate: String,
    pub confidence: f64,
}

#[derive(Debug, Serialize)]
pub struct DirectionCounts {
    pub incoming: u64,
    pub outgoing: u64,
    pub all: u64,
}

#[derive(Debug, Serialize)]
pub struct PredicateCount {
    pub predicate: String,
    pub count: u64,
}

#[derive(Debug, Serialize)]
pub struct RelationshipSource {
    pub hash: String,
    pub title: String,
    pub available_locally: bool,
}

#[derive(Debug, Serialize)]
pub struct RelationshipSourcePage {
    pub items: Vec<RelationshipSource>,
    pub total: u64,
    pub offset: u32,
    pub limit: u32,
    pub has_more: bool,
}

#[derive(Debug, Serialize)]
pub struct RelationshipNeighborhood {
    pub entity: RelationshipEntity,
    pub items: Vec<RelationshipEdge>,
    pub total: u64,
    pub offset: u32,
    pub limit: u32,
    pub has_more: bool,
    pub direction_counts: DirectionCounts,
    pub predicates: Vec<PredicateCount>,
    pub sources: Vec<RelationshipSource>,
    pub source_total: u64,
    pub source_has_more: bool,
}

// These row fields are shared by directory rows and both ends of an edge.
// Keeping the conversion in a macro avoids requiring a second rusqlite dependency.
macro_rules! entity_row {
    ($row:expr, $start:expr) => {
        RelationshipEntity {
            id: $row.get($start)?,
            label: $row.get($start + 1)?,
            entity_type: $row.get($start + 2)?,
            description: $row.get($start + 3)?,
            source_count: $row.get($start + 4)?,
            relationship_count: $row.get($start + 5)?,
        }
    };
}

#[tauri::command]
pub async fn list_relationship_entities(
    query: Option<String>,
    kind: String,
    entity_type: Option<String>,
    offset: Option<u32>,
    limit: Option<u32>,
    db: State<'_, StdMutex<L2GraphDB>>,
) -> Result<RelationshipEntityPage, String> {
    let db = db.lock().map_err(|error| error.to_string())?;
    entity_page(
        &db,
        query.as_deref(),
        &kind,
        entity_type.as_deref(),
        offset,
        limit,
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn get_relationship_neighborhood(
    entity_id: String,
    direction: String,
    predicate: Option<String>,
    offset: Option<u32>,
    limit: Option<u32>,
    db: State<'_, StdMutex<L2GraphDB>>,
    protocol: State<'_, Arc<Mutex<Option<ProtocolState>>>>,
) -> Result<RelationshipNeighborhood, String> {
    // Release the graph mutex before awaiting protocol state; no lock inversion.
    let (mut page, hashes) = {
        let db = db.lock().map_err(|error| error.to_string())?;
        neighborhood(
            &db,
            &entity_id,
            &direction,
            predicate.as_deref(),
            offset,
            limit,
        )
        .map_err(|error| error.to_string())?
    };
    let guard = protocol.lock().await;
    let state = guard.as_ref().ok_or("Unlock your workspace first.")?;
    page.sources = resolve_sources(&state.ops, hashes).map_err(|error| error.to_string())?;
    Ok(page)
}

#[tauri::command]
pub async fn list_relationship_sources(
    entity_id: String,
    offset: Option<u32>,
    limit: Option<u32>,
    db: State<'_, StdMutex<L2GraphDB>>,
    protocol: State<'_, Arc<Mutex<Option<ProtocolState>>>>,
) -> Result<RelationshipSourcePage, String> {
    let offset = offset.unwrap_or(0);
    let limit = limit.unwrap_or(12).clamp(1, 24);
    let (hashes, total) = {
        let db = db.lock().map_err(|error| error.to_string())?;
        check_entity(&db, &entity_id).map_err(|error| error.to_string())?;
        source_hash_page(&db, &entity_id, offset, limit).map_err(|error| error.to_string())?
    };
    let guard = protocol.lock().await;
    let state = guard.as_ref().ok_or("Unlock your workspace first.")?;
    let items = resolve_sources(&state.ops, hashes).map_err(|error| error.to_string())?;
    Ok(RelationshipSourcePage {
        has_more: u64::from(offset) + (items.len() as u64) < total,
        items,
        total,
        offset,
        limit,
    })
}

fn search_pattern(query: Option<&str>) -> Result<String> {
    let query = query.unwrap_or_default().trim();
    if query.len() > 200 {
        bail!("Keep the entity search under 200 bytes.");
    }
    Ok(format!(
        "%{}%",
        query
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_")
    ))
}

fn entity_page(
    db: &L2GraphDB,
    query: Option<&str>,
    kind: &str,
    entity_type: Option<&str>,
    offset: Option<u32>,
    limit: Option<u32>,
) -> Result<RelationshipEntityPage> {
    let notes = match kind {
        "notes" => true,
        "concepts" => false,
        _ => bail!("Choose notes or concepts."),
    };
    let pattern = search_pattern(query)?;
    let offset = offset.unwrap_or(0);
    let limit = limit.unwrap_or(12).clamp(1, 24);
    let conn = db.connection();
    let filter = format!("{IS_NOTE} = ?1 AND {NAME_MATCH}");
    let (notes_count, concepts): (u64, u64) = conn.query_row(
        &format!("SELECT COALESCE(SUM(CASE WHEN {IS_NOTE} THEN 1 ELSE 0 END), 0), COALESCE(SUM(CASE WHEN {IS_NOTE} THEN 0 ELSE 1 END), 0) FROM entities e"), [], |row| Ok((row.get(0)?, row.get(1)?)))?;
    let relationships = conn.query_row(
        &format!("{EDGE_CTE} SELECT COUNT(*) FROM valid_edges"),
        [],
        |row| row.get(0),
    )?;
    let mut statement = conn.prepare(&format!("SELECT e.entity_type, COUNT(*) FROM entities e WHERE {filter} GROUP BY e.entity_type ORDER BY COUNT(*) DESC, e.entity_type COLLATE NOCASE, e.entity_type"))?;
    let types = statement
        .query_map((notes, &pattern), |row| {
            Ok(RelationshipTypeCount {
                entity_type: row.get(0)?,
                count: row.get(1)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let total = conn.query_row(
        &format!(
            "SELECT COUNT(*) FROM entities e WHERE {filter} AND (?3 IS NULL OR e.entity_type = ?3)"
        ),
        (notes, &pattern, entity_type),
        |row| row.get(0),
    )?;
    let sql = format!("{EDGE_CTE}{DEGREE_CTE}
        SELECT e.id, e.canonical_label, e.entity_type, e.description, e.source_count, COALESCE(d.relationship_count, 0) AS relationship_count
        FROM entities e LEFT JOIN degrees d ON d.entity_id = e.id
        WHERE {filter} AND (?3 IS NULL OR e.entity_type = ?3)
        ORDER BY relationship_count DESC, e.canonical_label COLLATE NOCASE, e.id LIMIT ?4 OFFSET ?5");
    let mut statement = conn.prepare(&sql)?;
    let items = statement
        .query_map((notes, &pattern, entity_type, limit, offset), |row| {
            Ok(entity_row!(row, 0))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(RelationshipEntityPage {
        has_more: u64::from(offset) + (items.len() as u64) < total,
        items,
        total,
        offset,
        limit,
        counts: RelationshipCounts {
            notes: notes_count,
            concepts,
            relationships,
        },
        types,
    })
}

fn check_entity(db: &L2GraphDB, id: &str) -> Result<()> {
    let exists: bool = db.connection().query_row(
        "SELECT EXISTS(SELECT 1 FROM entities WHERE id = ?1)",
        [id],
        |row| row.get(0),
    )?;
    if !exists {
        bail!("This entity is no longer in the workspace.");
    }
    Ok(())
}

fn neighborhood(
    db: &L2GraphDB,
    entity_id: &str,
    direction: &str,
    predicate: Option<&str>,
    offset: Option<u32>,
    limit: Option<u32>,
) -> Result<(RelationshipNeighborhood, Vec<String>)> {
    if !["all", "incoming", "outgoing"].contains(&direction) {
        bail!("Choose incoming, outgoing, or all relationships.");
    }
    check_entity(db, entity_id)?;
    let offset = offset.unwrap_or(0);
    let limit = limit.unwrap_or(8).clamp(1, 16);
    let conn = db.connection();
    let entity = conn.query_row(&format!("{EDGE_CTE}{DEGREE_CTE}
        SELECT e.id, e.canonical_label, e.entity_type, e.description, e.source_count, COALESCE(d.relationship_count, 0)
        FROM entities e LEFT JOIN degrees d ON d.entity_id = e.id WHERE e.id = ?1"), [entity_id], |row| Ok(entity_row!(row, 0)))?;
    let direction_counts = conn.query_row(&format!("{EDGE_CTE}
        SELECT COALESCE(SUM(r.object_value = ?1), 0), COALESCE(SUM(r.subject_id = ?1), 0), COUNT(*) FROM valid_edges r
        WHERE (r.subject_id = ?1 OR r.object_value = ?1) AND (?2 IS NULL OR r.predicate = ?2)"),
        (entity_id, predicate), |row| Ok(DirectionCounts { incoming: row.get(0)?, outgoing: row.get(1)?, all: row.get(2)? }))?;
    let mut statement = conn.prepare(&format!("{EDGE_CTE} SELECT r.predicate, COUNT(*) FROM valid_edges r WHERE {DIRECTION_MATCH} GROUP BY r.predicate ORDER BY COUNT(*) DESC, r.predicate"))?;
    let predicates = statement
        .query_map((entity_id, direction), |row| {
            Ok(PredicateCount {
                predicate: row.get(0)?,
                count: row.get(1)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let total = conn.query_row(&format!("{EDGE_CTE} SELECT COUNT(*) FROM valid_edges r WHERE {DIRECTION_MATCH} AND (?3 IS NULL OR r.predicate = ?3)"), (entity_id, direction, predicate), |row| row.get(0))?;
    let sql = format!("{EDGE_CTE}{DEGREE_CTE}
        SELECT r.id, r.predicate, r.confidence,
            s.id, s.canonical_label, s.entity_type, s.description, s.source_count, COALESCE(ds.relationship_count, 0),
            t.id, t.canonical_label, t.entity_type, t.description, t.source_count, COALESCE(dt.relationship_count, 0)
        FROM valid_edges r JOIN entities s ON s.id = r.subject_id JOIN entities t ON t.id = r.object_value
        LEFT JOIN degrees ds ON ds.entity_id = s.id LEFT JOIN degrees dt ON dt.entity_id = t.id
        WHERE {DIRECTION_MATCH} AND (?3 IS NULL OR r.predicate = ?3)
        ORDER BY r.confidence DESC, r.predicate, s.canonical_label COLLATE NOCASE, t.canonical_label COLLATE NOCASE, r.id
        LIMIT ?4 OFFSET ?5");
    let mut statement = conn.prepare(&sql)?;
    let items = statement
        .query_map((entity_id, direction, predicate, limit, offset), |row| {
            Ok(RelationshipEdge {
                id: row.get(0)?,
                predicate: row.get(1)?,
                confidence: row.get(2)?,
                source: entity_row!(row, 3),
                target: entity_row!(row, 9),
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let (hashes, source_total) = source_hash_page(db, entity_id, 0, 12)?;
    Ok((
        RelationshipNeighborhood {
            entity,
            has_more: u64::from(offset) + (items.len() as u64) < total,
            items,
            total,
            offset,
            limit,
            direction_counts,
            predicates,
            source_has_more: (hashes.len() as u64) < source_total,
            source_total,
            sources: vec![],
        },
        hashes,
    ))
}

fn source_hash_page(
    db: &L2GraphDB,
    entity_id: &str,
    offset: u32,
    limit: u32,
) -> Result<(Vec<String>, u64)> {
    let conn = db.connection();
    let joins = "FROM entity_sources es JOIN content_registry cr ON cr.content_id = es.content_id WHERE es.entity_id = ?1 AND cr.deleted_at IS NULL";
    let total = conn.query_row(
        &format!("SELECT COUNT(DISTINCT cr.current_hash) {joins}"),
        [entity_id],
        |row| row.get(0),
    )?;
    let mut statement = conn.prepare(&format!("SELECT cr.current_hash {joins} GROUP BY cr.current_hash ORDER BY MAX(es.added_at) DESC, cr.current_hash LIMIT ?2 OFFSET ?3"))?;
    let hashes = statement
        .query_map((entity_id, limit, offset), |row| row.get(0))?
        .collect::<std::result::Result<Vec<String>, _>>()?;
    Ok((hashes, total))
}

fn resolve_sources(
    ops: &DefaultNodeOperations,
    hashes: Vec<String>,
) -> Result<Vec<RelationshipSource>> {
    hashes
        .into_iter()
        .map(|hash| {
            let parsed = parse_hash(&hash).ok();
            let manifest = match parsed {
                Some(hash) => ops
                    .get_content_manifest(&hash)
                    .context("Couldn’t read source metadata")?,
                None => None,
            };
            Ok(RelationshipSource {
                available_locally: manifest.is_some()
                    && parsed.is_some_and(|hash| ops.state().content.exists(&hash)),
                title: manifest
                    .map(|manifest| manifest.metadata.title)
                    .unwrap_or_else(|| "Original source unavailable".into()),
                hash,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use nodalync_crypto::{generate_identity, peer_id_from_public_key};
    use nodalync_store::{NodeState, NodeStateConfig};
    use nodalync_types::Metadata;
    use std::collections::HashSet;

    fn hub_graph() -> Result<L2GraphDB> {
        let graph = L2GraphDB::new(":memory:")?;
        let transaction = graph.connection().unchecked_transaction()?;
        transaction.execute("INSERT INTO entities (id, canonical_label, entity_type, first_seen, last_updated, source_count) VALUES ('vault:hub', 'Hub_100%', 'Document', 0, 0, 0)", [])?;
        transaction.execute("INSERT INTO entities (id, canonical_label, entity_type, first_seen, last_updated, source_count, metadata_json) VALUES ('metadata-note', 'Another note', 'Research', 0, 0, 0, '{\"extraction\":\"vault_import\"}')", [])?;
        {
            let mut entities = transaction.prepare("INSERT INTO entities (id, canonical_label, entity_type, first_seen, last_updated, source_count, metadata_json) VALUES (?1, ?2, ?3, 0, 0, 1, ?4)")?;
            let mut edges = transaction.prepare("INSERT INTO relationships (id, subject_id, predicate, object_type, object_value, confidence, extracted_at) VALUES (?1, ?2, ?3, 'entity', ?4, 0.8, 0)")?;
            for index in 0..10_000 {
                let id = format!("e{index:05}");
                entities.execute((
                    &id,
                    format!("Entity {index:05}"),
                    if index % 2 == 0 { "Concept" } else { "Person" },
                    if index % 10 == 0 {
                        "invalid metadata"
                    } else {
                        "{\"auto_extracted\":true}"
                    },
                ))?;
                let (source, target) = if index % 2 == 0 {
                    ("vault:hub", id.as_str())
                } else {
                    (id.as_str(), "vault:hub")
                };
                edges.execute((
                    format!("r{index:05}"),
                    source,
                    if index % 3 == 0 {
                        "supports"
                    } else {
                        "links_to"
                    },
                    target,
                ))?;
            }
        }
        transaction.execute(
            "INSERT INTO entity_aliases (entity_id, alias) VALUES ('e00000', 'Literal_%')",
            [],
        )?;
        transaction.execute("INSERT INTO relationships (id, subject_id, predicate, object_type, object_value, confidence, extracted_at) VALUES ('self', 'vault:hub', 'reflects', 'entity', 'vault:hub', 0.7, 0)", [])?;
        transaction.execute("INSERT INTO relationships (id, subject_id, predicate, object_type, object_value, confidence, extracted_at) VALUES ('dangling', 'vault:hub', 'missing', 'entity', 'absent', 1.0, 0)", [])?;
        transaction.execute("INSERT INTO relationships (id, subject_id, predicate, object_type, object_value, confidence, extracted_at) VALUES ('literal', 'vault:hub', 'literal', 'literal', 'vault:hub', 1.0, 0)", [])?;
        transaction.commit()?;
        Ok(graph)
    }

    #[test]
    fn ten_thousand_entities_are_paged_with_literal_filters_and_exact_counts() -> Result<()> {
        let graph = hub_graph()?;
        let changes_before: u64 =
            graph
                .connection()
                .query_row("SELECT total_changes()", [], |row| row.get(0))?;
        let notes = entity_page(&graph, None, "notes", None, None, None)?;
        assert_eq!((notes.total, notes.limit, notes.items.len()), (2, 12, 2));
        assert_eq!(notes.items[0].id, "vault:hub");
        assert_eq!(notes.items[0].relationship_count, 10_001);
        assert_eq!(
            (
                notes.counts.notes,
                notes.counts.concepts,
                notes.counts.relationships
            ),
            (2, 10_000, 10_001)
        );
        let first = entity_page(&graph, None, "concepts", None, Some(0), Some(5000))?;
        let second = entity_page(&graph, None, "concepts", None, Some(24), Some(24))?;
        assert_eq!(
            (first.total, first.limit, first.items.len()),
            (10_000, 24, 24)
        );
        assert!(first.has_more);
        assert_eq!(
            first.types.iter().map(|item| item.count).sum::<u64>(),
            10_000
        );
        let ids: HashSet<_> = first
            .items
            .iter()
            .chain(&second.items)
            .map(|item| &item.id)
            .collect();
        assert_eq!(ids.len(), 48);
        assert_eq!(first.items[0].id, "e00000");
        assert_eq!(second.items[0].id, "e00024");
        let filtered = entity_page(&graph, None, "concepts", Some("Person"), None, Some(1))?;
        assert_eq!(
            (filtered.total, filtered.items[0].id.as_str()),
            (5000, "e00001")
        );
        assert_eq!(filtered.types.len(), 2); // Type facet is before selected type.
        assert_eq!(
            entity_page(&graph, Some("_%"), "concepts", None, None, None)?.total,
            1
        );
        assert_eq!(
            entity_page(&graph, Some("%"), "notes", None, None, None)?.total,
            1
        );
        let end = entity_page(&graph, None, "concepts", None, Some(10_000), None)?;
        assert!(end.items.is_empty() && !end.has_more);
        assert!(entity_page(&graph, None, "invalid", None, None, None).is_err());
        assert!(entity_page(&graph, Some(&"x".repeat(201)), "notes", None, None, None).is_err());
        let changes_after: u64 =
            graph
                .connection()
                .query_row("SELECT total_changes()", [], |row| row.get(0))?;
        assert_eq!(changes_after, changes_before);
        Ok(())
    }

    #[test]
    fn large_hub_edges_page_both_directions_without_expansion_or_dangling_endpoints() -> Result<()>
    {
        let graph = hub_graph()?;
        let (first, _) = neighborhood(&graph, "vault:hub", "all", None, None, Some(9999))?;
        let (second, _) = neighborhood(&graph, "vault:hub", "all", None, Some(16), Some(16))?;
        assert_eq!(
            (first.total, first.limit, first.items.len()),
            (10_001, 16, 16)
        );
        assert_eq!(
            (
                first.direction_counts.incoming,
                first.direction_counts.outgoing,
                first.direction_counts.all
            ),
            (5001, 5001, 10_001)
        );
        assert_eq!(
            first.predicates.iter().map(|item| item.count).sum::<u64>(),
            10_001
        );
        let ids: HashSet<_> = first
            .items
            .iter()
            .chain(&second.items)
            .map(|edge| &edge.id)
            .collect();
        assert_eq!(ids.len(), 32);
        assert_eq!(
            serde_json::to_value(&first.items)?,
            serde_json::to_value(
                neighborhood(&graph, "vault:hub", "all", None, None, Some(16))?
                    .0
                    .items
            )?
        );
        assert!(first.items.iter().all(|edge| !edge.source.label.is_empty()
            && !edge.target.label.is_empty()
            && (edge.source.id == "vault:hub" || edge.target.id == "vault:hub")));
        let (incoming, _) = neighborhood(
            &graph,
            "vault:hub",
            "incoming",
            Some("supports"),
            None,
            None,
        )?;
        assert_eq!(incoming.total, 1667);
        assert_eq!(
            (
                incoming.direction_counts.incoming,
                incoming.direction_counts.outgoing,
                incoming.direction_counts.all
            ),
            (1667, 1667, 3334)
        );
        assert!(incoming
            .items
            .iter()
            .all(|edge| edge.target.id == "vault:hub" && edge.predicate == "supports"));
        assert_eq!(
            incoming
                .predicates
                .iter()
                .map(|item| item.count)
                .sum::<u64>(),
            5001
        );
        let (outgoing, _) = neighborhood(
            &graph,
            "vault:hub",
            "outgoing",
            Some("supports"),
            None,
            None,
        )?;
        assert_eq!(outgoing.total, 1667);
        assert!(outgoing
            .items
            .iter()
            .all(|edge| edge.source.id == "vault:hub"));
        let (self_edge, _) =
            neighborhood(&graph, "vault:hub", "all", Some("reflects"), None, None)?;
        assert_eq!(
            (
                self_edge.total,
                self_edge.items.len(),
                self_edge.direction_counts.all
            ),
            (1, 1, 1)
        );
        let (end, _) = neighborhood(&graph, "vault:hub", "all", None, Some(10_001), None)?;
        assert!(end.items.is_empty() && !end.has_more);
        assert!(neighborhood(&graph, "missing", "all", None, None, None).is_err());
        assert!(neighborhood(&graph, "vault:hub", "sideways", None, None, None).is_err());
        Ok(())
    }

    #[test]
    fn source_pages_use_protocol_hashes_and_metadata_without_reading_or_changing_originals(
    ) -> Result<()> {
        let graph = L2GraphDB::new(":memory:")?;
        graph.connection().execute("INSERT INTO entities (id, canonical_label, entity_type, first_seen, last_updated) VALUES ('vault:note', 'Note', 'Document', 0, 0)", [])?;
        let temp = tempfile::tempdir()?;
        let (_, public) = generate_identity();
        let mut ops = DefaultNodeOperations::with_defaults(
            NodeState::open(NodeStateConfig::new(temp.path()))?,
            peer_id_from_public_key(&public),
        );
        let mut originals = Vec::new();
        for index in 0..13 {
            let text = format!("Original {index}: café\r\n");
            let hash = ops.create_content(
                text.as_bytes(),
                Metadata::new(format!("Source {index}"), text.len() as u64),
            )?;
            let content_id = graph.register_content(&hash.to_string(), "L0")?;
            graph.link_entity_source("vault:note", &content_id)?;
            originals.push((hash, text));
        }
        let missing_hash = "f".repeat(64);
        let missing = graph.register_content(&missing_hash, "L0")?;
        graph.link_entity_source("vault:note", &missing)?;
        let duplicate = graph.register_content(&originals[0].0.to_string(), "L0")?;
        graph.link_entity_source("vault:note", &duplicate)?;
        let deleted = graph.register_content(&"e".repeat(64), "L0")?;
        graph.link_entity_source("vault:note", &deleted)?;
        graph.connection().execute(
            "UPDATE content_registry SET deleted_at = 1 WHERE content_id = ?1",
            [&deleted],
        )?;
        let (page, hashes) = neighborhood(&graph, "vault:note", "all", None, None, None)?;
        assert_eq!(
            (page.source_total, hashes.len(), page.source_has_more),
            (14, 12, true)
        );
        let (remaining, total) = source_hash_page(&graph, "vault:note", 12, 12)?;
        assert_eq!((total, remaining.len()), (14, 2));
        let all_hashes: HashSet<_> = hashes.iter().chain(&remaining).collect();
        assert_eq!(all_hashes.len(), 14);
        let resolved = resolve_sources(&ops, hashes.into_iter().chain(remaining).collect())?;
        assert_eq!(
            resolved
                .iter()
                .filter(|source| source.available_locally)
                .count(),
            13
        );
        assert!(
            !resolved
                .iter()
                .find(|source| source.hash == missing_hash)
                .unwrap()
                .available_locally
        );
        for (hash, text) in originals {
            let item = resolved
                .iter()
                .find(|item| item.hash == hash.to_string())
                .unwrap();
            assert_eq!(
                item.title,
                ops.get_content_manifest(&hash)?.unwrap().metadata.title
            );
            assert_eq!(ops.state().content.load(&hash)?.unwrap(), text.as_bytes());
        }
        Ok(())
    }
}
