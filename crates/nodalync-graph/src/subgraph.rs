use crate::{Entity, EntitySource, GraphError, Relationship, SubgraphResult};
use anyhow::Result;
use rusqlite::Connection;
use std::collections::{HashMap, HashSet, VecDeque};

/// Get subgraph around a center entity within max_hops distance
/// Uses breadth-first search to explore the graph
pub fn get_subgraph(
    conn: &Connection,
    center_entity_id: &str,
    max_hops: u32,
    max_results: u32,
) -> Result<SubgraphResult> {
    // Get the center entity
    let center_entity = get_entity_by_id(conn, center_entity_id)?
        .ok_or_else(|| GraphError::EntityNotFound(center_entity_id.to_string()))?;

    // BFS to find connected entities within max_hops
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    let mut connected_entities = Vec::new();
    let mut all_relationships = Vec::new();

    // Start with center entity
    queue.push_back((center_entity_id.to_string(), 0));
    visited.insert(center_entity_id.to_string());

    while let Some((entity_id, depth)) = queue.pop_front() {
        if depth >= max_hops || connected_entities.len() >= max_results as usize {
            break;
        }

        // Get relationships involving this entity
        let entity_relationships = get_entity_relationships(conn, &entity_id)?;

        for relationship in entity_relationships {
            // Add relationship to results if we haven't seen it
            if !all_relationships
                .iter()
                .any(|r: &Relationship| r.id == relationship.id)
            {
                all_relationships.push(relationship.clone());
            }

            // Find the other entity in this relationship
            let other_entity_id = if relationship.subject_id == entity_id {
                // This entity is subject, check if object is an entity
                if relationship.object_type == "entity" {
                    Some(relationship.object_value.clone())
                } else {
                    None
                }
            } else if relationship.object_type == "entity" && relationship.object_value == entity_id
            {
                // This entity is object, subject is the other entity
                Some(relationship.subject_id.clone())
            } else {
                None
            };

            // If we found a connected entity and haven't visited it yet
            if let Some(other_id) = other_entity_id {
                if !visited.contains(&other_id) && depth < max_hops {
                    visited.insert(other_id.clone());
                    queue.push_back((other_id.clone(), depth + 1));

                    // Add the connected entity to results
                    if let Some(entity) = get_entity_by_id(conn, &other_id)? {
                        connected_entities.push(entity);

                        if connected_entities.len() >= max_results as usize {
                            break;
                        }
                    }
                }
            }
        }
    }

    // Get sources for all entities in the subgraph
    let mut all_entity_ids = vec![center_entity_id.to_string()];
    all_entity_ids.extend(connected_entities.iter().map(|e| e.id.clone()));

    let sources = get_sources_for_entities(conn, &all_entity_ids)?;

    Ok(SubgraphResult {
        center_entity,
        connected_entities,
        relationships: all_relationships,
        sources,
    })
}

/// Get entity by ID with aliases
fn get_entity_by_id(conn: &Connection, entity_id: &str) -> Result<Option<Entity>> {
    let query = "
        SELECT e.id, e.canonical_label, e.entity_type, e.description,
               e.confidence, e.first_seen, e.last_updated, e.source_count,
               e.metadata_json, GROUP_CONCAT(a.alias) as aliases
        FROM entities e
        LEFT JOIN entity_aliases a ON e.id = a.entity_id
        WHERE e.id = ?1
        GROUP BY e.id";

    match conn.query_row(query, [entity_id], |row| {
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
            first_seen: chrono::DateTime::from_timestamp(row.get::<_, i64>("first_seen")?, 0)
                .unwrap_or_else(chrono::Utc::now),
            last_updated: chrono::DateTime::from_timestamp(row.get::<_, i64>("last_updated")?, 0)
                .unwrap_or_else(chrono::Utc::now),
            source_count: row.get("source_count")?,
            metadata_json: row.get("metadata_json")?,
            aliases,
        })
    }) {
        Ok(entity) => Ok(Some(entity)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Get all relationships involving an entity (as subject or object)
fn get_entity_relationships(conn: &Connection, entity_id: &str) -> Result<Vec<Relationship>> {
    let query = "
        SELECT id, subject_id, predicate, object_type, object_value,
               confidence, extracted_at, metadata_json
        FROM relationships
        WHERE subject_id = ?1 OR (object_type = 'entity' AND object_value = ?1)
        ORDER BY confidence DESC";

    let mut stmt = conn.prepare(query)?;
    let relationship_iter = stmt.query_map([entity_id], |row| {
        Ok(Relationship {
            id: row.get("id")?,
            subject_id: row.get("subject_id")?,
            predicate: row.get("predicate")?,
            object_type: row.get("object_type")?,
            object_value: row.get("object_value")?,
            confidence: row.get("confidence")?,
            extracted_at: chrono::DateTime::from_timestamp(row.get::<_, i64>("extracted_at")?, 0)
                .unwrap_or_else(chrono::Utc::now),
            metadata_json: row.get("metadata_json")?,
        })
    })?;

    let mut relationships = Vec::new();
    for relationship in relationship_iter {
        relationships.push(relationship?);
    }

    Ok(relationships)
}

/// Get entity sources for multiple entities
fn get_sources_for_entities(conn: &Connection, entity_ids: &[String]) -> Result<Vec<EntitySource>> {
    if entity_ids.is_empty() {
        return Ok(Vec::new());
    }

    // Build placeholders for IN clause
    let placeholders: Vec<String> = (0..entity_ids.len())
        .map(|i| format!("?{}", i + 1))
        .collect();
    let query = format!(
        "SELECT entity_id, content_id, l1_mention_id, added_at
         FROM entity_sources
         WHERE entity_id IN ({})
         ORDER BY added_at DESC",
        placeholders.join(",")
    );

    let mut stmt = conn.prepare(&query)?;

    // Convert entity_ids to rusqlite::types::Value for binding
    let params: Vec<&dyn rusqlite::ToSql> = entity_ids
        .iter()
        .map(|id| id as &dyn rusqlite::ToSql)
        .collect();

    let source_iter = stmt.query_map(&params[..], |row| {
        Ok(EntitySource {
            entity_id: row.get("entity_id")?,
            content_id: row.get("content_id")?,
            l1_mention_id: row.get("l1_mention_id")?,
            added_at: chrono::DateTime::from_timestamp(row.get::<_, i64>("added_at")?, 0)
                .unwrap_or_else(chrono::Utc::now),
        })
    })?;

    let mut sources = Vec::new();
    for source in source_iter {
        sources.push(source?);
    }

    Ok(sources)
}

/// Find shortest path between two entities
pub fn find_shortest_path(
    conn: &Connection,
    start_entity_id: &str,
    end_entity_id: &str,
    max_depth: u32,
) -> Result<Option<Vec<String>>> {
    if start_entity_id == end_entity_id {
        return Ok(Some(vec![start_entity_id.to_string()]));
    }

    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    let mut parent_map: HashMap<String, String> = HashMap::new();

    queue.push_back((start_entity_id.to_string(), 0));
    visited.insert(start_entity_id.to_string());

    while let Some((entity_id, depth)) = queue.pop_front() {
        if depth >= max_depth {
            continue;
        }

        let relationships = get_entity_relationships(conn, &entity_id)?;

        for relationship in relationships {
            let neighbor_id = if relationship.subject_id == entity_id {
                if relationship.object_type == "entity" {
                    Some(relationship.object_value.clone())
                } else {
                    None
                }
            } else if relationship.object_type == "entity" && relationship.object_value == entity_id
            {
                Some(relationship.subject_id.clone())
            } else {
                None
            };

            if let Some(neighbor) = neighbor_id {
                if neighbor == end_entity_id {
                    // Found the target, reconstruct path
                    let mut path = vec![neighbor];
                    let mut current = entity_id;

                    while let Some(parent) = parent_map.get(&current) {
                        path.push(parent.clone());
                        current = parent.clone();
                    }

                    path.reverse();
                    return Ok(Some(path));
                }

                if !visited.contains(&neighbor) {
                    visited.insert(neighbor.clone());
                    parent_map.insert(neighbor.clone(), entity_id.clone());
                    queue.push_back((neighbor, depth + 1));
                }
            }
        }
    }

    Ok(None) // No path found
}

/// Get all entities connected to a given entity (1-hop neighbors)
pub fn get_neighbors(conn: &Connection, entity_id: &str) -> Result<Vec<Entity>> {
    let relationships = get_entity_relationships(conn, entity_id)?;
    let mut neighbor_ids = HashSet::new();

    for relationship in relationships {
        if relationship.subject_id == entity_id && relationship.object_type == "entity" {
            neighbor_ids.insert(relationship.object_value);
        } else if relationship.object_type == "entity" && relationship.object_value == entity_id {
            neighbor_ids.insert(relationship.subject_id);
        }
    }

    let mut neighbors = Vec::new();
    for neighbor_id in neighbor_ids {
        if let Some(entity) = get_entity_by_id(conn, &neighbor_id)? {
            neighbors.push(entity);
        }
    }

    // Sort by source_count (most mentioned first)
    neighbors.sort_by(|a, b| b.source_count.cmp(&a.source_count));

    Ok(neighbors)
}

/// Get entities by type
pub fn get_entities_by_type(
    conn: &Connection,
    entity_type: &str,
    limit: u32,
) -> Result<Vec<Entity>> {
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

    let mut stmt = conn.prepare(query)?;
    let entity_iter = stmt.query_map([entity_type, &limit.to_string()], |row| {
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
            first_seen: chrono::DateTime::from_timestamp(row.get::<_, i64>("first_seen")?, 0)
                .unwrap_or_else(chrono::Utc::now),
            last_updated: chrono::DateTime::from_timestamp(row.get::<_, i64>("last_updated")?, 0)
                .unwrap_or_else(chrono::Utc::now),
            source_count: row.get("source_count")?,
            metadata_json: row.get("metadata_json")?,
            aliases,
        })
    })?;

    let mut entities = Vec::new();
    for entity in entity_iter {
        entities.push(entity?);
    }

    Ok(entities)
}
