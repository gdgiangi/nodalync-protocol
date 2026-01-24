//! Content types for L1 mentions and summaries.
//!
//! This module defines the types for extracted knowledge (mentions)
//! and content summaries as specified in Protocol Specification §4.4, §4.9.

use nodalync_crypto::Hash;
use serde::{Deserialize, Serialize};

use crate::enums::{Classification, Confidence, LocationType};

/// Location reference within source content.
///
/// Spec §4.4: Identifies where in L0 content a mention was extracted from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SourceLocation {
    /// Type of location reference
    pub location_type: LocationType,
    /// Location identifier (paragraph number, page, timestamp, etc.)
    pub reference: String,
    /// Exact quote from source (max 500 chars)
    pub quote: Option<String>,
}

impl SourceLocation {
    /// Create a new source location.
    pub fn new(location_type: LocationType, reference: impl Into<String>) -> Self {
        Self {
            location_type,
            reference: reference.into(),
            quote: None,
        }
    }

    /// Create a source location with a quote.
    pub fn with_quote(
        location_type: LocationType,
        reference: impl Into<String>,
        quote: impl Into<String>,
    ) -> Self {
        Self {
            location_type,
            reference: reference.into(),
            quote: Some(quote.into()),
        }
    }

    /// Create a paragraph reference.
    pub fn paragraph(number: u32) -> Self {
        Self::new(LocationType::Paragraph, number.to_string())
    }

    /// Create a page reference.
    pub fn page(number: u32) -> Self {
        Self::new(LocationType::Page, number.to_string())
    }

    /// Create a timestamp reference (for audio/video).
    pub fn timestamp(seconds: u64) -> Self {
        Self::new(LocationType::Timestamp, format!("{}s", seconds))
    }

    /// Create a line reference.
    pub fn line(number: u32) -> Self {
        Self::new(LocationType::Line, number.to_string())
    }

    /// Create a section reference.
    pub fn section(name: impl Into<String>) -> Self {
        Self::new(LocationType::Section, name)
    }
}

impl Default for SourceLocation {
    fn default() -> Self {
        Self {
            location_type: LocationType::Paragraph,
            reference: String::new(),
            quote: None,
        }
    }
}

/// An extracted atomic fact from L0 content.
///
/// Spec §4.4: A mention represents a single piece of knowledge
/// extracted from source content, with full provenance information.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Mention {
    /// H(content || source_location) - unique identifier
    pub id: Hash,
    /// The atomic fact (max 1000 chars)
    pub content: String,
    /// Where in L0 this fact came from
    pub source_location: SourceLocation,
    /// Type of fact
    pub classification: Classification,
    /// How certain we are this is in the source
    pub confidence: Confidence,
    /// Extracted entity names
    pub entities: Vec<String>,
}

impl Mention {
    /// Create a new mention.
    ///
    /// Note: The id should be computed as H(content || source_location)
    /// by the caller.
    pub fn new(
        id: Hash,
        content: impl Into<String>,
        source_location: SourceLocation,
        classification: Classification,
        confidence: Confidence,
    ) -> Self {
        Self {
            id,
            content: content.into(),
            source_location,
            classification,
            confidence,
            entities: Vec::new(),
        }
    }

    /// Add entities to the mention.
    pub fn with_entities(mut self, entities: Vec<String>) -> Self {
        self.entities = entities;
        self
    }

    /// Check if this mention is explicitly stated (not inferred).
    pub fn is_explicit(&self) -> bool {
        self.confidence == Confidence::Explicit
    }

    /// Check if this mention has a quote from the source.
    pub fn has_quote(&self) -> bool {
        self.source_location.quote.is_some()
    }
}

/// Summary of L1 content extracted from L0.
///
/// Spec §4.9: Provides a preview of extracted knowledge without
/// revealing all content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct L1Summary {
    /// Source L0 hash
    pub l0_hash: Hash,
    /// Total mentions extracted
    pub mention_count: u32,
    /// First N mentions (max 5)
    pub preview_mentions: Vec<Mention>,
    /// Main topics (max 5)
    pub primary_topics: Vec<String>,
    /// 2-3 sentence summary (max 500 chars)
    pub summary: String,
}

impl L1Summary {
    /// Create a new L1 summary.
    pub fn new(
        l0_hash: Hash,
        mention_count: u32,
        preview_mentions: Vec<Mention>,
        primary_topics: Vec<String>,
        summary: impl Into<String>,
    ) -> Self {
        Self {
            l0_hash,
            mention_count,
            preview_mentions,
            primary_topics,
            summary: summary.into(),
        }
    }

    /// Create an empty summary for content with no extractable mentions.
    pub fn empty(l0_hash: Hash) -> Self {
        Self {
            l0_hash,
            mention_count: 0,
            preview_mentions: Vec::new(),
            primary_topics: Vec::new(),
            summary: String::new(),
        }
    }

    /// Check if the summary has any preview mentions.
    pub fn has_previews(&self) -> bool {
        !self.preview_mentions.is_empty()
    }

    /// Get the number of mentions not shown in preview.
    pub fn hidden_mention_count(&self) -> u32 {
        self.mention_count
            .saturating_sub(self.preview_mentions.len() as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nodalync_crypto::content_hash;

    fn test_hash(data: &[u8]) -> Hash {
        content_hash(data)
    }

    #[test]
    fn test_source_location_new() {
        let loc = SourceLocation::new(LocationType::Paragraph, "5");

        assert_eq!(loc.location_type, LocationType::Paragraph);
        assert_eq!(loc.reference, "5");
        assert!(loc.quote.is_none());
    }

    #[test]
    fn test_source_location_with_quote() {
        let loc = SourceLocation::with_quote(LocationType::Page, "42", "The quick brown fox");

        assert_eq!(loc.location_type, LocationType::Page);
        assert_eq!(loc.reference, "42");
        assert_eq!(loc.quote, Some("The quick brown fox".to_string()));
    }

    #[test]
    fn test_source_location_helpers() {
        let para = SourceLocation::paragraph(5);
        assert_eq!(para.location_type, LocationType::Paragraph);
        assert_eq!(para.reference, "5");

        let page = SourceLocation::page(42);
        assert_eq!(page.location_type, LocationType::Page);
        assert_eq!(page.reference, "42");

        let ts = SourceLocation::timestamp(120);
        assert_eq!(ts.location_type, LocationType::Timestamp);
        assert_eq!(ts.reference, "120s");

        let line = SourceLocation::line(100);
        assert_eq!(line.location_type, LocationType::Line);
        assert_eq!(line.reference, "100");

        let section = SourceLocation::section("Introduction");
        assert_eq!(section.location_type, LocationType::Section);
        assert_eq!(section.reference, "Introduction");
    }

    #[test]
    fn test_mention_new() {
        let id = test_hash(b"mention content");
        let loc = SourceLocation::paragraph(1);

        let mention = Mention::new(
            id,
            "This is an atomic fact",
            loc,
            Classification::Claim,
            Confidence::Explicit,
        );

        assert_eq!(mention.id, id);
        assert_eq!(mention.content, "This is an atomic fact");
        assert_eq!(mention.classification, Classification::Claim);
        assert_eq!(mention.confidence, Confidence::Explicit);
        assert!(mention.entities.is_empty());
        assert!(mention.is_explicit());
    }

    #[test]
    fn test_mention_with_entities() {
        let id = test_hash(b"mention");
        let loc = SourceLocation::paragraph(1);

        let mention = Mention::new(
            id,
            "Apple announced new products",
            loc,
            Classification::Observation,
            Confidence::Explicit,
        )
        .with_entities(vec!["Apple".to_string()]);

        assert_eq!(mention.entities.len(), 1);
        assert_eq!(mention.entities[0], "Apple");
    }

    #[test]
    fn test_mention_has_quote() {
        let id = test_hash(b"mention");

        let loc_no_quote = SourceLocation::paragraph(1);
        let mention_no_quote = Mention::new(
            id,
            "fact",
            loc_no_quote,
            Classification::Claim,
            Confidence::Explicit,
        );
        assert!(!mention_no_quote.has_quote());

        let loc_with_quote =
            SourceLocation::with_quote(LocationType::Paragraph, "1", "original text");
        let mention_with_quote = Mention::new(
            id,
            "fact",
            loc_with_quote,
            Classification::Claim,
            Confidence::Explicit,
        );
        assert!(mention_with_quote.has_quote());
    }

    #[test]
    fn test_l1_summary_new() {
        let l0_hash = test_hash(b"source document");
        let mention_id = test_hash(b"mention1");
        let mention = Mention::new(
            mention_id,
            "Key finding",
            SourceLocation::paragraph(1),
            Classification::Result,
            Confidence::Explicit,
        );

        let summary = L1Summary::new(
            l0_hash,
            10,
            vec![mention],
            vec!["Science".to_string(), "Research".to_string()],
            "This document discusses scientific research findings.",
        );

        assert_eq!(summary.l0_hash, l0_hash);
        assert_eq!(summary.mention_count, 10);
        assert_eq!(summary.preview_mentions.len(), 1);
        assert_eq!(summary.primary_topics.len(), 2);
        assert!(summary.has_previews());
    }

    #[test]
    fn test_l1_summary_empty() {
        let l0_hash = test_hash(b"empty doc");
        let summary = L1Summary::empty(l0_hash);

        assert_eq!(summary.l0_hash, l0_hash);
        assert_eq!(summary.mention_count, 0);
        assert!(summary.preview_mentions.is_empty());
        assert!(summary.primary_topics.is_empty());
        assert!(summary.summary.is_empty());
        assert!(!summary.has_previews());
    }

    #[test]
    fn test_l1_summary_hidden_count() {
        let l0_hash = test_hash(b"doc");
        let mention_id = test_hash(b"m1");
        let mention = Mention::new(
            mention_id,
            "fact",
            SourceLocation::paragraph(1),
            Classification::Claim,
            Confidence::Explicit,
        );

        let summary = L1Summary::new(
            l0_hash,
            100,           // Total
            vec![mention], // Only 1 preview
            vec![],
            "",
        );

        assert_eq!(summary.hidden_mention_count(), 99);
    }

    #[test]
    fn test_source_location_serialization() {
        let loc = SourceLocation::with_quote(LocationType::Page, "42", "The quote");

        let json = serde_json::to_string(&loc).unwrap();
        let deserialized: SourceLocation = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized, loc);
    }

    #[test]
    fn test_mention_serialization() {
        let id = test_hash(b"mention");
        let mention = Mention::new(
            id,
            "content",
            SourceLocation::paragraph(1),
            Classification::Claim,
            Confidence::Explicit,
        );

        let json = serde_json::to_string(&mention).unwrap();
        let deserialized: Mention = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.id, mention.id);
        assert_eq!(deserialized.content, mention.content);
    }

    #[test]
    fn test_l1_summary_serialization() {
        let l0_hash = test_hash(b"doc");
        let summary = L1Summary::empty(l0_hash);

        let json = serde_json::to_string(&summary).unwrap();
        let deserialized: L1Summary = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.l0_hash, summary.l0_hash);
    }
}
