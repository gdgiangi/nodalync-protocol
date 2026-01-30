//! Provenance graph storage.
//!
//! This module implements the provenance graph for tracking content derivation
//! relationships. The graph enables efficient traversal to find all root
//! contributors for revenue distribution.

use rusqlite::{params, Connection};
use std::collections::{HashSet, VecDeque};
use std::sync::{Arc, Mutex};

use nodalync_crypto::{Hash, PeerId};
use nodalync_types::{ProvenanceEntry, Visibility};

use crate::error::Result;
use crate::traits::ProvenanceGraph;

/// SQLite-based provenance graph.
///
/// Uses two tables:
/// - `derived_from`: Forward edges (content -> sources)
/// - `root_cache`: Cached flattened roots with weights
pub struct SqliteProvenanceGraph {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteProvenanceGraph {
    /// Create a new provenance graph with the given database connection.
    pub fn new(conn: Arc<Mutex<Connection>>) -> Self {
        Self { conn }
    }
}

impl ProvenanceGraph for SqliteProvenanceGraph {
    fn add(&mut self, hash: &Hash, derived_from: &[Hash]) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let hash_bytes = hash.0.to_vec();

        // Insert forward edges
        for source in derived_from {
            let source_bytes = source.0.to_vec();
            conn.execute(
                "INSERT OR IGNORE INTO derived_from (content_hash, source_hash) VALUES (?1, ?2)",
                params![hash_bytes, source_bytes],
            )?;
        }

        Ok(())
    }

    fn get_roots(&self, hash: &Hash) -> Result<Vec<ProvenanceEntry>> {
        let conn = self.conn.lock().unwrap();
        let hash_bytes = hash.0.to_vec();

        // First check the cache
        let mut stmt = conn.prepare(
            "SELECT root_hash, owner, visibility, weight FROM root_cache WHERE content_hash = ?1",
        )?;

        let cached: Vec<ProvenanceEntry> = stmt
            .query_map([&hash_bytes], |row| {
                let root_bytes: Vec<u8> = row.get(0)?;
                let owner_bytes: Vec<u8> = row.get(1)?;
                let visibility_u8: u8 = row.get(2)?;
                let weight: u32 = row.get(3)?;

                let root_hash = bytes_to_hash(&root_bytes);
                let owner = bytes_to_peer_id(&owner_bytes);
                let visibility = u8_to_visibility(visibility_u8);

                Ok(ProvenanceEntry {
                    hash: root_hash,
                    owner,
                    visibility,
                    weight,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        if !cached.is_empty() {
            return Ok(cached);
        }

        // If not cached, traverse the graph
        // For L0 content (no derived_from edges), it is its own root
        let sources = self.get_direct_sources(&conn, hash)?;
        if sources.is_empty() {
            // This is L0 content - return empty (caller should handle self-reference)
            return Ok(vec![]);
        }

        // For derived content, flatten all roots from sources
        let mut all_roots = Vec::new();
        let mut visited = HashSet::new();
        let mut queue: VecDeque<Hash> = sources.into_iter().collect();

        while let Some(current) = queue.pop_front() {
            if visited.contains(&current) {
                continue;
            }
            visited.insert(current);

            let current_sources = self.get_direct_sources(&conn, &current)?;
            if current_sources.is_empty() {
                // This is an L0 source - it's a root
                // We'd need manifest data to get owner/visibility
                // For now, return with placeholder
                all_roots.push(current);
            } else {
                // Add sources to queue
                for source in current_sources {
                    if !visited.contains(&source) {
                        queue.push_back(source);
                    }
                }
            }
        }

        // Convert to entries (note: this returns partial data without owner/visibility)
        // Full entries require cross-referencing with manifest store
        let entries: Vec<ProvenanceEntry> = all_roots
            .into_iter()
            .map(|h| ProvenanceEntry {
                hash: h,
                owner: PeerId::from_bytes([0u8; 20]), // Placeholder
                visibility: Visibility::Private,      // Placeholder
                weight: 1,
            })
            .collect();

        Ok(entries)
    }

    fn get_derivations(&self, hash: &Hash) -> Result<Vec<Hash>> {
        let conn = self.conn.lock().unwrap();
        let hash_bytes = hash.0.to_vec();

        let mut stmt =
            conn.prepare("SELECT content_hash FROM derived_from WHERE source_hash = ?1")?;

        let derivations: Vec<Hash> = stmt
            .query_map([hash_bytes], |row| {
                let bytes: Vec<u8> = row.get(0)?;
                Ok(bytes_to_hash(&bytes))
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(derivations)
    }

    fn is_ancestor(&self, ancestor: &Hash, descendant: &Hash) -> Result<bool> {
        if ancestor == descendant {
            return Ok(false); // A hash is not its own ancestor
        }

        let conn = self.conn.lock().unwrap();

        // BFS from descendant towards roots
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(*descendant);

        while let Some(current) = queue.pop_front() {
            if visited.contains(&current) {
                continue;
            }
            visited.insert(current);

            let sources = self.get_direct_sources(&conn, &current)?;
            for source in sources {
                if source == *ancestor {
                    return Ok(true);
                }
                if !visited.contains(&source) {
                    queue.push_back(source);
                }
            }
        }

        Ok(false)
    }

    fn cache_root(&mut self, content_hash: &Hash, entry: &ProvenanceEntry) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        let content_bytes = content_hash.0.to_vec();
        let root_bytes = entry.hash.0.to_vec();
        let owner_bytes = entry.owner.0.to_vec();
        let visibility = entry.visibility as u8;

        // Use INSERT OR REPLACE to handle duplicates by accumulating weight
        conn.execute(
            "INSERT INTO root_cache (content_hash, root_hash, owner, visibility, weight)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(content_hash, root_hash) DO UPDATE SET
             weight = weight + excluded.weight",
            params![
                content_bytes,
                root_bytes,
                owner_bytes,
                visibility,
                entry.weight
            ],
        )?;

        Ok(())
    }
}

impl SqliteProvenanceGraph {
    /// Get direct sources for a content hash.
    fn get_direct_sources(&self, conn: &Connection, hash: &Hash) -> Result<Vec<Hash>> {
        let hash_bytes = hash.0.to_vec();

        let mut stmt =
            conn.prepare("SELECT source_hash FROM derived_from WHERE content_hash = ?1")?;

        let sources: Vec<Hash> = stmt
            .query_map([hash_bytes], |row| {
                let bytes: Vec<u8> = row.get(0)?;
                Ok(bytes_to_hash(&bytes))
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(sources)
    }

    /// Clear the root cache for a content hash.
    pub fn clear_cache(&mut self, hash: &Hash) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let hash_bytes = hash.0.to_vec();
        conn.execute(
            "DELETE FROM root_cache WHERE content_hash = ?1",
            [hash_bytes],
        )?;
        Ok(())
    }

    /// Get cached roots with full data.
    pub fn get_cached_roots(&self, hash: &Hash) -> Result<Option<Vec<ProvenanceEntry>>> {
        let conn = self.conn.lock().unwrap();
        let hash_bytes = hash.0.to_vec();

        let mut stmt = conn.prepare(
            "SELECT root_hash, owner, visibility, weight FROM root_cache WHERE content_hash = ?1",
        )?;

        let entries: Vec<ProvenanceEntry> = stmt
            .query_map([&hash_bytes], |row| {
                let root_bytes: Vec<u8> = row.get(0)?;
                let owner_bytes: Vec<u8> = row.get(1)?;
                let visibility_u8: u8 = row.get(2)?;
                let weight: u32 = row.get(3)?;

                Ok(ProvenanceEntry {
                    hash: bytes_to_hash(&root_bytes),
                    owner: bytes_to_peer_id(&owner_bytes),
                    visibility: u8_to_visibility(visibility_u8),
                    weight,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        if entries.is_empty() {
            Ok(None)
        } else {
            Ok(Some(entries))
        }
    }
}

/// Convert bytes to Hash.
fn bytes_to_hash(bytes: &[u8]) -> Hash {
    let mut arr = [0u8; 32];
    if bytes.len() >= 32 {
        arr.copy_from_slice(&bytes[..32]);
    }
    Hash(arr)
}

/// Convert bytes to PeerId.
fn bytes_to_peer_id(bytes: &[u8]) -> PeerId {
    let mut arr = [0u8; 20];
    if bytes.len() >= 20 {
        arr.copy_from_slice(&bytes[..20]);
    }
    PeerId::from_bytes(arr)
}

/// Convert u8 to Visibility.
fn u8_to_visibility(v: u8) -> Visibility {
    match v {
        0 => Visibility::Private,
        1 => Visibility::Unlisted,
        2 => Visibility::Shared,
        _ => Visibility::Private,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::initialize_schema;
    use nodalync_crypto::{content_hash, generate_identity, peer_id_from_public_key};
    use rusqlite::Connection;

    fn setup_graph() -> SqliteProvenanceGraph {
        let conn = Connection::open_in_memory().unwrap();
        initialize_schema(&conn).unwrap();
        SqliteProvenanceGraph::new(Arc::new(Mutex::new(conn)))
    }

    #[test]
    fn test_add_and_get_derivations() {
        let mut graph = setup_graph();

        let source1 = content_hash(b"source1");
        let source2 = content_hash(b"source2");
        let derived = content_hash(b"derived");

        // Add L0 sources (no derivations)
        graph.add(&source1, &[]).unwrap();
        graph.add(&source2, &[]).unwrap();

        // Add derived content
        graph.add(&derived, &[source1, source2]).unwrap();

        // Check derivations from sources
        let d1 = graph.get_derivations(&source1).unwrap();
        assert_eq!(d1.len(), 1);
        assert_eq!(d1[0], derived);

        let d2 = graph.get_derivations(&source2).unwrap();
        assert_eq!(d2.len(), 1);
        assert_eq!(d2[0], derived);
    }

    #[test]
    fn test_is_ancestor() {
        let mut graph = setup_graph();

        let root = content_hash(b"root");
        let middle = content_hash(b"middle");
        let leaf = content_hash(b"leaf");

        graph.add(&root, &[]).unwrap();
        graph.add(&middle, &[root]).unwrap();
        graph.add(&leaf, &[middle]).unwrap();

        assert!(graph.is_ancestor(&root, &middle).unwrap());
        assert!(graph.is_ancestor(&root, &leaf).unwrap());
        assert!(graph.is_ancestor(&middle, &leaf).unwrap());

        assert!(!graph.is_ancestor(&leaf, &root).unwrap());
        assert!(!graph.is_ancestor(&middle, &root).unwrap());
        assert!(!graph.is_ancestor(&leaf, &middle).unwrap());

        // Not ancestor of self
        assert!(!graph.is_ancestor(&root, &root).unwrap());
    }

    #[test]
    fn test_get_roots_l0() {
        let mut graph = setup_graph();

        let l0 = content_hash(b"l0 content");
        graph.add(&l0, &[]).unwrap();

        // L0 content should return empty roots (caller handles self-reference)
        let roots = graph.get_roots(&l0).unwrap();
        assert!(roots.is_empty());
    }

    #[test]
    fn test_get_roots_derived() {
        let mut graph = setup_graph();

        let l0_1 = content_hash(b"l0_1");
        let l0_2 = content_hash(b"l0_2");
        let l3 = content_hash(b"l3");

        graph.add(&l0_1, &[]).unwrap();
        graph.add(&l0_2, &[]).unwrap();
        graph.add(&l3, &[l0_1, l0_2]).unwrap();

        let roots = graph.get_roots(&l3).unwrap();
        assert_eq!(roots.len(), 2);

        let root_hashes: HashSet<Hash> = roots.iter().map(|e| e.hash).collect();
        assert!(root_hashes.contains(&l0_1));
        assert!(root_hashes.contains(&l0_2));
    }

    #[test]
    fn test_cache_root() {
        let mut graph = setup_graph();

        let content = content_hash(b"content");
        let (_, public_key) = generate_identity();
        let owner = peer_id_from_public_key(&public_key);

        let entry = ProvenanceEntry::new(content_hash(b"root"), owner, Visibility::Shared);

        graph.cache_root(&content, &entry).unwrap();

        let cached = graph.get_cached_roots(&content).unwrap();
        assert!(cached.is_some());

        let cached = cached.unwrap();
        assert_eq!(cached.len(), 1);
        assert_eq!(cached[0].weight, 1);
    }

    #[test]
    fn test_cache_root_weight_accumulation() {
        let mut graph = setup_graph();

        let content = content_hash(b"content");
        let (_, public_key) = generate_identity();
        let owner = peer_id_from_public_key(&public_key);
        let root = content_hash(b"root");

        let entry1 = ProvenanceEntry::with_weight(root, owner, Visibility::Shared, 1);
        let entry2 = ProvenanceEntry::with_weight(root, owner, Visibility::Shared, 2);

        graph.cache_root(&content, &entry1).unwrap();
        graph.cache_root(&content, &entry2).unwrap();

        let cached = graph.get_cached_roots(&content).unwrap().unwrap();
        assert_eq!(cached.len(), 1);
        assert_eq!(cached[0].weight, 3); // 1 + 2 = 3
    }

    #[test]
    fn test_clear_cache() {
        let mut graph = setup_graph();

        let content = content_hash(b"content");
        let (_, public_key) = generate_identity();
        let owner = peer_id_from_public_key(&public_key);

        let entry = ProvenanceEntry::new(content_hash(b"root"), owner, Visibility::Shared);
        graph.cache_root(&content, &entry).unwrap();

        assert!(graph.get_cached_roots(&content).unwrap().is_some());

        graph.clear_cache(&content).unwrap();

        assert!(graph.get_cached_roots(&content).unwrap().is_none());
    }

    #[test]
    fn test_diamond_provenance() {
        let mut graph = setup_graph();

        //     root
        //    /    \
        //   a      b
        //    \    /
        //     leaf

        let root = content_hash(b"root");
        let a = content_hash(b"a");
        let b = content_hash(b"b");
        let leaf = content_hash(b"leaf");

        graph.add(&root, &[]).unwrap();
        graph.add(&a, &[root]).unwrap();
        graph.add(&b, &[root]).unwrap();
        graph.add(&leaf, &[a, b]).unwrap();

        // Root is ancestor of all
        assert!(graph.is_ancestor(&root, &a).unwrap());
        assert!(graph.is_ancestor(&root, &b).unwrap());
        assert!(graph.is_ancestor(&root, &leaf).unwrap());

        // Both a and b are ancestors of leaf
        assert!(graph.is_ancestor(&a, &leaf).unwrap());
        assert!(graph.is_ancestor(&b, &leaf).unwrap());

        // Leaf should have only 'root' as ultimate root
        let roots = graph.get_roots(&leaf).unwrap();
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].hash, root);
    }
}
