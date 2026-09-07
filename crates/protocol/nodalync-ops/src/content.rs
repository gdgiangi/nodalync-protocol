//! Content operations implementation.
//!
//! This module implements content creation, update, derive, and reference operations
//! as specified in Protocol Specification §7.1.

use nodalync_crypto::{content_hash, Hash, Timestamp};
use nodalync_store::{CacheStore, ContentStore, ManifestStore, ProvenanceGraph};
use nodalync_types::{
    ContentType, Manifest, Metadata, Provenance, ProvenanceEntry, Version, Visibility,
};
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

        // 7. Store content and manifest
        self.state.content.store_verified(&hash, content)?;
        self.state.manifests.store(&manifest)?;

        // Also add to provenance graph
        self.state.provenance.add(&hash, &[])?;

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
        let new_provenance = if old_manifest.content_type == ContentType::L0 {
            Provenance::new_l0(new_hash, self.peer_id())
        } else {
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
        };

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

        // Store
        self.state.content.store_verified(&new_hash, new_content)?;
        self.state.manifests.store(&new_manifest)?;

        // Update provenance graph
        self.state.provenance.add(&new_hash, &[*old_hash])?;

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
        // 1. Verify all sources were queried (cached, imported, or owned)
        // Note: L2 sources are special - they can only be used if owned (never queried)
        for source_hash in sources {
            let manifest_opt = self.state.manifests.load(source_hash)?;
            let is_cached = self.state.cache.is_cached(source_hash);
            let is_owned = manifest_opt
                .as_ref()
                .is_some_and(|manifest| manifest.owner == self.peer_id());
            let is_imported = match &manifest_opt {
                Some(manifest) if manifest.content_type == ContentType::L3 => {
                    self.state.has_l3_reference(source_hash, &manifest.owner)?
                }
                _ => false,
            };

            // Check if this is an L2 source
            if let Some(ref manifest) = manifest_opt {
                if manifest.content_type == ContentType::L2 {
                    // L2 sources must be owned, not queried (L2 is never queryable)
                    if manifest.owner != self.peer_id() {
                        return Err(OpsError::AccessDenied);
                    }
                    continue; // L2 is valid if owned
                }
            }

            if !is_cached && !is_owned && !is_imported {
                return Err(OpsError::SourceNotQueried(*source_hash));
            }
        }

        // 2. Load source manifests
        let mut source_data: Vec<(Hash, Manifest)> = Vec::new();
        for source_hash in sources {
            // Try local manifest first, then check cache
            if let Some(manifest) = self.state.manifests.load(source_hash)? {
                source_data.push((*source_hash, manifest));
            } else if let Some(_cached) = self.state.cache.get(source_hash)? {
                // For cached content, we'd need to reconstruct the manifest
                // For MVP, we require sources to have known manifests
                return Err(OpsError::invalid_operation(format!(
                    "cached content {} does not have local manifest",
                    source_hash
                )));
            }
        }

        // 3-4. Build provenance from sources
        let provenance_sources: Vec<_> = source_data
            .iter()
            .map(|(hash, m)| (*hash, &m.provenance, m.owner, m.visibility))
            .collect();

        let mut provenance = Provenance::from_sources(&provenance_sources);
        // An imported L3 is a local reference, not a new owned L0 manifest.
        // Preserve every upstream root and add its creator as a foundation.
        for (source_hash, source) in &source_data {
            if source.content_type == ContentType::L3
                && self.state.has_l3_reference(source_hash, &source.owner)?
            {
                provenance.root_l0l1.push(ProvenanceEntry::new(
                    *source_hash,
                    source.owner,
                    source.visibility,
                ));
            }
        }
        provenance.root_l0l1 = Provenance::merge_entries(provenance.root_l0l1);

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
        let source_manifests: Vec<Manifest> = source_data.iter().map(|(_, m)| m.clone()).collect();
        self.validator
            .validate_provenance(&manifest, &source_manifests)?;
        self.validator.validate_content(insight, &manifest)?;

        // 7. Store
        self.state.content.store_verified(&hash, insight)?;
        self.state.manifests.store(&manifest)?;
        self.state.provenance.add(&hash, sources)?;

        Ok(hash)
    }

    /// Reference an L3 as L0.
    ///
    /// Spec §7.1.6:
    /// 1. Verifies L3 was queried (in cache)
    /// 2. Verifies content_type is L3
    /// 3. Stores a local reference without changing the source manifest
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
        let manifest = self
            .state
            .manifests
            .load(l3_hash)?
            .ok_or(OpsError::SourceNotQueried(*l3_hash))?;

        // A remote manifest alone is discovery metadata, not evidence of access.
        if manifest.owner != self.peer_id()
            && !self.state.cache.is_cached(l3_hash)
            && !self.state.has_l3_reference(l3_hash, &manifest.owner)?
        {
            return Err(OpsError::SourceNotQueried(*l3_hash));
        }

        // 2. Verify content_type is L3
        if manifest.content_type != ContentType::L3 {
            return Err(OpsError::NotAnL3);
        }

        self.state
            .store_l3_reference(l3_hash, &manifest.owner, timestamp)?;
        Ok(*l3_hash)
    }
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
    fn test_reference_l3_as_l0() {
        let (mut ops, _temp) = create_test_ops();

        // Create source and derive L3
        let source = b"Source content";
        let meta1 = Metadata::new("Source", source.len() as u64);
        let source_hash = ops.create_content(source, meta1).unwrap();

        let insight = b"Derived insight";
        let meta2 = Metadata::new("L3", insight.len() as u64);
        let l3_hash = ops.derive_content(&[source_hash], insight, meta2).unwrap();
        let original = ops.state.manifests.load(&l3_hash).unwrap().unwrap();

        // Reference L3 as L0
        let l0_hash = ops.reference_l3_as_l0(&l3_hash).unwrap();

        // L0 hash should be same (same content)
        assert_eq!(l0_hash, l3_hash);

        assert_eq!(
            ops.state.manifests.load(&l3_hash).unwrap().unwrap(),
            original
        );
        assert!(ops
            .state
            .has_l3_reference(&l3_hash, &original.owner)
            .unwrap());
        assert!(ops
            .state
            .provenance
            .is_ancestor(&source_hash, &l3_hash)
            .unwrap());
    }

    /// Simulate the persisted result of a successful query; settlement itself
    /// is covered by the economic loop tests rather than this import test.
    fn cache_source(ops: &mut DefaultNodeOperations, manifest: &Manifest, content: &[u8]) {
        let receipt = nodalync_wire::PaymentReceipt {
            payment_id: content_hash(b"import test receipt"),
            amount: 100,
            timestamp: 1000,
            channel_nonce: 1,
            distributor_signature: nodalync_crypto::Signature::from_bytes([0u8; 64]),
        };
        ops.state.manifests.store(manifest).unwrap();
        ops.state
            .cache
            .cache(nodalync_store::CachedContent::new(
                manifest.hash,
                content.to_vec(),
                manifest.owner,
                1000,
                receipt,
            ))
            .unwrap();
    }

    #[test]
    fn test_import_preserves_all_contributors_across_generations_and_restart() {
        let (mut alice, _alice_dir) = create_test_ops();
        let (mut bob, _bob_dir) = create_test_ops();
        let (mut carol, _carol_dir) = create_test_ops();
        let (mut dave, _dave_dir) = create_test_ops();

        let source = b"Alice's original observation";
        let root = alice
            .create_content(source, Metadata::new("Observation", source.len() as u64))
            .unwrap();
        let source_manifest = alice.state.manifests.load(&root).unwrap().unwrap();
        cache_source(&mut bob, &source_manifest, source);

        let insight = b"Bob's insight grounded in Alice's observation";
        let bob_hash = bob
            .derive_content(
                &[root],
                insight,
                Metadata::new("Insight", insight.len() as u64),
            )
            .unwrap();
        let bob_manifest = bob.state.manifests.load(&bob_hash).unwrap().unwrap();
        cache_source(&mut carol, &bob_manifest, insight);

        // A plain derivation preserves roots but does not promote the source
        // synthesizer. The local import is what changes that decision.
        let plain = carol
            .derive_content(&[bob_hash], b"Plain derivative", Metadata::new("Plain", 16))
            .unwrap();
        let plain_manifest = carol.state.manifests.load(&plain).unwrap().unwrap();
        assert_eq!(
            plain_manifest.provenance.root_l0l1,
            bob_manifest.provenance.root_l0l1
        );

        carol
            .reference_l3_as_l0_with_timestamp(&bob_hash, 2000)
            .unwrap();
        carol
            .reference_l3_as_l0_with_timestamp(&bob_hash, 3000)
            .unwrap();
        assert_eq!(
            carol.state.manifests.load(&bob_hash).unwrap().unwrap(),
            bob_manifest
        );
        // Import records stay local; cached response bytes are not promoted
        // into owned content storage or republished as Carol's L0.
        assert!(!carol.state.content.exists(&bob_hash));
        let imported_at: u64 = carol
            .state
            .connection()
            .lock()
            .unwrap()
            .query_row(
                "SELECT imported_at FROM l3_references WHERE hash = ?1",
                [bob_hash.0.as_slice()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(imported_at, 2000);
        let repeated = carol
            .derive_content(
                &[bob_hash, bob_hash],
                b"Repeated source",
                Metadata::new("Repeated", 15),
            )
            .unwrap();
        let repeated = carol.state.manifests.load(&repeated).unwrap().unwrap();
        assert_eq!(
            repeated
                .provenance
                .root_l0l1
                .iter()
                .find(|e| e.hash == root)
                .unwrap()
                .weight,
            4
        );
        assert_eq!(
            repeated
                .provenance
                .root_l0l1
                .iter()
                .find(|e| e.hash == bob_hash)
                .unwrap()
                .weight,
            2
        );

        // A durable import records prior access even after normal cache eviction.
        carol.state.cache.evict(0).unwrap();
        assert!(!carol.state.cache.is_cached(&bob_hash));
        carol.reference_l3_as_l0(&bob_hash).unwrap();

        let carol_id = carol.peer_id();
        let config = carol.state.config().clone();
        drop(carol);
        let mut carol = DefaultNodeOperations::with_defaults(
            nodalync_store::NodeState::open(config).unwrap(),
            carol_id,
        );
        let second = b"Carol's new conclusion";
        let carol_hash = carol
            .derive_content(
                &[bob_hash],
                second,
                Metadata::new("Conclusion", second.len() as u64),
            )
            .unwrap();
        let carol_manifest = carol.state.manifests.load(&carol_hash).unwrap().unwrap();
        let roots = &carol_manifest.provenance.root_l0l1;
        assert_eq!(roots.len(), 2);
        assert_eq!(
            roots.iter().find(|e| e.hash == root).unwrap(),
            &bob_manifest.provenance.root_l0l1[0]
        );
        assert_eq!(
            roots.iter().find(|e| e.hash == bob_hash).unwrap(),
            &ProvenanceEntry::new(bob_hash, bob.peer_id(), bob_manifest.visibility)
        );
        let distributions = nodalync_econ::distribute_revenue(600, &carol_id, roots);
        let amount_for = |peer| {
            distributions
                .iter()
                .find(|d| d.recipient == peer)
                .unwrap()
                .amount
        };
        assert_eq!(amount_for(alice.peer_id()), 380);
        assert_eq!(amount_for(bob.peer_id()), 190);
        assert_eq!(amount_for(carol_id), 30);

        // The new synthesizer can itself become a foundation, without losing
        // Alice or Bob or resetting the derivation depth.
        cache_source(&mut dave, &carol_manifest, second);
        dave.reference_l3_as_l0(&carol_hash).unwrap();
        let final_content = b"Dave's further conclusion";
        let final_hash = dave
            .derive_content(
                &[carol_hash],
                final_content,
                Metadata::new("Further", final_content.len() as u64),
            )
            .unwrap();
        let final_manifest = dave.state.manifests.load(&final_hash).unwrap().unwrap();
        assert_eq!(final_manifest.provenance.depth, 3);
        assert_eq!(final_manifest.provenance.root_l0l1.len(), 3);
        let distributions = nodalync_econ::distribute_revenue(
            800,
            &dave.peer_id(),
            &final_manifest.provenance.root_l0l1,
        );
        let amount_for = |peer| {
            distributions
                .iter()
                .find(|d| d.recipient == peer)
                .unwrap()
                .amount
        };
        assert_eq!(amount_for(alice.peer_id()), 380);
        assert_eq!(amount_for(bob.peer_id()), 190);
        assert_eq!(amount_for(carol_id), 190);
        assert_eq!(amount_for(dave.peer_id()), 40);
    }

    #[test]
    fn test_remote_manifest_alone_does_not_authorize_import_or_derivation() {
        let (mut author, _author_dir) = create_test_ops();
        let (mut requester, _requester_dir) = create_test_ops();
        let root = author
            .create_content(b"Root", Metadata::new("Root", 4))
            .unwrap();
        let l3 = author
            .derive_content(&[root], b"Insight", Metadata::new("Insight", 7))
            .unwrap();
        let manifest = author.state.manifests.load(&l3).unwrap().unwrap();
        requester.state.manifests.store(&manifest).unwrap();

        assert!(matches!(
            requester.reference_l3_as_l0(&l3),
            Err(OpsError::SourceNotQueried(_))
        ));
        assert!(matches!(
            requester.derive_content(&[l3], b"New", Metadata::new("New", 3)),
            Err(OpsError::SourceNotQueried(_))
        ));
        assert!(!requester
            .state
            .has_l3_reference(&l3, &manifest.owner)
            .unwrap());
        assert!(matches!(
            requester.reference_l3_as_l0(&content_hash(b"Missing")),
            Err(OpsError::SourceNotQueried(_))
        ));
    }

    #[tokio::test]
    async fn test_imported_visibility_snapshot_survives_source_publication() {
        let (mut ops, _dir) = create_test_ops();
        let root = ops
            .create_content(b"Root", Metadata::new("Root", 4))
            .unwrap();
        let first = ops
            .derive_content(&[root], b"First insight", Metadata::new("First", 13))
            .unwrap();
        ops.reference_l3_as_l0(&first).unwrap();
        let next = ops
            .derive_content(&[first], b"Next insight", Metadata::new("Next", 12))
            .unwrap();
        ops.publish_content(&first, Visibility::Shared, 0)
            .await
            .unwrap();

        let combined = ops
            .derive_content(&[first, next], b"Combined", Metadata::new("Combined", 8))
            .unwrap();
        let manifest = ops.state.manifests.load(&combined).unwrap().unwrap();
        let entry = manifest
            .provenance
            .root_l0l1
            .iter()
            .find(|entry| entry.hash == first)
            .unwrap();
        assert_eq!(entry.weight, 2);
        // Existing paths retain visibility at their original derivation time.
        assert_eq!(entry.visibility, Visibility::Private);
        assert_eq!(entry.owner, ops.peer_id());
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
