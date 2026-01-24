//! Provenance validation (§9.3).
//!
//! This module validates provenance chains:
//! - L0: self-referential provenance
//! - L3: derived from sources with correct root computation
//! - Depth constraints
//! - No self-references

use std::collections::HashSet;

use nodalync_types::{ContentType, Hash, Manifest, ProvenanceEntry, MAX_PROVENANCE_DEPTH};

use crate::error::{ValidationError, ValidationResult};

/// Validate provenance for a manifest against its sources.
///
/// Checks all provenance validation rules from §9.3:
///
/// For L0 content:
/// - `root_l0l1` must have exactly one entry (itself)
/// - `root_l0l1[0].hash` must equal the content hash
/// - `derived_from` must be empty
/// - `depth` must be 0
///
/// For L3 content:
/// - `root_l0l1` must have at least one entry
/// - `derived_from` must have at least one entry
/// - All `derived_from` hashes must exist in sources
/// - `root_l0l1` computation must be correct
/// - `depth` must equal `max(sources.depth) + 1`
/// - No self-reference (content hash not in `derived_from` or `root_l0l1`)
///
/// # Arguments
///
/// * `manifest` - The manifest to validate
/// * `sources` - The source manifests (for L3 content)
///
/// # Returns
///
/// `Ok(())` if provenance is valid, or `Err(ValidationError)`.
pub fn validate_provenance(manifest: &Manifest, sources: &[Manifest]) -> ValidationResult<()> {
    let prov = &manifest.provenance;

    // Check depth limit
    if prov.depth > MAX_PROVENANCE_DEPTH {
        return Err(ValidationError::DepthTooDeep {
            depth: prov.depth,
            max: MAX_PROVENANCE_DEPTH,
        });
    }

    match manifest.content_type {
        ContentType::L0 | ContentType::L1 => validate_l0_provenance(manifest),
        ContentType::L2 => {
            // L2 provenance is handled separately by validate_l2_provenance
            // For now, validate using L3 rules (derived content)
            validate_l3_provenance(manifest, sources)
        }
        ContentType::L3 => validate_l3_provenance(manifest, sources),
        // Handle future content types - for now treat unknown types as invalid
        _ => Err(ValidationError::Internal(
            "unknown content type".to_string(),
        )),
    }
}

/// Validate L0/L1 provenance (self-referential).
fn validate_l0_provenance(manifest: &Manifest) -> ValidationResult<()> {
    let prov = &manifest.provenance;

    // Must have exactly one root entry
    if prov.root_l0l1.len() != 1 {
        return Err(ValidationError::L0WrongRootCount);
    }

    // Root entry must reference self
    if prov.root_l0l1[0].hash != manifest.hash {
        return Err(ValidationError::L0RootNotSelf);
    }

    // Must not have derived_from entries
    if !prov.derived_from.is_empty() {
        return Err(ValidationError::L0HasDerivedFrom);
    }

    // Depth must be 0
    if prov.depth != 0 {
        return Err(ValidationError::L0WrongDepth { depth: prov.depth });
    }

    Ok(())
}

/// Validate L3 provenance (derived content).
fn validate_l3_provenance(manifest: &Manifest, sources: &[Manifest]) -> ValidationResult<()> {
    let prov = &manifest.provenance;

    // Must have at least one root
    if prov.root_l0l1.is_empty() {
        return Err(ValidationError::L3NoRoots);
    }

    // Must have at least one derived_from entry
    if prov.derived_from.is_empty() {
        return Err(ValidationError::L3NoDerivedFrom);
    }

    // No self-reference in derived_from
    if prov.derived_from.contains(&manifest.hash) {
        return Err(ValidationError::SelfReference);
    }

    // No self-reference in root_l0l1
    if prov.root_l0l1.iter().any(|e| e.hash == manifest.hash) {
        return Err(ValidationError::SelfRoot);
    }

    // All derived_from must exist in sources
    let source_hashes: HashSet<&Hash> = sources.iter().map(|s| &s.hash).collect();
    for df_hash in &prov.derived_from {
        if !source_hashes.contains(df_hash) {
            return Err(ValidationError::UnknownSource {
                hash: format!("{}", df_hash),
            });
        }
    }

    // Verify root_l0l1 computation
    let computed_roots = compute_root_entries(sources);
    if !roots_match(&prov.root_l0l1, &computed_roots) {
        return Err(ValidationError::RootEntriesMismatch);
    }

    // Verify depth
    let expected_depth = sources
        .iter()
        .map(|s| s.provenance.depth)
        .max()
        .unwrap_or(0)
        + 1;
    if prov.depth != expected_depth {
        return Err(ValidationError::DepthMismatch {
            expected: expected_depth,
            actual: prov.depth,
        });
    }

    Ok(())
}

/// Compute expected root entries from source manifests.
///
/// This mirrors the logic in `Provenance::from_sources` in nodalync-types:
/// - Collects all root_l0l1 entries from each source
/// - For L0 sources, adds an additional entry for the source itself
/// - Merges duplicates by accumulating weights
fn compute_root_entries(sources: &[Manifest]) -> Vec<ProvenanceEntry> {
    use std::collections::HashMap;

    let mut all_entries = Vec::new();

    for source in sources {
        // Collect all root entries from the source
        for entry in &source.provenance.root_l0l1 {
            all_entries.push(entry.clone());
        }

        // If the source is L0, add it as a root entry too
        // (This matches Provenance::from_sources behavior)
        if source.provenance.is_l0() {
            all_entries.push(ProvenanceEntry::new(
                source.hash,
                source.owner,
                source.visibility,
            ));
        }
    }

    // Merge duplicate entries by accumulating weights
    let mut merged: HashMap<Hash, ProvenanceEntry> = HashMap::new();
    for entry in all_entries {
        merged
            .entry(entry.hash)
            .and_modify(|e| e.weight += entry.weight)
            .or_insert(entry);
    }

    merged.into_values().collect()
}

/// Check if two sets of provenance entries match (ignoring order).
///
/// Entries match if they have the same hashes with the same weights.
fn roots_match(actual: &[ProvenanceEntry], expected: &[ProvenanceEntry]) -> bool {
    use std::collections::HashMap;

    if actual.len() != expected.len() {
        return false;
    }

    let actual_map: HashMap<Hash, u32> = actual.iter().map(|e| (e.hash, e.weight)).collect();
    let expected_map: HashMap<Hash, u32> = expected.iter().map(|e| (e.hash, e.weight)).collect();

    actual_map == expected_map
}

#[cfg(test)]
mod tests {
    use super::*;
    use nodalync_crypto::{content_hash, generate_identity, peer_id_from_public_key};
    use nodalync_types::{Metadata, Provenance, Visibility};

    fn test_peer_id() -> nodalync_types::PeerId {
        let (_, public_key) = generate_identity();
        peer_id_from_public_key(&public_key)
    }

    fn create_l0_manifest(content: &[u8]) -> Manifest {
        let hash = content_hash(content);
        let owner = test_peer_id();
        let metadata = Metadata::new("Test", content.len() as u64);
        Manifest::new_l0(hash, owner, metadata, 1234567890)
    }

    #[test]
    fn test_valid_l0_provenance() {
        let manifest = create_l0_manifest(b"L0 content");
        assert!(validate_provenance(&manifest, &[]).is_ok());
    }

    #[test]
    fn test_l0_wrong_root_count() {
        let mut manifest = create_l0_manifest(b"L0 content");
        // Add extra root entry
        manifest.provenance.root_l0l1.push(ProvenanceEntry::new(
            content_hash(b"extra"),
            test_peer_id(),
            Visibility::Shared,
        ));

        let result = validate_provenance(&manifest, &[]);
        assert!(matches!(result, Err(ValidationError::L0WrongRootCount)));
    }

    #[test]
    fn test_l0_root_not_self() {
        let mut manifest = create_l0_manifest(b"L0 content");
        // Change root entry to different hash
        manifest.provenance.root_l0l1[0].hash = content_hash(b"different");

        let result = validate_provenance(&manifest, &[]);
        assert!(matches!(result, Err(ValidationError::L0RootNotSelf)));
    }

    #[test]
    fn test_l0_has_derived_from() {
        let mut manifest = create_l0_manifest(b"L0 content");
        // Add derived_from (invalid for L0)
        manifest.provenance.derived_from.push(content_hash(b"source"));

        let result = validate_provenance(&manifest, &[]);
        assert!(matches!(result, Err(ValidationError::L0HasDerivedFrom)));
    }

    #[test]
    fn test_l0_wrong_depth() {
        let mut manifest = create_l0_manifest(b"L0 content");
        manifest.provenance.depth = 1;

        let result = validate_provenance(&manifest, &[]);
        assert!(matches!(
            result,
            Err(ValidationError::L0WrongDepth { depth: 1 })
        ));
    }

    #[test]
    fn test_valid_l3_provenance() {
        // Create two L0 sources
        let source1 = create_l0_manifest(b"Source 1");
        let source2 = create_l0_manifest(b"Source 2");

        // Create L3 derived from sources
        let l3_hash = content_hash(b"L3 content");
        let owner = test_peer_id();
        let metadata = Metadata::new("L3", 10);

        let mut l3_manifest = Manifest::new_l0(l3_hash, owner, metadata, 2000);
        l3_manifest.content_type = ContentType::L3;
        l3_manifest.provenance = Provenance::from_sources(&[
            (source1.hash, &source1.provenance, source1.owner, Visibility::Shared),
            (source2.hash, &source2.provenance, source2.owner, Visibility::Shared),
        ]);

        assert!(validate_provenance(&l3_manifest, &[source1, source2]).is_ok());
    }

    #[test]
    fn test_l3_no_roots() {
        let l3_hash = content_hash(b"L3 content");
        let owner = test_peer_id();
        let metadata = Metadata::new("L3", 10);

        let mut l3_manifest = Manifest::new_l0(l3_hash, owner, metadata, 2000);
        l3_manifest.content_type = ContentType::L3;
        l3_manifest.provenance = Provenance {
            root_l0l1: vec![],
            derived_from: vec![content_hash(b"source")],
            depth: 1,
        };

        let result = validate_provenance(&l3_manifest, &[]);
        assert!(matches!(result, Err(ValidationError::L3NoRoots)));
    }

    #[test]
    fn test_l3_no_derived_from() {
        let l3_hash = content_hash(b"L3 content");
        let owner = test_peer_id();
        let metadata = Metadata::new("L3", 10);

        let mut l3_manifest = Manifest::new_l0(l3_hash, owner, metadata, 2000);
        l3_manifest.content_type = ContentType::L3;
        l3_manifest.provenance = Provenance {
            root_l0l1: vec![ProvenanceEntry::new(
                content_hash(b"root"),
                owner,
                Visibility::Shared,
            )],
            derived_from: vec![],
            depth: 1,
        };

        let result = validate_provenance(&l3_manifest, &[]);
        assert!(matches!(result, Err(ValidationError::L3NoDerivedFrom)));
    }

    #[test]
    fn test_l3_self_reference_in_derived_from() {
        let source = create_l0_manifest(b"Source");
        let l3_hash = content_hash(b"L3 content");
        let owner = test_peer_id();
        let metadata = Metadata::new("L3", 10);

        let mut l3_manifest = Manifest::new_l0(l3_hash, owner, metadata, 2000);
        l3_manifest.content_type = ContentType::L3;
        l3_manifest.provenance = Provenance {
            root_l0l1: source.provenance.root_l0l1.clone(),
            derived_from: vec![source.hash, l3_hash], // Self-reference!
            depth: 1,
        };

        let result = validate_provenance(&l3_manifest, &[source]);
        assert!(matches!(result, Err(ValidationError::SelfReference)));
    }

    #[test]
    fn test_l3_self_root() {
        let source = create_l0_manifest(b"Source");
        let l3_hash = content_hash(b"L3 content");
        let owner = test_peer_id();
        let metadata = Metadata::new("L3", 10);

        let mut l3_manifest = Manifest::new_l0(l3_hash, owner, metadata, 2000);
        l3_manifest.content_type = ContentType::L3;
        l3_manifest.provenance = Provenance {
            root_l0l1: vec![
                source.provenance.root_l0l1[0].clone(),
                ProvenanceEntry::new(l3_hash, owner, Visibility::Shared), // Self as root!
            ],
            derived_from: vec![source.hash],
            depth: 1,
        };

        let result = validate_provenance(&l3_manifest, &[source]);
        assert!(matches!(result, Err(ValidationError::SelfRoot)));
    }

    #[test]
    fn test_l3_unknown_source() {
        let source = create_l0_manifest(b"Source");
        let l3_hash = content_hash(b"L3 content");
        let owner = test_peer_id();
        let metadata = Metadata::new("L3", 10);

        let mut l3_manifest = Manifest::new_l0(l3_hash, owner, metadata, 2000);
        l3_manifest.content_type = ContentType::L3;
        l3_manifest.provenance = Provenance {
            root_l0l1: source.provenance.root_l0l1.clone(),
            derived_from: vec![source.hash, content_hash(b"unknown")], // Unknown source
            depth: 1,
        };

        let result = validate_provenance(&l3_manifest, &[source]);
        assert!(matches!(result, Err(ValidationError::UnknownSource { .. })));
    }

    #[test]
    fn test_l3_depth_mismatch() {
        let source = create_l0_manifest(b"Source");
        let l3_hash = content_hash(b"L3 content");
        let owner = test_peer_id();
        let metadata = Metadata::new("L3", 10);

        let mut l3_manifest = Manifest::new_l0(l3_hash, owner, metadata, 2000);
        l3_manifest.content_type = ContentType::L3;
        l3_manifest.provenance = Provenance::from_sources(&[(
            source.hash,
            &source.provenance,
            source.owner,
            Visibility::Shared,
        )]);
        // Wrong depth (should be 1)
        l3_manifest.provenance.depth = 5;

        let result = validate_provenance(&l3_manifest, &[source]);
        assert!(matches!(
            result,
            Err(ValidationError::DepthMismatch {
                expected: 1,
                actual: 5
            })
        ));
    }

    #[test]
    fn test_depth_too_deep() {
        let mut manifest = create_l0_manifest(b"Content");
        manifest.provenance.depth = MAX_PROVENANCE_DEPTH + 1;

        let result = validate_provenance(&manifest, &[]);
        assert!(matches!(result, Err(ValidationError::DepthTooDeep { .. })));
    }

    #[test]
    fn test_weight_accumulation() {
        // Create source
        let source = create_l0_manifest(b"Source");

        // When deriving from an L0 source, Provenance::from_sources adds the source
        // twice (once from root_l0l1 and once explicitly for L0), so expected weight is 2
        let l3_hash = content_hash(b"L3");
        let owner = test_peer_id();
        let metadata = Metadata::new("L3", 2);

        let mut l3_manifest = Manifest::new_l0(l3_hash, owner, metadata, 2000);
        l3_manifest.content_type = ContentType::L3;
        l3_manifest.provenance = Provenance::from_sources(&[(
            source.hash,
            &source.provenance,
            source.owner,
            Visibility::Shared,
        )]);

        // This should pass because provenance was computed correctly
        assert!(validate_provenance(&l3_manifest, &[source.clone()]).is_ok());

        // Now test that wrong weight fails
        let mut wrong_weight_manifest = l3_manifest.clone();
        wrong_weight_manifest.provenance.root_l0l1[0].weight = 99; // Wrong weight

        let result = validate_provenance(&wrong_weight_manifest, &[source]);
        assert!(matches!(result, Err(ValidationError::RootEntriesMismatch)));
    }

    #[test]
    fn test_roots_match_function() {
        let owner = test_peer_id();
        let hash1 = content_hash(b"hash1");
        let hash2 = content_hash(b"hash2");

        let entries1 = vec![
            ProvenanceEntry::with_weight(hash1, owner, Visibility::Shared, 1),
            ProvenanceEntry::with_weight(hash2, owner, Visibility::Shared, 2),
        ];

        let entries2 = vec![
            ProvenanceEntry::with_weight(hash2, owner, Visibility::Shared, 2),
            ProvenanceEntry::with_weight(hash1, owner, Visibility::Shared, 1),
        ];

        // Same entries in different order should match
        assert!(roots_match(&entries1, &entries2));

        // Different weights should not match
        let entries3 = vec![
            ProvenanceEntry::with_weight(hash1, owner, Visibility::Shared, 1),
            ProvenanceEntry::with_weight(hash2, owner, Visibility::Shared, 3), // Different weight
        ];
        assert!(!roots_match(&entries1, &entries3));
    }
}
