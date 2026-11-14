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
        check_build_source_count(source_l1_hashes.len())?;

        // 2. Load and verify all L1 sources
        let (source_manifests, l1_references) = self.load_l1_sources(&source_l1_hashes)?;

        // 3-5. Extract entities and relationships
        // For MVP, we create a simple graph with extracted mentions
        let entities = self.entities_from_l1_sources(&source_l1_hashes);

        // 6. Build L2EntityGraph structure
        // First, serialize to compute hash
        let temp_hash = content_hash(b"temp"); // Placeholder
        let mut graph = L2EntityGraph::new(temp_hash);
        graph.source_l1s = l1_references;
        graph.entities = entities;
        graph.sync_counts();

        // 7. Compute hash
        // The graph's id is included in serialization, so the content is serialized
        // with the placeholder id; graph.id is then set to the stored content's hash
        // for validation (validator checks graph.id == manifest.hash).
        let content = serialize_graph(&graph)?;

        // Build provenance from L1 sources
        let provenance_sources: Vec<_> = source_manifests
            .iter()
            .map(|m| (m.hash, &m.provenance, m.owner, m.visibility))
            .collect();
        let provenance = Provenance::from_sources(&provenance_sources);

        // 8-10. Create manifest, validate and store
        self.store_l2(
            &mut graph,
            &content,
            provenance,
            "L2 Entity Graph",
            &source_l1_hashes,
            timestamp,
        )
    }

    /// Load each L1 source with its L1 reference, in order.
    fn load_l1_sources(
        &self,
        source_l1_hashes: &[Hash],
    ) -> OpsResult<(Vec<Manifest>, Vec<L1Reference>)> {
        let mut source_manifests: Vec<Manifest> = Vec::new();
        let mut l1_references: Vec<L1Reference> = Vec::new();

        for l1_hash in source_l1_hashes {
            let (manifest, l1_reference) = self.load_l1_source(l1_hash)?;
            l1_references.push(l1_reference);
            source_manifests.push(manifest);
        }
        Ok((source_manifests, l1_references))
    }

    /// Load an L1 source, which must be L1 content we own or have queried.
    fn load_l1_source(&self, l1_hash: &Hash) -> OpsResult<(Manifest, L1Reference)> {
        let manifest = self
            .state
            .manifests
            .load(l1_hash)?
            .ok_or(OpsError::NotFound(*l1_hash))?;

        // Verify it's L1 content
        if manifest.content_type != ContentType::L1 {
            return Err(OpsError::invalid_operation(format!(
                "source {} is {:?} content, but build-l2 requires L1 hashes. \
                 Use 'nodalync preview <hash>' to see L1 mentions extracted from your L0 content, \
                 then pass those L1 hashes to build-l2.",
                l1_hash, manifest.content_type
            )));
        }

        // Verify ownership or previous query
        let is_owned = manifest.owner == self.peer_id();
        if !is_owned && !self.state.cache.is_cached(l1_hash) {
            return Err(OpsError::SourceNotQueried(*l1_hash));
        }

        // Get the L0 hash from provenance (first root entry for L1)
        let l0_hash = manifest
            .provenance
            .root_l0l1
            .first()
            .map(|e| e.hash)
            .unwrap_or(*l1_hash);

        Ok((manifest, L1Reference::new(*l1_hash, l0_hash)))
    }

    /// One entity per previewed mention of each source, plus a placeholder entity for
    /// a source that has not yet produced one; at least one entity overall.
    fn entities_from_l1_sources(&mut self, source_l1_hashes: &[Hash]) -> Vec<Entity> {
        let mut entities: Vec<Entity> = Vec::new();

        for (idx, l1_hash) in source_l1_hashes.iter().enumerate() {
            // Try to get L1 summary for extraction
            if let Ok(l1_summary) = self.extract_l1_summary(l1_hash) {
                push_mention_entities(&mut entities, &l1_summary);
            }
            // Also add a placeholder entity for the source itself
            if entities.is_empty() || entities.len() <= idx {
                let entity = Entity::new(
                    format!("e{}", entities.len()),
                    format!("Source {}", idx + 1),
                )
                .with_confidence(1.0);
                entities.push(entity);
            }
        }

        // Ensure we have at least one entity
        if entities.is_empty() {
            entities.push(Entity::new("e0", "Root Entity").with_confidence(1.0));
        }
        entities
    }

    /// Store an L2 graph's content and a Private, free manifest for it.
    ///
    /// Sets `graph.id` to the stored content hash, validates the graph against the
    /// manifest, then stores the manifest and records `sources` in the provenance graph.
    fn store_l2(
        &mut self,
        graph: &mut L2EntityGraph,
        content: &[u8],
        provenance: Provenance,
        title: &str,
        sources: &[Hash],
        timestamp: nodalync_crypto::Timestamp,
    ) -> OpsResult<Hash> {
        // Store content first (compute actual hash from serialized content)
        let stored_hash = self.state.content.store(content)?;

        // Update graph with the actual stored hash
        graph.id = stored_hash;

        // Create manifest with the stored hash
        let manifest = Manifest {
            hash: stored_hash,
            content_type: ContentType::L2,
            owner: self.peer_id(),
            version: Version::new_v1(stored_hash, timestamp),
            visibility: Visibility::Private, // L2 is ALWAYS Private
            access: Default::default(),
            metadata: Metadata::new(title, content.len() as u64),
            economics: Default::default(), // price = 0
            provenance,
            created_at: timestamp,
            updated_at: timestamp,
        };

        // Validate L2 content
        validate_l2_content(graph, &manifest)?;

        self.state.manifests.store(&manifest)?;
        self.state.provenance.add(&stored_hash, sources)?;

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
        check_merge_source_count(source_l2_hashes.len())?;

        // 2-3. Load all L2 sources and verify ownership
        let (source_graphs, source_manifests) = self.load_l2_sources(&source_l2_hashes)?;

        // 4. Unify prefix mappings (use default + any custom from sources)
        // 5-6. Merge entities and relationships
        let (merged_entities, entity_id_map) = merge_entities(&source_graphs);
        let merged_relationships = merge_relationships(&source_graphs, &entity_id_map);

        // 7. Deduplicate L1 refs
        let all_l1_refs = source_graphs
            .iter()
            .flat_map(|graph| graph.source_l1s.iter().cloned());

        // Build the merged graph
        let temp_hash = content_hash(b"temp");
        let mut graph = L2EntityGraph::new(temp_hash);
        graph.prefixes = nodalync_types::PrefixMap::default();
        graph.source_l1s = dedupe_l1_refs(all_l1_refs);
        graph.source_l2s = source_l2_hashes.clone();
        graph.entities = merged_entities;
        graph.relationships = merged_relationships;
        graph.sync_counts();

        // Serialize content for hashing and storage
        let content = serialize_graph(&graph)?;

        // 8. Compute provenance from source L2s
        let provenance = merged_provenance(&source_manifests, &source_l2_hashes);

        // 9-10. Create manifest, validate and store
        self.store_l2(
            &mut graph,
            &content,
            provenance,
            "Merged L2 Entity Graph",
            &source_l2_hashes,
            timestamp,
        )
    }

    /// Load each L2 source's graph and manifest, in order.
    fn load_l2_sources(
        &self,
        source_l2_hashes: &[Hash],
    ) -> OpsResult<(Vec<L2EntityGraph>, Vec<Manifest>)> {
        let mut source_graphs: Vec<L2EntityGraph> = Vec::new();
        let mut source_manifests: Vec<Manifest> = Vec::new();

        for l2_hash in source_l2_hashes {
            let manifest = self.load_owned_l2_manifest(l2_hash)?;
            source_graphs.push(self.load_l2_graph(l2_hash)?);
            source_manifests.push(manifest);
        }
        Ok((source_graphs, source_manifests))
    }

    /// Load the manifest of an L2 source, which must be L2 content we own.
    fn load_owned_l2_manifest(&self, l2_hash: &Hash) -> OpsResult<Manifest> {
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
        Ok(manifest)
    }

    /// Load and parse the stored graph of an L2 source.
    fn load_l2_graph(&self, l2_hash: &Hash) -> OpsResult<L2EntityGraph> {
        let content = self
            .state
            .content
            .load(l2_hash)?
            .ok_or(OpsError::NotFound(*l2_hash))?;

        serde_json::from_slice(&content)
            .map_err(|e| OpsError::invalid_operation(format!("failed to parse L2 graph: {}", e)))
    }
}

/// build_l2 takes between 1 and MAX_SOURCE_L1S_PER_L2 sources.
fn check_build_source_count(count: usize) -> OpsResult<()> {
    if count == 0 {
        return Err(OpsError::invalid_operation(
            "build_l2 requires at least one L1 source",
        ));
    }
    if count > MAX_SOURCE_L1S_PER_L2 {
        return Err(OpsError::invalid_operation(format!(
            "build_l2 has too many sources: {} exceeds maximum {}",
            count, MAX_SOURCE_L1S_PER_L2
        )));
    }
    Ok(())
}

/// merge_l2 takes between 2 and MAX_SOURCE_L2S_PER_MERGE sources.
fn check_merge_source_count(count: usize) -> OpsResult<()> {
    if count < 2 {
        return Err(OpsError::invalid_operation(
            "merge_l2 requires at least two L2 sources",
        ));
    }
    if count > MAX_SOURCE_L2S_PER_MERGE {
        return Err(OpsError::invalid_operation(format!(
            "merge_l2 has too many sources: {} exceeds maximum {}",
            count, MAX_SOURCE_L2S_PER_MERGE
        )));
    }
    Ok(())
}

/// Serialize an L2 graph to the JSON bytes that are hashed and stored.
fn serialize_graph(graph: &L2EntityGraph) -> OpsResult<Vec<u8>> {
    serde_json::to_vec(graph)
        .map_err(|e| OpsError::invalid_operation(format!("failed to serialize L2 graph: {}", e)))
}

/// Add an entity for each previewed mention, numbering IDs on from the entities so far.
fn push_mention_entities(entities: &mut Vec<Entity>, l1_summary: &nodalync_types::L1Summary) {
    for mention in &l1_summary.preview_mentions {
        // Each unique mention becomes an entity
        let entity = Entity::new(
            format!("e{}", entities.len()),
            entity_label(&mention.content),
        )
        .with_confidence(0.8);
        entities.push(entity);
    }
}

/// A mention's text as an entity label, truncated to MAX_CANONICAL_LABEL_LENGTH (200 bytes)
/// as its first 197 characters plus "...".
fn entity_label(content: &str) -> String {
    if content.len() > 200 {
        let truncated: String = content.chars().take(197).collect();
        format!("{}...", truncated)
    } else {
        content.to_string()
    }
}

/// Every entity of every graph, renumbered e0, e1, … in order, and a map from
/// "<graph index>:<old id>" to the new ID.
fn merge_entities(
    source_graphs: &[L2EntityGraph],
) -> (Vec<Entity>, std::collections::HashMap<String, String>) {
    let mut merged_entities: Vec<Entity> = Vec::new();
    let mut entity_id_map: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();

    for (graph_idx, graph) in source_graphs.iter().enumerate() {
        for entity in &graph.entities {
            // Create new ID to avoid conflicts
            let new_id = format!("e{}", merged_entities.len());
            entity_id_map.insert(format!("{}:{}", graph_idx, entity.id), new_id.clone());

            let mut merged_entity = entity.clone();
            merged_entity.id = new_id;
            merged_entities.push(merged_entity);
        }
    }
    (merged_entities, entity_id_map)
}

/// Every relationship of every graph, renumbered r0, r1, … in order, with entity
/// references mapped through `entity_id_map` (unmapped references are kept as they are).
fn merge_relationships(
    source_graphs: &[L2EntityGraph],
    entity_id_map: &std::collections::HashMap<String, String>,
) -> Vec<nodalync_types::Relationship> {
    let mut merged_relationships: Vec<nodalync_types::Relationship> = Vec::new();

    for (graph_idx, graph) in source_graphs.iter().enumerate() {
        for rel in &graph.relationships {
            let new_id = format!("r{}", merged_relationships.len());
            let map_id = |id: &String| {
                entity_id_map
                    .get(&format!("{}:{}", graph_idx, id))
                    .cloned()
                    .unwrap_or_else(|| id.clone())
            };

            // Map subject and object entity IDs
            let new_subject = map_id(&rel.subject);
            let new_object = match &rel.object {
                nodalync_types::RelationshipObject::Entity { entity_id } => {
                    nodalync_types::RelationshipObject::entity(map_id(entity_id))
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
        }
    }
    merged_relationships
}

/// L1 references with repeated L1 hashes removed, keeping the first of each.
fn dedupe_l1_refs(l1_refs: impl IntoIterator<Item = L1Reference>) -> Vec<L1Reference> {
    let mut seen_l1_hashes: std::collections::HashSet<Hash> = std::collections::HashSet::new();
    l1_refs
        .into_iter()
        .filter(|l1_ref| seen_l1_hashes.insert(l1_ref.l1_hash))
        .collect()
}

/// Provenance of a merged L2: the sources' roots, with weights of repeated roots summed,
/// derived from `source_l2_hashes`, one level deeper than the deepest source.
fn merged_provenance(source_manifests: &[Manifest], source_l2_hashes: &[Hash]) -> Provenance {
    // For merged L2s, we collect all the root_l0l1 entries from sources
    let mut merged_roots: Vec<ProvenanceEntry> = Vec::new();
    for root in source_manifests
        .iter()
        .flat_map(|m| m.provenance.root_l0l1.iter())
    {
        // Check if already exists and accumulate weight
        match merged_roots.iter_mut().find(|r| r.hash == root.hash) {
            Some(existing) => existing.weight += root.weight,
            None => merged_roots.push(root.clone()),
        }
    }

    let max_depth = source_manifests
        .iter()
        .map(|m| m.provenance.depth)
        .max()
        .unwrap_or(0);

    Provenance {
        root_l0l1: merged_roots,
        derived_from: source_l2_hashes.to_vec(),
        depth: max_depth + 1,
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

    /// Store `content` and relabel it as an L1 we own, so it can feed build_l2.
    fn create_l1(ops: &mut DefaultNodeOperations, content: &[u8], title: &str) -> Hash {
        let hash = ops
            .create_content_with_timestamp(
                content,
                Metadata::new(title, content.len() as u64),
                1000,
            )
            .unwrap();
        let mut manifest = ops.state.manifests.load(&hash).unwrap().unwrap();
        manifest.content_type = ContentType::L1;
        ops.state.manifests.update(&manifest).unwrap();
        hash
    }

    /// Build two L2s and merge them; returns (l2_a, l2_b, merged).
    fn build_and_merge(ops: &mut DefaultNodeOperations) -> (Hash, Hash, Hash) {
        let l1_a = create_l1(
            ops,
            b"We found that Alice Smith leads the Paris lab. Results show 40% growth.",
            "A",
        );
        let l1_b = create_l1(
            ops,
            b"Researchers claim Bob Jones measured the effect. The definition refers to scope.",
            "B",
        );
        let l2_a = ops.build_l2_with_timestamp(vec![l1_a], None, 2000).unwrap();
        let l2_b = ops
            .build_l2_with_timestamp(vec![l1_a, l1_b], None, 3000)
            .unwrap();
        let merged = ops
            .merge_l2_with_timestamp(vec![l2_a, l2_b], None, 4000)
            .unwrap();
        (l2_a, l2_b, merged)
    }

    #[test]
    fn test_build_and_merge_l2_content_is_unchanged() {
        // Hashes recorded before build_l2/merge_l2 were split into helpers: the stored
        // graph bytes, and so the hashes, must not change.
        let (mut ops, _temp) = create_test_ops();
        let (a, b, m) = build_and_merge(&mut ops);
        assert_eq!(
            a.to_string(),
            "bbb0f2102ea2de964a95e67e0f4763a3834c0cddf8bcc014ffcf01236bb5c60d"
        );
        assert_eq!(
            b.to_string(),
            "ae5cc8c4ca2afda5ffd13250d5d630d85035fc9e85e4a18e230a0423ef713b02"
        );
        assert_eq!(
            m.to_string(),
            "1fc53db9753a0dfde39923caf760942e0a39775ec0d5ade81b92f9f6d815b272"
        );

        let merged = ops.state.manifests.load(&m).unwrap().unwrap();
        let roots: Vec<(String, u32)> = merged
            .provenance
            .root_l0l1
            .iter()
            .map(|r| (r.hash.to_string(), r.weight))
            .collect();
        assert_eq!(
            roots,
            vec![
                (
                    "7ec74f48b9ba04c0ecb65fa9c8fc1ba7fd33f368f5f41ba8beb38afd617fb435".to_string(),
                    4
                ),
                (
                    "df5da93e59631173f89019fddf6fde56caf072e8ff449016a3fa90788d1e44e5".to_string(),
                    2
                ),
            ]
        );
        assert_eq!(merged.provenance.depth, 2);
        assert_eq!(merged.provenance.derived_from, vec![a, b]);
        assert_eq!(merged.visibility, Visibility::Private);
        assert_eq!(merged.economics.price, 0);
    }

    #[test]
    fn test_entity_label_truncates_long_mentions() {
        assert_eq!(entity_label("short"), "short");
        let long = "x".repeat(250);
        let label = entity_label(&long);
        assert_eq!(label.len(), 200);
        assert!(label.ends_with("..."));
    }
}
