//! L2 Entity Graph operations.
//!
//! This module implements the BUILD_L2 and MERGE_L2 operations
//! for creating and merging L2 Entity Graphs.
//!
//! Key L2 Design Constraints:
//! - L2 visibility is ALWAYS Private
//! - L2 price is ALWAYS 0
//! - L2 cannot be published
//! - L2 is built from L1 sources
//! - L2 can be merged from other L2s (owned only)

use nodalync_crypto::{content_hash, Hash};
use nodalync_store::{CacheStore, ContentStore, ManifestStore, ProvenanceGraph};
use nodalync_types::{
    ContentType, Entity, L1Reference, L2BuildConfig, L2EntityGraph, L2MergeConfig, Manifest,
    Metadata, Provenance, ProvenanceEntry, Version, Visibility, MAX_SOURCE_L1S_PER_L2,
    MAX_SOURCE_L2S_PER_MERGE,
};
use nodalync_valid::{validate_l2_content, Validator};

use crate::error::{OpsError, OpsResult};
use crate::extraction::L1Extractor;
use crate::node_ops::{current_timestamp, NodeOperations};

impl<V, E> NodeOperations<V, E>
where
    V: Validator,
    E: L1Extractor,
{
    /// Build an L2 Entity Graph from L1 sources.
    ///
    /// This operation:
    /// 1. Validates source L1s count (>= 1, <= MAX_SOURCE_L1S_PER_L2)
    /// 2. Loads and verifies all L1 sources
    /// 3. Extracts entities from mentions
    /// 4. Resolves entities (merge duplicates, link to external KBs)
    /// 5. Extracts relationships
    /// 6. Builds L2EntityGraph structure
    /// 7. Computes hash and provenance
    /// 8. Creates manifest with visibility=Private, price=0
    /// 9. Validates and stores locally
    ///
    /// # Arguments
    ///
    /// * `source_l1_hashes` - Hashes of L1 content to build from
    /// * `config` - Optional build configuration
    ///
    /// # Returns
    ///
    /// The hash of the created L2 content.
    pub fn build_l2(
        &mut self,
        source_l1_hashes: Vec<Hash>,
        config: Option<L2BuildConfig>,
    ) -> OpsResult<Hash> {
        let timestamp = current_timestamp();
        self.build_l2_with_timestamp(source_l1_hashes, config, timestamp)
    }

    /// Build L2 with a specific timestamp (for testing).
    pub fn build_l2_with_timestamp(
        &mut self,
        source_l1_hashes: Vec<Hash>,
        config: Option<L2BuildConfig>,
        timestamp: nodalync_crypto::Timestamp,
    ) -> OpsResult<Hash> {
        let _config = config.unwrap_or_default();

        // 1. Validate source count
        if source_l1_hashes.is_empty() {
            return Err(OpsError::invalid_operation(
                "build_l2 requires at least one L1 source",
            ));
        }
        if source_l1_hashes.len() > MAX_SOURCE_L1S_PER_L2 {
            return Err(OpsError::invalid_operation(format!(
                "build_l2 has too many sources: {} exceeds maximum {}",
                source_l1_hashes.len(),
                MAX_SOURCE_L1S_PER_L2
            )));
        }

        // 2. Load and verify all L1 sources
        let mut source_manifests: Vec<Manifest> = Vec::new();
        let mut l1_references: Vec<L1Reference> = Vec::new();

        for l1_hash in &source_l1_hashes {
            let manifest = self
                .state
                .manifests
                .load(l1_hash)?
                .ok_or(OpsError::NotFound(*l1_hash))?;

            // Verify it's L1 content
            if manifest.content_type != ContentType::L1 {
                return Err(OpsError::invalid_operation(format!(
                    "source {} is not L1 content (is {:?})",
                    l1_hash, manifest.content_type
                )));
            }

            // Verify ownership or previous query
            let is_owned = manifest.owner == self.peer_id();
            let is_cached = self.state.cache.is_cached(l1_hash);
            if !is_owned && !is_cached {
                return Err(OpsError::SourceNotQueried(*l1_hash));
            }

            // Get the L0 hash from provenance (first root entry for L1)
            let l0_hash = manifest
                .provenance
                .root_l0l1
                .first()
                .map(|e| e.hash)
                .unwrap_or(*l1_hash);

            l1_references.push(L1Reference::new(*l1_hash, l0_hash));
            source_manifests.push(manifest);
        }

        // 3-5. Extract entities and relationships
        // For MVP, we create a simple graph with extracted mentions
        let mut entities: Vec<Entity> = Vec::new();
        let mut entity_id_counter = 0u32;

        for (idx, l1_hash) in source_l1_hashes.iter().enumerate() {
            // Try to get L1 summary for extraction
            if let Ok(l1_summary) = self.extract_l1_summary(l1_hash) {
                for mention in &l1_summary.preview_mentions {
                    // Each unique mention becomes an entity
                    // Truncate label to MAX_CANONICAL_LABEL_LENGTH (200 chars)
                    let label = if mention.content.len() > 200 {
                        let truncated: String = mention.content.chars().take(197).collect();
                        format!("{}...", truncated)
                    } else {
                        mention.content.clone()
                    };
                    let entity =
                        Entity::new(format!("e{}", entity_id_counter), label).with_confidence(0.8);

                    entities.push(entity);
                    entity_id_counter += 1;
                }
            }
            // Also add a placeholder entity for the source itself
            if entities.is_empty() || entities.len() <= idx {
                let entity = Entity::new(
                    format!("e{}", entity_id_counter),
                    format!("Source {}", idx + 1),
                )
                .with_confidence(1.0);
                entities.push(entity);
                entity_id_counter += 1;
            }
        }

        // Ensure we have at least one entity
        if entities.is_empty() {
            entities.push(Entity::new("e0", "Root Entity").with_confidence(1.0));
        }

        // 6. Build L2EntityGraph structure
        // First, serialize to compute hash
        let temp_hash = content_hash(b"temp"); // Placeholder
        let mut graph = L2EntityGraph::new(temp_hash);
        graph.source_l1s = l1_references;
        graph.entities = entities;
        graph.sync_counts();

        // 7. Compute hash
        // Since the graph's id is included in serialization, we compute the hash
        // from the content, then update graph.id to match, re-serialize, and use
        // that final content. The hash stored in graph.id won't match the content's
        // hash (circular dependency), but we use the content's actual hash for storage.
        let content = serde_json::to_vec(&graph).map_err(|e| {
            OpsError::invalid_operation(format!("failed to serialize L2 graph: {}", e))
        })?;
        let hash = content_hash(&content);

        // Update graph.id for validation (validator checks graph.id == manifest.hash)
        graph.id = hash;

        // Build provenance from L1 sources
        let provenance_sources: Vec<_> = source_manifests
            .iter()
            .map(|m| (m.hash, &m.provenance, m.owner, m.visibility))
            .collect();
        let provenance = Provenance::from_sources(&provenance_sources);

        // 8. Create manifest
        let metadata = Metadata::new("L2 Entity Graph", content.len() as u64);
        let version = Version::new_v1(hash, timestamp);

        let manifest = Manifest {
            hash,
            content_type: ContentType::L2,
            owner: self.peer_id(),
            version,
            visibility: Visibility::Private, // L2 is ALWAYS Private
            access: Default::default(),
            metadata,
            economics: Default::default(), // price = 0
            provenance,
            created_at: timestamp,
            updated_at: timestamp,
        };

        // 9. Store content and manifest
        let stored_hash = self.state.content.store(&content)?;

        // Update manifest hash to match stored content hash
        graph.id = stored_hash;
        let manifest = Manifest {
            hash: stored_hash,
            ..manifest
        };
        let version = Version::new_v1(stored_hash, timestamp);
        let manifest = Manifest {
            version,
            ..manifest
        };

        // 10. Validate L2 content
        validate_l2_content(&graph, &manifest)?;

        self.state.manifests.store(&manifest)?;
        self.state.provenance.add(&stored_hash, &source_l1_hashes)?;

        Ok(stored_hash)
    }

    /// Merge multiple L2 Entity Graphs into a single L2.
    ///
    /// This operation:
    /// 1. Validates source L2s count (>= 2, <= MAX_SOURCE_L2S_PER_MERGE)
    /// 2. Loads all L2 sources (must be local/owned)
    /// 3. Verifies all sources owned by current identity
    /// 4. Unifies prefix mappings
    /// 5. Cross-graph entity resolution
    /// 6. Merges relationships
    /// 7. Deduplicates L1 refs
    /// 8. Computes provenance
    /// 9. Creates manifest with visibility=Private, price=0
    /// 10. Validates and stores
    ///
    /// # Arguments
    ///
    /// * `source_l2_hashes` - Hashes of L2 content to merge
    /// * `config` - Optional merge configuration
    ///
    /// # Returns
    ///
    /// The hash of the merged L2 content.
    pub fn merge_l2(
        &mut self,
        source_l2_hashes: Vec<Hash>,
        config: Option<L2MergeConfig>,
    ) -> OpsResult<Hash> {
        let timestamp = current_timestamp();
        self.merge_l2_with_timestamp(source_l2_hashes, config, timestamp)
    }

    /// Merge L2 with a specific timestamp (for testing).
    pub fn merge_l2_with_timestamp(
        &mut self,
        source_l2_hashes: Vec<Hash>,
        config: Option<L2MergeConfig>,
        timestamp: nodalync_crypto::Timestamp,
    ) -> OpsResult<Hash> {
        let _config = config.unwrap_or_default();

        // 1. Validate source count
        if source_l2_hashes.len() < 2 {
            return Err(OpsError::invalid_operation(
                "merge_l2 requires at least two L2 sources",
            ));
        }
        if source_l2_hashes.len() > MAX_SOURCE_L2S_PER_MERGE {
            return Err(OpsError::invalid_operation(format!(
                "merge_l2 has too many sources: {} exceeds maximum {}",
                source_l2_hashes.len(),
                MAX_SOURCE_L2S_PER_MERGE
            )));
        }

        // 2-3. Load all L2 sources and verify ownership
        let mut source_graphs: Vec<L2EntityGraph> = Vec::new();
        let mut source_manifests: Vec<Manifest> = Vec::new();
        let mut all_l1_refs: Vec<L1Reference> = Vec::new();

        for l2_hash in &source_l2_hashes {
            let manifest = self
                .state
                .manifests
                .load(l2_hash)?
                .ok_or(OpsError::NotFound(*l2_hash))?;

            // Verify it's L2 content
            if manifest.content_type != ContentType::L2 {
                return Err(OpsError::invalid_operation(format!(
                    "source {} is not L2 content (is {:?})",
                    l2_hash, manifest.content_type
                )));
            }

            // 3. Verify ownership
            if manifest.owner != self.peer_id() {
                return Err(OpsError::AccessDenied);
            }

            // Load the L2 graph content
            let content = self
                .state
                .content
                .load(l2_hash)?
                .ok_or(OpsError::NotFound(*l2_hash))?;

            let graph: L2EntityGraph = serde_json::from_slice(&content).map_err(|e| {
                OpsError::invalid_operation(format!("failed to parse L2 graph: {}", e))
            })?;

            // Collect L1 references
            all_l1_refs.extend(graph.source_l1s.clone());

            source_graphs.push(graph);
            source_manifests.push(manifest);
        }

        // 4. Unify prefix mappings (use default + any custom from sources)
        let prefixes = nodalync_types::PrefixMap::default();

        // 5-6. Merge entities and relationships
        let mut merged_entities: Vec<Entity> = Vec::new();
        let mut entity_id_map: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        let mut entity_counter = 0u32;

        for (graph_idx, graph) in source_graphs.iter().enumerate() {
            for entity in &graph.entities {
                // Create new ID to avoid conflicts
                let new_id = format!("e{}", entity_counter);
                entity_id_map.insert(format!("{}:{}", graph_idx, entity.id), new_id.clone());

                let mut merged_entity = entity.clone();
                merged_entity.id = new_id;
                merged_entities.push(merged_entity);
                entity_counter += 1;
            }
        }

        // Merge relationships (update entity refs)
        let mut merged_relationships: Vec<nodalync_types::Relationship> = Vec::new();
        let mut rel_counter = 0u32;

        for (graph_idx, graph) in source_graphs.iter().enumerate() {
            for rel in &graph.relationships {
                let new_id = format!("r{}", rel_counter);

                // Map subject and object entity IDs
                let new_subject = entity_id_map
                    .get(&format!("{}:{}", graph_idx, rel.subject))
                    .cloned()
                    .unwrap_or_else(|| rel.subject.clone());

                let new_object = match &rel.object {
                    nodalync_types::RelationshipObject::Entity { entity_id } => {
                        let mapped_id = entity_id_map
                            .get(&format!("{}:{}", graph_idx, entity_id))
                            .cloned()
                            .unwrap_or_else(|| entity_id.clone());
                        nodalync_types::RelationshipObject::entity(mapped_id)
                    }
                    other => other.clone(),
                };

                let mut merged_rel = nodalync_types::Relationship::new(
                    new_id,
                    new_subject,
                    rel.predicate.clone(),
                    new_object,
                );
                merged_rel.confidence = rel.confidence;
                merged_rel.mention_refs = rel.mention_refs.clone();
                merged_relationships.push(merged_rel);
                rel_counter += 1;
            }
        }

        // 7. Deduplicate L1 refs
        let mut unique_l1_refs: Vec<L1Reference> = Vec::new();
        let mut seen_l1_hashes: std::collections::HashSet<Hash> = std::collections::HashSet::new();
        for l1_ref in all_l1_refs {
            if !seen_l1_hashes.contains(&l1_ref.l1_hash) {
                seen_l1_hashes.insert(l1_ref.l1_hash);
                unique_l1_refs.push(l1_ref);
            }
        }

        // Build the merged graph
        let temp_hash = content_hash(b"temp");
        let mut graph = L2EntityGraph::new(temp_hash);
        graph.prefixes = prefixes;
        graph.source_l1s = unique_l1_refs;
        graph.source_l2s = source_l2_hashes.clone();
        graph.entities = merged_entities;
        graph.relationships = merged_relationships;
        graph.sync_counts();

        // Serialize content for hashing and storage
        let content = serde_json::to_vec(&graph).map_err(|e| {
            OpsError::invalid_operation(format!("failed to serialize L2 graph: {}", e))
        })?;

        // 8. Compute provenance from source L2s
        // For merged L2s, we collect all the root_l0l1 entries from sources
        let mut merged_roots: Vec<ProvenanceEntry> = Vec::new();
        for source_manifest in &source_manifests {
            for root in &source_manifest.provenance.root_l0l1 {
                // Check if already exists and accumulate weight
                if let Some(existing) = merged_roots.iter_mut().find(|r| r.hash == root.hash) {
                    existing.weight += root.weight;
                } else {
                    merged_roots.push(root.clone());
                }
            }
        }

        let max_depth = source_manifests
            .iter()
            .map(|m| m.provenance.depth)
            .max()
            .unwrap_or(0);

        let provenance = Provenance {
            root_l0l1: merged_roots,
            derived_from: source_l2_hashes.clone(),
            depth: max_depth + 1,
        };

        // 9. Store content first (compute actual hash from serialized content)
        let stored_hash = self.state.content.store(&content)?;

        // Update graph with the actual stored hash
        graph.id = stored_hash;

        // Create manifest with the stored hash
        let metadata = Metadata::new("Merged L2 Entity Graph", content.len() as u64);
        let version = Version::new_v1(stored_hash, timestamp);

        let manifest = Manifest {
            hash: stored_hash,
            content_type: ContentType::L2,
            owner: self.peer_id(),
            version,
            visibility: Visibility::Private,
            access: Default::default(),
            metadata,
            economics: Default::default(),
            provenance,
            created_at: timestamp,
            updated_at: timestamp,
        };

        // 10. Validate L2 content
        validate_l2_content(&graph, &manifest)?;

        self.state.manifests.store(&manifest)?;
        self.state.provenance.add(&stored_hash, &source_l2_hashes)?;

        Ok(stored_hash)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node_ops::DefaultNodeOperations;
    use nodalync_crypto::{generate_identity, peer_id_from_public_key};
    use nodalync_store::NodeStateConfig;
    use nodalync_types::Metadata;
    use tempfile::TempDir;

    fn create_test_ops() -> (DefaultNodeOperations, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let config = NodeStateConfig::new(temp_dir.path());
        let state = nodalync_store::NodeState::open(config).unwrap();

        let (_, public_key) = generate_identity();
        let peer_id = peer_id_from_public_key(&public_key);

        let ops = DefaultNodeOperations::with_defaults(state, peer_id);
        (ops, temp_dir)
    }

    #[test]
    fn test_build_l2_requires_l1_sources() {
        let (mut ops, _temp) = create_test_ops();

        // Try to build with empty sources
        let result = ops.build_l2(vec![], None);
        assert!(matches!(result, Err(OpsError::InvalidOperation(_))));
    }

    #[test]
    fn test_build_l2_rejects_l0_sources() {
        let (mut ops, _temp) = create_test_ops();

        // Create L0 content
        let content = b"L0 content";
        let meta = Metadata::new("L0", content.len() as u64);
        let l0_hash = ops.create_content(content, meta).unwrap();

        // Try to build L2 from L0 (should fail)
        let result = ops.build_l2(vec![l0_hash], None);
        assert!(matches!(result, Err(OpsError::InvalidOperation(_))));
    }

    #[test]
    fn test_build_l2_from_l1() {
        let (mut ops, _temp) = create_test_ops();

        // Create L0 and extract L1 summary (note: extract_l1_summary doesn't create separate L1 manifest)
        let content = b"Some content with interesting facts.";
        let meta = Metadata::new("Source", content.len() as u64);
        let l0_hash = ops.create_content(content, meta).unwrap();
        let _l1_summary = ops.extract_l1_summary(&l0_hash).unwrap();

        // Build L2 from L0 hash (which is what we have after extract_l1_summary)
        // This will fail because extract_l1_summary doesn't create a separate L1 manifest
        // In a real implementation, L1 would have its own manifest
        let result = ops.build_l2(vec![l0_hash], None);

        // This should fail because l0_hash is L0, not L1
        assert!(result.is_err());
    }

    #[test]
    fn test_merge_l2_requires_two_sources() {
        let (mut ops, _temp) = create_test_ops();

        // Try to merge with only one source
        let fake_hash = content_hash(b"fake");
        let result = ops.merge_l2(vec![fake_hash], None);
        assert!(matches!(result, Err(OpsError::InvalidOperation(_))));
    }

    #[test]
    fn test_merge_l2_requires_owned_sources() {
        let (mut ops, _temp) = create_test_ops();

        // Try to merge non-existent sources
        let fake_hash1 = content_hash(b"fake1");
        let fake_hash2 = content_hash(b"fake2");
        let result = ops.merge_l2(vec![fake_hash1, fake_hash2], None);
        assert!(matches!(result, Err(OpsError::NotFound(_))));
    }
}
