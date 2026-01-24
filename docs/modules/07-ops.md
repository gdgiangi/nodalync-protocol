# Module: nodalync-ops

**Source:** Protocol Specification §7

## Overview

Core protocol operations. Combines storage, validation, and economics to implement the protocol's business logic.

**Key Design Decisions:**

1. **L1 Extraction:** Rule-based NLP for MVP. Future: plugin architecture for AI integration.
2. **Channel Auto-Open:** When querying a peer with no channel, auto-open with configurable minimum deposit. Return PAYMENT_REQUIRED if insufficient funds.
3. **Settlement Queue:** This module WRITES to the settlement queue (in nodalync-store). The nodalync-settle module READS from it.
4. **Payment Distribution:** All distributions go to the settlement queue. The settlement contract pays ALL recipients (owner + all root contributors).

## Dependencies

- `nodalync-types` — All data structures
- `nodalync-crypto` — Hashing, signing
- `nodalync-store` — Persistence (including settlement queue)
- `nodalync-valid` — Validation
- `nodalync-econ` — Revenue distribution
- `nodalync-wire` — Message encoding

---

## Operations Trait

```rust
#[async_trait]
pub trait Operations {
    // === Content Operations ===
    
    /// Create new content locally (not yet published)
    async fn create(
        &mut self,
        content: &[u8],
        content_type: ContentType,
        metadata: Metadata,
    ) -> Result<Hash>;
    
    /// Extract L1 mentions from L0 content (rule-based for MVP)
    async fn extract_l1(&mut self, hash: &Hash) -> Result<L1Summary>;
    
    /// Publish content to the network
    async fn publish(
        &mut self,
        hash: &Hash,
        visibility: Visibility,
        price: Amount,
        access: Option<AccessControl>,
    ) -> Result<()>;
    
    /// Unpublish content (set to Private)
    async fn unpublish(&mut self, hash: &Hash) -> Result<()>;
    
    /// Create new version of existing content
    async fn update(&mut self, old_hash: &Hash, new_content: &[u8]) -> Result<Hash>;
    
    /// Create L3 from multiple sources
    async fn derive(
        &mut self,
        sources: &[Hash],
        insight: &[u8],
        metadata: Metadata,
    ) -> Result<Hash>;
    
    /// Reference external L3 as L0 for derivations
    async fn reference_l3_as_l0(&mut self, l3_hash: &Hash) -> Result<()>;
    
    // === Query Operations ===
    
    /// Get L1 preview (free)
    async fn preview(&self, hash: &Hash) -> Result<(Manifest, L1Summary)>;
    
    /// Query content (paid) - auto-opens channel if needed
    async fn query(&mut self, hash: &Hash, payment: Payment) -> Result<QueryResponse>;
    
    /// Get version history for content
    async fn get_versions(&self, version_root: &Hash) -> Result<Vec<VersionInfo>>;
    
    // === Visibility Operations ===
    
    /// Change content visibility
    async fn set_visibility(&mut self, hash: &Hash, visibility: Visibility) -> Result<()>;
    
    /// Update access control
    async fn set_access(&mut self, hash: &Hash, access: AccessControl) -> Result<()>;
    
    // === Channel Operations ===
    
    /// Open payment channel with peer
    async fn open_channel(&mut self, peer: &PeerId, deposit: Amount) -> Result<Hash>;
    
    /// Accept incoming channel open request
    async fn accept_channel(&mut self, channel_id: &Hash, deposit: Amount) -> Result<()>;
    
    /// Update channel state (after payment)
    async fn update_channel(&mut self, channel_id: &Hash, payment: &Payment) -> Result<()>;
    
    /// Close channel cooperatively
    async fn close_channel(&mut self, channel_id: &Hash) -> Result<()>;
    
    /// Dispute channel with on-chain evidence
    async fn dispute_channel(&mut self, channel_id: &Hash, state: &ChannelUpdatePayload) -> Result<()>;
    
    // === Settlement Operations ===
    
    /// Trigger settlement batch (called by nodalync-settle or manually)
    async fn trigger_settlement(&mut self) -> Result<Option<SettlementBatch>>;
}
```
```

---

## §7.1.1 CREATE

```rust
async fn create(
    &mut self,
    content: &[u8],
    content_type: ContentType,
    metadata: Metadata,
) -> Result<Hash> {
    // 1. Compute hash
    let hash = content_hash(content);
    
    // 2. Create version (v1)
    let version = Version {
        number: 1,
        previous: None,
        root: hash.clone(),
        timestamp: current_timestamp(),
    };
    
    // 3. Compute provenance (L0: self-referential)
    let provenance = match content_type {
        ContentType::L0 | ContentType::L1 => Provenance {
            root_L0L1: vec![ProvenanceEntry {
                hash: hash.clone(),
                owner: self.identity.peer_id(),
                visibility: Visibility::Private,
                weight: 1,
            }],
            derived_from: vec![],
            depth: 0,
        },
        ContentType::L3 => {
            return Err(Error::InvalidOperation(
                "Use derive() for L3 content".into()
            ));
        }
    };
    
    // 4. Create manifest (includes owner)
    let manifest = Manifest {
        hash: hash.clone(),
        content_type,
        owner: self.identity.peer_id(),  // Owner is creator
        version,
        visibility: Visibility::Private,
        access: AccessControl::default(),
        metadata,
        economics: Economics {
            price: 0,
            currency: Currency::NDL,
            total_queries: 0,
            total_revenue: 0,
        },
        provenance,
        created_at: current_timestamp(),
        updated_at: current_timestamp(),
    };
    
    // 5. Validate
    self.validator.validate_content(content, &manifest)?;
    
    // 6. Store
    self.content_store.store_verified(&hash, content)?;
    self.manifest_store.store(&manifest)?;
    
    Ok(hash)
}
```

---

## §7.1.2 EXTRACT_L1 (Rule-Based MVP)

L1 extraction identifies atomic facts from L0 content. For MVP, we use rule-based NLP.
Future versions will support a plugin architecture for AI-powered extraction.

```rust
/// L1 Extraction trait for pluggable implementations
pub trait L1Extractor {
    fn extract(&self, content: &[u8], mime_type: Option<&str>) -> Result<Vec<Mention>>;
}

/// Rule-based extractor for MVP
pub struct RuleBasedExtractor;

impl L1Extractor for RuleBasedExtractor {
    fn extract(&self, content: &[u8], mime_type: Option<&str>) -> Result<Vec<Mention>> {
        let text = std::str::from_utf8(content)?;
        let mut mentions = Vec::new();
        
        // Split into sentences (basic approach)
        let sentences: Vec<&str> = text
            .split(|c| c == '.' || c == '!' || c == '?')
            .filter(|s| !s.trim().is_empty())
            .collect();
        
        for (idx, sentence) in sentences.iter().enumerate() {
            let trimmed = sentence.trim();
            if trimmed.len() < 10 || trimmed.len() > 1000 {
                continue; // Skip too short or too long
            }
            
            // Basic classification heuristics
            let classification = classify_sentence(trimmed);
            
            // Extract entities (basic: capitalized words)
            let entities = extract_entities(trimmed);
            
            let mention = Mention {
                id: content_hash(format!("{}:{}", idx, trimmed).as_bytes()),
                content: trimmed.to_string(),
                source_location: SourceLocation {
                    location_type: LocationType::Paragraph,
                    reference: format!("sentence_{}", idx),
                    quote: Some(trimmed.chars().take(500).collect()),
                },
                classification,
                confidence: Confidence::Explicit,
                entities,
            };
            
            mentions.push(mention);
        }
        
        Ok(mentions)
    }
}

fn classify_sentence(sentence: &str) -> Classification {
    let lower = sentence.to_lowercase();
    
    if lower.contains('%') || lower.contains("percent") || 
       lower.chars().any(|c| c.is_numeric()) {
        Classification::Statistic
    } else if lower.starts_with("according to") || lower.contains("claims") ||
              lower.contains("argues") || lower.contains("suggests") {
        Classification::Claim
    } else if lower.contains("defined as") || lower.contains("refers to") ||
              lower.contains("is a") || lower.contains("are a") {
        Classification::Definition
    } else if lower.contains("method") || lower.contains("approach") ||
              lower.contains("technique") || lower.contains("process") {
        Classification::Method
    } else if lower.contains("found") || lower.contains("result") ||
              lower.contains("showed") || lower.contains("demonstrated") {
        Classification::Result
    } else {
        Classification::Observation
    }
}

fn extract_entities(sentence: &str) -> Vec<String> {
    // Basic entity extraction: capitalized multi-word sequences
    sentence
        .split_whitespace()
        .filter(|w| w.chars().next().map(|c| c.is_uppercase()).unwrap_or(false))
        .filter(|w| w.len() > 1)
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_string())
        .filter(|w| !w.is_empty())
        .collect()
}

async fn extract_l1(&mut self, hash: &Hash) -> Result<L1Summary> {
    // 1. Load content
    let content = self.content_store.load(hash)?
        .ok_or(Error::NotFound)?;
    let manifest = self.manifest_store.load(hash)?
        .ok_or(Error::NotFound)?;
    
    // 2. Extract mentions using configured extractor
    let mentions = self.l1_extractor.extract(&content, manifest.metadata.mime_type.as_deref())?;
    
    // 3. Generate summary
    let primary_topics: Vec<String> = mentions.iter()
        .flat_map(|m| m.entities.iter().cloned())
        .take(5)
        .collect();
    
    let summary = if mentions.len() > 0 {
        format!(
            "Contains {} mentions covering topics: {}",
            mentions.len(),
            primary_topics.join(", ")
        )
    } else {
        "No structured mentions extracted.".to_string()
    };
    
    // 4. Create L1Summary
    let l1_summary = L1Summary {
        l0_hash: hash.clone(),
        mention_count: mentions.len() as u32,
        preview_mentions: mentions.iter().take(5).cloned().collect(),
        primary_topics,
        summary: summary.chars().take(500).collect(),
    };
    
    // 5. Store L1 data
    self.l1_store.store(hash, &l1_summary)?;
    
    Ok(l1_summary)
}
```

**Future Plugin Architecture:**

```rust
pub trait L1ExtractorPlugin: Send + Sync {
    fn name(&self) -> &str;
    fn supported_mime_types(&self) -> Vec<&str>;
    fn extract(&self, content: &[u8], mime_type: &str) -> Result<Vec<Mention>>;
}

// Example: AI-powered extractor (future)
pub struct OpenAIExtractor {
    api_key: String,
    model: String,
}

impl L1ExtractorPlugin for OpenAIExtractor {
    fn name(&self) -> &str { "openai" }
    fn supported_mime_types(&self) -> Vec<&str> { vec!["text/plain", "text/markdown"] }
    fn extract(&self, content: &[u8], mime_type: &str) -> Result<Vec<Mention>> {
        // Call OpenAI API...
        todo!()
    }
}
```

---

## §7.1.3 PUBLISH

```rust
async fn publish(
    &mut self,
    hash: &Hash,
    visibility: Visibility,
    price: Amount,
    access: Option<AccessControl>,
) -> Result<()> {
    // 1. Load manifest
    let mut manifest = self.manifest_store.load(hash)?
        .ok_or(Error::NotFound)?;
    
    // 2. Validate price
    validate_price(price)?;
    
    // 3. Update manifest
    manifest.visibility = visibility;
    manifest.economics.price = price;
    if let Some(access) = access {
        manifest.access = access;
    }
    manifest.updated_at = current_timestamp();
    
    // 4. Save
    self.manifest_store.update(&manifest)?;
    
    // 5. Announce to network (if Shared)
    if visibility == Visibility::Shared {
        let l1_summary = self.get_or_extract_l1(hash).await?;
        let announce = AnnouncePayload {
            hash: hash.clone(),
            content_type: manifest.content_type,
            title: manifest.metadata.title.clone(),
            l1_summary,
            price,
            addresses: self.network.listen_addresses(),
        };
        self.network.dht_announce(hash, announce).await?;
    }
    
    Ok(())
}
```

---

## §7.1.5 DERIVE (Create L3)

```rust
async fn derive(
    &mut self,
    sources: &[Hash],
    insight: &[u8],
    metadata: Metadata,
) -> Result<Hash> {
    // 1. Verify all sources were queried
    for source in sources {
        if !self.cache.is_cached(source) && !self.content_store.exists(source) {
            return Err(Error::SourceNotQueried(source.clone()));
        }
    }
    
    // 2. Load source manifests
    let source_manifests: Vec<Manifest> = sources.iter()
        .map(|h| self.get_manifest(h))
        .collect::<Result<Vec<_>>>()?;
    
    // 3. Compute provenance
    let mut root_entries: HashMap<Hash, ProvenanceEntry> = HashMap::new();
    
    for source in &source_manifests {
        for entry in &source.provenance.root_L0L1 {
            root_entries.entry(entry.hash.clone())
                .and_modify(|e| e.weight += entry.weight)
                .or_insert(entry.clone());
        }
    }
    
    let max_depth = source_manifests.iter()
        .map(|s| s.provenance.depth)
        .max()
        .unwrap_or(0);
    
    let provenance = Provenance {
        root_L0L1: root_entries.into_values().collect(),
        derived_from: sources.to_vec(),
        depth: max_depth + 1,
    };
    
    // 4. Compute hash
    let hash = content_hash(insight);
    
    // 5. Create version
    let version = Version {
        number: 1,
        previous: None,
        root: hash.clone(),
        timestamp: current_timestamp(),
    };
    
    // 6. Create manifest
    let manifest = Manifest {
        hash: hash.clone(),
        content_type: ContentType::L3,
        version,
        visibility: Visibility::Private,
        access: AccessControl::default(),
        metadata,
        economics: Economics::default(),
        provenance,
        created_at: current_timestamp(),
        updated_at: current_timestamp(),
    };
    
    // 7. Validate
    self.validator.validate_provenance(&manifest, &source_manifests)?;
    
    // 8. Store
    self.content_store.store_verified(&hash, insight)?;
    self.manifest_store.store(&manifest)?;
    self.provenance_graph.add(&hash, sources)?;
    
    Ok(hash)
}
```

---

## §7.2.3 QUERY

```rust
/// Configuration for channel auto-open
pub struct ChannelConfig {
    /// Minimum deposit when auto-opening a channel
    pub min_deposit: Amount,
    /// Default deposit for new channels
    pub default_deposit: Amount,
}

impl Default for ChannelConfig {
    fn default() -> Self {
        Self {
            min_deposit: 10_000_000_000,  // 100 NDL minimum
            default_deposit: 100_000_000_000,  // 1000 NDL default
        }
    }
}

async fn query(&mut self, hash: &Hash, payment: Payment) -> Result<QueryResponse> {
    // As requester
    
    // 1. Get preview for price check and owner discovery
    let (manifest, _) = self.preview(hash).await?;
    let owner = &manifest.owner;
    
    // 2. Ensure channel exists - AUTO-OPEN if not
    if !self.channels.exists(owner) {
        // Check if we have sufficient balance for auto-open
        let balance = self.get_available_balance().await?;
        if balance < self.config.channel.min_deposit {
            return Err(Error::PaymentRequired {
                message: format!(
                    "No channel with {} and insufficient balance to auto-open. Need {} NDL minimum.",
                    owner, self.config.channel.min_deposit
                ),
            });
        }
        
        // Auto-open channel with default deposit
        let deposit = std::cmp::min(balance, self.config.channel.default_deposit);
        self.open_channel(owner, deposit).await?;
    }
    
    // 3. Validate payment amount
    if payment.amount < manifest.economics.price {
        return Err(Error::PaymentInsufficient);
    }
    
    // 4. Check channel balance
    let channel = self.channels.get(owner)?
        .ok_or(Error::ChannelNotFound)?;
    if channel.my_balance < payment.amount {
        return Err(Error::InsufficientChannelBalance);
    }
    
    // 5. Send request
    let request = QueryRequestPayload {
        hash: hash.clone(),
        query: None,
        payment: payment.clone(),
        version_spec: None,
    };
    let response = self.network.send_query(owner, request).await?;
    
    // 6. Verify response
    if content_hash(&response.content) != *hash {
        return Err(Error::ContentHashMismatch);
    }
    
    // 7. Update channel state
    self.channels.debit(owner, payment.amount)?;
    self.channels.add_payment(owner, payment)?;
    
    // 8. Cache content
    self.cache.cache(CachedContent {
        hash: hash.clone(),
        content: response.content.clone(),
        source_peer: owner.clone(),
        queried_at: current_timestamp(),
        payment_proof: response.payment_receipt.clone(),
    })?;
    
    Ok(response)
}
```

### Query Handler (receiving side)

The handler queues ALL distributions to the settlement queue. The settlement contract
will distribute to all recipients (owner + all root contributors).

```rust
async fn handle_query_request(
    &mut self,
    sender: &PeerId,
    request: QueryRequestPayload,
) -> Result<QueryResponsePayload> {
    // 1. Load manifest
    let manifest = self.manifest_store.load(&request.hash)?
        .ok_or(Error::NotFound)?;
    
    // 2. Validate access
    self.validator.validate_access(sender, &manifest)?;
    
    // 3. Validate payment
    let channel = self.channels.get(sender)?
        .ok_or(Error::ChannelNotFound)?;
    self.validator.validate_payment(&request.payment, &channel, &manifest)?;
    
    // 4. Update channel state (credit the payment)
    self.channels.credit(sender, request.payment.amount)?;
    self.channels.increment_nonce(sender)?;
    
    // 5. Calculate distributions and queue ALL of them
    // The settlement contract will pay everyone, including us
    let distributions = distribute_revenue(
        request.payment.amount,
        &manifest.owner,
        &manifest.provenance.root_L0L1,
    );
    
    for dist in distributions {
        self.settlement_queue.enqueue(QueuedDistribution {
            payment_id: request.payment.id.clone(),
            recipient: dist.recipient,
            amount: dist.amount,
            source_hash: dist.source_hash,
            queued_at: current_timestamp(),
        })?;
    }
    
    // 6. Update economics
    let mut updated_manifest = manifest.clone();
    updated_manifest.economics.total_queries += 1;
    updated_manifest.economics.total_revenue += request.payment.amount;
    self.manifest_store.update(&updated_manifest)?;
    
    // 7. Check if settlement should be triggered
    let pending_total = self.settlement_queue.get_pending_total()?;
    let last_settlement = self.settlement_queue.get_last_settlement_time()?;
    if should_settle(pending_total, last_settlement.unwrap_or(0), current_timestamp()) {
        // Queue settlement for async processing
        self.settlement_trigger.notify();
    }
    
    // 8. Load and return content
    let content = self.content_store.load(&request.hash)?
        .ok_or(Error::ContentNotFound)?;
    
    let receipt_data = encode_receipt_data(&request.payment, channel.nonce + 1)?;
    let receipt = PaymentReceipt {
        payment_id: request.payment.id.clone(),
        amount: request.payment.amount,
        timestamp: current_timestamp(),
        channel_nonce: channel.nonce + 1,
        distributor_signature: self.identity.sign(&receipt_data)?,
    };
    
    Ok(QueryResponsePayload {
        hash: request.hash,
        content,
        manifest: updated_manifest,
        payment_receipt: receipt,
    })
}
```

---

## §7.1.6 REFERENCE_L3_AS_L0

```rust
async fn reference_l3_as_l0(&mut self, l3_hash: &Hash) -> Result<()> {
    // 1. Verify L3 was queried
    let cached = self.cache.get(l3_hash)?
        .ok_or(Error::SourceNotQueried(l3_hash.clone()))?;
    
    // 2. Verify it's an L3
    let manifest = self.get_manifest(l3_hash)?;
    if manifest.content_type != ContentType::L3 {
        return Err(Error::NotAnL3);
    }
    
    // 3. Store reference
    // Note: This is a reference, not a copy. The content stays
    // in cache/remote. When deriving, we use this hash in sources.
    self.references.add_l3_reference(l3_hash, &manifest)?;
    
    Ok(())
}
```

---

## §7.3 Channel Operations

### §7.3.1 CHANNEL_OPEN

```rust
async fn open_channel(&mut self, peer: &PeerId, deposit: Amount) -> Result<Hash> {
    // 1. Generate channel ID
    let nonce = rand::random::<u64>();
    let channel_id = content_hash(
        &[self.identity.peer_id().0.as_slice(), peer.0.as_slice(), &nonce.to_be_bytes()].concat()
    );
    
    // 2. Create channel state
    let channel = Channel {
        channel_id: channel_id.clone(),
        peer_id: peer.clone(),
        state: ChannelState::Opening,
        my_balance: deposit,
        their_balance: 0,
        nonce: 0,
        last_update: current_timestamp(),
        pending_payments: vec![],
    };
    
    // 3. Store locally
    self.channels.create(peer, channel)?;
    
    // 4. Send open request
    let open_msg = ChannelOpenPayload {
        channel_id: channel_id.clone(),
        initial_balance: deposit,
        funding_tx: None,  // Off-chain for now, on-chain funding optional
    };
    
    let response = self.network.send_channel_open(peer, open_msg).await?;
    
    // 5. Process accept response
    self.handle_channel_accept(&channel_id, &response)?;
    
    Ok(channel_id)
}
```

### §7.3.2 CHANNEL_ACCEPT (Handler)

```rust
async fn handle_channel_open(
    &mut self,
    sender: &PeerId,
    request: ChannelOpenPayload,
) -> Result<ChannelAcceptPayload> {
    // 1. Validate channel doesn't already exist
    if self.channels.exists(sender) {
        return Err(Error::ChannelAlreadyExists);
    }
    
    // 2. Decide on our deposit (could be configurable)
    let our_deposit = self.config.channel.default_deposit;
    
    // 3. Create channel state
    let channel = Channel {
        channel_id: request.channel_id.clone(),
        peer_id: sender.clone(),
        state: ChannelState::Open,
        my_balance: our_deposit,
        their_balance: request.initial_balance,
        nonce: 0,
        last_update: current_timestamp(),
        pending_payments: vec![],
    };
    
    // 4. Store
    self.channels.create(sender, channel)?;
    
    // 5. Return accept
    Ok(ChannelAcceptPayload {
        channel_id: request.channel_id,
        initial_balance: our_deposit,
        funding_tx: None,
    })
}

fn handle_channel_accept(&mut self, channel_id: &Hash, accept: &ChannelAcceptPayload) -> Result<()> {
    // Update channel to Open state with peer's deposit
    let channel = self.channels.get_by_id(channel_id)?
        .ok_or(Error::ChannelNotFound)?;
    
    let mut updated = channel.clone();
    updated.state = ChannelState::Open;
    updated.their_balance = accept.initial_balance;
    updated.last_update = current_timestamp();
    
    self.channels.update(&updated)?;
    Ok(())
}
```

### §7.3.3 CHANNEL_CLOSE

Cooperative channel close flow:
1. Initiator creates settlement_tx proposal
2. Send ChannelClosePayload to peer
3. Peer verifies and signs
4. Either party submits signed tx to chain

```rust
async fn close_channel(&mut self, channel_id: &Hash) -> Result<()> {
    // 1. Get channel
    let channel = self.channels.get_by_id(channel_id)?
        .ok_or(Error::ChannelNotFound)?;
    
    // 2. Compute final balances
    let final_balances = ChannelBalances {
        initiator: channel.my_balance,
        responder: channel.their_balance,
    };
    
    // 3. Create proposed settlement transaction bytes
    let settlement_tx = self.settlement.create_close_tx_bytes(
        channel_id,
        &final_balances,
    );
    
    // 4. Sign the proposal
    let my_signature = self.identity.sign(&settlement_tx)?;
    
    // 5. Send close request to peer
    let close_msg = ChannelClosePayload {
        channel_id: channel_id.clone(),
        final_balances: final_balances.clone(),
        settlement_tx: settlement_tx.clone(),
    };
    
    let response = self.network.send_channel_close(&channel.peer_id, close_msg).await?;
    
    // 6. Peer's response includes their signature - submit to chain
    // (The response handler on peer side also signs the settlement_tx)
    self.settlement.close_channel(
        channel_id,
        final_balances,
        [my_signature, response.peer_signature],
    ).await?;
    
    // 7. Update local state
    self.channels.set_state(channel_id, ChannelState::Closed)?;
    
    Ok(())
}
```

### §7.3.4 CHANNEL_DISPUTE

```rust
async fn dispute_channel(&mut self, channel_id: &Hash, our_state: &ChannelUpdatePayload) -> Result<()> {
    // 1. Submit dispute to chain with our latest signed state
    self.settlement.dispute_channel(channel_id, our_state).await?;
    
    // 2. Update local state
    self.channels.set_state(channel_id, ChannelState::Disputed)?;
    
    // 3. Wait for dispute period (24 hours) - handled by settlement module
    Ok(())
}
```

---

## §7.4 Version Operations

### handle_version_request

```rust
async fn handle_version_request(
    &mut self,
    _sender: &PeerId,
    request: VersionRequestPayload,
) -> Result<VersionResponsePayload> {
    // 1. Get all versions for this root
    let versions = self.manifest_store.get_versions(&request.version_root)?;
    
    if versions.is_empty() {
        return Err(Error::NotFound);
    }
    
    // 2. Find latest
    let latest = versions.iter()
        .max_by_key(|m| m.version.number)
        .unwrap();
    
    // 3. Convert to VersionInfo
    let version_infos: Vec<VersionInfo> = versions.iter()
        .map(|m| VersionInfo {
            hash: m.hash.clone(),
            number: m.version.number,
            timestamp: m.version.timestamp,
            visibility: m.visibility,
            price: m.economics.price,
        })
        .collect();
    
    Ok(VersionResponsePayload {
        version_root: request.version_root,
        versions: version_infos,
        latest: latest.hash.clone(),
    })
}
```

---

## §7.5 Settlement Operations

### trigger_settlement

Called periodically or when threshold reached. Creates batch and submits to chain.

```rust
async fn trigger_settlement(&mut self) -> Result<Option<SettlementBatch>> {
    // 1. Check if settlement needed
    let pending_total = self.settlement_queue.get_pending_total()?;
    let last_settlement = self.settlement_queue.get_last_settlement_time()?;
    
    if !should_settle(pending_total, last_settlement.unwrap_or(0), current_timestamp()) {
        return Ok(None);
    }
    
    // 2. Get pending distributions
    let pending = self.settlement_queue.get_pending()?;
    if pending.is_empty() {
        return Ok(None);
    }
    
    // 3. Create batch (aggregates by recipient)
    let payments: Vec<Payment> = pending.iter()
        .map(|d| reconstruct_payment(d))
        .collect();
    
    let batch = create_settlement_batch(&payments);
    
    // 4. Submit to chain
    let tx_id = self.settlement.settle_batch(batch.clone()).await?;
    
    // 5. Mark as settled
    let payment_ids: Vec<Hash> = pending.iter().map(|d| d.payment_id.clone()).collect();
    self.settlement_queue.mark_settled(&payment_ids, &batch.batch_id)?;
    self.settlement_queue.set_last_settlement_time(current_timestamp())?;
    
    // 6. Broadcast confirmation
    let confirm = SettleConfirmPayload {
        batch_id: batch.batch_id.clone(),
        transaction_id: tx_id.to_vec(),
        timestamp: current_timestamp(),
    };
    self.network.broadcast_settlement_confirm(confirm).await?;
    
    Ok(Some(batch))
}
```

---

## Public API Summary

```rust
// Content lifecycle
pub async fn create(...) -> Result<Hash>;
pub async fn extract_l1(...) -> Result<L1Summary>;
pub async fn publish(...) -> Result<()>;
pub async fn unpublish(...) -> Result<()>;
pub async fn update(...) -> Result<Hash>;
pub async fn derive(...) -> Result<Hash>;
pub async fn reference_l3_as_l0(...) -> Result<()>;

// Querying
pub async fn preview(...) -> Result<(Manifest, L1Summary)>;
pub async fn query(...) -> Result<QueryResponse>;  // Auto-opens channel if needed
pub async fn get_versions(...) -> Result<Vec<VersionInfo>>;

// Visibility/access
pub async fn set_visibility(...) -> Result<()>;
pub async fn set_access(...) -> Result<()>;

// Channel operations
pub async fn open_channel(...) -> Result<Hash>;
pub async fn accept_channel(...) -> Result<()>;
pub async fn close_channel(...) -> Result<()>;
pub async fn dispute_channel(...) -> Result<()>;

// Settlement
pub async fn trigger_settlement(...) -> Result<Option<SettlementBatch>>;

// Handlers (for incoming messages)
pub async fn handle_preview_request(...) -> Result<PreviewResponsePayload>;
pub async fn handle_query_request(...) -> Result<QueryResponsePayload>;
pub async fn handle_version_request(...) -> Result<VersionResponsePayload>;
pub async fn handle_channel_open(...) -> Result<ChannelAcceptPayload>;
pub async fn handle_channel_close(...) -> Result<ChannelClosePayload>;
```

---

## Test Cases

1. **Create L0**: Creates content, hash matches, owner set
2. **Extract L1**: Rule-based extraction produces mentions
3. **Create then publish**: Visibility changes, price set
4. **Unpublish**: Visibility returns to Private
5. **Update version**: New hash, version links correctly
6. **Derive L3**: Sources merged, depth incremented, owner set
7. **Query flow**: Request → auto-open channel → payment → response → cache
8. **Query with existing channel**: Uses existing channel
9. **Query insufficient balance**: Returns PAYMENT_REQUIRED
10. **Access denied**: Private content returns NotFound
11. **Unlisted access**: With hash works, without fails
12. **Insufficient payment**: Rejected
13. **Reference L3**: Only works if queried first
14. **Channel open**: Creates channel, both sides have state
15. **Channel close**: Cooperative close submits to chain
16. **Channel dispute**: Submits dispute with latest state
17. **Version request**: Returns all versions for root
18. **Settlement trigger**: Creates batch, submits to chain
19. **Settlement threshold**: Triggers when threshold reached
20. **Settlement interval**: Triggers after time elapsed
