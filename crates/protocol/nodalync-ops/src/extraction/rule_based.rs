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
    ///
    /// Each sentence is paired with its 1-based paragraph number; a blank line starts a
    /// new paragraph and ends any unfinished sentence.
    fn split_sentences(text: &str) -> Vec<(usize, String)> {
        let mut sentences = Vec::new();
        let mut current = String::new();
        let mut para_num = 1;

        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                flush_sentence(&mut sentences, &mut current, para_num);
                para_num += 1;
                continue;
            }
            // Simple sentence splitting on . ! ?
            for fragment in split_on_terminators(trimmed) {
                push_fragment(&mut sentences, &mut current, para_num, &fragment);
            }
        }

        // Don't forget the last sentence
        flush_sentence(&mut sentences, &mut current, para_num);

        sentences
    }

    /// Classify a sentence based on keywords.
    ///
    /// The first keyword group that matches decides; with no match, a sentence with a
    /// digit is a statistic and anything else a claim.
    fn classify_sentence(sentence: &str) -> Classification {
        let lower = sentence.to_lowercase();

        // Check for specific patterns
        let keyword_match = CLASSIFICATION_KEYWORDS
            .iter()
            .find(|(keywords, _)| keywords.iter().any(|keyword| lower.contains(keyword)))
            .map(|(_, classification)| *classification);

        match keyword_match {
            Some(classification) => classification,
            None if has_numbers(&lower) => Classification::Statistic,
            // Default to claim
            None => Classification::Claim,
        }
    }

    /// Extract entities (capitalized words) from text.
    ///
    /// Runs of capitalized words that are not common words form one entity; entities
    /// of a single character are dropped.
    fn extract_entities(&self, sentence: &str) -> Vec<String> {
        let mut entities = Vec::new();
        let mut current_entity = String::new();

        for word in sentence.split_whitespace() {
            let clean_word = word.trim_matches(|c: char| !c.is_alphanumeric());
            if clean_word.is_empty() {
                continue;
            }

            if is_entity_word(clean_word) {
                append_word(&mut current_entity, clean_word);
            } else {
                // End of entity
                finish_entity(&mut entities, &mut current_entity);
            }
        }

        // Don't forget last entity
        finish_entity(&mut entities, &mut current_entity);

        // Deduplicate and limit
        entities.sort();
        entities.dedup();
        entities.truncate(self.max_entities);

        entities
    }
}

/// Keyword groups, checked in order, and the classification each implies.
const CLASSIFICATION_KEYWORDS: &[(&[&str], Classification)] = &[
    (
        &["we found", "results show", "data indicates"],
        Classification::Result,
    ),
    (
        &["claim", "argue", "assert", "believe"],
        Classification::Claim,
    ),
    (
        &["observed", "measured", "recorded", "noted"],
        Classification::Observation,
    ),
    (
        &["define", "definition", "is a", "refers to"],
        Classification::Definition,
    ),
    // Treat quotes as observations
    (
        &["said", "stated", "according to"],
        Classification::Observation,
    ),
    (&["statistic", "percent", "%"], Classification::Statistic),
];

/// Push the unfinished sentence, if any, and start a new one.
fn flush_sentence(sentences: &mut Vec<(usize, String)>, current: &mut String, para_num: usize) {
    if !current.is_empty() {
        sentences.push((para_num, std::mem::take(current)));
    }
}

/// Append a fragment to the current sentence, ending the sentence if the fragment
/// ends with a terminator.
fn push_fragment(
    sentences: &mut Vec<(usize, String)>,
    current: &mut String,
    para_num: usize,
    fragment: &str,
) {
    if fragment.is_empty() {
        return;
    }
    append_word(current, fragment);

    // Check if this is a complete sentence
    if fragment.ends_with(['.', '!', '?']) {
        flush_sentence(sentences, current, para_num);
    }
}

/// Append `word` to `text`, separated by a space when `text` is not empty.
fn append_word(text: &mut String, word: &str) {
    if !text.is_empty() {
        text.push(' ');
    }
    text.push_str(word);
}

/// Whether a cleaned word can be part of an entity: capitalized and not a common word.
fn is_entity_word(word: &str) -> bool {
    let starts_upper = word.chars().next().is_some_and(char::is_uppercase);
    starts_upper && !is_common_word(word)
}

/// End the current entity, keeping it if it is longer than one character.
fn finish_entity(entities: &mut Vec<String>, current: &mut String) {
    let entity = std::mem::take(current);
    if entity.len() > 1 {
        entities.push(entity);
    }
}

impl L1Extractor for RuleBasedExtractor {
    fn extract(&self, content: &[u8], _mime_type: Option<&str>) -> OpsResult<Vec<Mention>> {
        // Convert to string (assume UTF-8 for MVP)
        let text = match std::str::from_utf8(content) {
            Ok(s) => s,
            Err(_) => return Ok(Vec::new()), // Binary content, no mentions
        };

        let clean_text = strip_markdown(text);
        let sentences = Self::split_sentences(&clean_text);
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

/// Strip common markdown formatting from text for cleaner mention extraction.
fn strip_markdown(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    for line in text.lines() {
        let trimmed = line.trim();
        // Strip heading markers: "## Title" -> "Title"
        let clean = if trimmed.starts_with('#') {
            trimmed.trim_start_matches('#').trim_start()
        } else if trimmed.starts_with('>') {
            // Strip blockquote markers
            trimmed.trim_start_matches('>').trim_start()
        } else {
            trimmed
        };
        // Strip bold markers (** and __)
        let clean = clean.replace("**", "").replace("__", "");
        // Strip inline code backticks
        let clean = clean.replace('`', "");
        // Strip link syntax: [text](url) -> text
        let clean = strip_link_syntax(&clean);
        result.push_str(&clean);
        result.push('\n');
    }
    result
}

/// Convert markdown links `[text](url)` to just `text`.
fn strip_link_syntax(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '[' {
            result.push_str(&take_bracketed(&mut chars));
        } else {
            result.push(ch);
        }
    }
    result
}

/// After a `[`, consume a link's `text](url)` and return `text`; for anything that is
/// not a link, return what was consumed unchanged (with its `[`).
fn take_bracketed(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> String {
    let (link_text, found_close) = take_until(chars, ']');
    if found_close && chars.peek() == Some(&'(') {
        chars.next(); // consume '('
        take_until(chars, ')');
        return link_text;
    }

    // Not a real link, preserve original
    let close = if found_close { "]" } else { "" };
    format!("[{}{}", link_text, close)
}

/// Consume characters up to and including `close`. Returns the characters before it
/// and whether `close` was found.
fn take_until(chars: &mut impl Iterator<Item = char>, close: char) -> (String, bool) {
    let mut taken = String::new();
    for ch in chars.by_ref() {
        if ch == close {
            return (taken, true);
        }
        taken.push(ch);
    }
    (taken, false)
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

    #[test]
    fn test_strip_markdown_headings() {
        assert_eq!(strip_markdown("# Heading").trim(), "Heading");
        assert_eq!(strip_markdown("## Sub Heading").trim(), "Sub Heading");
        assert_eq!(strip_markdown("### Deep Heading").trim(), "Deep Heading");
    }

    #[test]
    fn test_strip_markdown_bold() {
        assert_eq!(strip_markdown("**bold** text").trim(), "bold text");
        assert_eq!(strip_markdown("__also bold__").trim(), "also bold");
    }

    #[test]
    fn test_strip_markdown_blockquote() {
        assert_eq!(strip_markdown("> quoted text").trim(), "quoted text");
    }

    #[test]
    fn test_strip_markdown_links() {
        assert_eq!(
            strip_markdown("[link text](http://example.com)").trim(),
            "link text"
        );
    }

    #[test]
    fn test_strip_markdown_backticks() {
        assert_eq!(strip_markdown("`code`").trim(), "code");
    }

    #[test]
    fn test_strip_markdown_regular_text() {
        assert_eq!(
            strip_markdown("regular text unchanged").trim(),
            "regular text unchanged"
        );
    }

    #[test]
    fn test_strip_markdown_preserves_list_markers() {
        // Single * for lists should be preserved
        assert_eq!(strip_markdown("* list item").trim(), "* list item");
    }

    #[test]
    fn test_strip_link_syntax_not_a_link() {
        // Brackets without parens should be preserved
        assert_eq!(strip_link_syntax("[not a link]"), "[not a link]");
    }

    #[test]
    fn test_extract_strips_markdown() {
        let extractor = RuleBasedExtractor::new();
        let content = b"# The Impact of Technology\n\nResults show **significant** improvement.";

        let mentions = extractor.extract(content, Some("text/plain")).unwrap();

        // Should not contain markdown syntax
        for mention in &mentions {
            assert!(!mention.content.contains('#'));
            assert!(!mention.content.contains("**"));
        }
    }
}

#[cfg(test)]
mod golden_tests {
    //! Outputs recorded from the extractor before it was split into helper
    //! functions; they must not change.
    use super::*;

    const TEXT: &str = "# Heading Here\n\nWe found that Alice Smith met Bob in Paris. The data indicates 42% growth!\nIs this a test? Researchers claim New York is big\n\n> Quoted text said by John Doe.\nSee [the docs](http://x.y) and [not a link] and [open\n\nThe rate is 3 per day. Nothing special here at all\nObserved values were recorded. A definition refers to meaning.";

    #[test]
    fn test_golden_sentences_classes_and_entities() {
        let clean = strip_markdown(TEXT);
        assert_eq!(clean, "Heading Here\n\nWe found that Alice Smith met Bob in Paris. The data indicates 42% growth!\nIs this a test? Researchers claim New York is big\n\nQuoted text said by John Doe.\nSee the docs and [not a link] and [open\n\nThe rate is 3 per day. Nothing special here at all\nObserved values were recorded. A definition refers to meaning.\n");

        let sentences = RuleBasedExtractor::split_sentences(&clean);
        let expected: Vec<(usize, &str, Classification, Vec<&str>)> = vec![
            (
                1,
                "Heading Here",
                Classification::Claim,
                vec!["Heading Here"],
            ),
            (
                2,
                "We found that Alice Smith met Bob in Paris.",
                Classification::Result,
                vec!["Alice Smith", "Bob", "Paris"],
            ),
            (
                2,
                "The data indicates 42% growth!",
                Classification::Result,
                vec![],
            ),
            (2, "Is this a test?", Classification::Definition, vec![]),
            (
                2,
                "Researchers claim New York is big",
                Classification::Claim,
                vec!["New York", "Researchers"],
            ),
            (
                3,
                "Quoted text said by John Doe.",
                Classification::Observation,
                vec!["John Doe", "Quoted"],
            ),
            (
                3,
                "See the docs and [not a link] and [open",
                Classification::Claim,
                vec!["See"],
            ),
            (
                4,
                "The rate is 3 per day.",
                Classification::Statistic,
                vec![],
            ),
            (
                4,
                "Nothing special here at all Observed values were recorded.",
                Classification::Observation,
                vec!["Nothing", "Observed"],
            ),
            (
                4,
                "A definition refers to meaning.",
                Classification::Definition,
                vec![],
            ),
        ];
        assert_eq!(sentences.len(), expected.len());

        let extractor = RuleBasedExtractor::with_settings(10, 3);
        for ((para, sentence), (exp_para, exp_sentence, exp_class, exp_entities)) in
            sentences.iter().zip(expected)
        {
            assert_eq!(*para, exp_para);
            assert_eq!(sentence, exp_sentence);
            assert_eq!(
                RuleBasedExtractor::classify_sentence(sentence),
                exp_class,
                "{}",
                sentence
            );
            assert_eq!(
                extractor.extract_entities(sentence),
                exp_entities,
                "{}",
                sentence
            );
        }
    }

    #[test]
    fn test_golden_link_stripping() {
        for (input, expected) in [
            ("[a](b)c", "ac"),
            ("[a]x", "[a]x"),
            ("[abc", "[abc"),
            ("x[a](b", "xa"),
            ("[[a]](b)", "[[a]](b)"),
        ] {
            assert_eq!(strip_link_syntax(input), expected, "{}", input);
        }
    }
}
