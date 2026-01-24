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
        matches!(manifest.content_type, ContentType::L0 | ContentType::L1 | ContentType::L3),
        ContentValidation("invalid content type")
    );
    ensure!(
        matches!(manifest.visibility, Visibility::Private | Visibility::Unlisted | Visibility::Shared),
        ContentValidation("invalid visibility")
    );
    
    Ok(())
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
        ContentType::L3 => {
            // L3: must derive from sources
            ensure!(
                !prov.root_L0L1.is_empty(),
                ProvenanceValidation("L3 must have at least one root")
            );
            ensure!(
                !prov.derived_from.is_empty(),
                ProvenanceValidation("L3 must derive from at least one source")
            );
            
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
            
            // No self-reference
            ensure!(
                !prov.derived_from.contains(&manifest.hash),
                ProvenanceValidation("cannot derive from self")
            );
            ensure!(
                !prov.root_L0L1.iter().any(|e| e.hash == manifest.hash),
                ProvenanceValidation("cannot be own root")
            );
        }
        ContentType::L1 => {
            // L1 follows same rules as L0 (it's extracted from L0)
            // Similar to L0 validation
        }
    }
    
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
}
```

---

## Test Cases

**For each validation function, test:**
1. Valid input passes
2. Each invalid condition is caught
3. Error message is descriptive
4. Edge cases (empty arrays, zero values, max values)
