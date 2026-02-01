//! Query operations implementation.
//!
//! This module implements preview, query, get_versions, and extract_l1 operations
//! as specified in Protocol Specification §7.2 and §7.4.

use nodalync_crypto::{content_hash, Hash, PeerId, Signature, UNKNOWN_PEER_ID};
use nodalync_store::{
    CacheStore, CachedContent, ChannelStore, ContentStore, ManifestFilter, ManifestStore,
};
use nodalync_types::{
    Amount, ContentType, L1Summary, Manifest, Payment, ProvenanceEntry, Visibility,
};
use nodalync_valid::Validator;
use nodalync_wire::{
    PaymentReceipt, QueryRequestPayload, SearchFilters, SearchPayload, VersionInfo, VersionSpec,
};

use crate::channel::create_signed_payment;
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
    /// 1. Loads manifest (or announcement if not local)
    /// 2. If not found locally, try DHT lookup
    /// 3. Checks visibility (Private returns NotFound for external)
    /// 4. Checks access control
    /// 5. Gets or extracts L1Summary
    /// 6. Returns (Manifest, L1Summary)
    pub async fn preview_content(&mut self, hash: &Hash) -> OpsResult<PreviewResponse> {
        // 1. Try to load local manifest first
        if let Some(manifest) = self.state.manifests.load(hash)? {
            // 2-3. Check visibility and access
            // For MVP, we only serve our own content or shared content
            if manifest.visibility == Visibility::Private && manifest.owner != self.peer_id() {
                return Err(OpsError::AccessDenied);
            }

            // 4. Get or extract L1Summary
            let l1_summary = self.extract_l1_summary(hash)?;

            // 5. Return response
            // If manifest.owner is UNKNOWN_PEER_ID, this is cached content from a network query
            // We need to check for an announcement to get the provider's peer ID
            let provider_peer_id = if manifest.owner == UNKNOWN_PEER_ID {
                self.state
                    .get_announcement(hash)
                    .and_then(|a| a.publisher_peer_id)
            } else {
                None // We own this content, no remote provider needed
            };

            return Ok(PreviewResponse {
                manifest,
                l1_summary,
                provider_peer_id,
            });
        }

        // If not local, check if we have an announcement from a remote node
        if let Some(announcement) = self.state.get_announcement(hash) {
            tracing::debug!(
                hash = %hash,
                publisher_peer_id = ?announcement.publisher_peer_id,
                "Found announcement for hash"
            );
            return Ok(Self::announcement_to_preview(announcement));
        } else {
            tracing::debug!(hash = %hash, "No announcement found for hash");
        }

        // If not in local announcements, try DHT lookup
        if let Some(network) = self.network().cloned() {
            if let Ok(Some(announcement)) = network.dht_get(hash).await {
                // Store the announcement for future lookups
                self.state.store_announcement(announcement.clone());
                return Ok(Self::announcement_to_preview(announcement));
            }
        }

        Err(OpsError::ManifestNotFound(*hash))
    }

    /// Convert an AnnouncePayload to a PreviewResponse.
    fn announcement_to_preview(announcement: nodalync_wire::AnnouncePayload) -> PreviewResponse {
        use nodalync_types::{AccessControl, Currency, Economics, Metadata, Provenance, Version};

        let manifest = Manifest {
            hash: announcement.hash,
            content_type: announcement.content_type,
            owner: UNKNOWN_PEER_ID, // Unknown owner
            version: Version::new_v1(announcement.hash, 0),
            visibility: Visibility::Shared,
            access: AccessControl::default(),
            metadata: Metadata::new(&announcement.title, 0),
            economics: Economics {
                price: announcement.price,
                currency: Currency::HBAR,
                total_queries: 0,
                total_revenue: 0,
            },
            provenance: Provenance::new_l0(announcement.hash, UNKNOWN_PEER_ID),
            created_at: 0,
            updated_at: 0,
        };

        PreviewResponse {
            manifest,
            l1_summary: announcement.l1_summary,
            // Preserve the publisher peer ID from the announcement
            // This is the libp2p peer ID of the node that can serve the content
            provider_peer_id: announcement.publisher_peer_id,
        }
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
        match self.preview_content(hash).await {
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
                        payment_id: content_hash(
                            &[hash.0.as_slice(), &timestamp.to_be_bytes()].concat(),
                        ),
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
                    // If owner is unknown (all zeros), the preview came from a DHT announcement
                    // In that case, look up the announcement and use fetch_content_from_dht_announce
                    if manifest.owner == UNKNOWN_PEER_ID {
                        // Get the announcement from DHT or local cache
                        if let Some(announce) = self.state.get_announcement(hash) {
                            return self
                                .fetch_content_from_dht_announce(
                                    hash,
                                    &announce,
                                    payment_amount,
                                    &network,
                                )
                                .await;
                        }
                        // Try DHT lookup
                        if let Some(announce) = network.dht_get(hash).await? {
                            return self
                                .fetch_content_from_dht_announce(
                                    hash,
                                    &announce,
                                    payment_amount,
                                    &network,
                                )
                                .await;
                        }
                    }

                    // Known owner - try direct network fetch
                    return self
                        .fetch_content_from_network(hash, &manifest.owner, payment_amount, &network)
                        .await;
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
                        return self
                            .fetch_content_from_dht_announce(
                                hash,
                                &announce,
                                payment_amount,
                                &network,
                            )
                            .await;
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

        // For paid content, we need a channel and private key
        let (payment, payment_nonce) = if payment_amount > 0 {
            // Get channel with this peer
            let channel = self
                .state
                .channels
                .get(owner)?
                .ok_or(OpsError::ChannelRequired)?;

            // Require private key for signing
            let private_key = self.private_key().ok_or(OpsError::PrivateKeyRequired)?;

            // Get provenance from manifest or announcement
            let provenance = if let Some(manifest) = self.state.manifests.load(hash)? {
                manifest.provenance.root_l0l1.clone()
            } else if let Some(announce) = self.state.get_announcement(hash) {
                vec![ProvenanceEntry::new(
                    announce.hash,
                    UNKNOWN_PEER_ID,
                    Visibility::Shared,
                )]
            } else {
                vec![]
            };

            // Create signed payment
            let (payment, nonce) = create_signed_payment(
                private_key,
                &channel,
                payment_amount,
                *owner,
                *hash,
                provenance,
            );

            (payment, nonce)
        } else {
            // Free content - use placeholder payment (no signature needed)
            let payment_id =
                content_hash(&[hash.0.as_slice(), &owner.0, &timestamp.to_be_bytes()].concat());

            let payment = Payment::new(
                payment_id,
                Hash([0u8; 32]),
                0,
                *owner,
                *hash,
                vec![],
                timestamp,
                Signature::from_bytes([0u8; 64]),
            );
            (payment, 1u64)
        };

        let request = QueryRequestPayload {
            hash: *hash,
            query: None,
            payment: payment.clone(),
            version_spec: None,
            payment_nonce,
        };

        let response = network.send_query(libp2p_peer, request).await?;

        // Verify content hash
        if !verify_content_hash(&response.content, hash) {
            return Err(OpsError::ContentHashMismatch);
        }

        // Update channel balance after successful payment
        if payment_amount > 0 {
            self.update_payment_channel(owner, payment)?;
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

        tracing::debug!(
            "fetch_content_from_dht_announce: hash={}, publisher_peer_id={:?}, addresses={:?}",
            hash,
            announce.publisher_peer_id,
            announce.addresses
        );

        // If the announcement includes the publisher's libp2p peer ID, use it directly
        if let Some(ref peer_id_str) = announce.publisher_peer_id {
            if let Ok(libp2p_peer) = peer_id_str.parse::<nodalync_net::PeerId>() {
                tracing::debug!("Using publisher peer ID from announcement: {}", libp2p_peer);

                // Try to dial the peer directly
                if network.dial_peer(libp2p_peer).await.is_ok()
                    || network.connected_peers().contains(&libp2p_peer)
                {
                    // Query the specific publisher
                    if let Some(response) = self
                        .try_query_peer(hash, libp2p_peer, payment_amount, network)
                        .await?
                    {
                        return Ok(response);
                    }
                }

                // If dial_peer fails, try the addresses
                for addr_str in &announce.addresses {
                    if let Ok(addr) = addr_str.parse::<nodalync_net::Multiaddr>() {
                        if network.dial(addr).await.is_ok() {
                            // Now try querying the publisher
                            if let Some(response) = self
                                .try_query_peer(hash, libp2p_peer, payment_amount, network)
                                .await?
                            {
                                return Ok(response);
                            }
                        }
                    }
                }
            }
        }

        // Fallback: Try to connect via addresses and query any connected peer
        // This is less efficient but works when publisher_peer_id is not available
        for addr_str in &announce.addresses {
            if let Ok(addr) = addr_str.parse::<nodalync_net::Multiaddr>() {
                if network.dial(addr.clone()).await.is_ok() {
                    // Try all connected peers
                    for libp2p_peer in network.connected_peers() {
                        if let Some(response) = self
                            .try_query_peer(hash, libp2p_peer, payment_amount, network)
                            .await?
                        {
                            return Ok(response);
                        }
                    }
                }
            }
        }

        Err(OpsError::NotFound(*hash))
    }

    /// Helper to try querying a specific peer for content.
    async fn try_query_peer(
        &mut self,
        hash: &Hash,
        libp2p_peer: nodalync_net::PeerId,
        payment_amount: Amount,
        network: &std::sync::Arc<dyn nodalync_net::Network>,
    ) -> OpsResult<Option<QueryResponse>> {
        let timestamp = current_timestamp();

        // Get Nodalync peer ID from mapping
        let recipient = network
            .nodalync_peer_id(&libp2p_peer)
            .unwrap_or(UNKNOWN_PEER_ID);

        // For paid content, we need a channel and private key
        let (payment, payment_nonce) = if payment_amount > 0 {
            // Get channel with this peer
            let channel = match self.state.channels.get(&recipient)? {
                Some(ch) => ch,
                None => {
                    // No channel - use placeholders (server will reject if payment required)
                    let payment_id =
                        content_hash(&[hash.0.as_slice(), &timestamp.to_be_bytes()].concat());
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
                    return self
                        .try_query_peer_with_payment(
                            hash,
                            libp2p_peer,
                            payment_amount,
                            payment,
                            1u64,
                            network,
                        )
                        .await;
                }
            };

            // Require private key for signing
            let private_key = match self.private_key() {
                Some(pk) => pk,
                None => {
                    // No private key - use placeholder and let server reject
                    let payment_id =
                        content_hash(&[hash.0.as_slice(), &timestamp.to_be_bytes()].concat());
                    let payment = Payment::new(
                        payment_id,
                        channel.channel_id,
                        payment_amount,
                        recipient,
                        *hash,
                        vec![],
                        timestamp,
                        Signature::from_bytes([0u8; 64]),
                    );
                    return self
                        .try_query_peer_with_payment(
                            hash,
                            libp2p_peer,
                            payment_amount,
                            payment,
                            channel.nonce + 1,
                            network,
                        )
                        .await;
                }
            };

            // Get provenance from announcement or local manifest
            let provenance = if let Some(announce) = self.state.get_announcement(hash) {
                vec![ProvenanceEntry::new(
                    announce.hash,
                    UNKNOWN_PEER_ID,
                    Visibility::Shared,
                )]
            } else if let Some(manifest) = self.state.manifests.load(hash)? {
                manifest.provenance.root_l0l1.clone()
            } else {
                vec![]
            };

            // Create signed payment
            create_signed_payment(
                private_key,
                &channel,
                payment_amount,
                recipient,
                *hash,
                provenance,
            )
        } else {
            // Free content - use placeholder payment (no signature needed)
            let payment_id = content_hash(&[hash.0.as_slice(), &timestamp.to_be_bytes()].concat());

            let payment = Payment::new(
                payment_id,
                Hash([0u8; 32]),
                0,
                recipient,
                *hash,
                vec![],
                timestamp,
                Signature::from_bytes([0u8; 64]),
            );
            (payment, 1u64)
        };

        self.try_query_peer_with_payment(
            hash,
            libp2p_peer,
            payment_amount,
            payment,
            payment_nonce,
            network,
        )
        .await
    }

    /// Internal helper to execute a query with a prepared payment.
    async fn try_query_peer_with_payment(
        &mut self,
        hash: &Hash,
        libp2p_peer: nodalync_net::PeerId,
        payment_amount: Amount,
        payment: Payment,
        payment_nonce: u64,
        network: &std::sync::Arc<dyn nodalync_net::Network>,
    ) -> OpsResult<Option<QueryResponse>> {
        let timestamp = current_timestamp();
        let recipient = payment.recipient;

        let request = QueryRequestPayload {
            hash: *hash,
            query: None,
            payment: payment.clone(),
            version_spec: None,
            payment_nonce,
        };

        match network.send_query(libp2p_peer, request).await {
            Ok(response) => {
                // Verify content hash
                if verify_content_hash(&response.content, hash) {
                    // Update channel balance after successful payment
                    if payment_amount > 0 {
                        if let Err(e) = self.update_payment_channel(&recipient, payment) {
                            tracing::warn!(
                                "Failed to update channel after payment: {} (continuing)",
                                e
                            );
                        }
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

                    // Store manifest
                    self.state.manifests.store(&response.manifest)?;

                    return Ok(Some(QueryResponse {
                        content: response.content,
                        manifest: response.manifest,
                        receipt: response.payment_receipt,
                    }));
                }
            }
            Err(nodalync_net::NetworkError::ChannelRequired {
                nodalync_peer_id,
                libp2p_peer_id,
            }) => {
                // Convert to OpsError with peer info so client can open channel and retry
                tracing::debug!(
                    "Server {} requires payment channel, peer info: nodalync={:?}, libp2p={:?}",
                    libp2p_peer,
                    nodalync_peer_id,
                    libp2p_peer_id
                );
                return Err(OpsError::ChannelRequiredWithPeerInfo {
                    nodalync_peer_id: nodalync_peer_id.map(nodalync_crypto::PeerId),
                    libp2p_peer_id,
                });
            }
            Err(e) => {
                tracing::debug!("Failed to query peer {}: {}", libp2p_peer, e);
            }
        }

        Ok(None)
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

    /// Search the network for content matching query.
    ///
    /// Combines results from:
    /// 1. Local manifests
    /// 2. Cached announcements from network
    /// 3. Connected peers via SEARCH protocol
    ///
    /// Results are deduplicated by hash (local takes precedence).
    pub async fn search_network(
        &mut self,
        query: &str,
        content_type: Option<ContentType>,
        limit: u32,
    ) -> OpsResult<Vec<NetworkSearchResult>> {
        let mut all_results = Vec::new();
        let mut seen_hashes = std::collections::HashSet::new();

        // 1. Search local manifests
        let mut filter = ManifestFilter::new()
            .with_text_query(query)
            .with_visibility(Visibility::Shared)
            .limit(limit);

        if let Some(ct) = content_type {
            filter = filter.with_content_type(ct);
        }

        let local_manifests = self.state.manifests.list(filter)?;
        for manifest in local_manifests {
            if seen_hashes.insert(manifest.hash) {
                let l1_summary = self
                    .extract_l1_summary(&manifest.hash)
                    .unwrap_or_else(|_| L1Summary::empty(manifest.hash));

                all_results.push(NetworkSearchResult {
                    hash: manifest.hash,
                    title: manifest.metadata.title.clone(),
                    content_type: manifest.content_type,
                    price: manifest.economics.price,
                    owner: manifest.owner,
                    l1_summary,
                    total_queries: manifest.economics.total_queries,
                    source: SearchSource::Local,
                    publisher_peer_id: None, // Local content, no remote peer
                });
            }
        }

        // 2. Search cached announcements
        let announcements = self.state.search_announcements(query, content_type, limit);
        for announce in announcements {
            if seen_hashes.insert(announce.hash) {
                all_results.push(NetworkSearchResult {
                    hash: announce.hash,
                    title: announce.title.clone(),
                    content_type: announce.content_type,
                    price: announce.price,
                    owner: UNKNOWN_PEER_ID,
                    l1_summary: announce.l1_summary.clone(),
                    total_queries: 0,
                    source: SearchSource::Cached,
                    publisher_peer_id: announce.publisher_peer_id.clone(),
                });
            }
        }

        // 3. Query connected peers via SEARCH protocol
        if let Some(network) = self.network().cloned() {
            let search_payload = SearchPayload {
                query: query.to_string(),
                filters: content_type.map(|ct| SearchFilters {
                    content_types: Some(vec![ct]),
                    ..Default::default()
                }),
                limit,
                offset: 0,
            };

            // Query up to 5 connected peers
            for peer in network.connected_peers().iter().take(5) {
                match network.send_search(*peer, search_payload.clone()).await {
                    Ok(response) => {
                        for result in response.results {
                            if seen_hashes.insert(result.hash) {
                                // Create and cache an announcement so this content can be queried later
                                let announcement = nodalync_wire::AnnouncePayload {
                                    hash: result.hash,
                                    content_type: result.content_type,
                                    title: result.title.clone(),
                                    l1_summary: result.l1_summary.clone(),
                                    price: result.price,
                                    addresses: vec![],
                                    publisher_peer_id: Some(peer.to_string()),
                                };
                                self.state.store_announcement(announcement);

                                all_results.push(NetworkSearchResult {
                                    hash: result.hash,
                                    title: result.title.clone(),
                                    content_type: result.content_type,
                                    price: result.price,
                                    owner: result.owner,
                                    l1_summary: result.l1_summary.clone(),
                                    total_queries: result.total_queries,
                                    source: SearchSource::Peer,
                                    publisher_peer_id: Some(peer.to_string()),
                                });
                            }
                        }
                    }
                    Err(e) => {
                        tracing::debug!(peer = %peer, error = %e, "Peer search failed");
                    }
                }
            }
        }

        // Truncate to limit
        all_results.truncate(limit as usize);

        Ok(all_results)
    }
}

/// Source of a search result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchSource {
    /// Result from local manifests.
    Local,
    /// Result from cached network announcements.
    Cached,
    /// Result from a peer via SEARCH protocol.
    Peer,
}

impl std::fmt::Display for SearchSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Local => write!(f, "local"),
            Self::Cached => write!(f, "cached"),
            Self::Peer => write!(f, "peer"),
        }
    }
}

/// Search result with source information.
#[derive(Debug, Clone)]
pub struct NetworkSearchResult {
    /// Content hash.
    pub hash: Hash,
    /// Content title.
    pub title: String,
    /// Content type.
    pub content_type: ContentType,
    /// Query price.
    pub price: Amount,
    /// Content owner (may be UNKNOWN_PEER_ID for cached announcements).
    pub owner: PeerId,
    /// L1 summary preview.
    pub l1_summary: L1Summary,
    /// Total queries served.
    pub total_queries: u64,
    /// Where this result came from.
    pub source: SearchSource,
    /// Publisher peer ID (libp2p format, for dialing).
    /// Available for announcements; None for local content.
    pub publisher_peer_id: Option<String>,
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

    #[tokio::test]
    async fn test_preview_content() {
        let (mut ops, _temp) = create_test_ops();

        let content = b"Test content for preview";
        let meta = Metadata::new("Preview Test", content.len() as u64);
        let hash = ops.create_content(content, meta).unwrap();

        let preview = ops.preview_content(&hash).await.unwrap();

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
        assert!(!versions.is_empty());
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
