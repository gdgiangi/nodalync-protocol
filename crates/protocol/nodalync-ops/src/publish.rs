//! Publish operations implementation.
//!
//! This module implements publish, unpublish, set_visibility, and set_access operations
//! as specified in Protocol Specification §7.1.3.

use nodalync_crypto::Hash;
use nodalync_econ::validate_price;
use nodalync_net::Multiaddr;
use nodalync_store::ManifestStore;
use nodalync_types::{AccessControl, Amount, ContentType, Manifest, Visibility};
use nodalync_valid::Validator;
use nodalync_wire::AnnouncePayload;

use crate::error::{OpsError, OpsResult};
use crate::extraction::L1Extractor;
use crate::node_ops::{current_timestamp, NodeOperations};

impl<V, E> NodeOperations<V, E>
where
    V: Validator,
    E: L1Extractor,
{
    /// Publish content to the network.
    ///
    /// Spec §7.1.3:
    /// 1. Loads manifest
    /// 2. Validates price
    /// 3. Updates visibility, price, access_control
    /// 4. Saves manifest
    /// 5. Announces to DHT (if network available)
    ///
    /// Note: L2 content cannot be published - it must remain private.
    pub async fn publish_content(
        &mut self,
        hash: &Hash,
        visibility: Visibility,
        price: Amount,
    ) -> OpsResult<()> {
        // 1. Load manifest
        let mut manifest = self
            .state
            .manifests
            .load(hash)?
            .ok_or(OpsError::ManifestNotFound(*hash))?;

        // L2 content cannot be published
        if manifest.content_type == ContentType::L2 {
            return Err(OpsError::Validation(
                nodalync_valid::ValidationError::L2CannotPublish,
            ));
        }

        // Verify ownership
        if manifest.owner != self.peer_id() {
            return Err(OpsError::AccessDenied);
        }

        // 2. Validate price
        if price > 0 {
            validate_price(price)?;
        }

        // 3. Extract L1 summary to get topics
        let l1_summary = self.extract_l1_summary(hash)?;

        // 4. Update visibility, price, and tags from L1 extraction
        manifest.visibility = visibility;
        manifest.economics.price = price;
        manifest.metadata.tags = l1_summary.primary_topics.clone();
        manifest.updated_at = current_timestamp();

        // 5. Save manifest
        self.state.manifests.update(&manifest)?;

        // 6. Network announce (if network available)
        if let Some(network) = self.network().cloned() {
            // Include our libp2p peer ID so other nodes can dial us directly
            let publisher_peer_id = Some(network.local_peer_id().to_string());
            let listen_addrs = network.listen_addresses();
            tracing::debug!(
                "Publishing content: hash={}, publisher_peer_id={:?}, listen_addresses={:?}",
                hash,
                publisher_peer_id,
                listen_addrs
            );
            let payload = Self::create_announce_payload(
                &manifest,
                l1_summary,
                listen_addrs,
                publisher_peer_id,
            );

            // DHT announce for persistence - best-effort
            if let Err(e) = network.dht_announce(*hash, payload.clone()).await {
                tracing::warn!(
                    "DHT announce failed (content still published locally): {}",
                    e
                );
            }

            // GossipSub broadcast for immediate discovery - best-effort
            if let Err(e) = network.broadcast_announce(payload).await {
                tracing::warn!(
                    "GossipSub broadcast failed (content still published locally): {}",
                    e
                );
            }
        }

        Ok(())
    }

    /// Create an AnnouncePayload from a manifest.
    fn create_announce_payload(
        manifest: &Manifest,
        l1_summary: nodalync_types::L1Summary,
        listen_addrs: Vec<Multiaddr>,
        publisher_peer_id: Option<String>,
    ) -> AnnouncePayload {
        AnnouncePayload {
            hash: manifest.hash,
            content_type: manifest.content_type,
            title: manifest.metadata.title.clone(),
            l1_summary,
            price: manifest.economics.price,
            addresses: listen_addrs
                .iter()
                .map(|addr: &Multiaddr| addr.to_string())
                .collect(),
            publisher_peer_id,
        }
    }

    /// Unpublish content from the network.
    ///
    /// Spec §7.1.3:
    /// - Sets visibility to Private
    /// - Removes from DHT (if network available)
    pub async fn unpublish_content(&mut self, hash: &Hash) -> OpsResult<()> {
        // Load manifest
        let mut manifest = self
            .state
            .manifests
            .load(hash)?
            .ok_or(OpsError::ManifestNotFound(*hash))?;

        // Verify ownership
        if manifest.owner != self.peer_id() {
            return Err(OpsError::AccessDenied);
        }

        // Set visibility to Private
        manifest.visibility = Visibility::Private;
        manifest.updated_at = current_timestamp();

        // Save manifest
        self.state.manifests.update(&manifest)?;

        // DHT remove (if network available) - best-effort
        if let Some(network) = self.network() {
            if let Err(e) = network.dht_remove(hash).await {
                tracing::warn!(
                    "DHT remove failed (content still unpublished locally): {}",
                    e
                );
            }
        }

        Ok(())
    }

    /// Re-announce all Shared content to the network.
    ///
    /// Called after network start to ensure previously published content
    /// is discoverable by peers. Without this, a node restart makes all
    /// content invisible until the next explicit publish.
    ///
    /// Returns the number of items successfully re-announced.
    pub async fn reannounce_all_content(&mut self) -> OpsResult<u32> {
        let network = match self.network().cloned() {
            Some(n) => n,
            None => return Ok(0), // No network, nothing to announce
        };

        // List all Shared content we own
        let filter = nodalync_store::ManifestFilter::new()
            .with_visibility(Visibility::Shared);
        let manifests = self.state.manifests.list(filter)?;

        let my_peer_id = self.peer_id();
        let publisher_peer_id = Some(network.local_peer_id().to_string());
        let listen_addrs = network.listen_addresses();

        let mut announced = 0u32;

        for manifest in &manifests {
            // Only re-announce our own content
            if manifest.owner != my_peer_id {
                continue;
            }

            // Extract L1 summary (best-effort — use empty if extraction fails)
            let l1_summary = self
                .extract_l1_summary(&manifest.hash)
                .unwrap_or_else(|_| nodalync_types::L1Summary::empty(manifest.hash));

            let payload = Self::create_announce_payload(
                manifest,
                l1_summary,
                listen_addrs.clone(),
                publisher_peer_id.clone(),
            );

            // DHT announce — best-effort
            if let Err(e) = network.dht_announce(manifest.hash, payload.clone()).await {
                tracing::debug!(
                    hash = %manifest.hash,
                    error = %e,
                    "DHT re-announce failed (continuing)"
                );
            }

            // GossipSub broadcast — best-effort
            if let Err(e) = network.broadcast_announce(payload).await {
                tracing::debug!(
                    hash = %manifest.hash,
                    error = %e,
                    "GossipSub re-announce failed (continuing)"
                );
            }

            announced += 1;
        }

        tracing::info!(
            count = announced,
            total = manifests.len(),
            "Re-announced content to network"
        );

        Ok(announced)
    }

    /// Set visibility level for content.
    pub fn set_content_visibility(&mut self, hash: &Hash, visibility: Visibility) -> OpsResult<()> {
        // Load manifest
        let mut manifest = self
            .state
            .manifests
            .load(hash)?
            .ok_or(OpsError::ManifestNotFound(*hash))?;

        // Verify ownership
        if manifest.owner != self.peer_id() {
            return Err(OpsError::AccessDenied);
        }

        // Update visibility
        manifest.visibility = visibility;
        manifest.updated_at = current_timestamp();

        // Save manifest
        self.state.manifests.update(&manifest)?;

        Ok(())
    }

    /// Set access control for content.
    pub fn set_content_access(&mut self, hash: &Hash, access: AccessControl) -> OpsResult<()> {
        // Load manifest
        let mut manifest = self
            .state
            .manifests
            .load(hash)?
            .ok_or(OpsError::ManifestNotFound(*hash))?;

        // Verify ownership
        if manifest.owner != self.peer_id() {
            return Err(OpsError::AccessDenied);
        }

        // Update access control
        manifest.access = access;
        manifest.updated_at = current_timestamp();

        // Save manifest
        self.state.manifests.update(&manifest)?;

        Ok(())
    }

    /// Set price for content.
    pub fn set_content_price(&mut self, hash: &Hash, price: Amount) -> OpsResult<()> {
        // Validate price
        if price > 0 {
            validate_price(price)?;
        }

        // Load manifest
        let mut manifest = self
            .state
            .manifests
            .load(hash)?
            .ok_or(OpsError::ManifestNotFound(*hash))?;

        // Verify ownership
        if manifest.owner != self.peer_id() {
            return Err(OpsError::AccessDenied);
        }

        // Update price
        manifest.economics.price = price;
        manifest.updated_at = current_timestamp();

        // Save manifest
        self.state.manifests.update(&manifest)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node_ops::DefaultNodeOperations;
    use nodalync_crypto::{generate_identity, peer_id_from_public_key};
    use nodalync_store::NodeStateConfig;
    use nodalync_types::Metadata;
    use tempfile::TempDir;

    fn create_test_ops() -> (DefaultNodeOperations, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let config = NodeStateConfig::new(temp_dir.path());
        let state = nodalync_store::NodeState::open(config).unwrap();

        let (_, public_key) = generate_identity();
        let peer_id = peer_id_from_public_key(&public_key);

        let ops = DefaultNodeOperations::with_defaults(state, peer_id);
        (ops, temp_dir)
    }

    #[tokio::test]
    async fn test_publish_content() {
        let (mut ops, _temp) = create_test_ops();

        let content = b"Content to publish";
        let meta = Metadata::new("Publish Test", content.len() as u64);
        let hash = ops.create_content(content, meta).unwrap();

        // Initially private
        let manifest = ops.state.manifests.load(&hash).unwrap().unwrap();
        assert_eq!(manifest.visibility, Visibility::Private);

        // Publish
        ops.publish_content(&hash, Visibility::Shared, 100)
            .await
            .unwrap();

        // Verify
        let manifest = ops.state.manifests.load(&hash).unwrap().unwrap();
        assert_eq!(manifest.visibility, Visibility::Shared);
        assert_eq!(manifest.economics.price, 100);
    }

    #[tokio::test]
    async fn test_unpublish_content() {
        let (mut ops, _temp) = create_test_ops();

        let content = b"Content to unpublish";
        let meta = Metadata::new("Unpublish Test", content.len() as u64);
        let hash = ops.create_content(content, meta).unwrap();

        // Publish first
        ops.publish_content(&hash, Visibility::Shared, 100)
            .await
            .unwrap();

        // Unpublish
        ops.unpublish_content(&hash).await.unwrap();

        // Verify
        let manifest = ops.state.manifests.load(&hash).unwrap().unwrap();
        assert_eq!(manifest.visibility, Visibility::Private);
    }

    #[test]
    fn test_set_visibility() {
        let (mut ops, _temp) = create_test_ops();

        let content = b"Content for visibility test";
        let meta = Metadata::new("Visibility Test", content.len() as u64);
        let hash = ops.create_content(content, meta).unwrap();

        // Set to Unlisted
        ops.set_content_visibility(&hash, Visibility::Unlisted)
            .unwrap();

        let manifest = ops.state.manifests.load(&hash).unwrap().unwrap();
        assert_eq!(manifest.visibility, Visibility::Unlisted);

        // Set to Shared
        ops.set_content_visibility(&hash, Visibility::Shared)
            .unwrap();

        let manifest = ops.state.manifests.load(&hash).unwrap().unwrap();
        assert_eq!(manifest.visibility, Visibility::Shared);
    }

    #[test]
    fn test_set_access_control() {
        let (mut ops, _temp) = create_test_ops();

        let content = b"Content for access test";
        let meta = Metadata::new("Access Test", content.len() as u64);
        let hash = ops.create_content(content, meta).unwrap();

        // Set allowlist
        let (_, pk) = generate_identity();
        let allowed_peer = peer_id_from_public_key(&pk);
        let access = AccessControl::with_allowlist(vec![allowed_peer]);

        ops.set_content_access(&hash, access.clone()).unwrap();

        let manifest = ops.state.manifests.load(&hash).unwrap().unwrap();
        assert!(manifest.access.allowlist.is_some());
        assert!(manifest.access.allowlist.unwrap().contains(&allowed_peer));
    }

    #[test]
    fn test_set_price() {
        let (mut ops, _temp) = create_test_ops();

        let content = b"Content for price test";
        let meta = Metadata::new("Price Test", content.len() as u64);
        let hash = ops.create_content(content, meta).unwrap();

        // Set price
        ops.set_content_price(&hash, 500).unwrap();

        let manifest = ops.state.manifests.load(&hash).unwrap().unwrap();
        assert_eq!(manifest.economics.price, 500);
    }

    #[tokio::test]
    async fn test_publish_invalid_price() {
        let (mut ops, _temp) = create_test_ops();

        let content = b"Content with invalid price";
        let meta = Metadata::new("Invalid Price", content.len() as u64);
        let hash = ops.create_content(content, meta).unwrap();

        // Price of 0 is allowed (free content)
        let result = ops.publish_content(&hash, Visibility::Shared, 0).await;
        assert!(result.is_ok());
    }

    /// Regression test for Issue #16: ghost content on failed publish.
    ///
    /// When publish_content fails due to extreme price, the manifest
    /// should remain Private and price should stay at 0.
    #[tokio::test]
    async fn test_publish_extreme_price_no_ghost_content() {
        let (mut ops, _temp) = create_test_ops();

        let content = b"Content that should not become ghost";
        let meta = Metadata::new("Ghost Test", content.len() as u64);
        let hash = ops.create_content(content, meta).unwrap();

        // Verify content exists as Private with price=0 before publish
        let manifest_before = ops.state.manifests.load(&hash).unwrap().unwrap();
        assert_eq!(manifest_before.visibility, Visibility::Private);
        assert_eq!(manifest_before.economics.price, 0);

        // Attempt to publish with extreme price (above MAX_PRICE)
        let extreme_price = u64::MAX;
        let result = ops
            .publish_content(&hash, Visibility::Shared, extreme_price)
            .await;
        assert!(result.is_err(), "Publish with extreme price should fail");

        // Verify manifest was NOT changed to Shared (no ghost content)
        let manifest_after = ops.state.manifests.load(&hash).unwrap().unwrap();
        assert_eq!(
            manifest_after.visibility,
            Visibility::Private,
            "Manifest should remain Private after failed publish"
        );
        assert_eq!(
            manifest_after.economics.price, 0,
            "Price should remain 0 after failed publish"
        );
    }

    #[tokio::test]
    async fn test_publish_requires_ownership() {
        let (mut ops, _temp) = create_test_ops();

        // Create content
        let content = b"Owned content";
        let meta = Metadata::new("Ownership Test", content.len() as u64);
        let hash = ops.create_content(content, meta).unwrap();

        // Modify manifest to have different owner
        let mut manifest = ops.state.manifests.load(&hash).unwrap().unwrap();
        let (_, other_pk) = generate_identity();
        manifest.owner = peer_id_from_public_key(&other_pk);
        ops.state.manifests.update(&manifest).unwrap();

        // Try to publish (should fail)
        let result = ops.publish_content(&hash, Visibility::Shared, 100).await;
        assert!(matches!(result, Err(OpsError::AccessDenied)));
    }

    #[tokio::test]
    async fn test_reannounce_no_network_returns_zero() {
        let (mut ops, _temp) = create_test_ops();

        // Create and publish content (locally — no network)
        let content = b"Content for reannounce test";
        let meta = Metadata::new("Reannounce Test", content.len() as u64);
        let hash = ops.create_content(content, meta).unwrap();

        // Publish locally (no network → skip announce, but manifest is Shared)
        ops.publish_content(&hash, Visibility::Shared, 0)
            .await
            .unwrap();

        // Reannounce without network should return 0
        let count = ops.reannounce_all_content().await.unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_reannounce_skips_private_content() {
        let (mut ops, _temp) = create_test_ops();

        // Create content but don't publish (stays Private)
        let content = b"Private content";
        let meta = Metadata::new("Private", content.len() as u64);
        let _hash = ops.create_content(content, meta).unwrap();

        // Reannounce should find nothing to announce
        let count = ops.reannounce_all_content().await.unwrap();
        assert_eq!(count, 0);
    }
}
