# Contributing to Nodalync Protocol

Thank you for your interest in contributing to Nodalync! This guide will help you get started with development, understand our conventions, and make meaningful contributions to the protocol.

## Table of Contents

- [Quick Start](#quick-start)
- [Development Environment](#development-environment)
- [Project Structure](#project-structure)
- [Contribution Workflow](#contribution-workflow)
- [Code Style & Conventions](#code-style--conventions)
- [Testing Guidelines](#testing-guidelines)
- [Code Examples](#code-examples)
- [Adding New Features](#adding-new-features)
- [Debugging Tips](#debugging-tips)
- [Getting Help](#getting-help)

## Quick Start

```bash
# 1. Fork and clone
git clone https://github.com/YOUR-USERNAME/nodalync-protocol.git
cd nodalync-protocol

# 2. Install Rust (if not already installed)
winget install Rustlang.Rustup  # Windows
# or visit https://rustup.rs/

# 3. Build the project
cargo build --workspace

# 4. Run tests
cargo test --workspace

# 5. Create a feature branch
git checkout -b feature/my-improvement
```

## Development Environment

### Prerequisites

- **Rust 1.88+** (install via [rustup](https://rustup.rs/))
- **SQLite development headers** (usually included with system SQLite)
- **protoc** (Protocol Buffer compiler, for Hedera SDK integration)
- **Git**

### Windows-Specific Setup

```powershell
# Install Rust
winget install Rustlang.Rustup

# Install protoc (required for Hedera integration)
winget install ProtocolBuffers.Protobuf

# Install vcpkg (required for OpenSSL)
git clone https://github.com/Microsoft/vcpkg.git C:\vcpkg
C:\vcpkg\bootstrap-vcpkg.bat

# Integrate vcpkg with Visual Studio (CRITICAL for Rust compatibility)
C:\vcpkg\vcpkg integrate install

# Install OpenSSL with correct triplet for Rust
C:\vcpkg\vcpkg install openssl:x64-windows-static-md

# Set environment variable for vcpkg
$env:VCPKG_ROOT = "C:\vcpkg"
# Or set permanently:
[Environment]::SetEnvironmentVariable("VCPKG_ROOT", "C:\vcpkg", "User")

# Verify installation
rustc --version
cargo --version
protoc --version
```

**OpenSSL Setup Notes:**
- The MCP server (`nodalync-mcp`) requires OpenSSL for Hedera SDK integration
- **CRITICAL:** Use `x64-windows-static-md` triplet, not `x64-windows` (Rust compatibility)
- **CRITICAL:** Run `vcpkg integrate install` before installing OpenSSL
- vcpkg installation takes 10-30 minutes (compiles OpenSSL from source with MSVC)
- Alternative: Build without Hedera: `cargo build -p nodalync-mcp --no-default-features`
- Troubleshooting: Ensure `VCPKG_ROOT` environment variable is set if you see OpenSSL errors

**Common OpenSSL Errors:**
- `"could not find library openssl"`: Run vcpkg integration and install correct triplet
- `"linking with 'link.exe' failed"`: Ensure Visual Studio Build Tools are installed
- Tests timeout: OpenSSL compilation in progress, wait for vcpkg to complete

### Building

```bash
# Build all crates
cargo build --workspace

# Build specific crate
cargo build -p nodalync-types

# Build with Hedera features
cargo build --release -p nodalync-cli --features hedera-sdk

# Check without building (faster)
cargo check --workspace
```

### Running Tests

```bash
# Run all tests (may timeout - use per-crate instead)
cargo test --workspace

# Run tests for specific crate
cargo test -p nodalync-crypto
cargo test -p nodalync-types

# Run specific test
cargo test -p nodalync-cli test_publish_content

# Run tests with output
cargo test -p nodalync-net -- --nocapture
```

**⚠️ Important:** Full workspace tests may timeout. Always test individual crates during development.

## Project Structure

Nodalync follows a multi-crate architecture organized by responsibility:

```
nodalync-protocol/
├── crates/
│   ├── protocol/                    # Core protocol implementation
│   │   ├── nodalync-crypto/         # SHA-256, Ed25519, identity
│   │   ├── nodalync-types/          # Data structures & constants
│   │   ├── nodalync-wire/           # CBOR serialization, messages
│   │   ├── nodalync-store/          # SQLite persistence
│   │   ├── nodalync-valid/          # Validation logic
│   │   ├── nodalync-econ/           # Economic calculations
│   │   ├── nodalync-ops/            # High-level operations
│   │   ├── nodalync-net/            # P2P networking (libp2p)
│   │   └── nodalync-settle/         # Hedera settlement
│   ├── apps/                        # End-user applications
│   │   ├── nodalync-cli/            # Command-line interface
│   │   └── nodalync-mcp/            # MCP server for AI agents
│   └── nodalync-test-utils/         # Shared testing utilities
├── docs/                            # Protocol specification & guides
├── contracts/                       # Smart contracts (JavaScript/Hardhat)
└── infra/                          # Infrastructure configurations
```

### Dependency Order

Understand the crate dependency hierarchy to avoid circular dependencies:

```
Foundation Layer:
crypto → types → wire

Protocol Layer:  
store, valid, econ (depend on foundation)
    ↓
ops, net, settle (depend on protocol + foundation)

Application Layer:
cli, mcp (depend on all protocol crates)
```

## Contribution Workflow

### 1. Fork and Branch

```bash
# Fork the repository on GitHub
# Clone your fork
git clone https://github.com/YOUR-USERNAME/nodalync-protocol.git
cd nodalync-protocol

# Add upstream remote
git remote add upstream https://github.com/gdgiangi/nodalync-protocol.git

# Create feature branch from main
git checkout -b feature/short-description
```

### 2. Make Changes

- **Keep commits small and focused** - one logical change per commit
- **Write descriptive commit messages** - explain what and why, not just what
- **Test your changes** - run relevant tests before committing
- **Follow code style** - use `cargo fmt` and `cargo clippy`

Example commit message:
```
crypto: add content-addressed hash validation

Add validation for content hashes to ensure they match the actual
content before storing in the database. This prevents corruption
from network transmission errors.

Fixes #123
```

### 3. Test Thoroughly

```bash
# Format code
cargo fmt --all

# Check for common mistakes
cargo clippy --all-targets --all-features -- -D warnings

# Run tests for affected crates
cargo test -p nodalync-crypto
cargo test -p nodalync-types

# Build to ensure no compilation issues
cargo build --workspace
```

### 4. Submit Pull Request

- **Update documentation** if you changed APIs
- **Add tests** for new functionality
- **Reference issues** in the PR description
- **Keep PRs focused** - one feature/fix per PR

## Code Style & Conventions

### Rust Style

We follow standard Rust conventions with some project-specific guidelines:

```bash
# Format all code (required before commits)
cargo fmt --all

# Check for style issues
cargo clippy --all-targets --all-features -- -D warnings
```

### Naming Conventions

```rust
// Types: PascalCase
struct ContentManifest { }
enum MessageType { }

// Functions/variables: snake_case
fn validate_content_hash() -> bool { }
let peer_id = generate_peer_id();

// Constants: SCREAMING_SNAKE_CASE
const MAX_CONTENT_SIZE: usize = 16_777_216;
const DEFAULT_PRICE: Amount = Amount::new(1000);

// Modules: snake_case
mod content_validation;
mod payment_channels;
```

### Error Handling

Use `Result<T, E>` for fallible operations and provide meaningful error types:

```rust
// Good: Specific error types
pub enum ValidationError {
    InvalidHash(String),
    MissingSignature,
    ContentTooLarge { size: usize, max: usize },
}

// Good: Descriptive error messages
fn validate_manifest(manifest: &Manifest) -> Result<(), ValidationError> {
    if manifest.content_hash.len() != 32 {
        return Err(ValidationError::InvalidHash(
            format!("Expected 32 bytes, got {}", manifest.content_hash.len())
        ));
    }
    Ok(())
}
```

### Documentation

All public APIs must have documentation:

```rust
/// Validates a content manifest against protocol rules.
///
/// # Arguments
/// * `manifest` - The manifest to validate
/// * `content` - The actual content bytes
///
/// # Returns
/// * `Ok(())` if valid
/// * `Err(ValidationError)` if invalid with detailed error information
///
/// # Examples
/// ```
/// use nodalync_valid::validate_manifest;
/// let manifest = create_test_manifest();
/// let content = b"Hello, world!";
/// assert!(validate_manifest(&manifest, content).is_ok());
/// ```
pub fn validate_manifest(manifest: &Manifest, content: &[u8]) -> Result<(), ValidationError> {
    // Implementation...
}
```

## Testing Guidelines

### Test Organization

Tests are organized in several ways:

```rust
// Unit tests (in same file as implementation)
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_validation() {
        // Test implementation...
    }
}

// Integration tests (in tests/ directory)
// tests/integration_test.rs
use nodalync_crypto::*;

#[test]
fn test_end_to_end_signing() {
    // Multi-crate integration test...
}
```

### Test Naming

Use descriptive test names that explain the scenario:

```rust
#[test]
fn test_validate_manifest_accepts_valid_content() { }

#[test]
fn test_validate_manifest_rejects_mismatched_hash() { }

#[test]
fn test_validate_manifest_rejects_oversized_content() { }
```

### Test Data

Use the `nodalync-test-utils` crate for common test fixtures:

```rust
use nodalync_test_utils::*;

#[test]
fn test_content_operations() {
    let manifest = create_test_manifest();
    let content = create_test_content(1024);
    let keypair = create_test_keypair();
    
    // Test implementation...
}
```

### Running Tests

```bash
# Run all tests in a crate
cargo test -p nodalync-crypto

# Run specific test
cargo test -p nodalync-crypto test_hash_validation

# Run tests with output (for debugging)
cargo test -p nodalync-crypto -- --nocapture

# Run tests in release mode (for performance testing)
cargo test --release -p nodalync-net
```

## Code Examples

### Basic Protocol Operations

#### 1. Creating and Validating Content

```rust
use nodalync_crypto::*;
use nodalync_types::*;
use nodalync_valid::*;

// Generate a keypair for signing
let keypair = Keypair::generate();

// Create content
let content = b"# My Research\n\nThis is important knowledge...";
let content_hash = hash_content(content);

// Create manifest
let manifest = Manifest {
    content_hash,
    owner: keypair.public_key(),
    content_type: ContentType::Markdown,
    visibility: Visibility::Shared,
    price: Amount::from_hbar(0.01),
    created_at: current_timestamp(),
    // ... other fields
};

// Sign the manifest
let signature = sign_manifest(&manifest, &keypair)?;

// Validate everything
validate_manifest(&manifest, content)?;
validate_signature(&manifest, &signature, &keypair.public_key())?;
```

#### 2. Network Communication

```rust
use nodalync_net::*;
use nodalync_wire::*;

// Create a network node
let mut node = NetworkNode::new(keypair).await?;

// Start listening
node.start_listening("/ip4/0.0.0.0/tcp/0".parse()?).await?;

// Connect to bootstrap peers
let bootstrap_peers = vec![
    "/ip4/123.456.789.0/tcp/8765/p2p/12D3KooWBootstrapPeer".parse()?
];
node.bootstrap(bootstrap_peers).await?;

// Announce content to the network
let announce_msg = AnnounceMessage {
    content_hash,
    manifest: manifest.clone(),
    ttl: 3600, // 1 hour
};
node.broadcast_announce(announce_msg).await?;

// Search for content
let search_results = node.search_content("machine learning").await?;
```

#### 3. Payment Channels

```rust
use nodalync_econ::*;
use nodalync_settle::*;

// Open a payment channel
let channel = PaymentChannel::open(
    &payer_keypair,
    &payee_id,
    Amount::from_hbar(10.0), // 10 HBAR deposit
).await?;

// Make a micropayment
let payment = Payment {
    amount: Amount::from_hbar(0.01),
    recipient: payee_id,
    memo: "Query fee for content ABC123".to_string(),
};

channel.pay(payment, &payer_keypair).await?;

// Close and settle
channel.close_and_settle().await?;
```

### Testing Patterns

#### Mock Network for Testing

```rust
use nodalync_test_utils::*;

#[tokio::test]
async fn test_content_discovery() {
    // Create a test network with multiple nodes
    let network = TestNetwork::new(3).await;
    
    // Node 0 publishes content
    let content = create_test_content(1024);
    let manifest = network.nodes[0].publish_content(content).await?;
    
    // Node 1 searches and finds it
    let results = network.nodes[1].search_content(&manifest.content_hash).await?;
    assert_eq!(results.len(), 1);
    
    // Node 2 queries and pays for it
    let retrieved = network.nodes[2].query_content(&manifest.content_hash).await?;
    assert_eq!(retrieved.content, content);
}
```

## Adding New Features

### 1. Plan Your Feature

Before coding, consider:

- **Which crates are affected?** Start with the lowest-level crate and work up
- **What new types are needed?** Add to `nodalync-types` if used across crates
- **What wire protocol changes?** Update `nodalync-wire` and bump protocol version
- **What tests are needed?** Plan unit and integration tests

### 2. Protocol-Level Changes

If your feature affects the wire protocol:

1. **Update `nodalync-types`** with new data structures
2. **Update `nodalync-wire`** with new message types
3. **Update `nodalync-valid`** with new validation rules
4. **Increment protocol version** in `constants.rs`

Example - Adding a new message type:

```rust
// In nodalync-types/src/messages.rs
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewFeatureMessage {
    pub feature_data: String,
    pub timestamp: u64,
}

// In nodalync-wire/src/message_type.rs
pub enum MessageType {
    // ... existing types
    NewFeature = 18, // Next available number
}

// In nodalync-wire/src/envelope.rs
impl MessageEnvelope {
    pub fn new_feature(msg: NewFeatureMessage, keypair: &Keypair) -> Result<Self> {
        Self::new(MessageType::NewFeature, &msg, keypair)
    }
}
```

### 3. Application-Level Changes

For CLI or MCP features:

1. **Add command/tool** to the appropriate application crate
2. **Update help text and documentation**
3. **Add integration tests** showing end-to-end functionality

Example - Adding a CLI command:

```rust
// In nodalync-cli/src/commands/mod.rs
pub mod new_feature;

// In nodalync-cli/src/commands/new_feature.rs
use clap::Args;

#[derive(Args)]
pub struct NewFeatureArgs {
    /// Feature-specific argument
    #[arg(long)]
    pub param: String,
}

pub async fn handle_new_feature(args: NewFeatureArgs) -> Result<()> {
    // Implementation
    Ok(())
}

// In nodalync-cli/src/main.rs
#[derive(Subcommand)]
enum Commands {
    // ... existing commands
    NewFeature(NewFeatureArgs),
}

// In the match statement
Commands::NewFeature(args) => commands::new_feature::handle_new_feature(args).await,
```

### 4. Documentation Updates

- **Update README.md** if the feature is user-facing
- **Update relevant docs/*.md** files
- **Add code examples** to this CONTRIBUTING.md if useful for other developers
- **Update protocol spec** if wire protocol changes

## Debugging Tips

### Common Build Issues

**Issue: `protoc` not found**
```bash
# Windows
winget install ProtocolBuffers.Protobuf

# macOS
brew install protobuf

# Ubuntu/Debian
sudo apt install protobuf-compiler
```

**Issue: OpenSSL linking errors (Windows)**
```bash
# Install OpenSSL development libraries
# Or use the pre-built binaries approach in CI
```

**Issue: SQLite errors**
```bash
# Ensure SQLite development headers are installed
# Windows: Usually included with system SQLite
# Linux: sudo apt install libsqlite3-dev
```

### Debugging Network Issues

```rust
// Enable libp2p debug logging
use tracing_subscriber::EnvFilter;

tracing_subscriber::fmt()
    .with_env_filter(EnvFilter::from_default_env()
        .add_directive("libp2p=debug".parse().unwrap())
        .add_directive("nodalync_net=trace".parse().unwrap()))
    .init();
```

### Testing Locally

```bash
# Run a local node for testing
cargo run -p nodalync-cli -- start --health --health-port 8080

# In another terminal, test commands
cargo run -p nodalync-cli -- status
cargo run -p nodalync-cli -- publish test-content.md --price 0.01

# Check health endpoint
curl http://localhost:8080/health
```

### Using the Debugger

```toml
# In Cargo.toml for development
[profile.dev]
debug = true
opt-level = 0

# Run with debugger
cargo run -p nodalync-cli -- start
# Then attach your IDE's debugger or use gdb/lldb
```

### Performance Profiling

```bash
# Build with profiling info
cargo build --release --profile=profiling

# Use cargo flamegraph (install: cargo install flamegraph)
cargo flamegraph -p nodalync-cli -- command args

# Or use perf (Linux)
perf record --call-graph=dwarf ./target/release/nodalync command
perf report
```

## Getting Help

### Documentation

- **[Protocol Specification](docs/spec.md)** - Source of truth for protocol behavior
- **[Architecture Guide](docs/architecture.md)** - High-level system design
- **[Module Documentation](docs/modules/)** - Detailed per-crate documentation

### Community

- **[Discord](https://discord.gg/hYVrEAM6)** - Real-time discussion and help
- **[GitHub Issues](https://github.com/gdgiangi/nodalync-protocol/issues)** - Bug reports and feature requests
- **[GitHub Discussions](https://github.com/gdgiangi/nodalync-protocol/discussions)** - Longer-form questions

### Code Review Process

1. **Submit focused PRs** - one feature/fix per PR
2. **Respond to feedback** promptly and thoughtfully
3. **Test thoroughly** before requesting review
4. **Update documentation** as needed

### Maintainer Contact

- **Gabriel Giangi** - @gdgiangi on GitHub, gabegiangi@gmail.com

---

**Happy coding! 🦀**

Thanks for contributing to Nodalync. Every contribution, no matter how small, helps build the future of knowledge creator compensation.