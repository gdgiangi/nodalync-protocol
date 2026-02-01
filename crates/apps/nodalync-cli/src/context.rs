//! Node context for CLI operations.

use std::sync::Arc;

use nodalync_crypto::{PeerId, PrivateKey, PublicKey};
use nodalync_net::{Network, NetworkConfig, NetworkNode};
use nodalync_ops::DefaultNodeOperations;
use nodalync_settle::Settlement;
use nodalync_store::{NodeState, NodeStateConfig};

#[cfg(feature = "hedera-sdk")]
use nodalync_settle::{HederaConfig, HederaSettlement};

use crate::config::CliConfig;
use crate::error::{CliError, CliResult};
use crate::prompt::get_identity_password;

/// Create settlement instance based on configuration.
///
/// Supports:
/// - "hedera-testnet": Hedera testnet (requires `hedera` feature)
/// - "hedera-mainnet": Hedera mainnet (requires `hedera` feature)
///
/// For Hedera, credentials can come from:
/// 1. Config file (account_id, key_path, contract_id)
/// 2. Environment variables (HEDERA_ACCOUNT_ID, HEDERA_PRIVATE_KEY, HEDERA_CONTRACT_ID)
///
/// The settlement network can be overridden via HEDERA_NETWORK env var.
#[allow(unused_variables)]
async fn create_settlement(config: &CliConfig) -> CliResult<Arc<dyn Settlement>> {
    // Allow env var override for settlement network
    let network = std::env::var("HEDERA_NETWORK")
        .ok()
        .unwrap_or_else(|| config.settlement.network.clone());

    match network.as_str() {
        #[cfg(feature = "hedera-sdk")]
        "hedera-testnet" | "hedera-mainnet" => create_hedera_settlement(config, &network).await,
        #[cfg(not(feature = "hedera-sdk"))]
        "hedera-testnet" | "hedera-mainnet" => Err(CliError::User(
            "Hedera settlement requires the 'hedera' feature. \
                 Rebuild with: cargo build --features hedera"
                .into(),
        )),
        _ => Err(CliError::config(
            "Settlement network must be 'hedera-testnet' or 'hedera-mainnet'. \
             Set HEDERA_NETWORK env var or configure settlement.network in config.toml",
        )),
    }
}

/// Create Hedera settlement instance.
#[cfg(feature = "hedera-sdk")]
async fn create_hedera_settlement(
    config: &CliConfig,
    network: &str,
) -> CliResult<Arc<dyn Settlement>> {
    // Get account ID from config or environment
    let account_id = config
        .settlement
        .account_id
        .clone()
        .or_else(|| std::env::var("HEDERA_ACCOUNT_ID").ok())
        .ok_or_else(|| {
            CliError::config(
                "Hedera account ID required. Set in config or HEDERA_ACCOUNT_ID env var",
            )
        })?;

    // Get contract ID from config or environment
    let contract_id = config
        .settlement
        .contract_id
        .clone()
        .or_else(|| std::env::var("HEDERA_CONTRACT_ID").ok())
        .ok_or_else(|| {
            CliError::config(
                "Hedera contract ID required. Set in config or HEDERA_CONTRACT_ID env var",
            )
        })?;

    // Get private key path from config or write env var to temp file
    let private_key_path = if let Some(ref path) = config.settlement.key_path {
        path.clone()
    } else if let Ok(key) = std::env::var("HEDERA_PRIVATE_KEY") {
        // Write private key to a temporary file
        let key_path = config.base_dir().join("hedera.key");
        std::fs::write(&key_path, key.trim())?;
        key_path
    } else {
        return Err(CliError::config(
            "Hedera private key required. Set key_path in config or HEDERA_PRIVATE_KEY env var",
        ));
    };

    // Create Hedera config
    let hedera_config = if network == "hedera-mainnet" {
        HederaConfig::mainnet(&account_id, private_key_path, &contract_id)
    } else {
        HederaConfig::testnet(&account_id, private_key_path, &contract_id)
    };

    tracing::info!(
        network = network,
        account = %account_id,
        contract = %contract_id,
        "Initializing Hedera settlement"
    );

    let settlement = HederaSettlement::new(hedera_config)
        .await
        .map_err(|e| CliError::config(format!("Failed to initialize Hedera: {}", e)))?;

    Ok(Arc::new(settlement))
}

/// Convert a nodalync private key to a libp2p keypair.
///
/// Both use Ed25519, so the 32-byte seed can be used directly.
fn to_libp2p_keypair(private_key: &PrivateKey) -> CliResult<libp2p::identity::Keypair> {
    let secret = libp2p::identity::ed25519::SecretKey::try_from_bytes(*private_key.as_bytes())
        .map_err(|e| CliError::User(format!("Invalid Ed25519 key: {}", e)))?;
    let ed_keypair = libp2p::identity::ed25519::Keypair::from(secret);
    Ok(libp2p::identity::Keypair::from(ed_keypair))
}

/// Get the libp2p PeerId from a private key.
///
/// This is used for bootstrap addresses in multi-node configurations.
pub fn get_libp2p_peer_id(private_key: &PrivateKey) -> CliResult<libp2p::PeerId> {
    let keypair = to_libp2p_keypair(private_key)?;
    Ok(keypair.public().to_peer_id())
}

/// Node context containing all initialized components.
pub struct NodeContext {
    /// Operations interface.
    pub ops: DefaultNodeOperations,
    /// Settlement interface (optional, only available when Hedera is configured).
    pub settlement: Option<Arc<dyn Settlement>>,
    /// Network node (optional, for network operations).
    pub network: Option<Arc<NetworkNode>>,
    /// Configuration.
    pub config: CliConfig,
    /// Private key (for signing operations).
    pub private_key: PrivateKey,
}

impl NodeContext {
    /// Initialize node context for local-only operations.
    ///
    /// Does not start networking or settlement. Use this for commands like `list`, `versions`.
    pub fn local(config: CliConfig) -> CliResult<Self> {
        let base_dir = config.base_dir();

        // Open storage
        let state_config = NodeStateConfig::new(&base_dir);
        let state = NodeState::open(state_config)?;

        // Get peer ID (must exist)
        if !state.identity.exists() {
            return Err(CliError::IdentityNotInitialized);
        }
        let peer_id = state.identity.peer_id()?;

        // Create operations without network or settlement
        let ops = DefaultNodeOperations::with_defaults(state, peer_id);

        // Use a dummy private key for local-only operations
        // (signing operations will fail, but that's expected for local-only)
        let private_key = PrivateKey::from_bytes([0u8; 32]);

        Ok(Self {
            ops,
            settlement: None,
            network: None,
            config,
            private_key,
        })
    }

    /// Initialize node context with networking.
    ///
    /// Starts the network node. Use this for commands like `publish`, `query`, `start`.
    pub async fn with_network(config: CliConfig) -> CliResult<Self> {
        let base_dir = config.base_dir();

        // Open storage
        let state_config = NodeStateConfig::new(&base_dir);
        let state = NodeState::open(state_config)?;

        // Get peer ID (must exist)
        if !state.identity.exists() {
            return Err(CliError::IdentityNotInitialized);
        }
        let peer_id = state.identity.peer_id()?;

        // Load identity for network operations (requires password)
        let (private_key, public_key) = if config.network.enabled {
            let password = get_identity_password()?;
            state.identity.load(&password)?
        } else {
            // For local-only, we don't need the private key
            (
                PrivateKey::from_bytes([0u8; 32]),
                PublicKey::from_bytes([0u8; 32]),
            )
        };

        // Create network node
        let network = if config.network.enabled {
            let mut net_config = NetworkConfig::default();

            // Parse listen addresses
            for addr_str in &config.network.listen_addresses {
                if let Ok(addr) = addr_str.parse() {
                    net_config.listen_addresses.push(addr);
                }
            }

            // Parse bootstrap nodes
            // Format: /ip4/x.x.x.x/tcp/port/p2p/PeerId
            for bootstrap_str in &config.network.bootstrap_nodes {
                // Extract peer ID from the end of the multiaddr string
                if let Some(p2p_idx) = bootstrap_str.rfind("/p2p/") {
                    let peer_id_str = &bootstrap_str[p2p_idx + 5..];
                    let addr_str = &bootstrap_str[..p2p_idx];

                    // Parse peer ID
                    if let Ok(peer_id) = peer_id_str.parse::<nodalync_net::PeerId>() {
                        // Parse address
                        if let Ok(addr) = addr_str.parse::<nodalync_net::Multiaddr>() {
                            net_config.bootstrap_nodes.push((peer_id, addr.clone()));
                            tracing::info!("Added bootstrap node: {} at {}", peer_id, addr);
                        }
                    }
                }
            }

            // Convert nodalync keypair to libp2p keypair for consistent peer ID
            let libp2p_keypair = to_libp2p_keypair(&private_key)?;
            let node = NetworkNode::with_keypair(
                private_key.clone(),
                public_key,
                libp2p_keypair,
                net_config,
            )
            .await?;
            Some(Arc::new(node))
        } else {
            None
        };

        // Create settlement based on config (may fail if not configured)
        let settlement = match create_settlement(&config).await {
            Ok(s) => Some(s),
            Err(e) => {
                tracing::warn!("Failed to create settlement: {}", e);
                None
            }
        };

        // Create operations with network and/or settlement
        let mut ops = match (&network, &settlement) {
            (Some(net), Some(settle)) => {
                DefaultNodeOperations::with_defaults_network_and_settlement(
                    state,
                    peer_id,
                    Arc::clone(net) as Arc<dyn Network>,
                    Arc::clone(settle),
                )
            }
            (Some(net), None) => DefaultNodeOperations::with_defaults_and_network(
                state,
                peer_id,
                Arc::clone(net) as Arc<dyn Network>,
            ),
            (None, Some(settle)) => DefaultNodeOperations::with_defaults_and_settlement(
                state,
                peer_id,
                Arc::clone(settle),
            ),
            (None, None) => DefaultNodeOperations::with_defaults(state, peer_id),
        };

        // Set the private key for signing payments
        if config.network.enabled {
            ops.set_private_key(private_key.clone());
        }

        Ok(Self {
            ops,
            settlement,
            network,
            config,
            private_key,
        })
    }

    /// Initialize node context for the `init` command.
    ///
    /// Creates storage but does not require existing identity.
    pub fn for_init(config: CliConfig) -> CliResult<NodeState> {
        let base_dir = config.base_dir();

        // Create directories
        std::fs::create_dir_all(&base_dir)?;

        // Open storage
        let state_config = NodeStateConfig::new(&base_dir);
        let state = NodeState::open(state_config)?;

        Ok(state)
    }

    /// Get the peer ID.
    pub fn peer_id(&self) -> PeerId {
        self.ops.peer_id()
    }

    /// Bootstrap the network node.
    pub async fn bootstrap(&self) -> CliResult<()> {
        if let Some(ref network) = self.network {
            network.bootstrap().await?;
        }
        Ok(())
    }

    /// Get connected peer count.
    pub fn connected_peers(&self) -> usize {
        if let Some(ref network) = self.network {
            network.connected_peers().len()
        } else {
            0
        }
    }
}

/// Check if identity exists in the default location.
pub fn identity_exists(config: &CliConfig) -> bool {
    let base_dir = config.base_dir();
    let identity_dir = base_dir.join("identity");
    identity_dir.join("keypair.key").exists()
}

/// Parse a hash string to Hash type.
pub fn parse_hash(hash_str: &str) -> CliResult<nodalync_crypto::Hash> {
    // Hash is displayed as 64 hex chars
    if hash_str.len() != 64 {
        return Err(CliError::InvalidHash(hash_str.to_string()));
    }

    let mut bytes = [0u8; 32];
    for (i, chunk) in hash_str.as_bytes().chunks(2).enumerate() {
        if i >= 32 {
            return Err(CliError::InvalidHash(hash_str.to_string()));
        }
        let hex_str =
            std::str::from_utf8(chunk).map_err(|_| CliError::InvalidHash(hash_str.to_string()))?;
        bytes[i] = u8::from_str_radix(hex_str, 16)
            .map_err(|_| CliError::InvalidHash(hash_str.to_string()))?;
    }

    Ok(nodalync_crypto::Hash(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identity_exists() {
        let config = CliConfig::default();
        // In a fresh environment, identity shouldn't exist
        // This is just a smoke test
        let _ = identity_exists(&config);
    }

    #[test]
    fn test_parse_hash() {
        // Valid hash (base58 encoded)
        let result = parse_hash("invalid");
        assert!(result.is_err());
    }
}
