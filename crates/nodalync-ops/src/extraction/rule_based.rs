//! Rule-based L1 extraction implementation.
//!
//! This module provides a simple keyword-based extractor for the MVP.
//! It uses heuristics to classify sentences and extract entities.

use nodalync_crypto::content_hash;
use nodalync_types::{Classification, Confidence, LocationType, Mention, SourceLocation};

use super::L1Extractor;
use crate::error::OpsResult;

/// Rule-based L1 extractor using keyword heuristics.
///
/// This is the MVP implementation that uses simple rules to:
/// - Split text into sentences
/// - Classify each sentence by keywords
/// - Extract entities (capitalized words)
#[derive(Debug, Clone, Default)]
pub struct RuleBasedExtractor {
    /// Minimum sentence length to consider for extraction.
    min_sentence_length: usize,
    /// Maximum entities per mention.
    max_entities: usize,
}

impl RuleBasedExtractor {
    /// Create a new rule-based extractor with default settings.
    pub fn new() -> Self {
        Self {
            min_sentence_length: 10,
            max_entities: 10,
        }
    }

    /// Create an extractor with custom settings.
    pub fn with_settings(min_sentence_length: usize, max_entities: usize) -> Self {
        Self {
            min_sentence_length,
            max_entities,
        }
    }

    /// Split text into sentences.
    fn split_sentences(text: &str) -> Vec<(usize, String)> {
        let mut sentences = Vec::new();
        let mut current = String::new();
        let mut para_num = 1;

        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                if !current.is_empty() {
                    sentences.push((para_num, current.clone()));
                    current.clear();
                }
                para_num += 1;
            } else {
                // Simple sentence splitting on . ! ?
                for sentence in split_on_terminators(trimmed) {
                    if !sentence.is_empty() {
                        if !current.is_empty() {
                            current.push(' ');
                        }
                        current.push_str(&sentence);

                        // Check if this is a complete sentence
                        if sentence.ends_with('.')
                            || sentence.ends_with('!')
                            || sentence.ends_with('?')
                        {
                            sentences.push((para_num, current.clone()));
                            current.clear();
                        }
                    }
                }
            }
        }

        // Don't forget the last sentence
        if !current.is_empty() {
            sentences.push((para_num, current));
        }

        sentences
    }

    /// Classify a sentence based on keywords.
    fn classify_sentence(sentence: &str) -> Classification {
        let lower = sentence.to_lowercase();

        // Check for specific patterns
        if lower.contains("we found")
            || lower.contains("results show")
            || lower.contains("data indicates")
        {
            return Classification::Result;
        }

        if lower.contains("claim")
            || lower.contains("argue")
            || lower.contains("assert")
            || lower.contains("believe")
        {
            return Classification::Claim;
        }

        if lower.contains("observed")
            || lower.contains("measured")
            || lower.contains("recorded")
            || lower.contains("noted")
        {
            return Classification::Observation;
        }

        if lower.contains("define")
            || lower.contains("definition")
            || lower.contains("is a")
            || lower.contains("refers to")
        {
            return Classification::Definition;
        }

        if lower.contains("said") || lower.contains("stated") || lower.contains("according to") {
            return Classification::Observation; // Treat quotes as observations
        }

        if lower.contains("statistic")
            || lower.contains("percent")
            || lower.contains("%")
            || has_numbers(&lower)
        {
            return Classification::Statistic;
        }

        // Default to claim
        Classification::Claim
    }

    /// Extract entities (capitalized words) from text.
    fn extract_entities(&self, sentence: &str) -> Vec<String> {
        let mut entities = Vec::new();
        let mut current_entity = String::new();

        for word in sentence.split_whitespace() {
            let clean_word = word.trim_matches(|c: char| !c.is_alphanumeric());
            if clean_word.is_empty() {
                continue;
            }

            let first_char = clean_word.chars().next().unwrap();
            if first_char.is_uppercase() && !is_common_word(clean_word) {
                if !current_entity.is_empty() {
                    current_entity.push(' ');
                }
                current_entity.push_str(clean_word);
            } else if !current_entity.is_empty() {
                // End of entity
                if current_entity.len() > 1 {
                    entities.push(current_entity.clone());
                }
                current_entity.clear();
            }
        }

        // Don't forget last entity
        if !current_entity.is_empty() && current_entity.len() > 1 {
            entities.push(current_entity);
        }

        // Deduplicate and limit
        entities.sort();
        entities.dedup();
        entities.truncate(self.max_entities);

        entities
    }
}

impl L1Extractor for RuleBasedExtractor {
    fn extract(&self, content: &[u8], _mime_type: Option<&str>) -> OpsResult<Vec<Mention>> {
        // Convert to string (assume UTF-8 for MVP)
        let text = match std::str::from_utf8(content) {
            Ok(s) => s,
            Err(_) => return Ok(Vec::new()), // Binary content, no mentions
        };

        let sentences = Self::split_sentences(text);
        let mut mentions = Vec::new();

        for (para_num, sentence) in sentences {
            // Skip short sentences
            if sentence.len() < self.min_sentence_length {
                continue;
            }

            let classification = Self::classify_sentence(&sentence);
            let entities = self.extract_entities(&sentence);

            // Compute mention ID as hash of content + location
            let id_input = format!("{}:{}", sentence, para_num);
            let id = content_hash(id_input.as_bytes());

            let source_location = SourceLocation::with_quote(
                LocationType::Paragraph,
                para_num.to_string(),
                truncate(&sentence, 500),
            );

            let mention = Mention::new(
                id,
                sentence.clone(),
                source_location,
                classification,
                Confidence::Explicit,
            )
            .with_entities(entities);

            mentions.push(mention);
        }

        Ok(mentions)
    }
}

/// Split text on sentence terminators while preserving the terminator.
fn split_on_terminators(text: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();

    for ch in text.chars() {
        current.push(ch);
        if ch == '.' || ch == '!' || ch == '?' {
            result.push(current.trim().to_string());
            current.clear();
        }
    }

    if !current.trim().is_empty() {
        result.push(current.trim().to_string());
    }

    result
}

/// Check if text contains numbers.
fn has_numbers(text: &str) -> bool {
    text.chars().any(|c| c.is_ascii_digit())
}

/// Check if a word is a common word (not a proper noun).
fn is_common_word(word: &str) -> bool {
    let common = [
        "The",
        "A",
        "An",
        "This",
        "That",
        "These",
        "Those",
        "I",
        "We",
        "You",
        "He",
        "She",
        "It",
        "They",
        "Is",
        "Are",
        "Was",
        "Were",
        "Be",
        "Been",
        "Being",
        "Have",
        "Has",
        "Had",
        "Do",
        "Does",
        "Did",
        "If",
        "When",
        "Where",
        "Why",
        "How",
        "What",
        "Which",
        "And",
        "Or",
        "But",
        "So",
        "Yet",
        "For",
        "Nor",
        "In",
        "On",
        "At",
        "To",
        "From",
        "With",
        "By",
        "However",
        "Therefore",
        "Moreover",
        "Furthermore",
    ];
    common.contains(&word)
}

/// Truncate a string to a maximum length.
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        let mut result = s.chars().take(max_len - 3).collect::<String>();
        result.push_str("...");
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_simple_text() {
        let extractor = RuleBasedExtractor::new();
        let content = b"This is a test sentence. It contains some facts. Results show improvement.";

        let mentions = extractor.extract(content, Some("text/plain")).unwrap();

        assert!(!mentions.is_empty());
        // Should have extracted the sentences
        assert!(mentions.iter().any(|m| m.content.contains("Results show")));
    }

    #[test]
    fn test_classify_result() {
        let classification =
            RuleBasedExtractor::classify_sentence("We found that the system works.");
        assert_eq!(classification, Classification::Result);
    }

    #[test]
    fn test_classify_claim() {
        let classification =
            RuleBasedExtractor::classify_sentence("They argue that this is correct.");
        assert_eq!(classification, Classification::Claim);
    }

    #[test]
    fn test_classify_observation() {
        let classification =
            RuleBasedExtractor::classify_sentence("We observed significant changes.");
        assert_eq!(classification, Classification::Observation);
    }

    #[test]
    fn test_classify_definition() {
        let classification = RuleBasedExtractor::classify_sentence("A protocol is a set of rules.");
        assert_eq!(classification, Classification::Definition);
    }

    #[test]
    fn test_classify_statistic() {
        let classification =
            RuleBasedExtractor::classify_sentence("The statistic shows 75% improvement.");
        assert_eq!(classification, Classification::Statistic);
    }

    #[test]
    fn test_extract_entities() {
        let extractor = RuleBasedExtractor::new();
        let entities =
            extractor.extract_entities("Apple and Microsoft announced partnerships with OpenAI.");

        assert!(entities.contains(&"Apple".to_string()));
        assert!(entities.contains(&"Microsoft".to_string()));
        assert!(entities.contains(&"OpenAI".to_string()));
    }

    #[test]
    fn test_extract_binary_content() {
        let extractor = RuleBasedExtractor::new();
        let binary_content = &[0xFF, 0xFE, 0x00, 0x01];

        let mentions = extractor.extract(binary_content, None).unwrap();
        assert!(mentions.is_empty()); // Binary content should return no mentions
    }

    #[test]
    fn test_split_sentences() {
        let sentences =
            RuleBasedExtractor::split_sentences("First sentence. Second sentence! Third sentence?");

        assert_eq!(sentences.len(), 3);
        assert!(sentences[0].1.contains("First"));
        assert!(sentences[1].1.contains("Second"));
        assert!(sentences[2].1.contains("Third"));
    }

    #[test]
    fn test_truncate() {
        assert_eq!(truncate("short", 10), "short");
        // 10 chars - 3 for "..." = 7 chars of content
        assert_eq!(truncate("a very long string", 10), "a very ...");
    }
}
