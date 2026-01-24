//! Query operations implementation.
//!
//! This module implements preview, query, get_versions, and extract_l1 operations
//! as specified in Protocol Specification §7.2 and §7.4.

use nodalync_crypto::{content_hash, Hash, PeerId, Signature};
use nodalync_store::{CacheStore, CachedContent, ContentStore, ManifestStore};
use nodalync_types::{Amount, L1Summary, Manifest, Payment, Visibility};
use nodalync_valid::Validator;
use nodalync_wire::{PaymentReceipt, QueryRequestPayload, VersionInfo, VersionSpec};

use crate::error::{OpsError, OpsResult};
use crate::extraction::L1Extractor;
use crate::helpers::verify_content_hash;
use crate::node_ops::{current_timestamp, NodeOperations};
use crate::ops::{PreviewResponse, QueryResponse};

impl<V, E> NodeOperations<V, E>
where
    V: Validator,
    E: L1Extractor,
{
    /// Extract L1 mentions from L0 content.
    ///
    /// Spec §7.1.2:
    /// 1. Loads content and manifest
    /// 2. Uses configured extractor
    /// 3. Generates L1Summary
    /// 4. Stores L1 data (in-memory for MVP)
    pub fn extract_l1_summary(&mut self, hash: &Hash) -> OpsResult<L1Summary> {
        // 1. Load content and manifest
        let manifest = self
            .state
            .manifests
            .load(hash)?
            .ok_or(OpsError::ManifestNotFound(*hash))?;

        let content = self
            .state
            .content
            .load(hash)?
            .ok_or(OpsError::NotFound(*hash))?;

        // 2. Use configured extractor
        let mime_type = manifest.metadata.mime_type.as_deref();
        let mentions = self.extractor.extract(&content, mime_type)?;

        // 3. Generate L1Summary
        let preview_mentions: Vec<_> = mentions
            .iter()
            .take(self.config.max_preview_mentions)
            .cloned()
            .collect();

        let primary_topics: Vec<String> = extract_topics(&mentions);

        let summary_text = if !mentions.is_empty() {
            format!(
                "Content contains {} mentions covering topics: {}",
                mentions.len(),
                primary_topics.join(", ")
            )
        } else {
            "No structured mentions extracted from this content.".to_string()
        };

        let l1_summary = L1Summary::new(
            *hash,
            mentions.len() as u32,
            preview_mentions,
            primary_topics,
            summary_text,
        );

        // 4. For MVP, we don't persist L1 data separately
        // Future: Store in a dedicated L1 store

        Ok(l1_summary)
    }

    /// Preview content metadata and L1 summary.
    ///
    /// Spec §7.2.2:
    /// 1. Loads manifest
    /// 2. Checks visibility (Private returns NotFound for external)
    /// 3. Checks access control
    /// 4. Gets or extracts L1Summary
    /// 5. Returns (Manifest, L1Summary)
    pub fn preview_content(&mut self, hash: &Hash) -> OpsResult<PreviewResponse> {
        // 1. Load manifest
        let manifest = self
            .state
            .manifests
            .load(hash)?
            .ok_or(OpsError::ManifestNotFound(*hash))?;

        // 2-3. Check visibility and access
        // For MVP, we only serve our own content or shared content
        if manifest.visibility == Visibility::Private && manifest.owner != self.peer_id() {
            return Err(OpsError::AccessDenied);
        }

        // 4. Get or extract L1Summary
        let l1_summary = self.extract_l1_summary(hash)?;

        // 5. Return response
        Ok(PreviewResponse { manifest, l1_summary })
    }

    /// Query and retrieve full content.
    ///
    /// Spec §7.2.3 (requester side):
    /// 1. Check if content is local (owned or cached)
    /// 2. If not local, query from network (DHT lookup, payment, fetch)
    /// 3. Validates payment amount >= price
    /// 4. Verifies response hash
    /// 5. Caches content
    pub async fn query_content(
        &mut self,
        hash: &Hash,
        payment_amount: Amount,
        _version: Option<VersionSpec>,
    ) -> OpsResult<QueryResponse> {
        let timestamp = current_timestamp();

        // First, try to get content locally (from preview which loads manifest)
        match self.preview_content(hash) {
            Ok(preview) => {
                let manifest = &preview.manifest;

                // Check if we own this content - just load it directly
                if manifest.owner == self.peer_id() {
                    let content = self
                        .state
                        .content
                        .load(hash)?
                        .ok_or(OpsError::NotFound(*hash))?;

                    let receipt = PaymentReceipt {
                        payment_id: *hash,
                        amount: 0, // No payment for own content
                        timestamp,
                        channel_nonce: 0,
                        distributor_signature: Signature::from_bytes([0u8; 64]),
                    };

                    return Ok(QueryResponse {
                        content,
                        manifest: manifest.clone(),
                        receipt,
                    });
                }

                // Validate payment amount >= price
                if payment_amount < manifest.economics.price {
                    return Err(OpsError::PaymentInsufficient);
                }

                // If we have the content locally but don't own it, serve from local
                if let Some(content) = self.state.content.load(hash)? {
                    let receipt = PaymentReceipt {
                        payment_id: content_hash(&[hash.0.as_slice(), &timestamp.to_be_bytes()].concat()),
                        amount: payment_amount,
                        timestamp,
                        channel_nonce: 1,
                        distributor_signature: Signature::from_bytes([0u8; 64]),
                    };

                    // Cache content
                    let cached = CachedContent::new(
                        *hash,
                        content.clone(),
                        manifest.owner,
                        timestamp,
                        receipt.clone(),
                    );
                    self.state.cache.cache(cached)?;

                    return Ok(QueryResponse {
                        content,
                        manifest: manifest.clone(),
                        receipt,
                    });
                }

                // Content manifest exists but content not local - try network
                if let Some(network) = self.network().cloned() {
                    return self.fetch_content_from_network(
                        hash,
                        &manifest.owner,
                        payment_amount,
                        &network,
                    ).await;
                }

                // No network available and content not local
                Err(OpsError::NotFound(*hash))
            }
            Err(OpsError::ManifestNotFound(_)) => {
                // Content not known locally - try DHT lookup
                if let Some(network) = self.network().cloned() {
                    // Lookup content in DHT
                    if let Some(announce) = network.dht_get(hash).await? {
                        // Get libp2p peer ID from announcement's addresses
                        // For now, we need to find the peer who published this content
                        // In a real implementation, we'd track the publisher's peer ID
                        return self.fetch_content_from_dht_announce(
                            hash,
                            &announce,
                            payment_amount,
                            &network,
                        ).await;
                    }
                }

                Err(OpsError::NotFound(*hash))
            }
            Err(e) => Err(e),
        }
    }

    /// Fetch content from a known peer via the network.
    async fn fetch_content_from_network(
        &mut self,
        hash: &Hash,
        owner: &PeerId,
        payment_amount: Amount,
        network: &std::sync::Arc<dyn nodalync_net::Network>,
    ) -> OpsResult<QueryResponse> {
        let timestamp = current_timestamp();

        // Get libp2p peer ID for the owner
        let libp2p_peer = network
            .libp2p_peer_id(owner)
            .ok_or(OpsError::PeerIdNotFound)?;

        // Create payment
        let payment_id = content_hash(&[
            hash.0.as_slice(),
            &owner.0,
            &timestamp.to_be_bytes(),
        ].concat());

        let payment = Payment::new(
            payment_id,
            Hash([0u8; 32]), // Channel ID - in full impl, would get from channel
            payment_amount,
            *owner,
            *hash,
            vec![], // Provenance entries would be added in full impl
            timestamp,
            Signature::from_bytes([0u8; 64]), // Signature would be real in full impl
        );

        // Send query request
        let request = QueryRequestPayload {
            hash: *hash,
            query: None,
            payment,
            version_spec: None,
        };

        let response = network.send_query(libp2p_peer, request).await?;

        // Verify content hash
        if !verify_content_hash(&response.content, hash) {
            return Err(OpsError::ContentHashMismatch);
        }

        // Cache the content
        let cached = CachedContent::new(
            response.hash,
            response.content.clone(),
            response.manifest.owner,
            timestamp,
            response.payment_receipt.clone(),
        );
        self.state.cache.cache(cached)?;

        // Also store the manifest for future reference
        self.state.manifests.store(&response.manifest)?;

        Ok(QueryResponse {
            content: response.content,
            manifest: response.manifest,
            receipt: response.payment_receipt,
        })
    }

    /// Fetch content from a DHT announcement.
    async fn fetch_content_from_dht_announce(
        &mut self,
        hash: &Hash,
        announce: &nodalync_wire::AnnouncePayload,
        payment_amount: Amount,
        network: &std::sync::Arc<dyn nodalync_net::Network>,
    ) -> OpsResult<QueryResponse> {
        // Validate payment amount against announced price
        if payment_amount < announce.price {
            return Err(OpsError::PaymentInsufficient);
        }

        // Try to connect to one of the announced addresses
        for addr_str in &announce.addresses {
            if let Ok(addr) = addr_str.parse::<nodalync_net::Multiaddr>() {
                // Try to dial the address
                if network.dial(addr.clone()).await.is_ok() {
                    // Extract peer ID from the address if possible
                    // For simplicity, we try all connected peers that might serve this content
                    for libp2p_peer in network.connected_peers() {
                        // Try querying this peer
                        let timestamp = current_timestamp();
                        let payment_id = content_hash(&[
                            hash.0.as_slice(),
                            &timestamp.to_be_bytes(),
                        ].concat());

                        // We don't have the Nodalync peer ID, so we create a placeholder recipient
                        // In a real implementation, we'd exchange peer info first
                        let recipient = network
                            .nodalync_peer_id(&libp2p_peer)
                            .unwrap_or_else(|| PeerId([0u8; 20]));

                        let payment = Payment::new(
                            payment_id,
                            Hash([0u8; 32]),
                            payment_amount,
                            recipient,
                            *hash,
                            vec![],
                            timestamp,
                            Signature::from_bytes([0u8; 64]),
                        );

                        let request = QueryRequestPayload {
                            hash: *hash,
                            query: None,
                            payment,
                            version_spec: None,
                        };

                        if let Ok(response) = network.send_query(libp2p_peer, request).await {
                            // Verify content hash
                            if verify_content_hash(&response.content, hash) {
                                // Cache the content
                                let cached = CachedContent::new(
                                    response.hash,
                                    response.content.clone(),
                                    response.manifest.owner,
                                    timestamp,
                                    response.payment_receipt.clone(),
                                );
                                self.state.cache.cache(cached)?;

                                // Store manifest
                                self.state.manifests.store(&response.manifest)?;

                                return Ok(QueryResponse {
                                    content: response.content,
                                    manifest: response.manifest,
                                    receipt: response.payment_receipt,
                                });
                            }
                        }
                    }
                }
            }
        }

        Err(OpsError::NotFound(*hash))
    }

    /// Get all versions of content.
    ///
    /// Spec §7.4:
    /// 1. Gets all manifests with same version_root
    /// 2. Converts to VersionInfo
    pub fn get_content_versions(&self, root_hash: &Hash) -> OpsResult<Vec<VersionInfo>> {
        // Get all manifests with same version root
        let manifests = self.state.manifests.get_versions(root_hash)?;

        // Convert to VersionInfo
        let version_infos: Vec<VersionInfo> = manifests
            .iter()
            .map(|m| VersionInfo {
                hash: m.hash,
                number: m.version.number,
                timestamp: m.version.timestamp,
                visibility: m.visibility,
                price: m.economics.price,
            })
            .collect();

        Ok(version_infos)
    }

    /// Check if content was queried (is in cache).
    pub fn is_content_cached(&self, hash: &Hash) -> bool {
        self.state.cache.is_cached(hash)
    }

    /// Get a manifest by hash.
    pub fn get_content_manifest(&self, hash: &Hash) -> OpsResult<Option<Manifest>> {
        Ok(self.state.manifests.load(hash)?)
    }
}

/// Extract primary topics from mentions.
fn extract_topics(mentions: &[nodalync_types::Mention]) -> Vec<String> {
    use std::collections::HashMap;

    let mut entity_counts: HashMap<String, usize> = HashMap::new();

    for mention in mentions {
        for entity in &mention.entities {
            *entity_counts.entry(entity.clone()).or_insert(0) += 1;
        }
    }

    // Sort by count and take top 5
    let mut sorted: Vec<_> = entity_counts.into_iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1));

    sorted.into_iter().take(5).map(|(k, _)| k).collect()
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
    fn test_extract_l1_summary() {
        let (mut ops, _temp) = create_test_ops();

        let content = b"Apple announced new products. Microsoft released updates. We found significant improvements.";
        let meta = Metadata::new("Tech News", content.len() as u64);
        let hash = ops.create_content(content, meta).unwrap();

        let summary = ops.extract_l1_summary(&hash).unwrap();

        assert_eq!(summary.l0_hash, hash);
        assert!(summary.mention_count > 0);
        assert!(!summary.preview_mentions.is_empty());
    }

    #[test]
    fn test_preview_content() {
        let (mut ops, _temp) = create_test_ops();

        let content = b"Test content for preview";
        let meta = Metadata::new("Preview Test", content.len() as u64);
        let hash = ops.create_content(content, meta).unwrap();

        let preview = ops.preview_content(&hash).unwrap();

        assert_eq!(preview.manifest.hash, hash);
        assert_eq!(preview.l1_summary.l0_hash, hash);
    }

    #[tokio::test]
    async fn test_query_own_content() {
        let (mut ops, _temp) = create_test_ops();

        let content = b"Test content for query";
        let meta = Metadata::new("Query Test", content.len() as u64);
        let hash = ops.create_content(content, meta).unwrap();

        let response = ops.query_content(&hash, 100, None).await.unwrap();

        assert_eq!(response.content, content.to_vec());
        assert_eq!(response.manifest.hash, hash);
    }

    #[test]
    fn test_get_versions() {
        let (mut ops, _temp) = create_test_ops();

        // Create initial content
        let content1 = b"Version 1";
        let meta1 = Metadata::new("Test v1", content1.len() as u64);
        let hash1 = ops.create_content(content1, meta1).unwrap();

        // Update content
        let content2 = b"Version 2";
        let meta2 = Metadata::new("Test v2", content2.len() as u64);
        let _hash2 = ops.update_content(&hash1, content2, meta2).unwrap();

        // Get versions
        let versions = ops.get_content_versions(&hash1).unwrap();

        // Should have both versions
        assert!(versions.len() >= 1);
        assert!(versions.iter().any(|v| v.number == 1));
    }

    #[tokio::test]
    async fn test_query_insufficient_payment() {
        let (mut ops, _temp) = create_test_ops();

        // Create content with price
        let content = b"Paid content";
        let meta = Metadata::new("Paid", content.len() as u64);
        let hash = ops.create_content(content, meta).unwrap();

        // Set price on manifest
        let mut manifest = ops.state.manifests.load(&hash).unwrap().unwrap();
        manifest.economics.price = 1000;
        ops.state.manifests.update(&manifest).unwrap();

        // Query with insufficient payment should still work for own content
        // (owner doesn't pay themselves)
        let result = ops.query_content(&hash, 100, None).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_is_content_cached() {
        let (mut ops, _temp) = create_test_ops();

        let content = b"Cached content";
        let meta = Metadata::new("Cache Test", content.len() as u64);
        let hash = ops.create_content(content, meta).unwrap();

        // Before query, not cached (owned content doesn't go to cache)
        assert!(!ops.is_content_cached(&hash));

        // Query the content (this caches it for non-owned content)
        let _ = ops.query_content(&hash, 0, None).await;

        // Still not cached because we own it
        assert!(!ops.is_content_cached(&hash));
    }
}
