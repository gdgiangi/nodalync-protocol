# Basic Node Setup Example

**Duration: 5 minutes** | **Difficulty: Beginner**

This example demonstrates the fundamental operations of a Nodalync protocol node:

- Creating a node identity with cryptographic keys
- Content validation and storage
- Digital signing and verification
- Attribution calculation
- Error handling patterns

## What You'll Learn

- Core concepts of the Nodalync protocol
- How to create and configure a node
- Content processing workflow
- Basic cryptographic operations
- Attribution weight calculation

## Prerequisites

- Rust 1.70+ installed
- Basic familiarity with async/await in Rust
- Development environment set up (see [CONTRIBUTING.md](../../CONTRIBUTING.md))

## Running the Example

```bash
# Navigate to the example directory
cd examples/01-basic-node-setup

# Run the example
cargo run

# Run with debug logging to see more details
RUST_LOG=debug cargo run

# Run tests
cargo test
```

## Expected Output

```
🚀 Starting Nodalync Basic Node Setup Example
Creating new Nodalync node: Example Node
Generated node ID: node_abc123...
Storage initialized at: ./example_data

📊 Node Status: {
  "node_id": "node_abc123...",
  "name": "Example Node",
  "validation_level": "Standard",
  "is_healthy": true
}

🔍 Example 1: Processing Content
Processing content from creator: alice@example.com
✅ Content validation passed
✅ Content stored successfully

✍️  Example 2: Signing Content
✅ Content signed successfully
Content signature: sig_def456...

📥 Example 3: Retrieving Content
✅ Content found
Retrieved content item: ContentItem { ... }
Content data: Hello, Nodalync! This is some example content.

⚖️  Example 4: Attribution Calculation
✅ Attribution calculated for 1 creators

❌ Example 5: Error Handling
✅ Correctly handled missing content

🎉 Basic Node Setup Example Complete!
```

## Code Overview

### Node Configuration

```rust
struct NodeConfig {
    pub name: String,
    pub data_dir: String,
    pub validation_level: ValidationLevel,
}
```

The node configuration defines:
- **name**: Human-readable identifier
- **data_dir**: Where to store content and metadata
- **validation_level**: How strictly to validate content

### Node Creation

```rust
let mut node = BasicNode::new(config).await?;
```

Node creation involves:
1. Generating cryptographic identity (Ed25519 keypair)
2. Initializing content validator
3. Setting up local storage
4. Creating node ID from identity

### Content Processing

```rust
let content_hash = node.process_content(
    creator_id,
    content_data,
    metadata,
).await?;
```

Content processing workflow:
1. Hash the content using SHA-256
2. Create ContentItem with metadata
3. Validate content according to rules
4. Store in local database if valid
5. Return content hash for referencing

### Digital Signatures

```rust
let signature = node.sign_content(&content_hash)?;
```

Signing creates a digital signature that:
- Proves the node processed this content
- Can be verified by other nodes
- Uses Ed25519 cryptography for security

### Attribution Calculation

```rust
let attribution = node.calculate_attribution(&content_hashes)?;
```

Attribution determines:
- Which creators should be compensated
- How much each creator should receive
- Weight distribution across multiple creators

## Key Concepts

### Node Identity
Every node has a unique cryptographic identity consisting of:
- **Public Key**: Shared with the network for verification
- **Private Key**: Kept secret for signing operations
- **Node ID**: Derived from the public key

### Content Validation
The protocol validates content at multiple levels:
- **Basic**: Structure and format checks
- **Standard**: Content quality and metadata validation
- **Strict**: Full compliance with protocol specifications

### Content Hashing
All content is identified by cryptographic hashes:
- Uses SHA-256 for content integrity
- Provides content-addressed storage
- Enables deduplication and verification

### Attribution Weights
Attribution weights determine compensation:
- Range from 0.0 to 1.0
- Must sum to 1.0 across all creators
- Based on content contribution and usage

## Testing

The example includes comprehensive tests:

```bash
# Run all tests
cargo test

# Run specific test
cargo test test_content_processing

# Run with output
cargo test -- --nocapture
```

Test coverage includes:
- Node creation and configuration
- Content processing workflow  
- Cryptographic signing
- Attribution calculation
- Error handling scenarios

## Common Issues

### Storage Directory Permissions
```
Error: Failed to initialize content storage
```
**Solution**: Ensure the process has write permissions to the data directory

### Missing Dependencies
```
Error: Could not find nodalync-types
```
**Solution**: Run from the correct directory with proper dependencies

### Content Validation Failures
```
Error: Content validation failed: Invalid format
```
**Solution**: Check content format and metadata structure

## Next Steps

After completing this example:

1. **Experiment**: Modify validation levels and see how it affects processing
2. **Explore**: Try different content types and metadata structures  
3. **Extend**: Add your own validation rules or storage backends
4. **Continue**: Move on to the Network Communication example

## Related Documentation

- [Architecture Map](../../coordination/proposals/nodalync-architecture-map.md)
- [Protocol Specification](../../docs/PROTOCOL.md)
- [CONTRIBUTING.md](../../CONTRIBUTING.md)
- [Network Communication Example](../02-network-communication/)

## API Reference

### BasicNode

#### Methods

- `new(config: NodeConfig) -> Result<Self>`
  - Creates a new node with the given configuration
  - Generates cryptographic identity and sets up storage

- `process_content(creator_id, data, metadata) -> Result<ContentHash>`
  - Validates and stores content from a creator
  - Returns content hash for future reference

- `sign_content(content_hash) -> Result<Signature>`
  - Creates digital signature for content
  - Uses node's private key for authentication

- `get_content(hash) -> Result<Option<(ContentItem, Vec<u8>)>>`
  - Retrieves content and metadata by hash
  - Returns None if content not found

- `calculate_attribution(hashes) -> Result<HashMap<CreatorId, AttributionWeight>>`
  - Calculates attribution weights for creators
  - Distributes compensation based on contribution

- `status() -> NodeStatus`
  - Returns current node status and health information

### Error Types

Common errors you might encounter:
- `ValidationError`: Content failed validation rules
- `StorageError`: Issues with local storage operations
- `CryptographicError`: Problems with signing or key operations
- `IdentityError`: Invalid node or creator identities