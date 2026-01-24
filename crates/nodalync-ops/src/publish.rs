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

        // 3. Update visibility, price
        manifest.visibility = visibility;
        manifest.economics.price = price;
        manifest.updated_at = current_timestamp();

        // 4. Save manifest
        self.state.manifests.update(&manifest)?;

        // 5. DHT announce (if network available)
        // Extract L1 summary before borrowing network to avoid borrow checker issues
        let l1_summary = self.extract_l1_summary(hash)?;
        if let Some(network) = self.network().cloned() {
            let payload = Self::create_announce_payload(&manifest, l1_summary, network.listen_addresses());
            network.dht_announce(*hash, payload).await?;
        }

        Ok(())
    }

    /// Create an AnnouncePayload from a manifest.
    fn create_announce_payload(
        manifest: &Manifest,
        l1_summary: nodalync_types::L1Summary,
        listen_addrs: Vec<Multiaddr>,
    ) -> AnnouncePayload {
        AnnouncePayload {
            hash: manifest.hash,
            content_type: manifest.content_type,
            title: manifest.metadata.title.clone(),
            l1_summary,
            price: manifest.economics.price,
            addresses: listen_addrs.iter().map(|addr: &Multiaddr| addr.to_string()).collect(),
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

        // DHT remove (if network available)
        if let Some(network) = self.network() {
            network.dht_remove(hash).await?;
        }

        Ok(())
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
        ops.publish_content(&hash, Visibility::Shared, 100).await.unwrap();

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
        ops.publish_content(&hash, Visibility::Shared, 100).await.unwrap();

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
        ops.set_content_visibility(&hash, Visibility::Unlisted).unwrap();

        let manifest = ops.state.manifests.load(&hash).unwrap().unwrap();
        assert_eq!(manifest.visibility, Visibility::Unlisted);

        // Set to Shared
        ops.set_content_visibility(&hash, Visibility::Shared).unwrap();

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
}
