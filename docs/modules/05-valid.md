# Module: nodalync-valid

**Source:** Protocol Specification §9

## Overview

All validation rules for the protocol. Returns detailed errors for debugging.

## Dependencies

- `nodalync-types` — All data structures
- `nodalync-crypto` — Hash verification

---

## Validation Trait

```rust
pub trait Validator {
    fn validate_content(&self, content: &[u8], manifest: &Manifest) -> Result<(), ValidationError>;
    fn validate_version(&self, manifest: &Manifest, previous: Option<&Manifest>) -> Result<(), ValidationError>;
    fn validate_provenance(&self, manifest: &Manifest, sources: &[Manifest]) -> Result<(), ValidationError>;
    fn validate_payment(&self, payment: &Payment, channel: &Channel, manifest: &Manifest) -> Result<(), ValidationError>;
    fn validate_message(&self, message: &Message) -> Result<(), ValidationError>;
    fn validate_access(&self, requester: &PeerId, manifest: &Manifest) -> Result<(), ValidationError>;
}
```

---

## §9.1 Content Validation

```rust
fn validate_content(content: &[u8], manifest: &Manifest) -> Result<()> {
    // 1. Hash matches
    ensure!(
        content_hash(content) == manifest.hash,
        ContentValidation("hash mismatch")
    );
    
    // 2. Size matches
    ensure!(
        content.len() as u64 == manifest.metadata.content_size,
        ContentValidation("size mismatch")
    );
    
    // 3. Title length
    ensure!(
        manifest.metadata.title.len() <= MAX_TITLE_LENGTH,
        ContentValidation("title too long")
    );
    
    // 4. Description length
    if let Some(ref desc) = manifest.metadata.description {
        ensure!(
            desc.len() <= MAX_DESCRIPTION_LENGTH,
            ContentValidation("description too long")
        );
    }
    
    // 5. Tags
    ensure!(
        manifest.metadata.tags.len() <= MAX_TAGS,
        ContentValidation("too many tags")
    );
    for tag in &manifest.metadata.tags {
        ensure!(
            tag.len() <= MAX_TAG_LENGTH,
            ContentValidation("tag too long")
        );
    }
    
    // 6. Valid enums
    ensure!(
        matches!(manifest.content_type, ContentType::L0 | ContentType::L1 | ContentType::L2 | ContentType::L3),
        ContentValidation("invalid content type")
    );
    ensure!(
        matches!(manifest.visibility, Visibility::Private | Visibility::Unlisted | Visibility::Shared),
        ContentValidation("invalid visibility")
    );
    
    // 7. L2-specific validation
    if manifest.content_type == ContentType::L2 {
        validate_l2_content(content, manifest)?;
    }
    
    Ok(())
}
```

---

## §9.1a L2 Content Validation

```rust
fn validate_l2_content(content: &[u8], manifest: &Manifest) -> Result<()> {
    // L2 MUST be private
    ensure!(
        manifest.visibility == Visibility::Private,
        L2Validation("L2 must be private")
    );
    
    // L2 MUST have zero price
    ensure!(
        manifest.economics.price == 0,
        L2Validation("L2 must have zero price")
    );
    
    // Deserialize and validate structure
    let l2: L2EntityGraph = deserialize(content)
        .map_err(|_| L2Validation("invalid L2 structure"))?;
    
    // ID matches
    ensure!(
        l2.id == manifest.hash,
        L2Validation("L2 id must match manifest hash")
    );
    
    // Must have at least one source L1
    ensure!(
        !l2.source_l1s.is_empty(),
        L2Validation("L2 must have at least one source L1")
    );
    ensure!(
        l2.source_l1s.len() <= MAX_SOURCE_L1S_PER_L2,
        L2Validation("too many source L1s")
    );
    
    // Must have at least one entity
    ensure!(
        !l2.entities.is_empty(),
        L2Validation("L2 must have at least one entity")
    );
    ensure!(
        l2.entities.len() <= MAX_ENTITIES_PER_L2 as usize,
        L2Validation("too many entities")
    );
    
    // Relationship limits
    ensure!(
        l2.relationships.len() <= MAX_RELATIONSHIPS_PER_L2 as usize,
        L2Validation("too many relationships")
    );
    
    // Counts match
    ensure!(
        l2.entity_count as usize == l2.entities.len(),
        L2Validation("entity_count mismatch")
    );
    ensure!(
        l2.relationship_count as usize == l2.relationships.len(),
        L2Validation("relationship_count mismatch")
    );
    
    // Validate prefix map
    validate_prefix_map(&l2.prefixes)?;
    
    // Validate all entities
    let mut entity_ids: HashSet<Hash> = HashSet::new();
    for entity in &l2.entities {
        validate_entity(entity, &l2.prefixes, &l2.source_l1s)?;
        ensure!(
            entity_ids.insert(entity.id),
            L2Validation("duplicate entity ID")
        );
    }
    
    // Validate all relationships
    for rel in &l2.relationships {
        validate_relationship(rel, &entity_ids, &l2.prefixes, &l2.source_l1s)?;
    }
    
    Ok(())
}

fn validate_prefix_map(prefixes: &PrefixMap) -> Result<()> {
    let mut seen_prefixes: HashSet<&str> = HashSet::new();
    for entry in &prefixes.entries {
        ensure!(
            !entry.prefix.is_empty(),
            L2Validation("empty prefix")
        );
        ensure!(
            !entry.uri.is_empty(),
            L2Validation("empty URI")
        );
        ensure!(
            entry.uri.ends_with('/') || entry.uri.ends_with('#'),
            L2Validation("prefix URI must end with / or #")
        );
        ensure!(
            seen_prefixes.insert(&entry.prefix),
            L2Validation("duplicate prefix")
        );
    }
    Ok(())
}

fn validate_entity(
    entity: &Entity,
    prefixes: &PrefixMap,
    source_l1s: &[L1Reference],
) -> Result<()> {
    // Label constraints
    ensure!(
        !entity.canonical_label.is_empty(),
        L2Validation("empty canonical_label")
    );
    ensure!(
        entity.canonical_label.len() <= MAX_CANONICAL_LABEL_LENGTH,
        L2Validation("canonical_label too long")
    );
    
    // Aliases
    ensure!(
        entity.aliases.len() <= MAX_ALIASES_PER_ENTITY,
        L2Validation("too many aliases")
    );
    
    // Validate entity type URIs
    for uri in &entity.entity_types {
        validate_uri(uri, prefixes)?;
    }
    
    // Validate canonical_uri if present
    if let Some(ref uri) = entity.canonical_uri {
        validate_uri(uri, prefixes)?;
    }
    
    // Validate same_as URIs if present
    if let Some(ref same_as) = entity.same_as {
        for uri in same_as {
            validate_uri(uri, prefixes)?;
        }
    }
    
    // Confidence in range
    ensure!(
        entity.confidence >= 0.0 && entity.confidence <= 1.0,
        L2Validation("confidence out of range")
    );
    
    // All mention refs point to valid L1s
    let valid_l1_hashes: HashSet<_> = source_l1s.iter().map(|r| &r.l1_hash).collect();
    for mention_ref in &entity.source_mentions {
        ensure!(
            valid_l1_hashes.contains(&mention_ref.l1_hash),
            L2Validation("mention ref points to unknown L1")
        );
    }
    
    // Description length
    if let Some(ref desc) = entity.description {
        ensure!(
            desc.len() <= MAX_ENTITY_DESCRIPTION_LENGTH,
            L2Validation("entity description too long")
        );
    }
    
    Ok(())
}

fn validate_relationship(
    rel: &Relationship,
    entity_ids: &HashSet<Hash>,
    prefixes: &PrefixMap,
    source_l1s: &[L1Reference],
) -> Result<()> {
    // Subject must exist
    ensure!(
        entity_ids.contains(&rel.subject),
        L2Validation("relationship subject not found")
    );
    
    // Predicate must be valid URI
    validate_uri(&rel.predicate, prefixes)?;
    
    // Object validation
    match &rel.object {
        RelationshipObject::EntityRef(hash) => {
            ensure!(
                entity_ids.contains(hash),
                L2Validation("relationship object entity not found")
            );
        }
        RelationshipObject::ExternalRef(uri) => {
            validate_uri(uri, prefixes)?;
        }
        RelationshipObject::Literal(lit) => {
            if let Some(ref dt) = lit.datatype {
                validate_uri(dt, prefixes)?;
            }
        }
    }
    
    // Confidence in range
    ensure!(
        rel.confidence >= 0.0 && rel.confidence <= 1.0,
        L2Validation("relationship confidence out of range")
    );
    
    // Temporal validity
    if let (Some(from), Some(to)) = (rel.valid_from, rel.valid_to) {
        ensure!(from <= to, L2Validation("valid_from > valid_to"));
    }
    
    // Mention refs
    let valid_l1_hashes: HashSet<_> = source_l1s.iter().map(|r| &r.l1_hash).collect();
    for mention_ref in &rel.source_mentions {
        ensure!(
            valid_l1_hashes.contains(&mention_ref.l1_hash),
            L2Validation("relationship mention ref points to unknown L1")
        );
    }
    
    Ok(())
}
```

---

## §9.1b URI/CURIE Validation

```rust
/// Validate a URI or CURIE
fn validate_uri(uri: &Uri, prefixes: &PrefixMap) -> Result<()> {
    ensure!(!uri.is_empty(), L2Validation("empty URI"));
    
    if uri.contains("://") {
        // Full URI - basic syntax check
        ensure!(
            uri.starts_with("http://") || uri.starts_with("https://"),
            L2Validation("URI must be http(s)")
        );
    } else if let Some(colon_pos) = uri.find(':') {
        // CURIE - check prefix exists
        let prefix = &uri[..colon_pos];
        let has_prefix = prefixes.entries.iter().any(|e| e.prefix == prefix);
        ensure!(
            has_prefix,
            L2Validation(format!("unknown prefix: {}", prefix))
        );
    } else {
        // No scheme or prefix - invalid
        return Err(L2Validation("URI must be full URI or valid CURIE"));
    }
    
    Ok(())
}

/// Expand a CURIE to full URI
pub fn expand_curie(curie: &str, prefixes: &PrefixMap) -> Result<String> {
    if curie.contains("://") {
        // Already a full URI
        return Ok(curie.to_string());
    }
    
    if let Some(colon_pos) = curie.find(':') {
        let prefix = &curie[..colon_pos];
        let local = &curie[colon_pos + 1..];
        
        for entry in &prefixes.entries {
            if entry.prefix == prefix {
                return Ok(format!("{}{}", entry.uri, local));
            }
        }
        Err(L2Validation(format!("unknown prefix: {}", prefix)))
    } else {
        Err(L2Validation("not a valid CURIE"))
    }
}
```

---

## §9.2 Version Validation

```rust
fn validate_version(manifest: &Manifest, previous: Option<&Manifest>) -> Result<()> {
    let v = &manifest.version;
    
    if v.number == 1 {
        // First version
        ensure!(v.previous.is_none(), VersionValidation("v1 must have no previous"));
        ensure!(v.root == manifest.hash, VersionValidation("v1 root must equal hash"));
    } else {
        // Subsequent version
        ensure!(v.previous.is_some(), VersionValidation("v2+ must have previous"));
        
        if let Some(prev) = previous {
            ensure!(
                v.previous.as_ref() == Some(&prev.hash),
                VersionValidation("previous hash mismatch")
            );
            ensure!(
                v.root == prev.version.root,
                VersionValidation("root must equal previous root")
            );
            ensure!(
                v.number == prev.version.number + 1,
                VersionValidation("version number must increment by 1")
            );
            ensure!(
                v.timestamp > prev.version.timestamp,
                VersionValidation("timestamp must be after previous")
            );
        }
    }
    
    Ok(())
}
```

---

## §9.3 Provenance Validation

```rust
fn validate_provenance(manifest: &Manifest, sources: &[Manifest]) -> Result<()> {
    let prov = &manifest.provenance;
    
    match manifest.content_type {
        ContentType::L0 => {
            // L0: self-referential provenance
            ensure!(
                prov.root_L0L1.len() == 1,
                ProvenanceValidation("L0 must have exactly one root (self)")
            );
            ensure!(
                prov.root_L0L1[0].hash == manifest.hash,
                ProvenanceValidation("L0 root must be self")
            );
            ensure!(
                prov.derived_from.is_empty(),
                ProvenanceValidation("L0 must not derive from anything")
            );
            ensure!(
                prov.depth == 0,
                ProvenanceValidation("L0 depth must be 0")
            );
        }
        ContentType::L1 => {
            // L1: extracted from exactly one L0
            ensure!(
                !prov.root_L0L1.is_empty(),
                ProvenanceValidation("L1 must have at least one root")
            );
            ensure!(
                prov.derived_from.len() == 1,
                ProvenanceValidation("L1 must derive from exactly one L0")
            );
            ensure!(
                prov.depth == 1,
                ProvenanceValidation("L1 depth must be 1")
            );
            // All roots must be L0
            for root in &prov.root_L0L1 {
                if let Some(source) = sources.iter().find(|s| s.hash == root.hash) {
                    ensure!(
                        source.content_type == ContentType::L0,
                        ProvenanceValidation("L1 roots must all be L0")
                    );
                }
            }
        }
        ContentType::L2 => {
            // L2: built from L1s (and optionally other L2s)
            ensure!(
                !prov.root_L0L1.is_empty(),
                ProvenanceValidation("L2 must have at least one root")
            );
            ensure!(
                !prov.derived_from.is_empty(),
                ProvenanceValidation("L2 must derive from at least one source")
            );
            ensure!(
                prov.depth >= 2,
                ProvenanceValidation("L2 depth must be >= 2")
            );
            
            // All roots must be L0 or L1 (never L2 or L3)
            for root in &prov.root_L0L1 {
                if let Some(source) = sources.iter().find(|s| s.hash == root.hash) {
                    ensure!(
                        matches!(source.content_type, ContentType::L0 | ContentType::L1),
                        ProvenanceValidation("L2 roots must be L0 or L1 only")
                    );
                }
            }
            
            // derived_from must be L1 or L2
            for df in &prov.derived_from {
                if let Some(source) = sources.iter().find(|s| s.hash == *df) {
                    ensure!(
                        matches!(source.content_type, ContentType::L1 | ContentType::L2),
                        ProvenanceValidation("L2 must derive from L1 or L2")
                    );
                }
            }
            
            // Verify root_L0L1 computation
            let computed_roots = compute_root_L0L1(sources);
            ensure!(
                roots_match(&prov.root_L0L1, &computed_roots),
                ProvenanceValidation("root_L0L1 computation mismatch")
            );
            
            // Verify depth
            let expected_depth = sources.iter()
                .map(|s| s.provenance.depth)
                .max()
                .unwrap_or(0) + 1;
            ensure!(
                prov.depth == expected_depth,
                ProvenanceValidation("depth mismatch")
            );
        }
        ContentType::L3 => {
            // L3: must derive from sources (L0, L1, L2, or other L3)
            ensure!(
                !prov.root_L0L1.is_empty(),
                ProvenanceValidation("L3 must have at least one root")
            );
            ensure!(
                !prov.derived_from.is_empty(),
                ProvenanceValidation("L3 must derive from at least one source")
            );
            
            // All roots must be L0 or L1 (never L2 or L3)
            for root in &prov.root_L0L1 {
                if let Some(source) = sources.iter().find(|s| s.hash == root.hash) {
                    ensure!(
                        matches!(source.content_type, ContentType::L0 | ContentType::L1),
                        ProvenanceValidation("L3 roots must be L0 or L1 only")
                    );
                }
            }
            
            // All derived_from must exist in sources
            let source_hashes: HashSet<_> = sources.iter().map(|s| &s.hash).collect();
            for df in &prov.derived_from {
                ensure!(
                    source_hashes.contains(df),
                    ProvenanceValidation("derived_from references unknown source")
                );
            }
            
            // Verify root_L0L1 computation
            let computed_roots = compute_root_L0L1(sources);
            ensure!(
                roots_match(&prov.root_L0L1, &computed_roots),
                ProvenanceValidation("root_L0L1 computation mismatch")
            );
            
            // Verify depth
            let expected_depth = sources.iter()
                .map(|s| s.provenance.depth)
                .max()
                .unwrap_or(0) + 1;
            ensure!(
                prov.depth == expected_depth,
                ProvenanceValidation("depth mismatch")
            );
        }
    }
    
    // Common checks for all types
    // No self-reference
    ensure!(
        !prov.derived_from.contains(&manifest.hash),
        ProvenanceValidation("cannot derive from self")
    );
    ensure!(
        !prov.root_L0L1.iter().any(|e| e.hash == manifest.hash),
        ProvenanceValidation("cannot be own root")
    );
    
    // No cycles (basic check - full cycle detection is expensive)
    ensure!(
        prov.depth <= MAX_PROVENANCE_DEPTH,
        ProvenanceValidation("provenance too deep")
    );
    
    Ok(())
}
```

---

## §9.4 Payment Validation

```rust
fn validate_payment(payment: &Payment, channel: &Channel, manifest: &Manifest) -> Result<()> {
    // 1. Amount sufficient
    ensure!(
        payment.amount >= manifest.economics.price,
        PaymentValidation("insufficient payment")
    );
    
    // 2. Correct recipient
    ensure!(
        payment.recipient == manifest_owner(manifest),
        PaymentValidation("wrong recipient")
    );
    
    // 3. Query hash matches
    ensure!(
        payment.query_hash == manifest.hash,
        PaymentValidation("query hash mismatch")
    );
    
    // 4. Channel is open
    ensure!(
        channel.state == ChannelState::Open,
        PaymentValidation("channel not open")
    );
    
    // 5. Sufficient balance
    ensure!(
        channel.their_balance >= payment.amount,
        PaymentValidation("insufficient channel balance")
    );
    
    // 6. Nonce is valid (prevents replay)
    ensure!(
        payment_nonce(payment) > channel.nonce,
        PaymentValidation("invalid nonce (replay?)")
    );
    
    // 7. Signature valid
    let payer_pubkey = lookup_public_key(&payment_payer(payment, channel))?;
    ensure!(
        verify_payment_signature(&payer_pubkey, payment),
        PaymentValidation("invalid signature")
    );
    
    // 8. Provenance matches manifest
    ensure!(
        provenance_matches(&payment.provenance, &manifest.provenance.root_L0L1),
        PaymentValidation("provenance mismatch")
    );
    
    Ok(())
}
```

---

## §9.5 Message Validation

```rust
fn validate_message(msg: &Message) -> Result<()> {
    // 1. Protocol version
    ensure!(
        msg.version == PROTOCOL_VERSION,
        MessageValidation("unsupported protocol version")
    );
    
    // 2. Valid message type
    ensure!(
        is_valid_message_type(msg.message_type),
        MessageValidation("invalid message type")
    );
    
    // 3. Timestamp within skew
    let now = current_timestamp();
    let skew = if msg.timestamp > now {
        msg.timestamp - now
    } else {
        now - msg.timestamp
    };
    ensure!(
        skew <= MAX_CLOCK_SKEW_MS,
        MessageValidation("timestamp outside acceptable range")
    );
    
    // 4. Valid sender
    ensure!(
        is_valid_peer_id(&msg.sender),
        MessageValidation("invalid sender peer ID")
    );
    
    // 5. Signature valid
    let pubkey = lookup_public_key(&msg.sender)?;
    let msg_hash = message_hash(msg);
    ensure!(
        verify(&pubkey, &msg_hash.0, &msg.signature),
        MessageValidation("invalid signature")
    );
    
    // 6. Payload decodes
    ensure!(
        payload_decodes_for_type(&msg.payload, msg.message_type),
        MessageValidation("payload decode failed")
    );
    
    Ok(())
}
```

---

## §9.6 Access Validation

```rust
fn validate_access(requester: &PeerId, manifest: &Manifest) -> Result<()> {
    match manifest.visibility {
        Visibility::Private => {
            // Private: never accessible externally
            return Err(AccessValidation("content is private"));
        }
        Visibility::Unlisted => {
            // Check allowlist if set
            if let Some(ref allowlist) = manifest.access.allowlist {
                ensure!(
                    allowlist.contains(requester),
                    AccessValidation("not in allowlist")
                );
            }
            // Check denylist if set
            if let Some(ref denylist) = manifest.access.denylist {
                ensure!(
                    !denylist.contains(requester),
                    AccessValidation("in denylist")
                );
            }
        }
        Visibility::Shared => {
            // Only check denylist (allowlist ignored for Shared)
            if let Some(ref denylist) = manifest.access.denylist {
                ensure!(
                    !denylist.contains(requester),
                    AccessValidation("in denylist")
                );
            }
        }
    }
    
    // Check bond requirement
    if manifest.access.require_bond {
        ensure!(
            has_bond(requester, manifest.access.bond_amount.unwrap_or(0)),
            AccessValidation("bond required")
        );
    }
    
    Ok(())
}
```

---

## Error Types

```rust
#[derive(Debug, thiserror::Error)]
pub enum ValidationError {
    #[error("Content validation failed: {0}")]
    ContentValidation(String),
    
    #[error("Version validation failed: {0}")]
    VersionValidation(String),
    
    #[error("Provenance validation failed: {0}")]
    ProvenanceValidation(String),
    
    #[error("Payment validation failed: {0}")]
    PaymentValidation(String),
    
    #[error("Message validation failed: {0}")]
    MessageValidation(String),
    
    #[error("Access validation failed: {0}")]
    AccessValidation(String),
    
    #[error("L2 validation failed: {0}")]
    L2Validation(String),
    
    #[error("Publish validation failed: {0}")]
    PublishValidation(String),
}
```

---

## §9.7 Publish Validation

```rust
/// Validate that content can be published
fn validate_publish(manifest: &Manifest, visibility: Visibility) -> Result<()> {
    // L2 can NEVER be published
    if manifest.content_type == ContentType::L2 {
        return Err(PublishValidation("L2 content cannot be published"));
    }
    
    // Cannot publish to a more restricted visibility
    // (e.g., can't go from Shared back to Unlisted via PUBLISH)
    // This is handled by UNPUBLISH operation instead
    
    Ok(())
}
```

---

## Test Cases

**For each validation function, test:**
1. Valid input passes
2. Each invalid condition is caught
3. Error message is descriptive
4. Edge cases (empty arrays, zero values, max values)

**L2-specific tests:**
1. L2 with visibility != Private fails
2. L2 with price != 0 fails
3. L2 with empty entities fails
4. L2 with duplicate entity IDs fails
5. L2 with invalid entity reference in relationship fails
6. L2 with invalid URI/CURIE fails
7. L2 with unknown prefix fails
8. L2 PUBLISH attempt fails
9. CURIE expansion works correctly
10. Confidence values outside [0,1] fail
