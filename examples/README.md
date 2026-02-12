# Nodalync Protocol Examples

This directory contains practical, runnable examples demonstrating real-world integration patterns with the Nodalync protocol. Each example is a complete Rust project with comprehensive documentation.

## Quick Start

1. Ensure you have the development environment set up (see [CONTRIBUTING.md](../CONTRIBUTING.md))
2. Navigate to any example directory
3. Run `cargo run` to execute the example
4. Check the example's README for specific setup requirements

## Available Examples

### 1. [Basic Node Setup](./01-basic-node-setup/)
**Duration: 5 minutes** | **Difficulty: Beginner**

Learn the fundamentals of creating a Nodalync node, validating content, and basic protocol operations.

```bash
cd 01-basic-node-setup && cargo run
```

### 2. [Network Communication](./02-network-communication/) 
**Duration: 10 minutes** | **Difficulty: Intermediate**

Demonstrates multi-node communication using libp2p networking, DHT discovery, and message passing.

```bash
cd 02-network-communication && cargo run
```

### 3. [Payment Channels](./03-payment-channels/)
**Duration: 15 minutes** | **Difficulty: Intermediate**

Complete payment channel lifecycle: creation, micropayments, settlement via Hedera, and error recovery.

```bash
cd 03-payment-channels && cargo run
```

### 4. [MCP Integration](./04-mcp-integration/)
**Duration: 10 minutes** | **Difficulty: Intermediate**

Shows how AI agents integrate with Nodalync via Model Context Protocol (MCP) for budget tracking and content attribution.

```bash
cd 04-mcp-integration && cargo run
```

### 5. [Creator Dashboard](./05-creator-dashboard/)
**Duration: 20 minutes** | **Difficulty: Advanced**

Real-time creator earnings tracking, distribution calculations, and performance monitoring.

```bash
cd 05-creator-dashboard && cargo run
```

### 6. [Network Monitoring](./06-network-monitoring/)
**Duration: 10 minutes** | **Difficulty: Intermediate**

Network health monitoring, node discovery, and diagnostic tools for production deployments.

```bash
cd 06-network-monitoring && cargo run
```

## Common Patterns

All examples follow consistent patterns:

- **Error Handling**: Comprehensive error types with actionable messages
- **Logging**: Structured logging with tracing crate
- **Testing**: Unit tests covering happy path and edge cases  
- **Documentation**: Inline docs explaining protocol concepts
- **Configuration**: Environment-based configuration with defaults

## Development Guidelines

### Running Examples
```bash
# Run a specific example
cd examples/01-basic-node-setup && cargo run

# Run tests for all examples
cargo test --manifest-path examples/Cargo.toml

# Run with debug logging
RUST_LOG=debug cargo run
```

### Modifying Examples
Each example is self-contained. Feel free to:
- Modify parameters and see how behavior changes
- Add your own features building on the examples
- Use them as starting points for your own projects

### Dependencies
Examples use the same dependency versions as the main protocol crates:
- Latest stable Nodalync protocol crates
- Compatible versions of libp2p, tokio, tracing
- See each example's `Cargo.toml` for specific versions

## Integration Patterns

### Content Validation
```rust
use nodalync_valid::ContentValidator;

let validator = ContentValidator::new();
let result = validator.validate_content(&content_hash)?;
```

### Network Operations  
```rust
use nodalync_net::NetworkManager;

let mut network = NetworkManager::new().await?;
network.connect_to_bootstrap_peers(&bootstrap_nodes).await?;
```

### Economic Operations
```rust
use nodalync_econ::PaymentChannel;

let channel = PaymentChannel::new(creator_id, consumer_id)?;
channel.create_micropayment(amount, content_hash)?;
```

## Troubleshooting

### Common Issues

**Build Errors**: Ensure OpenSSL is installed (see CONTRIBUTING.md Windows setup)
**Network Timeouts**: Check firewall settings for peer-to-peer communication
**Permission Errors**: Ensure proper file permissions for data directories

### Getting Help

- Review the [Architecture Map](../coordination/proposals/nodalync-architecture-map.md)
- Check [CONTRIBUTING.md](../CONTRIBUTING.md) for development setup
- Consult protocol documentation in `docs/`
- Open issues on the repository for bugs or questions

## Contributing Examples

Want to add a new example? Follow this structure:

1. Create `examples/07-your-example/` directory
2. Add `Cargo.toml` with appropriate dependencies
3. Create `src/main.rs` with clear, commented code
4. Write `README.md` explaining the example purpose and setup
5. Add tests in `src/tests.rs` or `tests/` directory
6. Update this main README with your example

Keep examples:
- **Focused**: One concept per example
- **Complete**: Runnable without external dependencies when possible
- **Documented**: Clear explanations of what's happening and why
- **Tested**: Include test coverage for major functionality