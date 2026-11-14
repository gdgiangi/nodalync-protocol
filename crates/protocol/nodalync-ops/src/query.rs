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
        let l1_summary = build_l1_summary(hash, &mentions, self.config.max_preview_mentions);

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
            return self.preview_local_manifest(hash, manifest);
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
        self.preview_from_dht(hash)
            .await
            .ok_or(OpsError::ManifestNotFound(*hash))
    }

    /// Preview a manifest stored locally, enforcing visibility and access.
    fn preview_local_manifest(
        &mut self,
        hash: &Hash,
        manifest: Manifest,
    ) -> OpsResult<PreviewResponse> {
        // 2-3. Check visibility and access
        // For MVP, we only serve our own content or shared content
        let is_hidden = matches!(
            manifest.visibility,
            Visibility::Private | Visibility::Offline
        );
        if is_hidden && manifest.owner != self.peer_id() {
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

        Ok(PreviewResponse {
            manifest,
            l1_summary,
            provider_peer_id,
        })
    }

    /// Look up an announcement in the DHT and store it for future lookups.
    async fn preview_from_dht(&mut self, hash: &Hash) -> Option<PreviewResponse> {
        let network = self.network().cloned()?;
        let announcement = network.dht_get(hash).await.ok().flatten()?;
        // Store the announcement for future lookups
        self.state.store_announcement(announcement.clone());
        Some(Self::announcement_to_preview(announcement))
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
                self.query_previewed_content(hash, &preview.manifest, payment_amount, timestamp)
                    .await
            }
            // Content not known locally - try DHT lookup
            Err(OpsError::ManifestNotFound(_)) => {
                self.query_unknown_content(hash, payment_amount).await
            }
            Err(e) => Err(e),
        }
    }

    /// Query content whose manifest (local, or from an announcement) is known.
    async fn query_previewed_content(
        &mut self,
        hash: &Hash,
        manifest: &Manifest,
        payment_amount: Amount,
        timestamp: u64,
    ) -> OpsResult<QueryResponse> {
        // Check if we own this content - just load it directly
        if manifest.owner == self.peer_id() {
            return self.serve_own_content(hash, manifest, timestamp);
        }

        // Validate payment amount >= price
        if payment_amount < manifest.economics.price {
            return Err(OpsError::PaymentInsufficient);
        }

        // If we have the content locally but don't own it, serve from local
        if let Some(content) = self.state.content.load(hash)? {
            return self.serve_local_copy(hash, manifest, content, payment_amount, timestamp);
        }

        // Content manifest exists but content not local - try network
        let Some(network) = self.network().cloned() else {
            // No network available and content not local
            return Err(OpsError::NotFound(*hash));
        };
        self.fetch_known_content(hash, manifest, payment_amount, &network)
            .await
    }

    /// Return our own content, with a zero-amount receipt.
    fn serve_own_content(
        &self,
        hash: &Hash,
        manifest: &Manifest,
        timestamp: u64,
    ) -> OpsResult<QueryResponse> {
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

        Ok(QueryResponse {
            content,
            manifest: manifest.clone(),
            receipt,
        })
    }

    /// Return content held locally but owned by someone else, caching it with a local receipt.
    fn serve_local_copy(
        &mut self,
        hash: &Hash,
        manifest: &Manifest,
        content: Vec<u8>,
        payment_amount: Amount,
        timestamp: u64,
    ) -> OpsResult<QueryResponse> {
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

        Ok(QueryResponse {
            content,
            manifest: manifest.clone(),
            receipt,
        })
    }

    /// Fetch content with a known manifest from the network.
    ///
    /// If the owner is unknown (all zeros), the preview came from a DHT announcement:
    /// fetch through that announcement (local, else from the DHT) when one is found.
    /// Otherwise query the owner directly.
    async fn fetch_known_content(
        &mut self,
        hash: &Hash,
        manifest: &Manifest,
        payment_amount: Amount,
        network: &std::sync::Arc<dyn nodalync_net::Network>,
    ) -> OpsResult<QueryResponse> {
        if manifest.owner == UNKNOWN_PEER_ID {
            if let Some(announce) = self.find_announcement(hash, network).await? {
                return self
                    .fetch_content_from_dht_announce(hash, &announce, payment_amount, network)
                    .await;
            }
        }

        // Known owner - try direct network fetch
        self.fetch_content_from_network(hash, &manifest.owner, payment_amount, network)
            .await
    }

    /// The announcement for `hash`: the stored one, else a DHT lookup.
    async fn find_announcement(
        &self,
        hash: &Hash,
        network: &std::sync::Arc<dyn nodalync_net::Network>,
    ) -> OpsResult<Option<nodalync_wire::AnnouncePayload>> {
        // Get the announcement from DHT or local cache
        if let Some(announce) = self.state.get_announcement(hash) {
            return Ok(Some(announce));
        }
        // Try DHT lookup
        Ok(network.dht_get(hash).await?)
    }

    /// Query content with no local manifest or announcement, through a DHT lookup.
    async fn query_unknown_content(
        &mut self,
        hash: &Hash,
        payment_amount: Amount,
    ) -> OpsResult<QueryResponse> {
        let Some(network) = self.network().cloned() else {
            return Err(OpsError::NotFound(*hash));
        };
        // Lookup content in DHT
        match network.dht_get(hash).await? {
            Some(announce) => {
                self.fetch_content_from_dht_announce(hash, &announce, payment_amount, &network)
                    .await
            }
            None => Err(OpsError::NotFound(*hash)),
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
        let (payment, payment_nonce) =
            self.owner_query_payment(hash, owner, payment_amount, timestamp)?;

        let request = QueryRequestPayload {
            hash: *hash,
            query: None,
            payment: payment.clone(),
            version_spec: None,
            payment_nonce,
        };

        let response = network.send_query(libp2p_peer, request).await?;
        self.settle_owner_response(hash, owner, payment, payment_amount, &response)?;
        self.store_query_response(response, timestamp)
    }

    /// Check an owner's response matches `hash`, then update the channel for a paid query.
    fn settle_owner_response(
        &mut self,
        hash: &Hash,
        owner: &PeerId,
        payment: Payment,
        payment_amount: Amount,
        response: &nodalync_wire::QueryResponsePayload,
    ) -> OpsResult<()> {
        // Verify content hash
        if !verify_content_hash(&response.content, hash) {
            return Err(OpsError::ContentHashMismatch);
        }

        // Update channel balance after successful payment
        if payment_amount > 0 {
            self.update_payment_channel(owner, payment)?;
        }
        Ok(())
    }

    /// The payment for a query sent to the content's owner.
    ///
    /// Paid content needs a channel with the owner and our private key; free content
    /// gets an unsigned placeholder payment.
    fn owner_query_payment(
        &self,
        hash: &Hash,
        owner: &PeerId,
        payment_amount: Amount,
        timestamp: u64,
    ) -> OpsResult<(Payment, u64)> {
        if payment_amount == 0 {
            // Free content - use placeholder payment (no signature needed)
            let payment_id =
                content_hash(&[hash.0.as_slice(), &owner.0, &timestamp.to_be_bytes()].concat());
            let payment =
                placeholder_payment(payment_id, Hash([0u8; 32]), 0, *owner, hash, timestamp);
            return Ok((payment, 1u64));
        }

        // Get channel with this peer
        let channel = self
            .state
            .channels
            .get(owner)?
            .ok_or(OpsError::ChannelRequired)?;

        // Require private key for signing
        let private_key = self.private_key().ok_or(OpsError::PrivateKeyRequired)?;

        // Get provenance from manifest or announcement
        let provenance = self.local_or_announced_provenance(hash)?;

        // Create signed payment
        Ok(create_signed_payment(
            private_key,
            &channel,
            payment_amount,
            *owner,
            *hash,
            provenance,
        ))
    }

    /// Provenance roots for a payment: from the local manifest, else from the announcement.
    fn local_or_announced_provenance(&self, hash: &Hash) -> OpsResult<Vec<ProvenanceEntry>> {
        if let Some(manifest) = self.state.manifests.load(hash)? {
            return Ok(manifest.provenance.root_l0l1.clone());
        }
        Ok(self
            .state
            .get_announcement(hash)
            .map(|announce| announced_provenance(&announce))
            .unwrap_or_default())
    }

    /// Provenance roots for a payment: from the announcement, else from the local manifest.
    fn announced_or_local_provenance(&self, hash: &Hash) -> OpsResult<Vec<ProvenanceEntry>> {
        if let Some(announce) = self.state.get_announcement(hash) {
            return Ok(announced_provenance(&announce));
        }
        Ok(self
            .state
            .manifests
            .load(hash)?
            .map(|manifest| manifest.provenance.root_l0l1.clone())
            .unwrap_or_default())
    }

    /// Cache the content of a verified query response and store its manifest.
    fn store_query_response(
        &mut self,
        response: nodalync_wire::QueryResponsePayload,
        timestamp: u64,
    ) -> OpsResult<QueryResponse> {
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
    ///
    /// Tries the announced publisher first (directly, then through each announced
    /// address), then any peer reachable through the announced addresses.
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
        if let Some(response) = self
            .query_announced_publisher(hash, announce, payment_amount, network)
            .await?
        {
            return Ok(response);
        }

        // Fallback: Try to connect via addresses and query any connected peer
        // This is less efficient but works when publisher_peer_id is not available
        if let Some(response) = self
            .query_peers_at_addresses(hash, &announce.addresses, payment_amount, network)
            .await?
        {
            return Ok(response);
        }

        Err(OpsError::NotFound(*hash))
    }

    /// Query the publisher named in an announcement, if it names a valid libp2p peer ID:
    /// dial it directly (or use an existing connection), else dial each announced address.
    async fn query_announced_publisher(
        &mut self,
        hash: &Hash,
        announce: &nodalync_wire::AnnouncePayload,
        payment_amount: Amount,
        network: &std::sync::Arc<dyn nodalync_net::Network>,
    ) -> OpsResult<Option<QueryResponse>> {
        let Some(libp2p_peer) = announce
            .publisher_peer_id
            .as_ref()
            .and_then(|peer_id_str| peer_id_str.parse::<nodalync_net::PeerId>().ok())
        else {
            return Ok(None);
        };
        tracing::debug!("Using publisher peer ID from announcement: {}", libp2p_peer);

        if let Some(response) = self
            .query_publisher_directly(hash, libp2p_peer, payment_amount, network)
            .await?
        {
            return Ok(Some(response));
        }

        // If dial_peer fails, try the addresses
        self.query_publisher_via_addresses(
            hash,
            libp2p_peer,
            &announce.addresses,
            payment_amount,
            network,
        )
        .await
    }

    /// Dial the publisher by peer ID (or use an existing connection) and query it.
    async fn query_publisher_directly(
        &mut self,
        hash: &Hash,
        libp2p_peer: nodalync_net::PeerId,
        payment_amount: Amount,
        network: &std::sync::Arc<dyn nodalync_net::Network>,
    ) -> OpsResult<Option<QueryResponse>> {
        // Try to dial the peer directly
        if network.dial_peer(libp2p_peer).await.is_ok()
            || network.connected_peers().contains(&libp2p_peer)
        {
            // Query the specific publisher
            return self
                .try_query_peer(hash, libp2p_peer, payment_amount, network)
                .await;
        }
        Ok(None)
    }

    /// Dial each address in turn and query the publisher after each successful dial.
    async fn query_publisher_via_addresses(
        &mut self,
        hash: &Hash,
        libp2p_peer: nodalync_net::PeerId,
        addresses: &[String],
        payment_amount: Amount,
        network: &std::sync::Arc<dyn nodalync_net::Network>,
    ) -> OpsResult<Option<QueryResponse>> {
        for addr_str in addresses {
            if !dial_address(network, addr_str).await {
                continue;
            }
            // Now try querying the publisher
            if let Some(response) = self
                .try_query_peer(hash, libp2p_peer, payment_amount, network)
                .await?
            {
                return Ok(Some(response));
            }
        }
        Ok(None)
    }

    /// Dial each address in turn and query every connected peer until one serves the content.
    async fn query_peers_at_addresses(
        &mut self,
        hash: &Hash,
        addresses: &[String],
        payment_amount: Amount,
        network: &std::sync::Arc<dyn nodalync_net::Network>,
    ) -> OpsResult<Option<QueryResponse>> {
        for addr_str in addresses {
            if !dial_address(network, addr_str).await {
                continue;
            }
            // Try all connected peers
            for libp2p_peer in network.connected_peers() {
                if let Some(response) = self
                    .try_query_peer(hash, libp2p_peer, payment_amount, network)
                    .await?
                {
                    return Ok(Some(response));
                }
            }
        }
        Ok(None)
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
        let (payment, payment_nonce) =
            self.peer_query_payment(hash, recipient, payment_amount, timestamp)?;

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

    /// The payment for a query sent to `recipient`, found through the network.
    ///
    /// Without a channel or a private key, paid content gets an unsigned placeholder
    /// payment and the server decides whether to reject it. Free content always gets
    /// a placeholder payment.
    fn peer_query_payment(
        &self,
        hash: &Hash,
        recipient: PeerId,
        payment_amount: Amount,
        timestamp: u64,
    ) -> OpsResult<(Payment, u64)> {
        let placeholder_id = content_hash(&[hash.0.as_slice(), &timestamp.to_be_bytes()].concat());
        if payment_amount == 0 {
            // Free content - use placeholder payment (no signature needed)
            let payment = placeholder_payment(
                placeholder_id,
                Hash([0u8; 32]),
                0,
                recipient,
                hash,
                timestamp,
            );
            return Ok((payment, 1u64));
        }

        // Get channel with this peer
        let Some(channel) = self.state.channels.get(&recipient)? else {
            // No channel - use placeholders (server will reject if payment required)
            let payment = placeholder_payment(
                placeholder_id,
                Hash([0u8; 32]),
                payment_amount,
                recipient,
                hash,
                timestamp,
            );
            return Ok((payment, 1u64));
        };

        // Require private key for signing
        let Some(private_key) = self.private_key() else {
            // No private key - use placeholder and let server reject
            let payment = placeholder_payment(
                placeholder_id,
                channel.channel_id,
                payment_amount,
                recipient,
                hash,
                timestamp,
            );
            return Ok((payment, channel.nonce + 1));
        };

        // Get provenance from announcement or local manifest
        let provenance = self.announced_or_local_provenance(hash)?;

        // Create signed payment
        Ok(create_signed_payment(
            private_key,
            &channel,
            payment_amount,
            recipient,
            *hash,
            provenance,
        ))
    }

    /// Internal helper to execute a query with a prepared payment.
    ///
    /// Returns `None` when the peer fails or returns content that does not match `hash`.
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

        let request = QueryRequestPayload {
            hash: *hash,
            query: None,
            payment: payment.clone(),
            version_spec: None,
            payment_nonce,
        };

        match network.send_query(libp2p_peer, request).await {
            // Verify content hash
            Ok(response) if verify_content_hash(&response.content, hash) => self
                .accept_peer_response(payment, payment_amount, response, timestamp)
                .map(Some),
            Ok(_) => Ok(None),
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
                Err(OpsError::ChannelRequiredWithPeerInfo {
                    nodalync_peer_id: nodalync_peer_id.map(nodalync_crypto::PeerId),
                    libp2p_peer_id,
                })
            }
            Err(e) => {
                tracing::debug!("Failed to query peer {}: {}", libp2p_peer, e);
                Ok(None)
            }
        }
    }

    /// Record a verified response from a peer: update the channel for a paid query
    /// (a failure is logged, not returned), then cache and store the response.
    fn accept_peer_response(
        &mut self,
        payment: Payment,
        payment_amount: Amount,
        response: nodalync_wire::QueryResponsePayload,
        timestamp: u64,
    ) -> OpsResult<QueryResponse> {
        // Update channel balance after successful payment
        if payment_amount > 0 {
            let recipient = payment.recipient;
            if let Err(e) = self.update_payment_channel(&recipient, payment) {
                tracing::warn!("Failed to update channel after payment: {} (continuing)", e);
            }
        }

        self.store_query_response(response, timestamp)
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
        self.search_local_manifests(
            query,
            content_type,
            limit,
            &mut seen_hashes,
            &mut all_results,
        )?;

        // 2. Search cached announcements
        let announcements = self.state.search_announcements(query, content_type, limit);
        for announce in announcements {
            if seen_hashes.insert(announce.hash) {
                all_results.push(cached_announcement_result(&announce));
            }
        }

        // 3. Query connected peers via SEARCH protocol
        if let Some(network) = self.network().cloned() {
            self.search_connected_peers(
                &network,
                query,
                content_type,
                limit,
                &mut seen_hashes,
                &mut all_results,
            )
            .await;
        }

        // Truncate to limit
        all_results.truncate(limit as usize);

        Ok(all_results)
    }

    /// Add shared local manifests matching `query` that are not yet in `seen_hashes`.
    fn search_local_manifests(
        &mut self,
        query: &str,
        content_type: Option<ContentType>,
        limit: u32,
        seen_hashes: &mut std::collections::HashSet<Hash>,
        all_results: &mut Vec<NetworkSearchResult>,
    ) -> OpsResult<()> {
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
                all_results.push(local_manifest_result(&manifest, l1_summary));
            }
        }
        Ok(())
    }

    /// Send the search to up to 5 connected peers and add their new results.
    /// A peer that fails is logged and skipped.
    async fn search_connected_peers(
        &mut self,
        network: &std::sync::Arc<dyn nodalync_net::Network>,
        query: &str,
        content_type: Option<ContentType>,
        limit: u32,
        seen_hashes: &mut std::collections::HashSet<Hash>,
        all_results: &mut Vec<NetworkSearchResult>,
    ) {
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
                    tracing::info!(
                        peer = %peer,
                        results_count = response.results.len(),
                        "Received search response from peer"
                    );
                    self.record_peer_search_results(
                        peer,
                        response.results,
                        seen_hashes,
                        all_results,
                    );
                }
                Err(e) => {
                    tracing::debug!(peer = %peer, error = %e, "Peer search failed");
                }
            }
        }
    }

    /// Add a peer's search results not yet in `seen_hashes`, storing an announcement
    /// for each so the content can be queried later.
    fn record_peer_search_results(
        &mut self,
        peer: &nodalync_net::PeerId,
        results: Vec<nodalync_wire::SearchResult>,
        seen_hashes: &mut std::collections::HashSet<Hash>,
        all_results: &mut Vec<NetworkSearchResult>,
    ) {
        for result in results {
            if !seen_hashes.insert(result.hash) {
                continue;
            }
            // Create and cache an announcement so this content can be queried later
            // Use the publisher_addresses from the search result for robust reconnection
            tracing::info!(
                hash = %result.hash,
                title = %result.title,
                publisher_addresses_count = result.publisher_addresses.len(),
                publisher_addresses = ?result.publisher_addresses,
                "Creating announcement from search result"
            );
            let announcement = nodalync_wire::AnnouncePayload {
                hash: result.hash,
                content_type: result.content_type,
                title: result.title.clone(),
                l1_summary: result.l1_summary.clone(),
                price: result.price,
                addresses: result.publisher_addresses.clone(),
                publisher_peer_id: Some(peer.to_string()),
            };
            self.state.store_announcement(announcement);

            all_results.push(NetworkSearchResult {
                hash: result.hash,
                title: result.title,
                content_type: result.content_type,
                price: result.price,
                owner: result.owner,
                l1_summary: result.l1_summary,
                total_queries: result.total_queries,
                source: SearchSource::Peer,
                publisher_peer_id: Some(peer.to_string()),
            });
        }
    }
}

/// A search result for a shared local manifest.
fn local_manifest_result(manifest: &Manifest, l1_summary: L1Summary) -> NetworkSearchResult {
    NetworkSearchResult {
        hash: manifest.hash,
        title: manifest.metadata.title.clone(),
        content_type: manifest.content_type,
        price: manifest.economics.price,
        owner: manifest.owner,
        l1_summary,
        total_queries: manifest.economics.total_queries,
        source: SearchSource::Local,
        publisher_peer_id: None, // Local content, no remote peer
    }
}

/// A search result for a cached network announcement (owner unknown).
fn cached_announcement_result(announce: &nodalync_wire::AnnouncePayload) -> NetworkSearchResult {
    NetworkSearchResult {
        hash: announce.hash,
        title: announce.title.clone(),
        content_type: announce.content_type,
        price: announce.price,
        owner: UNKNOWN_PEER_ID,
        l1_summary: announce.l1_summary.clone(),
        total_queries: 0,
        source: SearchSource::Cached,
        publisher_peer_id: announce.publisher_peer_id.clone(),
    }
}

/// An unsigned payment for a query whose real payment cannot (or need not) be signed.
fn placeholder_payment(
    payment_id: Hash,
    channel_id: Hash,
    amount: Amount,
    recipient: PeerId,
    hash: &Hash,
    timestamp: u64,
) -> Payment {
    Payment::new(
        payment_id,
        channel_id,
        amount,
        recipient,
        *hash,
        vec![],
        timestamp,
        Signature::from_bytes([0u8; 64]),
    )
}

/// The provenance a payment carries for content known only from an announcement.
fn announced_provenance(announce: &nodalync_wire::AnnouncePayload) -> Vec<ProvenanceEntry> {
    vec![ProvenanceEntry::new(
        announce.hash,
        UNKNOWN_PEER_ID,
        Visibility::Shared,
    )]
}

/// Parse `addr_str` as a multiaddr and dial it; false if it does not parse or the dial fails.
async fn dial_address(network: &std::sync::Arc<dyn nodalync_net::Network>, addr_str: &str) -> bool {
    match addr_str.parse::<nodalync_net::Multiaddr>() {
        Ok(addr) => network.dial(addr).await.is_ok(),
        Err(_) => false,
    }
}

/// Build the L1 summary for `mentions`, previewing at most `max_preview_mentions`.
fn build_l1_summary(
    hash: &Hash,
    mentions: &[nodalync_types::Mention],
    max_preview_mentions: usize,
) -> L1Summary {
    let preview_mentions: Vec<_> = mentions
        .iter()
        .take(max_preview_mentions)
        .cloned()
        .collect();

    let primary_topics: Vec<String> = extract_topics(mentions);

    let summary_text = if !mentions.is_empty() {
        format!(
            "Content contains {} mentions covering topics: {}",
            mentions.len(),
            primary_topics.join(", ")
        )
    } else {
        "No structured mentions extracted from this content.".to_string()
    };

    L1Summary::new(
        *hash,
        mentions.len() as u32,
        preview_mentions,
        primary_topics,
        summary_text,
    )
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
    fn test_peer_query_payment_free_is_unsigned_placeholder() {
        let (ops, _temp) = create_test_ops();
        let hash = content_hash(b"free");
        let recipient = PeerId::from_bytes([7u8; 20]);

        let (payment, nonce) = ops.peer_query_payment(&hash, recipient, 0, 42).unwrap();

        assert_eq!(nonce, 1);
        assert_eq!(payment.amount, 0);
        assert_eq!(payment.recipient, recipient);
        assert_eq!(payment.channel_id, Hash([0u8; 32]));
        assert_eq!(
            payment.id,
            content_hash(&[hash.0.as_slice(), &42u64.to_be_bytes()].concat())
        );
    }

    #[test]
    fn test_peer_query_payment_without_channel_sends_placeholder_for_server_to_reject() {
        let (ops, _temp) = create_test_ops();
        let hash = content_hash(b"paid");
        let recipient = PeerId::from_bytes([7u8; 20]);

        let (payment, nonce) = ops.peer_query_payment(&hash, recipient, 500, 42).unwrap();

        assert_eq!(nonce, 1);
        assert_eq!(payment.amount, 500);
        assert_eq!(payment.channel_id, Hash([0u8; 32]));
        assert_eq!(payment.signature, Signature::from_bytes([0u8; 64]));
    }

    #[test]
    fn test_owner_query_payment_paid_without_channel_is_channel_required() {
        let (ops, _temp) = create_test_ops();
        let hash = content_hash(b"paid");
        let owner = PeerId::from_bytes([7u8; 20]);

        let result = ops.owner_query_payment(&hash, &owner, 500, 42);
        assert!(matches!(result, Err(OpsError::ChannelRequired)));
    }

    #[test]
    fn test_owner_query_payment_free_id_includes_owner() {
        let (ops, _temp) = create_test_ops();
        let hash = content_hash(b"free");
        let owner = PeerId::from_bytes([7u8; 20]);

        let (payment, nonce) = ops.owner_query_payment(&hash, &owner, 0, 42).unwrap();

        assert_eq!(nonce, 1);
        assert_eq!(
            payment.id,
            content_hash(&[hash.0.as_slice(), &owner.0, &42u64.to_be_bytes()].concat())
        );
    }

    #[test]
    fn test_build_l1_summary_without_mentions() {
        let hash = content_hash(b"empty");
        let summary = build_l1_summary(&hash, &[], 5);
        assert_eq!(summary.mention_count, 0);
        assert_eq!(
            summary.summary,
            "No structured mentions extracted from this content."
        );
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

    #[test]
    fn test_get_content_manifest_existing() {
        let (mut ops, _temp) = create_test_ops();

        // Create content (this also stores the manifest)
        let content = b"Manifest retrieval test";
        let meta = Metadata::new("Manifest Test", content.len() as u64);
        let hash = ops.create_content(content, meta).unwrap();

        // Retrieve the manifest
        let manifest = ops.get_content_manifest(&hash).unwrap();
        assert!(manifest.is_some());
        let manifest = manifest.unwrap();
        assert_eq!(manifest.hash, hash);
        assert_eq!(manifest.metadata.title, "Manifest Test");
    }

    #[test]
    fn test_get_content_manifest_nonexistent() {
        let (ops, _temp) = create_test_ops();

        // Try to get a manifest for a hash that doesn't exist
        let unknown_hash = nodalync_crypto::content_hash(b"nonexistent content");
        let manifest = ops.get_content_manifest(&unknown_hash).unwrap();
        assert!(manifest.is_none());
    }
}
