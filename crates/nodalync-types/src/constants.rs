//! Protocol constants from Nodalync Specification Appendix B.
//!
//! These constants define protocol-level limits, economic parameters,
//! timing constraints, and DHT configuration.

use crate::Amount;

// =============================================================================
// Protocol Version
// =============================================================================

/// Protocol magic byte (first byte of all messages)
pub const PROTOCOL_MAGIC: u8 = 0x00;

/// Current protocol version
pub const PROTOCOL_VERSION: u8 = 0x01;

// =============================================================================
// Content Limits
// =============================================================================

/// Maximum content size: 100 MB
pub const MAX_CONTENT_SIZE: u64 = 104_857_600;

/// Maximum message size: 10 MB
pub const MAX_MESSAGE_SIZE: u64 = 10_485_760;

/// Maximum mentions that can be extracted from a single L0
pub const MAX_MENTIONS_PER_L0: u32 = 1000;

/// Maximum sources that can be combined in a single L3
pub const MAX_SOURCES_PER_L3: u32 = 100;

/// Maximum provenance chain depth
pub const MAX_PROVENANCE_DEPTH: u32 = 100;

// =============================================================================
// Metadata Limits
// =============================================================================

/// Maximum number of tags per content item
pub const MAX_TAGS: usize = 20;

/// Maximum length of a single tag (characters)
pub const MAX_TAG_LENGTH: usize = 50;

/// Maximum title length (characters)
pub const MAX_TITLE_LENGTH: usize = 200;

/// Maximum description length (characters)
pub const MAX_DESCRIPTION_LENGTH: usize = 2000;

/// Maximum summary length (characters)
pub const MAX_SUMMARY_LENGTH: usize = 500;

/// Maximum mention content length (characters)
pub const MAX_MENTION_CONTENT_LENGTH: usize = 1000;

/// Maximum quote length in source location (characters)
pub const MAX_QUOTE_LENGTH: usize = 500;

/// Maximum preview mentions in L1Summary
pub const MAX_PREVIEW_MENTIONS: usize = 5;

/// Maximum primary topics in L1Summary
pub const MAX_PRIMARY_TOPICS: usize = 5;

// =============================================================================
// Economics
// =============================================================================

/// Minimum price per query (in smallest unit, 10^-8 NDL)
pub const MIN_PRICE: Amount = 1;

/// Maximum price per query (10^16 smallest units)
pub const MAX_PRICE: Amount = 10_000_000_000_000_000;

/// Synthesis fee numerator (5%)
pub const SYNTHESIS_FEE_NUMERATOR: u64 = 5;

/// Synthesis fee denominator (100)
pub const SYNTHESIS_FEE_DENOMINATOR: u64 = 100;

/// Settlement batch threshold: 100 NDL (in smallest units)
pub const SETTLEMENT_BATCH_THRESHOLD: Amount = 10_000_000_000;

/// Settlement batch interval: 1 hour (in milliseconds)
pub const SETTLEMENT_BATCH_INTERVAL_MS: u64 = 3_600_000;

// =============================================================================
// Timing
// =============================================================================

/// Message timeout: 30 seconds (in milliseconds)
pub const MESSAGE_TIMEOUT_MS: u64 = 30_000;

/// Channel dispute period: 24 hours (in milliseconds)
pub const CHANNEL_DISPUTE_PERIOD_MS: u64 = 86_400_000;

/// Maximum allowed clock skew: 5 minutes (in milliseconds)
pub const MAX_CLOCK_SKEW_MS: u64 = 300_000;

// =============================================================================
// DHT Configuration
// =============================================================================

/// Kademlia bucket size (k)
pub const DHT_BUCKET_SIZE: usize = 20;

/// Kademlia concurrency parameter (alpha)
pub const DHT_ALPHA: usize = 3;

/// Replication factor for DHT records
pub const DHT_REPLICATION: usize = 20;

// =============================================================================
// Retry Configuration
// =============================================================================

/// Maximum retry attempts for message delivery
pub const MAX_RETRY_ATTEMPTS: u32 = 3;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_limits_are_reasonable() {
        // 100 MB max content
        assert_eq!(MAX_CONTENT_SIZE, 100 * 1024 * 1024);
        // 10 MB max message
        assert_eq!(MAX_MESSAGE_SIZE, 10 * 1024 * 1024);
        // Message must be able to fit in content (with overhead)
        assert!(MAX_MESSAGE_SIZE < MAX_CONTENT_SIZE);
    }

    #[test]
    fn test_timing_constants() {
        // 30 second timeout
        assert_eq!(MESSAGE_TIMEOUT_MS, 30 * 1000);
        // 24 hour dispute period
        assert_eq!(CHANNEL_DISPUTE_PERIOD_MS, 24 * 60 * 60 * 1000);
        // 5 minute clock skew
        assert_eq!(MAX_CLOCK_SKEW_MS, 5 * 60 * 1000);
        // 1 hour batch interval
        assert_eq!(SETTLEMENT_BATCH_INTERVAL_MS, 60 * 60 * 1000);
    }

    #[test]
    fn test_economics_constants() {
        // Synthesis fee is 5%
        assert_eq!(
            SYNTHESIS_FEE_NUMERATOR as f64 / SYNTHESIS_FEE_DENOMINATOR as f64,
            0.05
        );
        // Min price is positive
        assert!(MIN_PRICE > 0);
        // Max price is greater than min
        assert!(MAX_PRICE > MIN_PRICE);
        // Batch threshold is 100 NDL (100 * 10^8 smallest units)
        assert_eq!(SETTLEMENT_BATCH_THRESHOLD, 100 * 100_000_000);
    }

    #[test]
    fn test_dht_constants() {
        // Standard Kademlia values
        assert_eq!(DHT_BUCKET_SIZE, 20);
        assert_eq!(DHT_ALPHA, 3);
        assert_eq!(DHT_REPLICATION, 20);
    }

    #[test]
    fn test_protocol_version() {
        assert_eq!(PROTOCOL_MAGIC, 0x00);
        assert_eq!(PROTOCOL_VERSION, 0x01);
    }

    #[test]
    fn test_metadata_limits() {
        assert_eq!(MAX_TAGS, 20);
        assert_eq!(MAX_TAG_LENGTH, 50);
        assert_eq!(MAX_TITLE_LENGTH, 200);
        assert_eq!(MAX_DESCRIPTION_LENGTH, 2000);
        assert_eq!(MAX_SUMMARY_LENGTH, 500);
        assert_eq!(MAX_MENTION_CONTENT_LENGTH, 1000);
        assert_eq!(MAX_QUOTE_LENGTH, 500);
    }
}
