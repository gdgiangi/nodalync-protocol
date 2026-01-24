//! L2 Entity Graph types for the Nodalync protocol.
//!
//! This module contains all types related to L2 Entity Graphs, which are
//! personal knowledge graphs that are always private and enable L3 insights.
//!
//! Key L2 Design Constraints:
//! - Visibility = Private always (never Shared/Unlisted)
//! - Price = 0 always (never monetized directly)
//! - Uses URI-based ontology (RDF interop via PrefixMap/CURIEs)
//! - Creators earn through L3 synthesis fees, not L2 queries
//! - `root_L0L1` contains ONLY L0/L1 entries (never L2/L3)

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::enums::ResolutionMethod;
use nodalync_crypto::Hash;

/// URI type alias for semantic web compatibility.
pub type Uri = String;

/// A prefix entry in a PrefixMap for CURIE expansion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrefixEntry {
    /// The prefix (e.g., "schema", "foaf", "dc")
    pub prefix: String,
    /// The full URI the prefix expands to
    pub uri: Uri,
}

impl PrefixEntry {
    /// Create a new prefix entry.
    pub fn new(prefix: impl Into<String>, uri: impl Into<String>) -> Self {
        Self {
            prefix: prefix.into(),
            uri: uri.into(),
        }
    }
}

/// A mapping of prefixes to URIs for CURIE expansion.
///
/// CURIEs (Compact URIs) allow writing `schema:Person` instead of
/// `http://schema.org/Person`. The PrefixMap stores these mappings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrefixMap {
    /// The prefix entries
    pub entries: Vec<PrefixEntry>,
}

impl Default for PrefixMap {
    /// Create a PrefixMap with standard prefixes.
    fn default() -> Self {
        Self {
            entries: vec![
                PrefixEntry::new("ndl", "https://nodalync.io/ontology/"),
                PrefixEntry::new("schema", "http://schema.org/"),
                PrefixEntry::new("foaf", "http://xmlns.com/foaf/0.1/"),
                PrefixEntry::new("dc", "http://purl.org/dc/elements/1.1/"),
                PrefixEntry::new("rdf", "http://www.w3.org/1999/02/22-rdf-syntax-ns#"),
                PrefixEntry::new("rdfs", "http://www.w3.org/2000/01/rdf-schema#"),
                PrefixEntry::new("xsd", "http://www.w3.org/2001/XMLSchema#"),
                PrefixEntry::new("owl", "http://www.w3.org/2002/07/owl#"),
            ],
        }
    }
}

impl PrefixMap {
    /// Create a new empty PrefixMap.
    pub fn new() -> Self {
        Self { entries: vec![] }
    }

    /// Create a PrefixMap with standard prefixes.
    pub fn with_defaults() -> Self {
        Self::default()
    }

    /// Add a prefix entry.
    pub fn add(&mut self, prefix: impl Into<String>, uri: impl Into<String>) {
        self.entries.push(PrefixEntry::new(prefix, uri));
    }

    /// Get the URI for a prefix.
    pub fn get(&self, prefix: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|e| e.prefix == prefix)
            .map(|e| e.uri.as_str())
    }

    /// Expand a CURIE to a full URI.
    ///
    /// Returns `None` if the prefix is not found.
    pub fn expand(&self, curie: &str) -> Option<String> {
        if let Some((prefix, local)) = curie.split_once(':') {
            self.get(prefix).map(|uri| format!("{}{}", uri, local))
        } else {
            None
        }
    }

    /// Check if a string is a valid CURIE with a known prefix.
    pub fn is_valid_curie(&self, curie: &str) -> bool {
        if let Some((prefix, _)) = curie.split_once(':') {
            self.get(prefix).is_some()
        } else {
            false
        }
    }
}

/// Reference to an L1 source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct L1Reference {
    /// Hash of the L1 content
    pub l1_hash: Hash,
    /// Hash of the L0 source (for provenance tracking)
    pub l0_hash: Hash,
}

impl L1Reference {
    /// Create a new L1 reference.
    pub fn new(l1_hash: Hash, l0_hash: Hash) -> Self {
        Self { l1_hash, l0_hash }
    }
}

/// Reference to a specific mention within an L1.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MentionRef {
    /// Reference to the L1 containing the mention
    pub l1_ref: L1Reference,
    /// Index of the mention within the L1
    pub mention_index: u32,
    /// The resolution method used to link this mention
    pub resolution_method: ResolutionMethod,
    /// Confidence score (0.0 to 1.0)
    pub confidence: f32,
}

impl MentionRef {
    /// Create a new mention reference.
    pub fn new(
        l1_ref: L1Reference,
        mention_index: u32,
        resolution_method: ResolutionMethod,
        confidence: f32,
    ) -> Self {
        Self {
            l1_ref,
            mention_index,
            resolution_method,
            confidence,
        }
    }
}

/// A literal value in a relationship (for data properties).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiteralValue {
    /// The literal value as a string
    pub value: String,
    /// The datatype URI (e.g., xsd:string, xsd:integer)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub datatype: Option<Uri>,
    /// Language tag for string literals (e.g., "en", "fr")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
}

impl LiteralValue {
    /// Create a new string literal.
    pub fn string(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            datatype: None,
            language: None,
        }
    }

    /// Create a new typed literal.
    pub fn typed(value: impl Into<String>, datatype: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            datatype: Some(datatype.into()),
            language: None,
        }
    }

    /// Create a new language-tagged literal.
    pub fn lang(value: impl Into<String>, language: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            datatype: None,
            language: Some(language.into()),
        }
    }
}

/// The object of a relationship (either an entity or a literal).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RelationshipObject {
    /// Reference to another entity by ID
    Entity { entity_id: String },
    /// A literal value
    Literal(LiteralValue),
    /// An external URI reference
    Uri { uri: Uri },
}

impl RelationshipObject {
    /// Create an entity reference.
    pub fn entity(id: impl Into<String>) -> Self {
        Self::Entity {
            entity_id: id.into(),
        }
    }

    /// Create a literal value.
    pub fn literal(value: LiteralValue) -> Self {
        Self::Literal(value)
    }

    /// Create a URI reference.
    pub fn uri(uri: impl Into<String>) -> Self {
        Self::Uri { uri: uri.into() }
    }
}

/// An entity in the knowledge graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    /// Unique identifier within this L2 graph
    pub id: String,
    /// Canonical label for the entity
    pub canonical_label: String,
    /// Alternative names/aliases for the entity
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    /// Entity type URI (e.g., schema:Person, foaf:Organization)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_type: Option<Uri>,
    /// Optional description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// External KB URIs this entity is linked to (owl:sameAs)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub external_links: Vec<Uri>,
    /// Mentions that resolved to this entity
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mention_refs: Vec<MentionRef>,
    /// Overall confidence score for this entity (0.0 to 1.0)
    pub confidence: f32,
    /// Arbitrary metadata
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, String>,
}

impl Entity {
    /// Create a new entity.
    pub fn new(id: impl Into<String>, canonical_label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            canonical_label: canonical_label.into(),
            aliases: Vec::new(),
            entity_type: None,
            description: None,
            external_links: Vec::new(),
            mention_refs: Vec::new(),
            confidence: 1.0,
            metadata: HashMap::new(),
        }
    }

    /// Set the entity type.
    pub fn with_type(mut self, entity_type: impl Into<String>) -> Self {
        self.entity_type = Some(entity_type.into());
        self
    }

    /// Add an alias.
    pub fn with_alias(mut self, alias: impl Into<String>) -> Self {
        self.aliases.push(alias.into());
        self
    }

    /// Add aliases.
    pub fn with_aliases(mut self, aliases: Vec<String>) -> Self {
        self.aliases.extend(aliases);
        self
    }

    /// Set the description.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Add an external link.
    pub fn with_external_link(mut self, uri: impl Into<String>) -> Self {
        self.external_links.push(uri.into());
        self
    }

    /// Add a mention reference.
    pub fn with_mention_ref(mut self, mention_ref: MentionRef) -> Self {
        self.mention_refs.push(mention_ref);
        self
    }

    /// Set the confidence score.
    pub fn with_confidence(mut self, confidence: f32) -> Self {
        self.confidence = confidence;
        self
    }
}

/// A relationship between entities or between an entity and a literal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relationship {
    /// Unique identifier within this L2 graph
    pub id: String,
    /// Subject entity ID
    pub subject: String,
    /// Predicate URI (e.g., schema:knows, foaf:name)
    pub predicate: Uri,
    /// Object (entity reference, literal, or URI)
    pub object: RelationshipObject,
    /// Confidence score (0.0 to 1.0)
    pub confidence: f32,
    /// Mentions that support this relationship
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mention_refs: Vec<MentionRef>,
    /// Arbitrary metadata
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, String>,
}

impl Relationship {
    /// Create a new relationship.
    pub fn new(
        id: impl Into<String>,
        subject: impl Into<String>,
        predicate: impl Into<String>,
        object: RelationshipObject,
    ) -> Self {
        Self {
            id: id.into(),
            subject: subject.into(),
            predicate: predicate.into(),
            object,
            confidence: 1.0,
            mention_refs: Vec::new(),
            metadata: HashMap::new(),
        }
    }

    /// Set the confidence score.
    pub fn with_confidence(mut self, confidence: f32) -> Self {
        self.confidence = confidence;
        self
    }

    /// Add a mention reference.
    pub fn with_mention_ref(mut self, mention_ref: MentionRef) -> Self {
        self.mention_refs.push(mention_ref);
        self
    }
}

/// An L2 Entity Graph.
///
/// L2 Entity Graphs are personal knowledge graphs that:
/// - Are always private (visibility = Private)
/// - Have price = 0 (never monetized directly)
/// - Enable L3 insights
/// - Creators earn through L3 synthesis fees
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct L2EntityGraph {
    /// Unique identifier (should match manifest hash)
    pub id: Hash,
    /// Prefix mappings for CURIE expansion
    #[serde(default)]
    pub prefixes: PrefixMap,
    /// L1 sources this graph was built from
    pub source_l1s: Vec<L1Reference>,
    /// Source L2 graphs (for merged graphs)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_l2s: Vec<Hash>,
    /// Entities in the graph
    pub entities: Vec<Entity>,
    /// Relationships in the graph
    pub relationships: Vec<Relationship>,
    /// Count of entities (for quick validation)
    pub entity_count: u32,
    /// Count of relationships (for quick validation)
    pub relationship_count: u32,
    /// Schema version for this L2 format
    #[serde(default = "default_schema_version")]
    pub schema_version: String,
    /// Arbitrary metadata
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, String>,
}

fn default_schema_version() -> String {
    "0.2.0".to_string()
}

impl L2EntityGraph {
    /// Create a new empty L2 Entity Graph.
    pub fn new(id: Hash) -> Self {
        Self {
            id,
            prefixes: PrefixMap::default(),
            source_l1s: Vec::new(),
            source_l2s: Vec::new(),
            entities: Vec::new(),
            relationships: Vec::new(),
            entity_count: 0,
            relationship_count: 0,
            schema_version: default_schema_version(),
            metadata: HashMap::new(),
        }
    }

    /// Add an entity to the graph.
    pub fn add_entity(&mut self, entity: Entity) {
        self.entities.push(entity);
        self.entity_count = self.entities.len() as u32;
    }

    /// Add a relationship to the graph.
    pub fn add_relationship(&mut self, relationship: Relationship) {
        self.relationships.push(relationship);
        self.relationship_count = self.relationships.len() as u32;
    }

    /// Add an L1 source.
    pub fn add_source_l1(&mut self, l1_ref: L1Reference) {
        self.source_l1s.push(l1_ref);
    }

    /// Add an L2 source (for merging).
    pub fn add_source_l2(&mut self, l2_hash: Hash) {
        self.source_l2s.push(l2_hash);
    }

    /// Get an entity by ID.
    pub fn get_entity(&self, id: &str) -> Option<&Entity> {
        self.entities.iter().find(|e| e.id == id)
    }

    /// Get a mutable entity by ID.
    pub fn get_entity_mut(&mut self, id: &str) -> Option<&mut Entity> {
        self.entities.iter_mut().find(|e| e.id == id)
    }

    /// Get relationships where the entity is the subject.
    pub fn get_outgoing_relationships(&self, entity_id: &str) -> Vec<&Relationship> {
        self.relationships
            .iter()
            .filter(|r| r.subject == entity_id)
            .collect()
    }

    /// Get relationships where the entity is the object.
    pub fn get_incoming_relationships(&self, entity_id: &str) -> Vec<&Relationship> {
        self.relationships
            .iter()
            .filter(|r| match &r.object {
                RelationshipObject::Entity { entity_id: id } => id == entity_id,
                _ => false,
            })
            .collect()
    }

    /// Update counts to match actual collections.
    pub fn sync_counts(&mut self) {
        self.entity_count = self.entities.len() as u32;
        self.relationship_count = self.relationships.len() as u32;
    }
}

/// Configuration for building an L2 Entity Graph from L1 sources.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct L2BuildConfig {
    /// Minimum confidence threshold for entity resolution
    #[serde(default = "default_min_confidence")]
    pub min_confidence: f32,
    /// Whether to link entities to external knowledge bases
    #[serde(default)]
    pub enable_external_linking: bool,
    /// Whether to merge entities with high confidence
    #[serde(default = "default_auto_merge")]
    pub auto_merge_entities: bool,
    /// Custom prefix mappings to add
    #[serde(default)]
    pub custom_prefixes: Vec<PrefixEntry>,
}

fn default_min_confidence() -> f32 {
    0.5
}

fn default_auto_merge() -> bool {
    true
}

impl Default for L2BuildConfig {
    fn default() -> Self {
        Self {
            min_confidence: default_min_confidence(),
            enable_external_linking: false,
            auto_merge_entities: default_auto_merge(),
            custom_prefixes: Vec::new(),
        }
    }
}

/// Configuration for merging L2 Entity Graphs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct L2MergeConfig {
    /// Strategy for handling entity conflicts
    #[serde(default)]
    pub conflict_resolution: ConflictResolution,
    /// Minimum confidence for cross-graph entity matching
    #[serde(default = "default_min_confidence")]
    pub min_match_confidence: f32,
    /// Whether to preserve source graph metadata
    #[serde(default)]
    pub preserve_metadata: bool,
}

impl Default for L2MergeConfig {
    fn default() -> Self {
        Self {
            conflict_resolution: ConflictResolution::default(),
            min_match_confidence: default_min_confidence(),
            preserve_metadata: false,
        }
    }
}

/// Strategy for resolving entity conflicts during merge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ConflictResolution {
    /// Keep the entity with higher confidence
    #[default]
    HigherConfidence,
    /// Keep the first entity encountered
    First,
    /// Keep the most recent entity (by source timestamp)
    MostRecent,
    /// Merge all data from both entities
    MergeAll,
}

#[cfg(test)]
mod tests {
    use super::*;
    use nodalync_crypto::content_hash;

    #[test]
    fn test_prefix_map_default() {
        let map = PrefixMap::default();
        assert!(map.get("schema").is_some());
        assert!(map.get("foaf").is_some());
        assert!(map.get("ndl").is_some());
    }

    #[test]
    fn test_prefix_map_expand() {
        let map = PrefixMap::default();
        assert_eq!(
            map.expand("schema:Person"),
            Some("http://schema.org/Person".to_string())
        );
        assert_eq!(
            map.expand("foaf:name"),
            Some("http://xmlns.com/foaf/0.1/name".to_string())
        );
        assert_eq!(map.expand("unknown:thing"), None);
    }

    #[test]
    fn test_prefix_map_is_valid_curie() {
        let map = PrefixMap::default();
        assert!(map.is_valid_curie("schema:Person"));
        assert!(!map.is_valid_curie("unknown:thing"));
        assert!(!map.is_valid_curie("not-a-curie"));
    }

    #[test]
    fn test_entity_creation() {
        let entity = Entity::new("e1", "John Doe")
            .with_type("schema:Person")
            .with_alias("John")
            .with_alias("J. Doe")
            .with_description("A person named John")
            .with_confidence(0.95);

        assert_eq!(entity.id, "e1");
        assert_eq!(entity.canonical_label, "John Doe");
        assert_eq!(entity.aliases.len(), 2);
        assert_eq!(entity.entity_type, Some("schema:Person".to_string()));
        assert_eq!(entity.confidence, 0.95);
    }

    #[test]
    fn test_relationship_creation() {
        let rel = Relationship::new(
            "r1",
            "e1",
            "schema:knows",
            RelationshipObject::entity("e2"),
        )
        .with_confidence(0.9);

        assert_eq!(rel.id, "r1");
        assert_eq!(rel.subject, "e1");
        assert_eq!(rel.predicate, "schema:knows");
        assert!(matches!(rel.object, RelationshipObject::Entity { .. }));
        assert_eq!(rel.confidence, 0.9);
    }

    #[test]
    fn test_l2_entity_graph() {
        let hash = content_hash(b"test");
        let mut graph = L2EntityGraph::new(hash);

        let entity1 = Entity::new("e1", "Alice").with_type("schema:Person");
        let entity2 = Entity::new("e2", "Bob").with_type("schema:Person");

        graph.add_entity(entity1);
        graph.add_entity(entity2);

        let rel = Relationship::new(
            "r1",
            "e1",
            "schema:knows",
            RelationshipObject::entity("e2"),
        );
        graph.add_relationship(rel);

        assert_eq!(graph.entity_count, 2);
        assert_eq!(graph.relationship_count, 1);
        assert!(graph.get_entity("e1").is_some());
        assert!(graph.get_entity("e3").is_none());
    }

    #[test]
    fn test_literal_value() {
        let string_lit = LiteralValue::string("hello");
        assert_eq!(string_lit.value, "hello");
        assert!(string_lit.datatype.is_none());

        let typed_lit = LiteralValue::typed("42", "xsd:integer");
        assert_eq!(typed_lit.datatype, Some("xsd:integer".to_string()));

        let lang_lit = LiteralValue::lang("bonjour", "fr");
        assert_eq!(lang_lit.language, Some("fr".to_string()));
    }

    #[test]
    fn test_relationship_object_variants() {
        let entity_obj = RelationshipObject::entity("e1");
        assert!(matches!(entity_obj, RelationshipObject::Entity { .. }));

        let literal_obj = RelationshipObject::literal(LiteralValue::string("test"));
        assert!(matches!(literal_obj, RelationshipObject::Literal(_)));

        let uri_obj = RelationshipObject::uri("http://example.org/resource");
        assert!(matches!(uri_obj, RelationshipObject::Uri { .. }));
    }

    #[test]
    fn test_l2_build_config_default() {
        let config = L2BuildConfig::default();
        assert_eq!(config.min_confidence, 0.5);
        assert!(!config.enable_external_linking);
        assert!(config.auto_merge_entities);
    }

    #[test]
    fn test_l2_merge_config_default() {
        let config = L2MergeConfig::default();
        assert_eq!(config.conflict_resolution, ConflictResolution::HigherConfidence);
        assert_eq!(config.min_match_confidence, 0.5);
        assert!(!config.preserve_metadata);
    }

    #[test]
    fn test_graph_traversal() {
        let hash = content_hash(b"test");
        let mut graph = L2EntityGraph::new(hash);

        graph.add_entity(Entity::new("e1", "Alice"));
        graph.add_entity(Entity::new("e2", "Bob"));
        graph.add_entity(Entity::new("e3", "Charlie"));

        graph.add_relationship(Relationship::new(
            "r1",
            "e1",
            "schema:knows",
            RelationshipObject::entity("e2"),
        ));
        graph.add_relationship(Relationship::new(
            "r2",
            "e1",
            "schema:knows",
            RelationshipObject::entity("e3"),
        ));
        graph.add_relationship(Relationship::new(
            "r3",
            "e2",
            "schema:knows",
            RelationshipObject::entity("e1"),
        ));

        let outgoing = graph.get_outgoing_relationships("e1");
        assert_eq!(outgoing.len(), 2);

        let incoming = graph.get_incoming_relationships("e1");
        assert_eq!(incoming.len(), 1);
    }
}
