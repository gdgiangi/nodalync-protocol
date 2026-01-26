//! Enum types for the Nodalync protocol.
//!
//! This module contains all enumeration types used across the protocol,
//! as defined in Protocol Specification §4.

use serde::{Deserialize, Serialize};

/// Type of content in the knowledge hierarchy.
///
/// Spec §4.1: ContentType identifies the layer of processed knowledge.
/// - L0: Raw input (documents, notes, transcripts)
/// - L1: Mentions (extracted atomic facts)
/// - L2: Entity Graph (personal knowledge graph, always private)
/// - L3: Insights (emergent synthesis)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[repr(u8)]
#[non_exhaustive]
pub enum ContentType {
    /// Raw input (documents, notes, transcripts)
    #[default]
    L0 = 0x00,
    /// Mentions (extracted atomic facts)
    L1 = 0x01,
    /// Entity Graph (personal knowledge graph, always private)
    L2 = 0x02,
    /// Insights (emergent synthesis)
    L3 = 0x03,
}

/// Visibility level for content.
///
/// Spec §4.2: Controls how content is discovered and served.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[repr(u8)]
#[non_exhaustive]
pub enum Visibility {
    /// Local only, not served to others
    #[default]
    Private = 0x00,
    /// Served if hash known, not announced to DHT
    Unlisted = 0x01,
    /// Announced to DHT, publicly queryable
    Shared = 0x02,
}

/// Type of location reference within source content.
///
/// Spec §4.4: Used in SourceLocation to identify where in L0 content
/// a mention was extracted from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[repr(u8)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum LocationType {
    /// Reference by paragraph number
    #[default]
    Paragraph = 0x00,
    /// Reference by page number
    Page = 0x01,
    /// Reference by timestamp (for audio/video)
    Timestamp = 0x02,
    /// Reference by line number
    Line = 0x03,
    /// Reference by section name/number
    Section = 0x04,
}

/// Classification of a mention (atomic fact type).
///
/// Spec §4.4: Categorizes the type of information in a mention.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[repr(u8)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum Classification {
    /// An assertion or statement
    #[default]
    Claim = 0x00,
    /// Numerical data or metric
    Statistic = 0x01,
    /// Explanation of a term or concept
    Definition = 0x02,
    /// Noted phenomenon or behavior
    Observation = 0x03,
    /// Process or technique description
    Method = 0x04,
    /// Outcome or finding
    Result = 0x05,
}

/// Confidence level for mention extraction.
///
/// Spec §4.4: Indicates how certain we are that the information
/// is present in the source content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[repr(u8)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    /// Directly stated in source
    #[default]
    Explicit = 0x00,
    /// Reasonably inferred from context
    Inferred = 0x01,
}

/// Currency type for payments.
///
/// Spec §4.7: The protocol uses HBAR (Hedera native token) for all payments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[repr(u8)]
#[non_exhaustive]
pub enum Currency {
    /// Hedera native token (1 HBAR = 10^8 tinybars)
    #[default]
    HBAR = 0x00,
}

/// State of a payment channel.
///
/// Spec §5.3: Tracks the lifecycle of a payment channel between peers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[repr(u8)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum ChannelState {
    /// Channel open request sent, awaiting acceptance
    #[default]
    Opening = 0x00,
    /// Channel is active and can process payments
    Open = 0x01,
    /// Channel close initiated, awaiting finalization
    Closing = 0x02,
    /// Channel is closed and settled
    Closed = 0x03,
    /// Channel state is disputed on-chain
    Disputed = 0x04,
}

/// Method used to resolve an entity mention to a canonical entity.
///
/// Spec §4.5: Tracks how entity resolution was performed in L2 Entity Graphs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[repr(u8)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum ResolutionMethod {
    /// Exact string match
    #[default]
    ExactMatch = 0x00,
    /// Normalized form match (case, whitespace, punctuation)
    Normalized = 0x01,
    /// Matched via alias
    Alias = 0x02,
    /// Coreference resolution (pronouns, anaphora)
    Coreference = 0x03,
    /// Linked to external knowledge base
    ExternalLink = 0x04,
    /// Manual user assignment
    Manual = 0x05,
    /// AI-assisted resolution
    AIAssisted = 0x06,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_type_values() {
        assert_eq!(ContentType::L0 as u8, 0x00);
        assert_eq!(ContentType::L1 as u8, 0x01);
        assert_eq!(ContentType::L2 as u8, 0x02);
        assert_eq!(ContentType::L3 as u8, 0x03);
    }

    #[test]
    fn test_visibility_values() {
        assert_eq!(Visibility::Private as u8, 0x00);
        assert_eq!(Visibility::Unlisted as u8, 0x01);
        assert_eq!(Visibility::Shared as u8, 0x02);
    }

    #[test]
    fn test_location_type_values() {
        assert_eq!(LocationType::Paragraph as u8, 0x00);
        assert_eq!(LocationType::Page as u8, 0x01);
        assert_eq!(LocationType::Timestamp as u8, 0x02);
        assert_eq!(LocationType::Line as u8, 0x03);
        assert_eq!(LocationType::Section as u8, 0x04);
    }

    #[test]
    fn test_classification_values() {
        assert_eq!(Classification::Claim as u8, 0x00);
        assert_eq!(Classification::Statistic as u8, 0x01);
        assert_eq!(Classification::Definition as u8, 0x02);
        assert_eq!(Classification::Observation as u8, 0x03);
        assert_eq!(Classification::Method as u8, 0x04);
        assert_eq!(Classification::Result as u8, 0x05);
    }

    #[test]
    fn test_confidence_values() {
        assert_eq!(Confidence::Explicit as u8, 0x00);
        assert_eq!(Confidence::Inferred as u8, 0x01);
    }

    #[test]
    fn test_currency_values() {
        assert_eq!(Currency::HBAR as u8, 0x00);
    }

    #[test]
    fn test_channel_state_values() {
        assert_eq!(ChannelState::Opening as u8, 0x00);
        assert_eq!(ChannelState::Open as u8, 0x01);
        assert_eq!(ChannelState::Closing as u8, 0x02);
        assert_eq!(ChannelState::Closed as u8, 0x03);
        assert_eq!(ChannelState::Disputed as u8, 0x04);
    }

    #[test]
    fn test_content_type_serialization() {
        let ct = ContentType::L1;
        let json = serde_json::to_string(&ct).unwrap();
        assert_eq!(json, "\"L1\"");
        let deserialized: ContentType = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, ct);
    }

    #[test]
    fn test_visibility_serialization() {
        let v = Visibility::Shared;
        let json = serde_json::to_string(&v).unwrap();
        assert_eq!(json, "\"Shared\"");
        let deserialized: Visibility = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, v);
    }

    #[test]
    fn test_channel_state_serialization() {
        let state = ChannelState::Open;
        let json = serde_json::to_string(&state).unwrap();
        assert_eq!(json, "\"open\"");
        let deserialized: ChannelState = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, state);
    }

    #[test]
    fn test_defaults() {
        assert_eq!(ContentType::default(), ContentType::L0);
        assert_eq!(Visibility::default(), Visibility::Private);
        assert_eq!(LocationType::default(), LocationType::Paragraph);
        assert_eq!(Classification::default(), Classification::Claim);
        assert_eq!(Confidence::default(), Confidence::Explicit);
        assert_eq!(Currency::default(), Currency::HBAR);
        assert_eq!(ChannelState::default(), ChannelState::Opening);
        assert_eq!(ResolutionMethod::default(), ResolutionMethod::ExactMatch);
    }

    #[test]
    fn test_copy_trait() {
        let ct = ContentType::L3;
        let ct_copy = ct; // Copy, not move
        assert_eq!(ct, ct_copy);

        let v = Visibility::Unlisted;
        let v_copy = v;
        assert_eq!(v, v_copy);
    }

    #[test]
    fn test_resolution_method_values() {
        assert_eq!(ResolutionMethod::ExactMatch as u8, 0x00);
        assert_eq!(ResolutionMethod::Normalized as u8, 0x01);
        assert_eq!(ResolutionMethod::Alias as u8, 0x02);
        assert_eq!(ResolutionMethod::Coreference as u8, 0x03);
        assert_eq!(ResolutionMethod::ExternalLink as u8, 0x04);
        assert_eq!(ResolutionMethod::Manual as u8, 0x05);
        assert_eq!(ResolutionMethod::AIAssisted as u8, 0x06);
    }

    #[test]
    fn test_hash_trait() {
        use std::collections::HashSet;

        let mut set = HashSet::new();
        set.insert(ContentType::L0);
        set.insert(ContentType::L1);
        set.insert(ContentType::L2);
        set.insert(ContentType::L3);
        assert_eq!(set.len(), 4);
        assert!(set.contains(&ContentType::L2));
    }
}
