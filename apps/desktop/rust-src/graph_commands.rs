//! Tauri commands for L2 Graph DB queries.
//! Provides the bridge between the React frontend and the SQLite graph database.

use nodalync_graph::L2GraphDB;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::State;
use tracing::info;

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
pub async fn get_graph_data(db: State<'_, Mutex<L2GraphDB>>) -> Result<GraphData, String> {
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
    db: State<'_, Mutex<L2GraphDB>>,
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
    db: State<'_, Mutex<L2GraphDB>>,
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
pub async fn get_graph_stats(db: State<'_, Mutex<L2GraphDB>>) -> Result<GraphStats, String> {
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
    db: State<'_, Mutex<L2GraphDB>>,
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
