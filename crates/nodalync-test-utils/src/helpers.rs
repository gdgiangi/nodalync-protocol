//! Helper functions for creating test fixtures.
//!
//! Provides convenience functions for generating test identities,
//! manifests, announce payloads, and pre-configured `NodeOperations`.

use nodalync_crypto::{
    content_hash, generate_identity, peer_id_from_public_key, Hash, PeerId, PrivateKey, PublicKey,
};
use nodalync_ops::DefaultNodeOperations;
use nodalync_store::{NodeState, NodeStateConfig};
use nodalync_types::{Amount, ContentType, L1Summary, Manifest, Metadata, Visibility};
use nodalync_wire::AnnouncePayload;
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::TempDir;

use crate::{MockNetwork, MockSettlement};

/// Load environment variables from the project root `.env` file.
///
/// Finds the project root by walking up from `CARGO_MANIFEST_DIR` looking
/// for the workspace `Cargo.toml` (contains `[workspace]`), then loads
/// `.env` from that directory. Variables already set in the environment
/// are not overwritten.
///
/// This is safe to call multiple times and from multiple tests.
pub fn try_load_dotenv() {
    if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        let mut path = PathBuf::from(manifest_dir);
        // Walk up until we find a Cargo.toml with [workspace]
        loop {
            let cargo_toml = path.join("Cargo.toml");
            if cargo_toml.exists() {
                if let Ok(contents) = std::fs::read_to_string(&cargo_toml) {
                    if contents.contains("[workspace]") {
                        break;
                    }
                }
            }
            if !path.pop() {
                return; // reached filesystem root without finding workspace
            }
        }
        let env_file = path.join(".env");
        if env_file.exists() {
            if let Ok(contents) = std::fs::read_to_string(&env_file) {
                for line in contents.lines() {
                    let line = line.trim();
                    if line.is_empty() || line.starts_with('#') {
                        continue;
                    }
                    if let Some((key, value)) = line.split_once('=') {
                        let key = key.trim();
                        let value = value.trim().trim_matches('"').trim_matches('\'');
                        // Only set if not already present
                        if std::env::var(key).is_err() {
                            std::env::set_var(key, value);
                        }
                    }
                }
            }
        }
    }
}

/// Try to load Hedera testnet credentials from environment (with `.env` fallback).
///
/// Returns `None` if credentials are not available, allowing tests to skip gracefully.
pub fn get_hedera_credentials() -> Option<(String, String, String)> {
    try_load_dotenv();
    let account_id = std::env::var("HEDERA_ACCOUNT_ID").ok()?;
    let private_key = std::env::var("HEDERA_PRIVATE_KEY").ok()?;
    let contract_id = std::env::var("HEDERA_CONTRACT_ID").ok()?;
    Some((account_id, private_key, contract_id))
}

/// Create test ops with both mock network and mock settlement.
///
/// Returns the ops instance, both mocks (for assertions), and the
/// temp directory (must be kept alive for the duration of the test).
pub fn create_test_ops_with_mocks() -> (DefaultNodeOperations, MockNetwork, MockSettlement, TempDir)
{
    let temp_dir = TempDir::new().unwrap();
    let config = NodeStateConfig::new(temp_dir.path());
    let state = NodeState::open(config).unwrap();
    let (_, public_key) = generate_identity();
    let peer_id = peer_id_from_public_key(&public_key);

    let mock_net = MockNetwork::new();
    let mock_settle = MockSettlement::new();

    let ops = DefaultNodeOperations::with_defaults_network_and_settlement(
        state,
        peer_id,
        Arc::new(mock_net.clone()),
        Arc::new(mock_settle.clone()),
    );

    (ops, mock_net, mock_settle, temp_dir)
}

/// Create test ops with a specific settlement (no network).
///
/// Returns the ops instance and the temp directory.
pub fn create_test_ops_with_settlement(
    settlement: Arc<dyn nodalync_settle::Settlement>,
) -> (DefaultNodeOperations, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let config = NodeStateConfig::new(temp_dir.path());
    let state = NodeState::open(config).unwrap();
    let (_, public_key) = generate_identity();
    let peer_id = peer_id_from_public_key(&public_key);

    let ops = DefaultNodeOperations::with_defaults_and_settlement(state, peer_id, settlement);
    (ops, temp_dir)
}

/// Create test ops with a specific network (no settlement).
///
/// Returns the ops instance and the temp directory.
pub fn create_test_ops_with_network(
    network: Arc<dyn nodalync_net::Network>,
) -> (DefaultNodeOperations, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let config = NodeStateConfig::new(temp_dir.path());
    let state = NodeState::open(config).unwrap();
    let (_, public_key) = generate_identity();
    let peer_id = peer_id_from_public_key(&public_key);

    let ops = DefaultNodeOperations::with_defaults_and_network(state, peer_id, network);
    (ops, temp_dir)
}

/// Generate a test keypair and peer ID.
pub fn test_keypair() -> (PrivateKey, PublicKey, PeerId) {
    let (private_key, public_key) = generate_identity();
    let peer_id = peer_id_from_public_key(&public_key);
    (private_key, public_key, peer_id)
}

/// Create a test manifest for L0 content.
///
/// Uses `Manifest::new_l0` with the provided hash, owner, and price.
pub fn test_manifest(hash: Hash, owner: PeerId, price: Amount) -> Manifest {
    let metadata = Metadata::new("Test Content", 100);
    let mut manifest = Manifest::new_l0(hash, owner, metadata, 1234567890000);
    manifest.visibility = Visibility::Shared;
    manifest.economics.price = price;
    manifest
}

/// Create a test `AnnouncePayload`.
pub fn test_announce_payload(hash: Hash, title: &str, price: Amount) -> AnnouncePayload {
    AnnouncePayload {
        hash,
        content_type: ContentType::L0,
        title: title.to_string(),
        l1_summary: L1Summary::empty(hash),
        price,
        addresses: vec!["/ip4/127.0.0.1/tcp/9000".to_string()],
        publisher_peer_id: None,
    }
}

/// Create a test content hash from a string label.
///
/// Convenience wrapper around `content_hash` for quick test hashes.
pub fn test_hash(label: &str) -> Hash {
    content_hash(label.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_test_keypair() {
        let (private_key, public_key, peer_id) = test_keypair();
        // Verify the peer ID is derived from the public key
        let derived = peer_id_from_public_key(&public_key);
        assert_eq!(peer_id, derived);
        // Private key should be non-zero
        assert!(private_key.as_bytes().iter().any(|&b| b != 0));
    }

    #[test]
    fn test_test_manifest() {
        let hash = test_hash("content");
        let (_, _, peer_id) = test_keypair();
        let manifest = test_manifest(hash, peer_id, 500);

        assert_eq!(manifest.hash, hash);
        assert_eq!(manifest.owner, peer_id);
        assert_eq!(manifest.economics.price, 500);
        assert_eq!(manifest.content_type, ContentType::L0);
        assert_eq!(manifest.visibility, Visibility::Shared);
    }

    #[test]
    fn test_test_announce_payload() {
        let hash = test_hash("announce");
        let payload = test_announce_payload(hash, "My Content", 100);

        assert_eq!(payload.hash, hash);
        assert_eq!(payload.title, "My Content");
        assert_eq!(payload.price, 100);
        assert_eq!(payload.content_type, ContentType::L0);
        assert!(!payload.addresses.is_empty());
    }

    #[test]
    fn test_test_hash() {
        let h1 = test_hash("a");
        let h2 = test_hash("b");
        assert_ne!(h1, h2);

        // Deterministic
        let h3 = test_hash("a");
        assert_eq!(h1, h3);
    }

    #[test]
    fn test_create_test_ops_with_mocks() {
        let (ops, mock_net, mock_settle, _temp) = create_test_ops_with_mocks();
        assert!(ops.has_network());
        assert!(ops.has_settlement());
        assert_eq!(mock_net.sent_message_count(), 0);
        assert_eq!(mock_settle.current_balance(), 0);
    }

    #[test]
    fn test_create_test_ops_with_settlement() {
        let settle = Arc::new(MockSettlement::new());
        let (ops, _temp) = create_test_ops_with_settlement(settle);
        assert!(!ops.has_network());
        assert!(ops.has_settlement());
    }

    #[test]
    fn test_create_test_ops_with_network() {
        let net = Arc::new(MockNetwork::new());
        let (ops, _temp) = create_test_ops_with_network(net);
        assert!(ops.has_network());
        assert!(!ops.has_settlement());
    }
}
