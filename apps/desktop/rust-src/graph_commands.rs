//! Tauri commands for L2 Graph DB queries, L1 extraction pipeline, and L3 synthesis.
//! Provides the bridge between the React frontend, protocol content, and the
//! SQLite graph database.

use nodalync_graph::L2GraphDB;
use serde::{Deserialize, Serialize};
use std::sync::Mutex as StdMutex;
use tauri::State;
use tokio::sync::Mutex as TokioMutex;
use tracing::info;

use crate::protocol::ProtocolState;

/// Graph node for D3 force simulation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: String,
    pub label: String,
    pub entity_type: String,
    pub description: Option<String>,
    pub source_count: i32,
    pub confidence: f64,
}

/// Graph link for D3 force simulation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphLink {
    pub id: String,
    pub source: String,
    pub target: String,
    pub predicate: String,
    pub confidence: f64,
}

/// Full graph data payload for D3
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphData {
    pub nodes: Vec<GraphNode>,
    pub links: Vec<GraphLink>,
}

/// Entity search result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub id: String,
    pub label: String,
    pub entity_type: String,
    pub description: Option<String>,
    pub source_count: i32,
    pub aliases: Vec<String>,
}

/// Database statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphStats {
    pub entity_count: i32,
    pub relationship_count: i32,
    pub content_count: i32,
    pub type_breakdown: Vec<TypeCount>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeCount {
    pub entity_type: String,
    pub count: i32,
}

/// Load the full graph for D3 visualization.
/// Returns all entities as nodes and all relationships as links.
#[tauri::command]
pub async fn get_graph_data(db: State<'_, StdMutex<L2GraphDB>>) -> Result<GraphData, String> {
    info!("Loading full graph data for visualization");
    let db = db.lock().map_err(|e| format!("DB lock error: {}", e))?;

    // Load all entities
    let conn = db.connection();
    let mut stmt = conn
        .prepare(
            "SELECT e.id, e.canonical_label, e.entity_type, e.description,
                    e.source_count, e.confidence
             FROM entities e
             ORDER BY e.source_count DESC",
        )
        .map_err(|e| format!("Query error: {}", e))?;

    let nodes: Vec<GraphNode> = stmt
        .query_map([], |row| {
            Ok(GraphNode {
                id: row.get(0)?,
                label: row.get(1)?,
                entity_type: row.get(2)?,
                description: row.get(3)?,
                source_count: row.get(4)?,
                confidence: row.get(5)?,
            })
        })
        .map_err(|e| format!("Query error: {}", e))?
        .filter_map(|r| r.ok())
        .collect();

    // Load all relationships
    let mut stmt = conn
        .prepare(
            "SELECT r.id, r.subject_id, r.object_value, r.predicate, r.confidence
             FROM relationships r
             WHERE r.object_type = 'entity'",
        )
        .map_err(|e| format!("Query error: {}", e))?;

    let links: Vec<GraphLink> = stmt
        .query_map([], |row| {
            Ok(GraphLink {
                id: row.get(0)?,
                source: row.get(1)?,
                target: row.get(2)?,
                predicate: row.get(3)?,
                confidence: row.get(4)?,
            })
        })
        .map_err(|e| format!("Query error: {}", e))?
        .filter_map(|r| r.ok())
        .collect();

    info!(
        "Loaded graph: {} nodes, {} links",
        nodes.len(),
        links.len()
    );
    Ok(GraphData { nodes, links })
}

/// Get subgraph around a specific entity
#[tauri::command]
pub async fn get_subgraph(
    db: State<'_, StdMutex<L2GraphDB>>,
    entity_id: String,
    max_hops: u32,
    max_results: u32,
) -> Result<GraphData, String> {
    info!(
        "Getting subgraph for {} (hops={}, max={})",
        entity_id, max_hops, max_results
    );
    let db = db.lock().map_err(|e| format!("DB lock error: {}", e))?;

    let subgraph = db
        .get_subgraph(&entity_id, max_hops, max_results)
        .map_err(|e| format!("Subgraph error: {}", e))?;

    // Convert center + connected entities to nodes
    let mut nodes = vec![GraphNode {
        id: subgraph.center_entity.id.clone(),
        label: subgraph.center_entity.canonical_label.clone(),
        entity_type: subgraph.center_entity.entity_type.clone(),
        description: subgraph.center_entity.description.clone(),
        source_count: subgraph.center_entity.source_count,
        confidence: subgraph.center_entity.confidence,
    }];

    for entity in &subgraph.connected_entities {
        nodes.push(GraphNode {
            id: entity.id.clone(),
            label: entity.canonical_label.clone(),
            entity_type: entity.entity_type.clone(),
            description: entity.description.clone(),
            source_count: entity.source_count,
            confidence: entity.confidence,
        });
    }

    let links: Vec<GraphLink> = subgraph
        .relationships
        .iter()
        .map(|rel| GraphLink {
            id: rel.id.clone(),
            source: rel.subject_id.clone(),
            target: rel.object_value.clone(),
            predicate: rel.predicate.clone(),
            confidence: rel.confidence,
        })
        .collect();

    Ok(GraphData { nodes, links })
}

/// Search entities by keyword
#[tauri::command]
pub async fn search_entities(
    db: State<'_, StdMutex<L2GraphDB>>,
    query: String,
    limit: u32,
) -> Result<Vec<SearchResult>, String> {
    info!("Searching entities: '{}'", query);
    let db = db.lock().map_err(|e| format!("DB lock error: {}", e))?;

    let entities = db
        .search_entities(&query, limit)
        .map_err(|e| format!("Search error: {}", e))?;

    let results: Vec<SearchResult> = entities
        .into_iter()
        .map(|e| SearchResult {
            id: e.id,
            label: e.canonical_label,
            entity_type: e.entity_type,
            description: e.description,
            source_count: e.source_count,
            aliases: e.aliases,
        })
        .collect();

    Ok(results)
}

/// Get database statistics
#[tauri::command]
pub async fn get_graph_stats(db: State<'_, StdMutex<L2GraphDB>>) -> Result<GraphStats, String> {
    info!("Getting graph statistics");
    let db = db.lock().map_err(|e| format!("DB lock error: {}", e))?;

    let stats = db
        .get_stats()
        .map_err(|e| format!("Stats error: {}", e))?;

    let entity_count = *stats.get("entities").unwrap_or(&0);
    let relationship_count = *stats.get("relationships").unwrap_or(&0);
    let content_count = *stats.get("content_items").unwrap_or(&0);

    let mut type_breakdown: Vec<TypeCount> = stats
        .iter()
        .filter(|(k, _)| k.starts_with("  type:"))
        .map(|(k, v)| TypeCount {
            entity_type: k.trim_start_matches("  type:").to_string(),
            count: *v,
        })
        .collect();
    type_breakdown.sort_by(|a, b| b.count.cmp(&a.count));

    Ok(GraphStats {
        entity_count,
        relationship_count,
        content_count,
        type_breakdown,
    })
}

/// Get focused context for a query (for agent integration)
#[tauri::command]
pub async fn get_context(
    db: State<'_, StdMutex<L2GraphDB>>,
    query: String,
    max_entities: u32,
) -> Result<serde_json::Value, String> {
    info!("Getting context for: '{}'", query);
    let db = db.lock().map_err(|e| format!("DB lock error: {}", e))?;

    let context = db
        .get_context(&query, max_entities)
        .map_err(|e| format!("Context error: {}", e))?;

    serde_json::to_value(&context).map_err(|e| format!("Serialize error: {}", e))
}

// ─── L1 Extraction Pipeline ─────────────────────────────────────────────────

/// Result of mention extraction — an entity matched or created in the graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedEntity {
    /// Entity ID in the L2 graph DB.
    pub entity_id: String,
    /// Canonical label.
    pub label: String,
    /// Entity type (concept, organization, person, technology, etc.).
    pub entity_type: String,
    /// Whether this entity already existed in the graph.
    pub existing: bool,
    /// Confidence score (1.0 for exact match, <1.0 for new stubs).
    pub confidence: f64,
    /// Source mention text that generated this entity.
    pub source_mention: Option<String>,
}

/// Full extraction result returned to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractionResult {
    /// Content hash that was analyzed.
    pub content_hash: String,
    /// Content ID registered in the graph DB.
    pub content_id: String,
    /// Total mentions extracted from the content.
    pub mention_count: u32,
    /// Entities matched or created in the L2 graph.
    pub entities: Vec<ExtractedEntity>,
    /// Primary topics detected.
    pub topics: Vec<String>,
    /// Summary text.
    pub summary: String,
}

/// Extract L1 mentions from content and bridge them to the L2 graph.
///
/// This is the core pipeline that bridges L0 (raw content) → L1 (mentions) → L2 (graph):
/// 1. Runs L1 extraction on the content (via protocol ops)
/// 2. Matches extracted entities against existing L2 graph entities
/// 3. Creates entity stubs for unmatched mentions (confidence < 1.0)
/// 4. Stores entity↔content links in entity_sources
/// 5. Returns the full extraction result
///
/// Hephaestus uses this to show the extraction results after content is added.
#[tauri::command]
pub async fn extract_mentions(
    content_hash: String,
    db: State<'_, StdMutex<L2GraphDB>>,
    protocol: State<'_, TokioMutex<Option<ProtocolState>>>,
) -> Result<ExtractionResult, String> {
    info!("Extracting mentions for content: {}", content_hash);

    // 1. Run L1 extraction via protocol ops
    let l1_summary = {
        let mut guard = protocol.lock().await;
        let state = guard.as_mut().ok_or("Node not initialized — unlock first")?;

        let hash = crate::publish_commands::parse_hash(&content_hash)?;
        state.ops.extract_l1_summary(&hash)
            .map_err(|e| format!("Extraction failed: {}", e))?
    };

    // 2. Lock graph DB and match entities
    let db = db.lock().map_err(|e| format!("DB lock error: {}", e))?;

    // Register this content in the graph DB if not already there
    let content_id = match db.content_hash_exists(&content_hash)
        .map_err(|e| format!("DB error: {}", e))?
    {
        Some(id) => id,
        None => db.register_content(&content_hash, "L0")
            .map_err(|e| format!("Failed to register content: {}", e))?,
    };

    // 3. Process each mention's entities
    let mut extracted_entities: Vec<ExtractedEntity> = Vec::new();
    let mut seen_labels: std::collections::HashSet<String> = std::collections::HashSet::new();

    for mention in &l1_summary.preview_mentions {
        for entity_name in &mention.entities {
            let normalized = entity_name.trim().to_string();
            if normalized.is_empty() || normalized.len() < 2 {
                continue;
            }
            // Deduplicate within this extraction
            let key = normalized.to_lowercase();
            if seen_labels.contains(&key) {
                continue;
            }
            seen_labels.insert(key);

            // Try to match against existing L2 graph entities
            match db.find_entity(&normalized)
                .map_err(|e| format!("Entity lookup error: {}", e))?
            {
                Some(existing_entity) => {
                    // Match found — link to content and report
                    db.link_entity_source(&existing_entity.id, &content_id)
                        .map_err(|e| format!("Failed to link entity source: {}", e))?;
                    db.increment_source_count(&existing_entity.id)
                        .map_err(|e| format!("Failed to increment source count: {}", e))?;

                    extracted_entities.push(ExtractedEntity {
                        entity_id: existing_entity.id,
                        label: existing_entity.canonical_label,
                        entity_type: existing_entity.entity_type,
                        existing: true,
                        confidence: existing_entity.confidence,
                        source_mention: Some(mention.content.clone()),
                    });
                }
                None => {
                    // No match — create a new entity stub
                    let entity_id = db.next_entity_id()
                        .map_err(|e| format!("Failed to get entity ID: {}", e))?;

                    let entity_type = classify_entity_type(&normalized);
                    let now = chrono::Utc::now();

                    let new_entity = nodalync_graph::Entity {
                        id: entity_id.clone(),
                        canonical_label: normalized.clone(),
                        entity_type: entity_type.clone(),
                        description: None,
                        confidence: 0.6, // Sub-1.0 — flagged for review
                        first_seen: now,
                        last_updated: now,
                        source_count: 1,
                        metadata_json: Some(serde_json::json!({
                            "auto_extracted": true,
                            "needs_review": true,
                            "source_content": content_hash,
                        }).to_string()),
                        aliases: Vec::new(),
                    };

                    db.upsert_entity(&new_entity)
                        .map_err(|e| format!("Failed to create entity: {}", e))?;
                    db.link_entity_source(&entity_id, &content_id)
                        .map_err(|e| format!("Failed to link entity source: {}", e))?;

                    extracted_entities.push(ExtractedEntity {
                        entity_id,
                        label: normalized,
                        entity_type,
                        existing: false,
                        confidence: 0.6,
                        source_mention: Some(mention.content.clone()),
                    });
                }
            }
        }
    }

    // Also extract entities from topics that aren't covered by mention entities
    for topic in &l1_summary.primary_topics {
        let key = topic.to_lowercase();
        if seen_labels.contains(&key) || topic.len() < 2 {
            continue;
        }
        seen_labels.insert(key);

        if let Some(existing) = db.find_entity(topic)
            .map_err(|e| format!("Entity lookup error: {}", e))?
        {
            db.link_entity_source(&existing.id, &content_id)
                .map_err(|e| format!("Failed to link entity source: {}", e))?;
            db.increment_source_count(&existing.id)
                .map_err(|e| format!("Failed to increment source count: {}", e))?;

            extracted_entities.push(ExtractedEntity {
                entity_id: existing.id,
                label: existing.canonical_label,
                entity_type: existing.entity_type,
                existing: true,
                confidence: existing.confidence,
                source_mention: None,
            });
        }
        // Don't auto-create stubs for topics — only for mention-level entities
    }

    info!(
        "Extraction complete: {} mentions, {} entities ({} new, {} existing)",
        l1_summary.mention_count,
        extracted_entities.len(),
        extracted_entities.iter().filter(|e| !e.existing).count(),
        extracted_entities.iter().filter(|e| e.existing).count(),
    );

    Ok(ExtractionResult {
        content_hash,
        content_id,
        mention_count: l1_summary.mention_count,
        entities: extracted_entities,
        topics: l1_summary.primary_topics,
        summary: l1_summary.summary,
    })
}

/// Simple heuristic to classify entity type from the label.
///
/// In the future this should use a proper NER model, but for MVP
/// we use pattern matching.
fn classify_entity_type(label: &str) -> String {
    let lower = label.to_lowercase();

    // Technology/protocol patterns
    if lower.ends_with("protocol")
        || lower.ends_with("network")
        || lower.ends_with("chain")
        || lower.ends_with("api")
        || lower.contains("sdk")
        || lower.contains("framework")
    {
        return "technology".to_string();
    }

    // Organization patterns
    if lower.ends_with("inc")
        || lower.ends_with("corp")
        || lower.ends_with("foundation")
        || lower.ends_with("labs")
        || lower.ends_with("dao")
    {
        return "organization".to_string();
    }

    // Concept patterns (multi-word, lowercase tendency)
    if label.contains(' ') && label.split_whitespace().count() >= 3 {
        return "concept".to_string();
    }

    // Default: concept for single/double words
    "concept".to_string()
}

// ─── L3 Synthesis Commands ───────────────────────────────────────────────────

/// An L3 summary entity — a synthesis of multiple L2 entities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct L3Summary {
    /// Entity ID in the graph DB (e.g. "e42").
    pub entity_id: String,
    /// User-provided title for the synthesis.
    pub title: String,
    /// User-provided or auto-generated summary text.
    pub summary_text: String,
    /// IDs of L2 entities that were synthesized.
    pub source_entity_ids: Vec<String>,
    /// Labels of source entities (for display without extra lookups).
    pub source_entity_labels: Vec<String>,
    /// When this synthesis was created.
    pub created_at: String,
}

/// Result of L3 creation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct L3CreateResult {
    /// The newly created L3 entity.
    pub summary: L3Summary,
    /// How many "synthesizes" relationships were created (should equal source count).
    pub relationships_created: u32,
}

/// Create an L3 synthesis entity from selected L2 entities.
///
/// This is the knowledge synthesis operation: select entities → create a summary
/// that references all of them. The summary entity has type "summary" and
/// "synthesizes" relationships pointing to each source entity.
///
/// Hephaestus uses this for the "Create Summary" action in the graph view.
#[tauri::command]
pub async fn create_l3_summary(
    title: String,
    summary_text: String,
    entity_ids: Vec<String>,
    db: State<'_, StdMutex<L2GraphDB>>,
) -> Result<L3CreateResult, String> {
    if entity_ids.is_empty() {
        return Err("At least one entity must be selected for synthesis".to_string());
    }
    if title.trim().is_empty() {
        return Err("Summary title cannot be empty".to_string());
    }
    if entity_ids.len() > 100 {
        return Err("Cannot synthesize more than 100 entities at once".to_string());
    }

    info!(
        "Creating L3 summary '{}' from {} entities",
        title,
        entity_ids.len()
    );

    let db = db.lock().map_err(|e| format!("DB lock error: {}", e))?;
    let now = chrono::Utc::now();

    // Verify all source entities exist and collect their labels
    let mut source_labels = Vec::with_capacity(entity_ids.len());
    for eid in &entity_ids {
        match db
            .find_entity_by_id(eid)
            .map_err(|e| format!("Failed to look up entity {}: {}", eid, e))?
        {
            Some(entity) => source_labels.push(entity.canonical_label),
            None => return Err(format!("Entity '{}' not found in graph", eid)),
        }
    }

    // Create the L3 summary entity
    let summary_id = db
        .next_entity_id()
        .map_err(|e| format!("Failed to get entity ID: {}", e))?;

    let metadata = serde_json::json!({
        "level": "L3",
        "synthesis": true,
        "source_entity_ids": entity_ids,
        "source_count": entity_ids.len(),
    });

    let summary_entity = nodalync_graph::Entity {
        id: summary_id.clone(),
        canonical_label: title.clone(),
        entity_type: "summary".to_string(),
        description: Some(summary_text.clone()),
        confidence: 1.0, // User-created, full confidence
        first_seen: now,
        last_updated: now,
        source_count: entity_ids.len() as i32,
        metadata_json: Some(metadata.to_string()),
        aliases: Vec::new(),
    };

    db.upsert_entity(&summary_entity)
        .map_err(|e| format!("Failed to create L3 entity: {}", e))?;

    // Create "synthesizes" relationships from the L3 entity to each source
    let mut rels_created = 0u32;
    for source_id in &entity_ids {
        let rel_id = db
            .next_relationship_id()
            .map_err(|e| format!("Failed to get relationship ID: {}", e))?;

        let rel = nodalync_graph::Relationship {
            id: rel_id,
            subject_id: summary_id.clone(),
            predicate: "synthesizes".to_string(),
            object_type: "entity".to_string(),
            object_value: source_id.clone(),
            confidence: 1.0,
            extracted_at: now,
            metadata_json: None,
        };

        let created = db
            .add_relationship(&rel)
            .map_err(|e| format!("Failed to create relationship: {}", e))?;
        if created {
            rels_created += 1;
        }
    }

    info!(
        "L3 summary '{}' ({}) created with {} relationships",
        title, summary_id, rels_created
    );

    Ok(L3CreateResult {
        summary: L3Summary {
            entity_id: summary_id,
            title,
            summary_text,
            source_entity_ids: entity_ids,
            source_entity_labels: source_labels,
            created_at: now.to_rfc3339(),
        },
        relationships_created: rels_created,
    })
}

/// List all L3 summaries in the graph.
///
/// Returns summaries sorted newest-first, with their source entity references.
#[tauri::command]
pub async fn get_l3_summaries(
    limit: Option<u32>,
    db: State<'_, StdMutex<L2GraphDB>>,
) -> Result<Vec<L3Summary>, String> {
    let limit = limit.unwrap_or(50).min(500);
    info!("Listing L3 summaries (limit={})", limit);

    let db = db.lock().map_err(|e| format!("DB lock error: {}", e))?;
    let conn = db.connection();

    // Find entities with metadata containing "level":"L3"
    let mut stmt = conn
        .prepare(
            "SELECT id, canonical_label, description, metadata_json, first_seen
             FROM entities
             WHERE entity_type = 'summary'
               AND metadata_json LIKE '%\"level\":\"L3\"%'
             ORDER BY first_seen DESC
             LIMIT ?1",
        )
        .map_err(|e| format!("Query error: {}", e))?;

    let summaries: Vec<L3Summary> = stmt
        .query_map([limit], |row| {
            let id: String = row.get(0)?;
            let title: String = row.get(1)?;
            let description: Option<String> = row.get(2)?;
            let metadata_str: Option<String> = row.get(3)?;
            let first_seen: i64 = row.get(4)?;

            // Extract source_entity_ids from metadata
            let (source_ids, source_labels) = if let Some(ref meta) = metadata_str {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(meta) {
                    let ids: Vec<String> = parsed
                        .get("source_entity_ids")
                        .and_then(|v| serde_json::from_value(v.clone()).ok())
                        .unwrap_or_default();
                    (ids, Vec::new()) // Labels populated below
                } else {
                    (Vec::new(), Vec::new())
                }
            } else {
                (Vec::new(), Vec::new())
            };

            Ok(L3Summary {
                entity_id: id,
                title,
                summary_text: description.unwrap_or_default(),
                source_entity_ids: source_ids,
                source_entity_labels: source_labels,
                created_at: chrono::DateTime::from_timestamp(first_seen, 0)
                    .map(|dt| dt.to_rfc3339())
                    .unwrap_or_default(),
            })
        })
        .map_err(|e| format!("Query error: {}", e))?
        .filter_map(|r| r.ok())
        .collect();

    // Populate source labels for each summary
    let mut result = Vec::with_capacity(summaries.len());
    for mut summary in summaries {
        let mut labels = Vec::with_capacity(summary.source_entity_ids.len());
        for eid in &summary.source_entity_ids {
            let label: String = conn
                .query_row(
                    "SELECT canonical_label FROM entities WHERE id = ?1",
                    [eid],
                    |row| row.get(0),
                )
                .unwrap_or_else(|_| format!("[deleted: {}]", eid));
            labels.push(label);
        }
        summary.source_entity_labels = labels;
        result.push(summary);
    }

    info!("Found {} L3 summaries", result.len());
    Ok(result)
}

/// Get L0 content items linked to a specific entity.
///
/// This powers the "L0 focus → L1 tendrils" interaction: when a user clicks
/// an L2 entity, show which L0 content items contributed to it.
/// Returns content hashes, types, and the link timestamps.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityContentLink {
    /// Content ID in the registry.
    pub content_id: String,
    /// Current hash of the content.
    pub content_hash: String,
    /// Content type (e.g. "L0").
    pub content_type: String,
    /// When this entity↔content link was created.
    pub linked_at: String,
}

#[tauri::command]
pub async fn get_entity_content_links(
    entity_id: String,
    db: State<'_, StdMutex<L2GraphDB>>,
) -> Result<Vec<EntityContentLink>, String> {
    info!("Getting content links for entity: {}", entity_id);
    let db = db.lock().map_err(|e| format!("DB lock error: {}", e))?;
    let conn = db.connection();

    let mut stmt = conn
        .prepare(
            "SELECT es.content_id, cr.current_hash, cr.content_type, es.added_at
             FROM entity_sources es
             JOIN content_registry cr ON es.content_id = cr.content_id
             WHERE es.entity_id = ?1
               AND cr.deleted_at IS NULL
             ORDER BY es.added_at DESC",
        )
        .map_err(|e| format!("Query error: {}", e))?;

    let links: Vec<EntityContentLink> = stmt
        .query_map([&entity_id], |row| {
            Ok(EntityContentLink {
                content_id: row.get(0)?,
                content_hash: row.get(1)?,
                content_type: row.get(2)?,
                linked_at: chrono::DateTime::from_timestamp(row.get::<_, i64>(3)?, 0)
                    .map(|dt| dt.to_rfc3339())
                    .unwrap_or_default(),
            })
        })
        .map_err(|e| format!("Query error: {}", e))?
        .filter_map(|r| r.ok())
        .collect();

    info!("Found {} content links for entity {}", links.len(), entity_id);
    Ok(links)
}
