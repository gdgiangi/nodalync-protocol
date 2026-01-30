//! Operations trait definition.
//!
//! This module defines the main `Operations` trait that provides
//! the protocol operation interface as specified in §7.

use async_trait::async_trait;
use nodalync_crypto::{Hash, PeerId, Timestamp};
use nodalync_types::{
    AccessControl, Amount, Channel, L1Summary, L2BuildConfig, L2MergeConfig, Manifest, Metadata,
    Payment, Visibility,
};
use nodalync_wire::{VersionInfo, VersionSpec};

use crate::error::OpsResult;

/// Response from a query operation.
#[derive(Debug, Clone)]
pub struct QueryResponse {
    /// The requested content.
    pub content: Vec<u8>,
    /// The content manifest.
    pub manifest: Manifest,
    /// Payment receipt.
    pub receipt: nodalync_wire::PaymentReceipt,
}

/// Response from a preview operation.
#[derive(Debug, Clone)]
pub struct PreviewResponse {
    /// The content manifest (without full content).
    pub manifest: Manifest,
    /// L1 summary with preview mentions.
    pub l1_summary: L1Summary,
    /// The libp2p peer ID of the content provider (for opening payment channels).
    /// This is set when content is discovered via announcements, indicating
    /// which peer can serve the content (as opposed to the content owner).
    pub provider_peer_id: Option<String>,
}

/// Main operations trait for the Nodalync protocol.
///
/// This trait defines all protocol operations as specified in §7.
/// Implementations provide the core business logic that orchestrates
/// the storage, validation, and economic layers.
#[async_trait]
pub trait Operations: Send + Sync {
    // =========================================================================
    // Content Operations (§7.1)
    // =========================================================================

    /// Create new L0 content.
    ///
    /// Spec §7.1.1:
    /// - Computes content hash
    /// - Creates v1 Version
    /// - Creates L0 Provenance (self-referential)
    /// - Sets owner to creator
    /// - Creates Manifest
    /// - Validates content
    /// - Stores content and manifest
    ///
    /// Returns the content hash.
    async fn create(&mut self, content: &[u8], metadata: Metadata) -> OpsResult<Hash>;

    /// Extract L1 mentions from L0 content.
    ///
    /// Spec §7.1.2:
    /// - Loads content and manifest
    /// - Uses configured extractor
    /// - Generates L1Summary
    /// - Stores L1 data
    ///
    /// Returns the L1 summary.
    async fn extract_l1(&mut self, hash: &Hash) -> OpsResult<L1Summary>;

    /// Publish content to the network.
    ///
    /// Spec §7.1.3:
    /// - Loads manifest
    /// - Validates price
    /// - Updates visibility, price, access_control
    /// - Saves manifest
    /// - (DHT announce - stub for MVP)
    async fn publish(
        &mut self,
        hash: &Hash,
        visibility: Visibility,
        price: Amount,
    ) -> OpsResult<()>;

    /// Unpublish content from the network.
    ///
    /// Spec §7.1.3:
    /// - Sets visibility to Private
    /// - (DHT remove - stub for MVP)
    async fn unpublish(&mut self, hash: &Hash) -> OpsResult<()>;

    /// Update existing content.
    ///
    /// Spec §7.1.4:
    /// - Computes new hash
    /// - Links version (previous, root from previous.root)
    /// - Inherits visibility
    /// - Stores
    ///
    /// Returns the new content hash.
    async fn update(
        &mut self,
        old_hash: &Hash,
        new_content: &[u8],
        new_metadata: Metadata,
    ) -> OpsResult<Hash>;

    /// Derive new content from sources.
    ///
    /// Spec §7.1.5:
    /// - Verifies all sources were queried
    /// - Loads source manifests
    /// - Merges root_L0L1 with weight accumulation
    /// - Calculates depth = max(sources.depth) + 1
    /// - Creates L3 manifest with provenance
    /// - Validates provenance
    /// - Stores
    ///
    /// Returns the derived content hash.
    async fn derive(
        &mut self,
        sources: &[Hash],
        insight: &[u8],
        metadata: Metadata,
    ) -> OpsResult<Hash>;

    /// Reference an L3 as L0 (promotes synthesis to primary source).
    ///
    /// Spec §7.1.6:
    /// - Verifies L3 was queried (in cache)
    /// - Verifies content_type is L3
    /// - Stores reference
    async fn reference_l3_as_l0(&mut self, l3_hash: &Hash) -> OpsResult<Hash>;

    // =========================================================================
    // Query Operations (§7.2)
    // =========================================================================

    /// Preview content metadata and L1 summary.
    ///
    /// Spec §7.2.2:
    /// - Loads manifest
    /// - Checks visibility (Private returns NotFound for external)
    /// - Checks access control
    /// - Gets or extracts L1Summary
    /// - Returns (Manifest, L1Summary)
    async fn preview(&self, hash: &Hash) -> OpsResult<PreviewResponse>;

    /// Query and retrieve full content.
    ///
    /// Spec §7.2.3 (requester side):
    /// - Gets preview for price/owner
    /// - Auto-opens channel if none exists
    /// - Validates payment amount >= price
    /// - Checks channel balance
    /// - (Network call - stub for MVP)
    /// - Verifies response hash
    /// - Updates channel state (debit)
    /// - Caches content
    async fn query(
        &mut self,
        hash: &Hash,
        payment_amount: Amount,
        version: Option<VersionSpec>,
    ) -> OpsResult<QueryResponse>;

    /// Get all versions of content.
    ///
    /// Spec §7.4:
    /// - Gets all manifests with same version_root
    /// - Converts to VersionInfo
    async fn get_versions(&self, root_hash: &Hash) -> OpsResult<Vec<VersionInfo>>;

    // =========================================================================
    // Visibility Operations (§7.1.3)
    // =========================================================================

    /// Set visibility level for content.
    async fn set_visibility(&mut self, hash: &Hash, visibility: Visibility) -> OpsResult<()>;

    /// Set access control for content.
    async fn set_access(&mut self, hash: &Hash, access: AccessControl) -> OpsResult<()>;

    // =========================================================================
    // Channel Operations (§7.3)
    // =========================================================================

    /// Open a new payment channel with a peer.
    ///
    /// Spec §7.3.1:
    /// - Generates channel_id from hash(my_peer_id || peer_id || nonce)
    /// - Creates Channel with state=Opening
    /// - Stores locally
    /// - (Send ChannelOpen - stub for MVP)
    async fn open_channel(&mut self, peer: &PeerId, deposit: Amount) -> OpsResult<Channel>;

    /// Accept an incoming channel open request.
    ///
    /// Spec §7.3.2:
    /// - Validates no existing channel
    /// - Creates reciprocal Channel state
    /// - Stores
    async fn accept_channel(
        &mut self,
        channel_id: &Hash,
        peer: &PeerId,
        their_deposit: Amount,
        my_deposit: Amount,
    ) -> OpsResult<Channel>;

    /// Update channel state.
    async fn update_channel(&mut self, peer: &PeerId, payment: Payment) -> OpsResult<()>;

    /// Close a payment channel.
    ///
    /// Spec §7.3.3:
    /// - Gets channel
    /// - Computes final balances
    /// - (Submit to settlement - stub for MVP)
    /// - Updates state to Closed
    async fn close_channel(&mut self, peer: &PeerId) -> OpsResult<()>;

    /// Dispute a channel with latest signed state.
    ///
    /// Spec §7.3.4:
    /// - (Submit dispute to chain - stub for MVP)
    /// - Updates state to Disputed
    async fn dispute_channel(&mut self, peer: &PeerId) -> OpsResult<()>;

    // =========================================================================
    // Settlement Operations (§7.5)
    // =========================================================================

    /// Trigger settlement batch.
    ///
    /// Spec §7.5:
    /// - Checks should_settle (threshold OR interval)
    /// - Gets pending from queue
    /// - Creates batch via create_settlement_batch
    /// - (Submit to chain - stub for MVP)
    /// - Marks as settled
    /// - Updates last_settlement_time
    async fn trigger_settlement(&mut self) -> OpsResult<Option<Hash>>;

    // =========================================================================
    // L2 Entity Graph Operations
    // =========================================================================

    /// Build an L2 Entity Graph from L1 sources.
    ///
    /// L2 Entity Graphs are personal knowledge graphs that:
    /// - Are always private (visibility = Private)
    /// - Have price = 0 (never monetized directly)
    /// - Enable L3 insights
    /// - Creators earn through L3 synthesis fees
    ///
    /// # Arguments
    ///
    /// * `source_l1_hashes` - Hashes of L1 content to build from
    /// * `config` - Optional build configuration
    ///
    /// # Returns
    ///
    /// The hash of the created L2 content.
    async fn build_l2(
        &mut self,
        source_l1_hashes: Vec<Hash>,
        config: Option<L2BuildConfig>,
    ) -> OpsResult<Hash>;

    /// Merge multiple L2 Entity Graphs into a single L2.
    ///
    /// All source L2s must be owned by the current identity.
    ///
    /// # Arguments
    ///
    /// * `source_l2_hashes` - Hashes of L2 content to merge
    /// * `config` - Optional merge configuration
    ///
    /// # Returns
    ///
    /// The hash of the merged L2 content.
    async fn merge_l2(
        &mut self,
        source_l2_hashes: Vec<Hash>,
        config: Option<L2MergeConfig>,
    ) -> OpsResult<Hash>;

    // =========================================================================
    // Utility Methods
    // =========================================================================

    /// Get the local peer ID.
    fn my_peer_id(&self) -> PeerId;

    /// Get the current timestamp.
    fn now(&self) -> Timestamp;

    /// Get a manifest by hash (if owned locally).
    fn get_manifest(&self, hash: &Hash) -> OpsResult<Option<Manifest>>;

    /// Check if content was queried (is in cache).
    fn was_queried(&self, hash: &Hash) -> bool;
}
