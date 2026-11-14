//! Content operations implementation.
//!
//! This module implements content creation, update, derive, and reference operations
//! as specified in Protocol Specification §7.1.

use nodalync_crypto::{content_hash, Hash, PeerId, Timestamp};
use nodalync_store::{CacheStore, ContentStore, ManifestStore, ProvenanceGraph};
use nodalync_types::{ContentType, Manifest, Metadata, Provenance, Version, Visibility};
use nodalync_valid::Validator;

use crate::error::{OpsError, OpsResult};
use crate::extraction::L1Extractor;
use crate::node_ops::{current_timestamp, NodeOperations};

impl<V, E> NodeOperations<V, E>
where
    V: Validator,
    E: L1Extractor,
{
    /// Create new L0 content.
    ///
    /// Spec §7.1.1:
    /// 1. Computes content hash
    /// 2. Creates v1 Version
    /// 3. Creates L0 Provenance (self-referential)
    /// 4. Sets owner to creator
    /// 5. Creates Manifest
    /// 6. Validates content
    /// 7. Stores content and manifest
    pub fn create_content(&mut self, content: &[u8], metadata: Metadata) -> OpsResult<Hash> {
        let timestamp = current_timestamp();
        self.create_content_with_timestamp(content, metadata, timestamp)
    }

    /// Create content with a specific timestamp (for testing).
    pub fn create_content_with_timestamp(
        &mut self,
        content: &[u8],
        metadata: Metadata,
        timestamp: Timestamp,
    ) -> OpsResult<Hash> {
        // 1. Compute content hash
        let hash = content_hash(content);

        // 2. Create v1 Version
        let version = Version::new_v1(hash, timestamp);

        // 3. Create L0 Provenance (self-referential)
        let provenance = Provenance::new_l0(hash, self.peer_id());

        // 4-5. Create Manifest with owner set to creator
        let manifest = Manifest {
            hash,
            content_type: ContentType::L0,
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

        // 6. Validate content
        self.validator.validate_content(content, &manifest)?;
        self.validator.validate_version(&manifest, None)?;
        self.validator.validate_provenance(&manifest, &[])?;

        // 7. Store content and manifest, and add to provenance graph
        self.store_new_content(&hash, content, &manifest, &[])?;

        Ok(hash)
    }

    /// Update existing content.
    ///
    /// Spec §7.1.4:
    /// 1. Computes new hash
    /// 2. Links version (previous, root from previous.root)
    /// 3. Inherits visibility
    /// 4. Stores
    pub fn update_content(
        &mut self,
        old_hash: &Hash,
        new_content: &[u8],
        new_metadata: Metadata,
    ) -> OpsResult<Hash> {
        let timestamp = current_timestamp();
        self.update_content_with_timestamp(old_hash, new_content, new_metadata, timestamp)
    }

    /// Update content with a specific timestamp (for testing).
    pub fn update_content_with_timestamp(
        &mut self,
        old_hash: &Hash,
        new_content: &[u8],
        new_metadata: Metadata,
        timestamp: Timestamp,
    ) -> OpsResult<Hash> {
        // Load the previous manifest
        let old_manifest = self
            .state
            .manifests
            .load(old_hash)?
            .ok_or(OpsError::ManifestNotFound(*old_hash))?;

        // Compute new hash
        let new_hash = content_hash(new_content);

        // Create version linked to previous
        let new_version = Version::new_from_previous(&old_manifest.version, *old_hash, timestamp);

        // Inherit provenance (for L0, create new L0 provenance with same structure)
        let new_provenance = updated_provenance(&old_manifest, old_hash, new_hash, self.peer_id());

        // Create new manifest inheriting visibility
        let new_manifest = Manifest {
            hash: new_hash,
            content_type: old_manifest.content_type,
            owner: self.peer_id(),
            version: new_version,
            visibility: old_manifest.visibility,
            access: old_manifest.access.clone(),
            metadata: new_metadata,
            economics: Default::default(), // Reset economics for new version
            provenance: new_provenance,
            created_at: timestamp,
            updated_at: timestamp,
        };

        // Validate
        self.validator
            .validate_content(new_content, &new_manifest)?;
        self.validator
            .validate_version(&new_manifest, Some(&old_manifest))?;

        // Store, and update provenance graph
        self.store_new_content(&new_hash, new_content, &new_manifest, &[*old_hash])?;

        Ok(new_hash)
    }

    /// Derive new content from sources.
    ///
    /// Spec §7.1.5:
    /// 1. Verifies all sources were queried
    /// 2. Loads source manifests
    /// 3. Merges root_L0L1 with weight accumulation
    /// 4. Calculates depth = max(sources.depth) + 1
    /// 5. Creates L3 manifest with provenance
    /// 6. Validates provenance
    /// 7. Stores
    pub fn derive_content(
        &mut self,
        sources: &[Hash],
        insight: &[u8],
        metadata: Metadata,
    ) -> OpsResult<Hash> {
        let timestamp = current_timestamp();
        self.derive_content_with_timestamp(sources, insight, metadata, timestamp)
    }

    /// Derive content with a specific timestamp (for testing).
    pub fn derive_content_with_timestamp(
        &mut self,
        sources: &[Hash],
        insight: &[u8],
        metadata: Metadata,
        timestamp: Timestamp,
    ) -> OpsResult<Hash> {
        if sources.is_empty() {
            return Err(OpsError::invalid_operation(
                "derive requires at least one source",
            ));
        }

        // 1. Verify all sources were queried (in cache or owned)
        // Note: L2 sources are special - they can only be used if owned (never queried)
        self.verify_sources_queried(sources)?;

        // 2. Load source manifests
        let source_data = self.load_source_manifests(sources)?;

        // 3-4. Build provenance from sources
        let provenance_sources: Vec<_> = source_data
            .iter()
            .map(|(hash, m)| (*hash, &m.provenance, m.owner, m.visibility))
            .collect();

        let provenance = Provenance::from_sources(&provenance_sources);

        // Compute content hash
        let hash = content_hash(insight);

        // Create version
        let version = Version::new_v1(hash, timestamp);

        // 5. Create L3 manifest
        let manifest = Manifest {
            hash,
            content_type: ContentType::L3,
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

        // 6. Validate provenance
        self.validate_derived(&manifest, insight, &source_data)?;

        // 7. Store
        self.store_new_content(&hash, insight, &manifest, sources)?;

        Ok(hash)
    }

    /// Reference an L3 as L0.
    ///
    /// Spec §7.1.6:
    /// 1. Verifies L3 was queried (in cache)
    /// 2. Verifies content_type is L3
    /// 3. Stores reference as new L0
    pub fn reference_l3_as_l0(&mut self, l3_hash: &Hash) -> OpsResult<Hash> {
        let timestamp = current_timestamp();
        self.reference_l3_as_l0_with_timestamp(l3_hash, timestamp)
    }

    /// Reference L3 as L0 with a specific timestamp (for testing).
    pub fn reference_l3_as_l0_with_timestamp(
        &mut self,
        l3_hash: &Hash,
        timestamp: Timestamp,
    ) -> OpsResult<Hash> {
        // 1. Verify L3 was queried (in cache or owned)
        let (content, manifest) = self.load_l3_for_reference(l3_hash)?;

        // 2. Verify content_type is L3
        if manifest.content_type != ContentType::L3 {
            return Err(OpsError::NotAnL3);
        }

        // 3. Create new L0 reference
        // The new L0 hash is the same content, but treated as a new L0
        let new_hash = *l3_hash; // Same content = same hash

        // Create L0 provenance (self-referential)
        let provenance = Provenance::new_l0(new_hash, self.peer_id());

        // Create new L0 manifest
        let new_manifest = Manifest {
            hash: new_hash,
            content_type: ContentType::L0, // Now it's L0
            owner: self.peer_id(),
            version: Version::new_v1(new_hash, timestamp),
            visibility: Visibility::Private,
            access: Default::default(),
            metadata: manifest.metadata.clone(),
            economics: Default::default(),
            provenance,
            created_at: timestamp,
            updated_at: timestamp,
        };

        // Store as L0
        self.store_new_content(&new_hash, &content, &new_manifest, &[])?;

        Ok(new_hash)
    }

    /// Store verified content and its manifest, and record its sources in the provenance graph.
    fn store_new_content(
        &mut self,
        hash: &Hash,
        content: &[u8],
        manifest: &Manifest,
        sources: &[Hash],
    ) -> OpsResult<()> {
        self.state.content.store_verified(hash, content)?;
        self.state.manifests.store(manifest)?;
        self.state.provenance.add(hash, sources)?;
        Ok(())
    }

    /// Check every derive source was queried or is owned.
    fn verify_sources_queried(&self, sources: &[Hash]) -> OpsResult<()> {
        for source_hash in sources {
            self.check_source_queried(source_hash)?;
        }
        Ok(())
    }

    /// A derive source must be owned (any manifest here) or queried (cached).
    /// An L2 source must be owned by us: L2 is never queryable.
    fn check_source_queried(&self, source_hash: &Hash) -> OpsResult<()> {
        match self.state.manifests.load(source_hash)? {
            // L2 sources must be owned, not queried (L2 is never queryable)
            Some(manifest) if manifest.content_type == ContentType::L2 => {
                if manifest.owner != self.peer_id() {
                    return Err(OpsError::AccessDenied);
                }
                Ok(())
            }
            Some(_) => Ok(()),
            None if self.state.cache.is_cached(source_hash) => Ok(()),
            None => Err(OpsError::SourceNotQueried(*source_hash)),
        }
    }

    /// The local manifest of each source. A source that is only cached is an error,
    /// since its manifest would have to be reconstructed; a source that is neither is skipped.
    fn load_source_manifests(&self, sources: &[Hash]) -> OpsResult<Vec<(Hash, Manifest)>> {
        let mut source_data: Vec<(Hash, Manifest)> = Vec::new();
        for source_hash in sources {
            // Try local manifest first, then check cache
            if let Some(manifest) = self.state.manifests.load(source_hash)? {
                source_data.push((*source_hash, manifest));
            } else if self.state.cache.get(source_hash)?.is_some() {
                // For cached content, we'd need to reconstruct the manifest
                // For MVP, we require sources to have known manifests
                return Err(OpsError::invalid_operation(format!(
                    "cached content {} does not have local manifest",
                    source_hash
                )));
            }
        }
        Ok(source_data)
    }

    /// Validate a derived manifest's provenance against its sources, then its content.
    fn validate_derived(
        &self,
        manifest: &Manifest,
        insight: &[u8],
        source_data: &[(Hash, Manifest)],
    ) -> OpsResult<()> {
        let source_manifests: Vec<Manifest> = source_data.iter().map(|(_, m)| m.clone()).collect();
        self.validator
            .validate_provenance(manifest, &source_manifests)?;
        self.validator.validate_content(insight, manifest)?;
        Ok(())
    }

    /// The content and manifest of an L3 to reference: a cached copy (which must also
    /// have a local manifest) or our own stored content.
    fn load_l3_for_reference(&self, l3_hash: &Hash) -> OpsResult<(Vec<u8>, Manifest)> {
        let cached = self.state.cache.get(l3_hash)?;
        let owned_manifest = self.state.manifests.load(l3_hash)?;

        match (cached, owned_manifest) {
            // Get manifest for cached content
            (Some(cached_content), Some(m)) => Ok((cached_content.content, m)),
            (Some(_), None) => Err(OpsError::invalid_operation(
                "cached L3 must have local manifest for reference",
            )),
            (None, Some(m)) => {
                // Load owned content
                let content = self
                    .state
                    .content
                    .load(l3_hash)?
                    .ok_or(OpsError::NotFound(*l3_hash))?;
                Ok((content, m))
            }
            (None, None) => Err(OpsError::SourceNotQueried(*l3_hash)),
        }
    }
}

/// The provenance for a new version of `old_manifest`.
///
/// L0 gets fresh self-referential provenance. Derived content keeps its sources,
/// with any self-reference to `old_hash` updated to `new_hash`.
fn updated_provenance(
    old_manifest: &Manifest,
    old_hash: &Hash,
    new_hash: Hash,
    owner: PeerId,
) -> Provenance {
    if old_manifest.content_type == ContentType::L0 {
        return Provenance::new_l0(new_hash, owner);
    }
    // For L3, we need to update the provenance to reference the new hash
    // but keep the same sources
    let mut prov = old_manifest.provenance.clone();
    // Update self-reference if present
    for entry in &mut prov.root_l0l1 {
        if entry.hash == *old_hash {
            entry.hash = new_hash;
        }
    }
    prov
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node_ops::DefaultNodeOperations;
    use nodalync_crypto::{generate_identity, peer_id_from_public_key};
    use nodalync_store::NodeStateConfig;
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
    fn test_create_content() {
        let (mut ops, _temp) = create_test_ops();
        let content = b"Hello, Nodalync!";
        let metadata = Metadata::new("Test", content.len() as u64);

        let hash = ops.create_content(content, metadata).unwrap();

        // Verify content was stored
        let loaded = ops.state.content.load(&hash).unwrap();
        assert_eq!(loaded, Some(content.to_vec()));

        // Verify manifest was stored
        let manifest = ops.state.manifests.load(&hash).unwrap().unwrap();
        assert_eq!(manifest.hash, hash);
        assert_eq!(manifest.content_type, ContentType::L0);
        assert_eq!(manifest.owner, ops.peer_id());
        assert!(manifest.version.is_first_version());
        assert!(manifest.provenance.is_l0());
    }

    #[test]
    fn test_update_content() {
        let (mut ops, _temp) = create_test_ops();

        // Create initial content
        let content1 = b"Version 1";
        let metadata1 = Metadata::new("Test v1", content1.len() as u64);
        let hash1 = ops.create_content(content1, metadata1).unwrap();

        // Update content
        let content2 = b"Version 2 with more content";
        let metadata2 = Metadata::new("Test v2", content2.len() as u64);
        let hash2 = ops.update_content(&hash1, content2, metadata2).unwrap();

        // Verify new content
        let manifest2 = ops.state.manifests.load(&hash2).unwrap().unwrap();
        assert_eq!(manifest2.version.number, 2);
        assert_eq!(manifest2.version.previous, Some(hash1));
        assert_eq!(manifest2.version.root, hash1); // Root is the original v1 hash
    }

    #[test]
    fn test_derive_content() {
        let (mut ops, _temp) = create_test_ops();

        // Create two source contents
        let source1 = b"Source document 1";
        let meta1 = Metadata::new("Source 1", source1.len() as u64);
        let hash1 = ops.create_content(source1, meta1).unwrap();

        let source2 = b"Source document 2";
        let meta2 = Metadata::new("Source 2", source2.len() as u64);
        let hash2 = ops.create_content(source2, meta2).unwrap();

        // Derive new content
        let insight = b"Synthesis of source 1 and 2";
        let meta3 = Metadata::new("Derived", insight.len() as u64);
        let hash3 = ops.derive_content(&[hash1, hash2], insight, meta3).unwrap();

        // Verify derived content
        let manifest3 = ops.state.manifests.load(&hash3).unwrap().unwrap();
        assert_eq!(manifest3.content_type, ContentType::L3);
        assert_eq!(manifest3.provenance.depth, 1);
        assert_eq!(manifest3.provenance.derived_from.len(), 2);
        assert!(manifest3.provenance.root_l0l1.len() >= 2);
    }

    #[test]
    fn test_derive_requires_queried_sources() {
        let (mut ops, _temp) = create_test_ops();

        // Try to derive from non-existent source
        let fake_hash = content_hash(b"nonexistent");
        let insight = b"This should fail";
        let meta = Metadata::new("Fail", insight.len() as u64);

        let result = ops.derive_content(&[fake_hash], insight, meta);
        assert!(matches!(result, Err(OpsError::SourceNotQueried(_))));
    }

    #[test]
    fn test_derive_requires_sources() {
        let (mut ops, _temp) = create_test_ops();
        let meta = Metadata::new("Fail", 4);
        let result = ops.derive_content(&[], b"none", meta);
        assert!(matches!(result, Err(OpsError::InvalidOperation(_))));
    }

    #[test]
    fn test_derive_rejects_l2_source_owned_by_someone_else() {
        let (mut ops, _temp) = create_test_ops();
        let source = b"Graph source";
        let source_hash = ops
            .create_content(source, Metadata::new("L2", source.len() as u64))
            .unwrap();

        // Turn the stored manifest into another node's L2.
        let mut manifest = ops.state.manifests.load(&source_hash).unwrap().unwrap();
        manifest.content_type = ContentType::L2;
        manifest.owner = PeerId::from_bytes([9u8; 20]);
        ops.state.manifests.update(&manifest).unwrap();

        let insight = b"Derived from someone's graph";
        let result = ops.derive_content(
            &[source_hash],
            insight,
            Metadata::new("L3", insight.len() as u64),
        );
        assert!(matches!(result, Err(OpsError::AccessDenied)));
    }

    #[test]
    fn test_reference_unknown_content_is_source_not_queried() {
        let (mut ops, _temp) = create_test_ops();
        let result = ops.reference_l3_as_l0(&content_hash(b"never seen"));
        assert!(matches!(result, Err(OpsError::SourceNotQueried(_))));
    }

    #[test]
    fn test_updated_provenance_rewrites_l3_self_reference() {
        let (mut ops, _temp) = create_test_ops();
        let source = b"Source content";
        let source_hash = ops
            .create_content(source, Metadata::new("Source", source.len() as u64))
            .unwrap();
        let insight = b"Derived insight";
        let l3_hash = ops
            .derive_content(
                &[source_hash],
                insight,
                Metadata::new("L3", insight.len() as u64),
            )
            .unwrap();
        let mut manifest = ops.state.manifests.load(&l3_hash).unwrap().unwrap();
        manifest.provenance.root_l0l1[0].hash = l3_hash;

        let new_hash = content_hash(b"v2");
        let provenance = updated_provenance(&manifest, &l3_hash, new_hash, ops.peer_id());

        assert_eq!(provenance.root_l0l1[0].hash, new_hash);
        assert_eq!(provenance.derived_from, manifest.provenance.derived_from);
    }

    #[test]
    fn test_reference_l3_as_l0() {
        let (mut ops, _temp) = create_test_ops();

        // Create source and derive L3
        let source = b"Source content";
        let meta1 = Metadata::new("Source", source.len() as u64);
        let source_hash = ops.create_content(source, meta1).unwrap();

        let insight = b"Derived insight";
        let meta2 = Metadata::new("L3", insight.len() as u64);
        let l3_hash = ops.derive_content(&[source_hash], insight, meta2).unwrap();

        // Reference L3 as L0
        let l0_hash = ops.reference_l3_as_l0(&l3_hash).unwrap();

        // L0 hash should be same (same content)
        assert_eq!(l0_hash, l3_hash);

        // But manifest should now be L0
        // Note: This would overwrite the L3 manifest in current implementation
        // In practice, you'd want to handle this differently
    }

    #[test]
    fn test_reference_requires_l3() {
        let (mut ops, _temp) = create_test_ops();

        // Create L0 content
        let content = b"L0 content";
        let meta = Metadata::new("L0", content.len() as u64);
        let l0_hash = ops.create_content(content, meta).unwrap();

        // Try to reference L0 as L0 (should fail)
        let result = ops.reference_l3_as_l0(&l0_hash);
        assert!(matches!(result, Err(OpsError::NotAnL3)));
    }
}
