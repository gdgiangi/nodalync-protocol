//! L1 extraction module.
//!
//! This module provides functionality for extracting L1 mentions from L0 content.
//! The extraction process identifies atomic facts and entities within source content.
//!
//! # Design
//!
//! The module uses a trait-based design (`L1Extractor`) to allow for different
//! extraction implementations:
//!
//! - `RuleBasedExtractor`: MVP implementation using keyword heuristics
//! - Future: AI-powered extractors (OpenAI, Claude, etc.)

mod rule_based;

pub use rule_based::RuleBasedExtractor;

use nodalync_types::Mention;

use crate::error::OpsResult;

/// Trait for extracting L1 mentions from L0 content.
///
/// Implementations of this trait analyze content and extract atomic facts
/// (mentions) with source locations, classifications, and entities.
pub trait L1Extractor: Send + Sync {
    /// Extract mentions from content.
    ///
    /// # Arguments
    /// * `content` - The raw content bytes
    /// * `mime_type` - Optional MIME type hint for content format
    ///
    /// # Returns
    /// A vector of extracted mentions
    fn extract(&self, content: &[u8], mime_type: Option<&str>) -> OpsResult<Vec<Mention>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extractor_is_object_safe() {
        // Verify the trait is object-safe by creating a trait object
        fn _takes_extractor(_: &dyn L1Extractor) {}
    }
}
