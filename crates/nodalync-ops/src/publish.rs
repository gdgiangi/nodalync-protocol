//! Publish operations implementation.
//!
//! This module implements publish, unpublish, set_visibility, and set_access operations
//! as specified in Protocol Specification §7.1.3.

use nodalync_crypto::Hash;
use nodalync_econ::validate_price;
use nodalync_store::ManifestStore;
use nodalync_types::{AccessControl, Amount, Visibility};
use nodalync_valid::Validator;

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
    /// 5. (DHT announce - stub for MVP)
    pub fn publish_content(
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

        // 5. DHT announce (stub for MVP)
        // In full implementation: self.network.announce(hash, &manifest)?;

        Ok(())
    }

    /// Unpublish content from the network.
    ///
    /// Spec §7.1.3:
    /// - Sets visibility to Private
    /// - (DHT remove - stub for MVP)
    pub fn unpublish_content(&mut self, hash: &Hash) -> OpsResult<()> {
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

        // DHT remove (stub for MVP)
        // In full implementation: self.network.remove(hash)?;

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

    #[test]
    fn test_publish_content() {
        let (mut ops, _temp) = create_test_ops();

        let content = b"Content to publish";
        let meta = Metadata::new("Publish Test", content.len() as u64);
        let hash = ops.create_content(content, meta).unwrap();

        // Initially private
        let manifest = ops.state.manifests.load(&hash).unwrap().unwrap();
        assert_eq!(manifest.visibility, Visibility::Private);

        // Publish
        ops.publish_content(&hash, Visibility::Shared, 100).unwrap();

        // Verify
        let manifest = ops.state.manifests.load(&hash).unwrap().unwrap();
        assert_eq!(manifest.visibility, Visibility::Shared);
        assert_eq!(manifest.economics.price, 100);
    }

    #[test]
    fn test_unpublish_content() {
        let (mut ops, _temp) = create_test_ops();

        let content = b"Content to unpublish";
        let meta = Metadata::new("Unpublish Test", content.len() as u64);
        let hash = ops.create_content(content, meta).unwrap();

        // Publish first
        ops.publish_content(&hash, Visibility::Shared, 100).unwrap();

        // Unpublish
        ops.unpublish_content(&hash).unwrap();

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

    #[test]
    fn test_publish_invalid_price() {
        let (mut ops, _temp) = create_test_ops();

        let content = b"Content with invalid price";
        let meta = Metadata::new("Invalid Price", content.len() as u64);
        let hash = ops.create_content(content, meta).unwrap();

        // Price of 0 is allowed (free content)
        let result = ops.publish_content(&hash, Visibility::Shared, 0);
        assert!(result.is_ok());
    }

    #[test]
    fn test_publish_requires_ownership() {
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
        let result = ops.publish_content(&hash, Visibility::Shared, 100);
        assert!(matches!(result, Err(OpsError::AccessDenied)));
    }
}
