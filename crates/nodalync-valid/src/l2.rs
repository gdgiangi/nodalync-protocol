//! L2 Entity Graph validation (§9.1).
//!
//! This module validates L2 Entity Graphs:
//! - L2 visibility MUST be Private
//! - L2 price MUST be 0
//! - Entity and relationship constraints
//! - URI/CURIE validation
//! - Provenance rules (root_L0L1 contains only L0/L1)

use nodalync_types::{
    ContentType, L2EntityGraph, Manifest, PrefixMap, Visibility, MAX_ALIASES_PER_ENTITY,
    MAX_CANONICAL_LABEL_LENGTH, MAX_ENTITIES_PER_L2, MAX_ENTITY_DESCRIPTION_LENGTH,
    MAX_PREDICATE_LENGTH, MAX_RELATIONSHIPS_PER_L2, MAX_SOURCE_L1S_PER_L2,
};

use crate::error::{ValidationError, ValidationResult};

/// Validate an L2 Entity Graph against its manifest.
///
/// Checks all L2 validation rules:
/// 1. Visibility MUST be Private
/// 2. Price MUST be 0
/// 3. Entity and relationship counts within limits
/// 4. All entity IDs are unique
/// 5. All relationship references are valid
/// 6. URIs/CURIEs are valid
/// 7. Confidence scores are in range [0.0, 1.0]
///
/// # Arguments
///
/// * `graph` - The L2 Entity Graph to validate
/// * `manifest` - The manifest describing the content
///
/// # Returns
///
/// `Ok(())` if all validations pass, or `Err(ValidationError)`.
pub fn validate_l2_content(graph: &L2EntityGraph, manifest: &Manifest) -> ValidationResult<()> {
    // 1. Content type must be L2
    if manifest.content_type != ContentType::L2 {
        return Err(ValidationError::Internal(
            "manifest content_type must be L2".to_string(),
        ));
    }

    // 2. L2 visibility MUST be Private
    if manifest.visibility != Visibility::Private {
        return Err(ValidationError::L2VisibilityNotPrivate {
            visibility: format!("{:?}", manifest.visibility),
        });
    }

    // 3. L2 price MUST be 0
    if manifest.economics.price != 0 {
        return Err(ValidationError::L2PriceNotZero {
            price: manifest.economics.price,
        });
    }

    // 4. Graph ID should match manifest hash
    if graph.id != manifest.hash {
        return Err(ValidationError::L2IdMismatch);
    }

    // 5. Source L1s must be present
    if graph.source_l1s.is_empty() && graph.source_l2s.is_empty() {
        return Err(ValidationError::L2NoSources);
    }

    // 6. Source L1s count within limit
    if graph.source_l1s.len() > MAX_SOURCE_L1S_PER_L2 {
        return Err(ValidationError::L2TooManySources {
            count: graph.source_l1s.len(),
            max: MAX_SOURCE_L1S_PER_L2,
        });
    }

    // 7. Entity count within limit
    if graph.entities.len() > MAX_ENTITIES_PER_L2 as usize {
        return Err(ValidationError::L2TooManyEntities {
            count: graph.entities.len(),
            max: MAX_ENTITIES_PER_L2 as usize,
        });
    }

    // 8. Relationship count within limit
    if graph.relationships.len() > MAX_RELATIONSHIPS_PER_L2 as usize {
        return Err(ValidationError::L2TooManyRelationships {
            count: graph.relationships.len(),
            max: MAX_RELATIONSHIPS_PER_L2 as usize,
        });
    }

    // 9. Entity count matches
    if graph.entity_count != graph.entities.len() as u32 {
        return Err(ValidationError::L2EntityCountMismatch {
            declared: graph.entity_count,
            actual: graph.entities.len() as u32,
        });
    }

    // 10. Relationship count matches
    if graph.relationship_count != graph.relationships.len() as u32 {
        return Err(ValidationError::L2RelationshipCountMismatch {
            declared: graph.relationship_count,
            actual: graph.relationships.len() as u32,
        });
    }

    // 11. Validate prefix map
    validate_prefix_map(&graph.prefixes)?;

    // 12. Validate entities
    let entity_ids: std::collections::HashSet<&str> =
        graph.entities.iter().map(|e| e.id.as_str()).collect();
    if entity_ids.len() != graph.entities.len() {
        return Err(ValidationError::L2DuplicateEntityId);
    }

    for entity in &graph.entities {
        validate_entity(entity, &graph.prefixes)?;
    }

    // 13. Validate relationships
    for relationship in &graph.relationships {
        validate_relationship(relationship, &entity_ids, &graph.prefixes)?;
    }

    Ok(())
}

/// Validate a prefix map.
fn validate_prefix_map(prefixes: &PrefixMap) -> ValidationResult<()> {
    for entry in &prefixes.entries {
        // Prefix should not be empty
        if entry.prefix.is_empty() {
            return Err(ValidationError::L2InvalidPrefix {
                prefix: "(empty)".to_string(),
                reason: "prefix cannot be empty".to_string(),
            });
        }

        // URI should be a valid URL
        if !is_valid_uri(&entry.uri) {
            return Err(ValidationError::L2InvalidUri {
                uri: entry.uri.clone(),
                reason: "invalid URI format".to_string(),
            });
        }
    }
    Ok(())
}

/// Validate an entity.
fn validate_entity(entity: &nodalync_types::Entity, prefixes: &PrefixMap) -> ValidationResult<()> {
    // Entity ID cannot be empty
    if entity.id.is_empty() {
        return Err(ValidationError::L2InvalidEntityId {
            id: "(empty)".to_string(),
        });
    }

    // Canonical label length
    if entity.canonical_label.len() > MAX_CANONICAL_LABEL_LENGTH {
        return Err(ValidationError::L2LabelTooLong {
            length: entity.canonical_label.len(),
            max: MAX_CANONICAL_LABEL_LENGTH,
        });
    }

    // Aliases count
    if entity.aliases.len() > MAX_ALIASES_PER_ENTITY {
        return Err(ValidationError::L2TooManyAliases {
            count: entity.aliases.len(),
            max: MAX_ALIASES_PER_ENTITY,
        });
    }

    // Entity type URI validation (if present)
    if let Some(ref entity_type) = entity.entity_type {
        validate_uri_or_curie(entity_type, prefixes)?;
    }

    // Description length (if present)
    if let Some(ref desc) = entity.description {
        if desc.len() > MAX_ENTITY_DESCRIPTION_LENGTH {
            return Err(ValidationError::L2DescriptionTooLong {
                length: desc.len(),
                max: MAX_ENTITY_DESCRIPTION_LENGTH,
            });
        }
    }

    // External links must be valid URIs
    for link in &entity.external_links {
        if !is_valid_uri(link) {
            return Err(ValidationError::L2InvalidUri {
                uri: link.clone(),
                reason: "invalid external link URI".to_string(),
            });
        }
    }

    // Confidence score must be in range [0.0, 1.0]
    if !(0.0..=1.0).contains(&entity.confidence) {
        return Err(ValidationError::L2InvalidConfidence {
            value: entity.confidence,
        });
    }

    Ok(())
}

/// Validate a relationship.
fn validate_relationship(
    relationship: &nodalync_types::Relationship,
    entity_ids: &std::collections::HashSet<&str>,
    prefixes: &PrefixMap,
) -> ValidationResult<()> {
    // Relationship ID cannot be empty
    if relationship.id.is_empty() {
        return Err(ValidationError::L2InvalidRelationshipId {
            id: "(empty)".to_string(),
        });
    }

    // Subject must exist
    if !entity_ids.contains(relationship.subject.as_str()) {
        return Err(ValidationError::L2InvalidEntityRef {
            entity_id: relationship.subject.clone(),
            context: "relationship subject".to_string(),
        });
    }

    // Predicate must be valid URI or CURIE
    if relationship.predicate.len() > MAX_PREDICATE_LENGTH {
        return Err(ValidationError::L2PredicateTooLong {
            length: relationship.predicate.len(),
            max: MAX_PREDICATE_LENGTH,
        });
    }
    validate_uri_or_curie(&relationship.predicate, prefixes)?;

    // Object validation
    match &relationship.object {
        nodalync_types::RelationshipObject::Entity { entity_id } => {
            if !entity_ids.contains(entity_id.as_str()) {
                return Err(ValidationError::L2InvalidEntityRef {
                    entity_id: entity_id.clone(),
                    context: "relationship object".to_string(),
                });
            }
        }
        nodalync_types::RelationshipObject::Literal(lit) => {
            // Validate datatype URI if present
            if let Some(ref datatype) = lit.datatype {
                validate_uri_or_curie(datatype, prefixes)?;
            }
        }
        nodalync_types::RelationshipObject::Uri { uri } => {
            if !is_valid_uri(uri) {
                return Err(ValidationError::L2InvalidUri {
                    uri: uri.clone(),
                    reason: "invalid object URI".to_string(),
                });
            }
        }
    }

    // Confidence score must be in range [0.0, 1.0]
    if !(0.0..=1.0).contains(&relationship.confidence) {
        return Err(ValidationError::L2InvalidConfidence {
            value: relationship.confidence,
        });
    }

    Ok(())
}

/// Check if a string is a valid URI or CURIE.
fn validate_uri_or_curie(uri_or_curie: &str, prefixes: &PrefixMap) -> ValidationResult<()> {
    if is_valid_uri(uri_or_curie) || prefixes.is_valid_curie(uri_or_curie) {
        Ok(())
    } else {
        Err(ValidationError::L2InvalidUri {
            uri: uri_or_curie.to_string(),
            reason: "not a valid URI or known CURIE".to_string(),
        })
    }
}

/// Check if a string is a valid URI.
///
/// A valid URI starts with http:// or https://
pub fn is_valid_uri(uri: &str) -> bool {
    uri.starts_with("http://") || uri.starts_with("https://")
}

/// Expand a CURIE to a full URI.
///
/// Returns `Ok(expanded_uri)` if the prefix is found, or `Err` if not.
pub fn expand_curie(curie: &str, prefixes: &PrefixMap) -> ValidationResult<String> {
    prefixes.expand(curie).ok_or_else(|| {
        let prefix = curie.split(':').next().unwrap_or("(unknown)");
        ValidationError::L2InvalidUri {
            uri: curie.to_string(),
            reason: format!("unknown prefix: {}", prefix),
        }
    })
}

/// Validate L2 provenance constraints.
///
/// For L2 content:
/// - root_l0l1 must contain ONLY L0/L1 entries (never L2/L3)
/// - This ensures proper provenance tracking
pub fn validate_l2_provenance(manifest: &Manifest, sources: &[Manifest]) -> ValidationResult<()> {
    // All sources must be L0 or L1
    for source in sources {
        match source.content_type {
            ContentType::L0 | ContentType::L1 => {}
            ContentType::L2 => {
                return Err(ValidationError::L2InvalidSourceType {
                    hash: format!("{}", source.hash),
                    content_type: "L2".to_string(),
                });
            }
            ContentType::L3 => {
                return Err(ValidationError::L2InvalidSourceType {
                    hash: format!("{}", source.hash),
                    content_type: "L3".to_string(),
                });
            }
            _ => {
                return Err(ValidationError::Internal(
                    "unknown content type in L2 source".to_string(),
                ));
            }
        }
    }

    // Verify root_l0l1 is properly computed from sources
    // (This reuses the existing provenance validation logic)
    crate::provenance::validate_provenance(manifest, sources)
}

/// Check if L2 content can be published (it cannot).
///
/// L2 content is always private and cannot be published to the network.
pub fn validate_l2_publish(_manifest: &Manifest) -> ValidationResult<()> {
    Err(ValidationError::L2CannotPublish)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nodalync_crypto::{content_hash, generate_identity, peer_id_from_public_key};
    use nodalync_types::{
        Entity, L1Reference, L2EntityGraph, Metadata, Relationship, RelationshipObject,
    };

    fn test_peer_id() -> nodalync_types::PeerId {
        let (_, public_key) = generate_identity();
        peer_id_from_public_key(&public_key)
    }

    fn create_l2_manifest(hash: nodalync_crypto::Hash) -> Manifest {
        let owner = test_peer_id();
        let metadata = Metadata::new("Test L2", 100);
        let mut manifest = Manifest::new_l0(hash, owner, metadata, 1234567890);
        manifest.content_type = ContentType::L2;
        // L2 provenance should have depth 1 and reference source L1s
        // For testing, we'll use a simplified provenance
        manifest
    }

    fn create_test_graph(hash: nodalync_crypto::Hash) -> L2EntityGraph {
        let mut graph = L2EntityGraph::new(hash);

        // Add a source L1
        let l1_hash = content_hash(b"test l1");
        let l0_hash = content_hash(b"test l0");
        graph.add_source_l1(L1Reference::new(l1_hash, l0_hash));

        // Add some entities
        graph.add_entity(Entity::new("e1", "Alice").with_type("schema:Person"));
        graph.add_entity(Entity::new("e2", "Bob").with_type("schema:Person"));

        // Add a relationship
        graph.add_relationship(Relationship::new(
            "r1",
            "e1",
            "schema:knows",
            RelationshipObject::entity("e2"),
        ));

        graph
    }

    #[test]
    fn test_valid_l2() {
        let hash = content_hash(b"test l2");
        let graph = create_test_graph(hash);
        let manifest = create_l2_manifest(hash);

        let result = validate_l2_content(&graph, &manifest);
        assert!(result.is_ok(), "Expected Ok, got {:?}", result);
    }

    #[test]
    fn test_l2_visibility_must_be_private() {
        let hash = content_hash(b"test l2");
        let graph = create_test_graph(hash);
        let mut manifest = create_l2_manifest(hash);
        manifest.visibility = Visibility::Shared;

        let result = validate_l2_content(&graph, &manifest);
        assert!(matches!(
            result,
            Err(ValidationError::L2VisibilityNotPrivate { .. })
        ));
    }

    #[test]
    fn test_l2_price_must_be_zero() {
        let hash = content_hash(b"test l2");
        let graph = create_test_graph(hash);
        let mut manifest = create_l2_manifest(hash);
        manifest.economics.price = 100;

        let result = validate_l2_content(&graph, &manifest);
        assert!(matches!(
            result,
            Err(ValidationError::L2PriceNotZero { .. })
        ));
    }

    #[test]
    fn test_l2_id_mismatch() {
        let hash = content_hash(b"test l2");
        let different_hash = content_hash(b"different");
        let graph = create_test_graph(different_hash);
        let manifest = create_l2_manifest(hash);

        let result = validate_l2_content(&graph, &manifest);
        assert!(matches!(result, Err(ValidationError::L2IdMismatch)));
    }

    #[test]
    fn test_l2_no_sources() {
        let hash = content_hash(b"test l2");
        let graph = L2EntityGraph::new(hash); // No sources
        let manifest = create_l2_manifest(hash);

        let result = validate_l2_content(&graph, &manifest);
        assert!(matches!(result, Err(ValidationError::L2NoSources)));
    }

    #[test]
    fn test_l2_duplicate_entity_id() {
        let hash = content_hash(b"test l2");
        let mut graph = L2EntityGraph::new(hash);

        let l1_hash = content_hash(b"test l1");
        let l0_hash = content_hash(b"test l0");
        graph.add_source_l1(L1Reference::new(l1_hash, l0_hash));

        // Add duplicate entity IDs
        graph.add_entity(Entity::new("e1", "Alice"));
        graph.add_entity(Entity::new("e1", "Bob")); // Duplicate!

        let manifest = create_l2_manifest(hash);

        let result = validate_l2_content(&graph, &manifest);
        assert!(matches!(result, Err(ValidationError::L2DuplicateEntityId)));
    }

    #[test]
    fn test_l2_invalid_entity_ref_in_relationship() {
        let hash = content_hash(b"test l2");
        let mut graph = L2EntityGraph::new(hash);

        let l1_hash = content_hash(b"test l1");
        let l0_hash = content_hash(b"test l0");
        graph.add_source_l1(L1Reference::new(l1_hash, l0_hash));

        graph.add_entity(Entity::new("e1", "Alice"));
        // Reference non-existent entity
        graph.add_relationship(Relationship::new(
            "r1",
            "e1",
            "schema:knows",
            RelationshipObject::entity("e_nonexistent"),
        ));

        let manifest = create_l2_manifest(hash);

        let result = validate_l2_content(&graph, &manifest);
        assert!(matches!(
            result,
            Err(ValidationError::L2InvalidEntityRef { .. })
        ));
    }

    #[test]
    fn test_is_valid_uri() {
        assert!(is_valid_uri("http://example.org/"));
        assert!(is_valid_uri("https://schema.org/Person"));
        assert!(!is_valid_uri("schema:Person")); // CURIE, not URI
        assert!(!is_valid_uri("not-a-uri"));
    }

    #[test]
    fn test_expand_curie() {
        let prefixes = PrefixMap::default();

        let result = expand_curie("schema:Person", &prefixes);
        assert_eq!(result.unwrap(), "http://schema.org/Person");

        let result = expand_curie("unknown:thing", &prefixes);
        assert!(result.is_err());
    }

    #[test]
    fn test_l2_confidence_out_of_range() {
        let hash = content_hash(b"test l2");
        let mut graph = L2EntityGraph::new(hash);

        let l1_hash = content_hash(b"test l1");
        let l0_hash = content_hash(b"test l0");
        graph.add_source_l1(L1Reference::new(l1_hash, l0_hash));

        // Entity with invalid confidence
        let mut entity = Entity::new("e1", "Alice");
        entity.confidence = 1.5; // Invalid: > 1.0
        graph.add_entity(entity);

        let manifest = create_l2_manifest(hash);

        let result = validate_l2_content(&graph, &manifest);
        assert!(matches!(
            result,
            Err(ValidationError::L2InvalidConfidence { .. })
        ));
    }

    #[test]
    fn test_l2_cannot_publish() {
        let hash = content_hash(b"test l2");
        let manifest = create_l2_manifest(hash);

        let result = validate_l2_publish(&manifest);
        assert!(matches!(result, Err(ValidationError::L2CannotPublish)));
    }

    #[test]
    fn test_entity_count_mismatch() {
        let hash = content_hash(b"test l2");
        let mut graph = create_test_graph(hash);
        graph.entity_count = 999; // Wrong count

        let manifest = create_l2_manifest(hash);

        let result = validate_l2_content(&graph, &manifest);
        assert!(matches!(
            result,
            Err(ValidationError::L2EntityCountMismatch { .. })
        ));
    }
}
