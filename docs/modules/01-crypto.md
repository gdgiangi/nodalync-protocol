# Module: nodalync-crypto

**Source:** Protocol Specification §3

## Overview

This module provides all cryptographic primitives for the Nodalync protocol. It has no internal dependencies and should be implemented first.

## Dependencies

External only:
- `sha2` — SHA-256 implementation
- `ed25519-dalek` — Ed25519 signatures
- `rand` — Random number generation
- `bs58` — Base58 encoding (for human-readable IDs)

---

## §3.1 Hash Function

**Algorithm:** SHA-256

Content hashes are computed as:

```
ContentHash(content) = H(
    0x00 ||                    # Domain separator for content
    len(content) as uint64 ||  # Big-endian length prefix
    content                    # Raw content bytes
)
```

### Implementation Notes

- Use domain separator `0x00` to prevent hash collision across different uses
- Length is encoded as big-endian uint64
- Returns 32-byte hash

### Test Cases

1. **Determinism**: Same content → same hash
2. **Uniqueness**: Different content → different hash (probabilistic)
3. **Domain separation**: `ContentHash(x) ≠ H(x)` (raw hash without prefix)

---

## §3.2 Identity

**Algorithm:** Ed25519

### Keypair Generation

```rust
fn generate_keypair() -> (PrivateKey, PublicKey)
```

### PeerId Derivation

PeerId is derived from public key:

```
PeerId = H(
    0x00 ||                    # Key type: Ed25519
    public_key                 # 32 bytes
)[0:20]                        # Truncate to 20 bytes
```

### Human-Readable Format

Format: `ndl1` + base32(PeerId)

Example: `ndl1qpzry9x8gf2tvdw0s3jn54khce6mua7l`

### Implementation Notes

- PeerId is 20 bytes (160 bits) — sufficient entropy, compact
- Prefix `ndl1` identifies Nodalync addresses (like `bc1` for Bitcoin)
- Use Bech32 or similar for human-readable encoding with checksum

### Test Cases

1. **Determinism**: Same public key → same PeerId
2. **Roundtrip**: encode → decode → original PeerId
3. **Checksum**: Invalid checksum rejected

---

## §3.3 Signatures

All protocol messages requiring authentication are signed.

### Signature Creation

```rust
fn sign(private_key: &PrivateKey, message: &[u8]) -> Signature
```

Internally:
```
signature = Ed25519_Sign(private_key, H(message))
```

### Signature Verification

```rust
fn verify(public_key: &PublicKey, message: &[u8], signature: &Signature) -> bool
```

Internally:
```
Ed25519_Verify(public_key, H(message), signature)
```

### SignedMessage Structure

```rust
pub struct SignedMessage {
    pub payload: Vec<u8>,
    pub signer: PeerId,
    pub signature: Signature,
}
```

### Test Cases

1. **Valid signature**: Sign → Verify succeeds
2. **Tampered message**: Modify payload → Verify fails
3. **Wrong key**: Verify with different public key → fails
4. **Truncated signature**: Short signature → fails

---

## §3.4 Content Addressing

Content is referenced by its hash. The hash serves as a unique, verifiable identifier.

### Verification

```rust
fn verify_content(content: &[u8], expected_hash: &Hash) -> bool {
    ContentHash(content) == expected_hash
}
```

### Test Cases

1. **Valid content**: Verify succeeds
2. **Tampered content**: Single byte change → Verify fails

---

## Data Types

```rust
/// 32-byte SHA-256 hash
pub struct Hash(pub [u8; 32]);

/// Ed25519 private key (32 bytes, keep secret)
pub struct PrivateKey([u8; 32]);

/// Ed25519 public key (32 bytes)
pub struct PublicKey(pub [u8; 32]);

/// Ed25519 signature (64 bytes)
pub struct Signature(pub [u8; 64]);

/// Truncated hash of public key (20 bytes)
pub struct PeerId(pub [u8; 20]);

/// Milliseconds since Unix epoch
pub type Timestamp = u64;
```

---

## Public API

```rust
// Content hashing
pub fn content_hash(content: &[u8]) -> Hash;
pub fn verify_content(content: &[u8], expected: &Hash) -> bool;

// Identity
pub fn generate_identity() -> (PrivateKey, PublicKey);
pub fn peer_id_from_public_key(public_key: &PublicKey) -> PeerId;
pub fn peer_id_to_string(peer_id: &PeerId) -> String;
pub fn peer_id_from_string(s: &str) -> Result<PeerId, ParseError>;

// Signing
pub fn sign(private_key: &PrivateKey, message: &[u8]) -> Signature;
pub fn verify(public_key: &PublicKey, message: &[u8], signature: &Signature) -> bool;
```

---

## Appendix: Hash Domain Separators

From spec Appendix A.2:

| Use | Domain Byte | Description |
|-----|-------------|-------------|
| Content | `0x00` | Content hashing |
| Messages | `0x01` | Message signing |
| Channels | `0x02` | Channel state |

These ensure hashes computed for different purposes never collide.
