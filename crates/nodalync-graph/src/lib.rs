use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::{Connection, Row};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use uuid::Uuid;

pub mod entity_extraction;
pub mod error;
pub mod schema;
pub mod subgraph;

pub use error::GraphError;

/// Content registry entry - tracks stable content IDs across versions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentRegistryEntry {
    pub content_id: String,
    pub current_hash: String,
    pub content_type: String,
    pub created_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

/// Entity in the L2 graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    pub id: String,
    pub canonical_label: String,
    pub entity_type: String,
    pub description: Option<String>,
    pub confidence: f64,
    pub first_seen: DateTime<Utc>,
    pub last_updated: DateTime<Utc>,
    pub source_count: i32,
    pub metadata_json: Option<String>,
    pub aliases: Vec<String>,
}

/// Relationship between entities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relationship {
    pub id: String,
    pub subject_id: String,
    pub predicate: String,
    pub object_type: String,
    pub object_value: String,
    pub confidence: f64,
    pub extracted_at: DateTime<Utc>,
    pub metadata_json: Option<String>,
}

/// Link between entity and source content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntitySource {
    pub entity_id: String,
    pub content_id: String,
    pub l1_mention_id: Option<String>,
    pub added_at: DateTime<Utc>,
}

/// Subgraph query result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubgraphResult {
    pub center_entity: Entity,
    pub connected_entities: Vec<Entity>,
    pub relationships: Vec<Relationship>,
    pub sources: Vec<EntitySource>,
}

/// Context result for agent queries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextResult {
    pub query: String,
    pub relevant_entities: Vec<Entity>,
    pub relevant_relationships: Vec<Relationship>,
    pub source_content: Vec<String>, // Compact representations
    pub confidence_score: f64,
}

/// Main L2 Graph database interface
pub struct L2GraphDB {
    conn: Connection,
}

impl L2GraphDB {
    pub fn new<P: AsRef<Path>>(db_path: P) -> Result<Self> {
        let conn = Connection::open(db_path)?;
        let mut db = Self { conn };
        db.initialize_schema()?;
        Ok(db)
    }

    pub fn initialize_schema(&mut self) -> Result<()> {
        schema::create_tables(&self.conn)?;
        schema::initialize_counters(&self.conn)?;
        Ok(())
    }

    /// Register content in the registry with a stable ID
    pub fn register_content(&self, hash: &str, content_type: &str) -> Result<String> {
        let content_id = Uuid::new_v4().to_string();
        let now = Utc::now();

        self.conn.execute(
            "INSERT INTO content_registry (content_id, current_hash, content_type, created_at)
             VALUES (?1, ?2, ?3, ?4)",
            (content_id.as_str(), hash, content_type, now.timestamp()),
        )?;

        Ok(content_id)
    }

    /// Get next entity ID (e.g., "e42")
    pub fn next_entity_id(&self) -> Result<String> {
        let id: i64 = self.conn.query_row(
            "UPDATE id_counters SET next_value = next_value + 1 
             WHERE counter_name = 'entity' 
             RETURNING next_value - 1",
            [],
            |row| row.get(0),
        )?;
        Ok(format!("e{}", id))
    }

    /// Get next relationship ID (e.g., "r17")
    pub fn next_relationship_id(&self) -> Result<String> {
        let id: i64 = self.conn.query_row(
            "UPDATE id_counters SET next_value = next_value + 1 
             WHERE counter_name = 'relationship' 
             RETURNING next_value - 1",
            [],
            |row| row.get(0),
        )?;
        Ok(format!("r{}", id))
    }

    /// Create or update an entity
    pub fn upsert_entity(&self, entity: &Entity) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;

        // Insert or update entity
        tx.execute(
            "INSERT OR REPLACE INTO entities 
             (id, canonical_label, entity_type, description, confidence, 
              first_seen, last_updated, source_count, metadata_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            (
                &entity.id,
                &entity.canonical_label,
                &entity.entity_type,
                &entity.description,
                entity.confidence,
                entity.first_seen.timestamp(),
                entity.last_updated.timestamp(),
                entity.source_count,
                &entity.metadata_json,
            ),
        )?;

        // Update aliases
        tx.execute(
            "DELETE FROM entity_aliases WHERE entity_id = ?1",
            (&entity.id,),
        )?;
        for alias in &entity.aliases {
            tx.execute(
                "INSERT INTO entity_aliases (entity_id, alias) VALUES (?1, ?2)",
                (&entity.id, alias),
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    /// Find entity by label or alias.
    /// Prefers exact canonical_label match over alias match.
    pub fn find_entity(&self, label: &str) -> Result<Option<Entity>> {
        let query = "
            SELECT e.id, e.canonical_label, e.entity_type, e.description,
                   e.confidence, e.first_seen, e.last_updated, e.source_count,
                   e.metadata_json, GROUP_CONCAT(a.alias) as aliases
            FROM entities e
            LEFT JOIN entity_aliases a ON e.id = a.entity_id
            WHERE LOWER(e.canonical_label) = LOWER(?1)
               OR e.id IN (SELECT entity_id FROM entity_aliases WHERE LOWER(alias) = LOWER(?1))
            GROUP BY e.id
            ORDER BY CASE WHEN LOWER(e.canonical_label) = LOWER(?1) THEN 0 ELSE 1 END
            LIMIT 1";

        match self.conn.query_row(query, [label], Self::entity_from_row) {
            Ok(entity) => Ok(Some(entity)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Add relationship between entities.
    /// Deduplicates: if same (subject, predicate, object_type, object_value) exists, skips.
    pub fn add_relationship(&self, relationship: &Relationship) -> Result<bool> {
        // Check for existing duplicate
        let exists: bool = self.conn.query_row(
            "SELECT COUNT(*) > 0 FROM relationships 
             WHERE subject_id = ?1 AND predicate = ?2 AND object_type = ?3 AND object_value = ?4",
            (
                &relationship.subject_id,
                &relationship.predicate,
                &relationship.object_type,
                &relationship.object_value,
            ),
            |row| row.get(0),
        )?;

        if exists {
            return Ok(false);
        }

        self.conn.execute(
            "INSERT INTO relationships 
             (id, subject_id, predicate, object_type, object_value, 
              confidence, extracted_at, metadata_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            (
                &relationship.id,
                &relationship.subject_id,
                &relationship.predicate,
                &relationship.object_type,
                &relationship.object_value,
                relationship.confidence,
                relationship.extracted_at.timestamp(),
                &relationship.metadata_json,
            ),
        )?;
        Ok(true)
    }

    /// Link entity to source content
    pub fn link_entity_source(&self, entity_id: &str, content_id: &str) -> Result<()> {
        let now = Utc::now();
        self.conn.execute(
            "INSERT OR IGNORE INTO entity_sources (entity_id, content_id, added_at)
             VALUES (?1, ?2, ?3)",
            (entity_id, content_id, now.timestamp()),
        )?;
        Ok(())
    }

    /// Get subgraph around an entity
    pub fn get_subgraph(
        &self,
        entity_id: &str,
        max_hops: u32,
        max_results: u32,
    ) -> Result<SubgraphResult> {
        subgraph::get_subgraph(&self.conn, entity_id, max_hops, max_results)
    }

    /// Get focused context for agent queries.
    ///
    /// Returns matched entities, only the relationships *between* those entities,
    /// and compact source-file snippets (first ~300 chars of body) for each entity.
    pub fn get_context(&self, query: &str, max_entities: u32) -> Result<ContextResult> {
        let keywords = query.to_lowercase();
        let entities = self.search_entities(&keywords, max_entities)?;

        // Collect entity IDs for inter-entity relationship filtering
        let entity_ids: std::collections::HashSet<String> =
            entities.iter().map(|e| e.id.clone()).collect();

        // Only keep relationships where BOTH endpoints are in the result set
        let mut relationships = Vec::new();
        let mut seen_rel_ids = std::collections::HashSet::new();
        for entity in &entities {
            let entity_rels = self.get_entity_relationships(&entity.id)?;
            for rel in entity_rels {
                if seen_rel_ids.contains(&rel.id) {
                    continue;
                }
                let other_id = if rel.subject_id == entity.id {
                    if rel.object_type == "entity" {
                        &rel.object_value
                    } else {
                        continue;
                    }
                } else {
                    &rel.subject_id
                };
                if entity_ids.contains(other_id) {
                    seen_rel_ids.insert(rel.id.clone());
                    relationships.push(rel);
                }
            }
        }

        // Load compact source-file snippets from metadata → source_file path
        let mut source_content = Vec::new();
        for entity in &entities {
            if let Some(ref meta_json) = entity.metadata_json {
                if let Ok(meta) = serde_json::from_str::<serde_json::Value>(meta_json) {
                    if let Some(path_str) = meta.get("source_file").and_then(|v| v.as_str()) {
                        let path = std::path::Path::new(path_str);
                        if path.exists() {
                            if let Ok(content) = std::fs::read_to_string(path) {
                                let body = strip_frontmatter(&content);
                                let snippet = if body.len() > 300 {
                                    format!(
                                        "[{}] {}...",
                                        entity.canonical_label,
                                        safe_truncate(body, 300)
                                    )
                                } else {
                                    format!("[{}] {}", entity.canonical_label, body)
                                };
                                source_content.push(snippet);
                            }
                        }
                    }
                }
            }
        }

        // Compute confidence: ratio of query words that matched at least one entity label
        let words: Vec<&str> = keywords
            .split_whitespace()
            .filter(|w| w.len() >= 2)
            .collect();
        let matched_words = words
            .iter()
            .filter(|w| {
                entities.iter().any(|e| {
                    e.canonical_label.to_lowercase().contains(*w)
                        || e.description
                            .as_deref()
                            .unwrap_or("")
                            .to_lowercase()
                            .contains(*w)
                })
            })
            .count();
        let confidence_score = if words.is_empty() {
            0.0
        } else {
            matched_words as f64 / words.len() as f64
        };

        Ok(ContextResult {
            query: query.to_string(),
            relevant_entities: entities,
            relevant_relationships: relationships,
            source_content,
            confidence_score,
        })
    }

    /// Search entities by keywords.
    /// For multi-word queries, splits into individual words and matches any.
    /// Results matching more words rank higher.
    pub fn search_entities(&self, keywords: &str, limit: u32) -> Result<Vec<Entity>> {
        let words: Vec<&str> = keywords
            .split_whitespace()
            .filter(|w| w.len() >= 2)
            .collect();

        if words.is_empty() {
            return Ok(Vec::new());
        }

        // Build dynamic WHERE clause: match ANY word in label, description, or alias
        let mut where_parts = Vec::new();
        let mut params: Vec<String> = Vec::new();

        for (i, word) in words.iter().enumerate() {
            let idx = i + 1;
            where_parts.push(format!(
                "(LOWER(e.canonical_label) LIKE LOWER(?{idx}) OR LOWER(e.description) LIKE LOWER(?{idx}) OR LOWER(a.alias) LIKE LOWER(?{idx}))"
            ));
            params.push(format!("%{}%", word));
        }

        let where_clause = where_parts.join(" OR ");

        // Score: count how many words match the label (for ranking)
        let mut score_parts = Vec::new();
        for (i, _) in words.iter().enumerate() {
            let idx = i + 1;
            score_parts.push(format!(
                "CASE WHEN LOWER(e.canonical_label) LIKE LOWER(?{idx}) THEN 2 WHEN LOWER(e.description) LIKE LOWER(?{idx}) THEN 1 ELSE 0 END"
            ));
        }
        let score_expr = score_parts.join(" + ");

        let query = format!(
            "SELECT e.id, e.canonical_label, e.entity_type, e.description,
                    e.confidence, e.first_seen, e.last_updated, e.source_count,
                    e.metadata_json, GROUP_CONCAT(a.alias) as aliases,
                    ({score_expr}) as match_score
             FROM entities e
             LEFT JOIN entity_aliases a ON e.id = a.entity_id
             WHERE {where_clause}
             GROUP BY e.id
             ORDER BY match_score DESC, e.source_count DESC, e.last_updated DESC
             LIMIT ?{}",
            words.len() + 1
        );

        params.push(limit.to_string());

        let mut stmt = self.conn.prepare(&query)?;
        let param_refs: Vec<&dyn rusqlite::ToSql> =
            params.iter().map(|p| p as &dyn rusqlite::ToSql).collect();
        let entity_iter = stmt.query_map(&param_refs[..], Self::entity_from_row)?;

        let mut entities = Vec::new();
        for entity_result in entity_iter {
            entities.push(entity_result?);
        }

        Ok(entities)
    }

    /// Get relationships involving an entity
    pub fn get_entity_relationships(&self, entity_id: &str) -> Result<Vec<Relationship>> {
        let query = "
            SELECT id, subject_id, predicate, object_type, object_value,
                   confidence, extracted_at, metadata_json
            FROM relationships
            WHERE subject_id = ?1 OR (object_type = 'entity' AND object_value = ?1)";

        let mut stmt = self.conn.prepare(query)?;
        let rel_iter = stmt.query_map([entity_id], Self::relationship_from_row)?;

        let mut relationships = Vec::new();
        for rel_result in rel_iter {
            relationships.push(rel_result?);
        }

        Ok(relationships)
    }

    /// Get sources for an entity
    pub fn get_entity_sources(&self, entity_id: &str) -> Result<Vec<EntitySource>> {
        let query = "
            SELECT entity_id, content_id, l1_mention_id, added_at
            FROM entity_sources
            WHERE entity_id = ?1";

        let mut stmt = self.conn.prepare(query)?;
        let source_iter = stmt.query_map([entity_id], |row| {
            Ok(EntitySource {
                entity_id: row.get(0)?,
                content_id: row.get(1)?,
                l1_mention_id: row.get(2)?,
                added_at: DateTime::from_timestamp(row.get::<_, i64>(3)?, 0)
                    .unwrap_or_else(Utc::now),
            })
        })?;

        let mut sources = Vec::new();
        for source_result in source_iter {
            sources.push(source_result?);
        }

        Ok(sources)
    }

    /// Helper to convert database row to Entity
    fn entity_from_row(row: &Row) -> rusqlite::Result<Entity> {
        let aliases_str: Option<String> = row.get("aliases")?;
        let aliases = aliases_str
            .map(|s| s.split(',').map(|alias| alias.trim().to_string()).collect())
            .unwrap_or_default();

        Ok(Entity {
            id: row.get("id")?,
            canonical_label: row.get("canonical_label")?,
            entity_type: row.get("entity_type")?,
            description: row.get("description")?,
            confidence: row.get("confidence")?,
            first_seen: DateTime::from_timestamp(row.get::<_, i64>("first_seen")?, 0)
                .unwrap_or_else(Utc::now),
            last_updated: DateTime::from_timestamp(row.get::<_, i64>("last_updated")?, 0)
                .unwrap_or_else(Utc::now),
            source_count: row.get("source_count")?,
            metadata_json: row.get("metadata_json")?,
            aliases,
        })
    }

    /// Helper to convert database row to Relationship
    fn relationship_from_row(row: &Row) -> rusqlite::Result<Relationship> {
        Ok(Relationship {
            id: row.get("id")?,
            subject_id: row.get("subject_id")?,
            predicate: row.get("predicate")?,
            object_type: row.get("object_type")?,
            object_value: row.get("object_value")?,
            confidence: row.get("confidence")?,
            extracted_at: DateTime::from_timestamp(row.get::<_, i64>("extracted_at")?, 0)
                .unwrap_or_else(Utc::now),
            metadata_json: row.get("metadata_json")?,
        })
    }

    /// List entities by type using the entity_type column directly
    pub fn list_entities_by_type(&self, entity_type: &str, limit: u32) -> Result<Vec<Entity>> {
        let query = "
            SELECT e.id, e.canonical_label, e.entity_type, e.description,
                   e.confidence, e.first_seen, e.last_updated, e.source_count,
                   e.metadata_json, GROUP_CONCAT(a.alias) as aliases
            FROM entities e
            LEFT JOIN entity_aliases a ON e.id = a.entity_id
            WHERE e.entity_type = ?1
            GROUP BY e.id
            ORDER BY e.source_count DESC, e.last_updated DESC
            LIMIT ?2";

        let mut stmt = self.conn.prepare(query)?;
        let entity_iter =
            stmt.query_map(rusqlite::params![entity_type, limit], Self::entity_from_row)?;

        let mut entities = Vec::new();
        for entity_result in entity_iter {
            entities.push(entity_result?);
        }
        Ok(entities)
    }

    /// Check if a content hash already exists in the registry.
    /// Returns the content_id if found.
    pub fn content_hash_exists(&self, hash: &str) -> Result<Option<String>> {
        match self.conn.query_row(
            "SELECT content_id FROM content_registry WHERE current_hash = ?1 AND deleted_at IS NULL",
            [hash],
            |row| row.get::<_, String>(0),
        ) {
            Ok(id) => Ok(Some(id)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Clear all data from the database (for fresh scans)
    pub fn clear_all(&self) -> Result<()> {
        self.conn.execute_batch(
            "DELETE FROM entity_sources;
             DELETE FROM relationship_sources;
             DELETE FROM entity_aliases;
             DELETE FROM relationships;
             DELETE FROM entities;
             DELETE FROM content_registry;
             UPDATE id_counters SET next_value = 1;",
        )?;
        Ok(())
    }

    /// Increment source_count for an existing entity
    pub fn increment_source_count(&self, entity_id: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE entities SET source_count = source_count + 1, last_updated = ?1 WHERE id = ?2",
            (Utc::now().timestamp(), entity_id),
        )?;
        Ok(())
    }

    /// Get database statistics
    pub fn get_stats(&self) -> Result<HashMap<String, i32>> {
        let mut stats = HashMap::new();

        stats.insert(
            "entities".to_string(),
            self.conn
                .query_row("SELECT COUNT(*) FROM entities", [], |row| row.get(0))?,
        );

        stats.insert(
            "relationships".to_string(),
            self.conn
                .query_row("SELECT COUNT(*) FROM relationships", [], |row| row.get(0))?,
        );

        stats.insert(
            "content_items".to_string(),
            self.conn.query_row(
                "SELECT COUNT(*) FROM content_registry WHERE deleted_at IS NULL",
                [],
                |row| row.get(0),
            )?,
        );

        // Type breakdown
        let mut stmt = self.conn.prepare(
            "SELECT entity_type, COUNT(*) as cnt FROM entities GROUP BY entity_type ORDER BY cnt DESC"
        )?;
        let type_iter = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i32>(1)?))
        })?;
        for (etype, count) in type_iter.flatten() {
            stats.insert(format!("  type:{}", etype), count);
        }

        Ok(stats)
    }

    /// Expose the inner connection for tests / advanced queries
    pub fn connection(&self) -> &Connection {
        &self.conn
    }
}

/// Truncate a string to at most `max_bytes` without splitting a multi-byte character.
fn safe_truncate(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// Strip YAML frontmatter delimiters from markdown content, returning the body.
fn strip_frontmatter(content: &str) -> &str {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return content;
    }
    let after = &trimmed[3..];
    if let Some(pos) = after.find("\n---") {
        let start = pos + 4; // skip past the closing "---"
        if start < after.len() {
            return after[start..].trim_start();
        }
    }
    content
}
