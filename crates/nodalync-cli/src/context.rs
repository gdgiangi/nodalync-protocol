//! Node context for CLI operations.

use std::sync::Arc;

use nodalync_crypto::{PeerId, PrivateKey, PublicKey};
use nodalync_net::{Network, NetworkConfig, NetworkNode};
use nodalync_ops::{DefaultNodeOperations, OpsConfig};
use nodalync_settle::{AccountId, MockSettlement, Settlement};
use nodalync_store::{NodeState, NodeStateConfig};

use crate::config::CliConfig;
use crate::error::{CliError, CliResult};

/// Get password from environment variable or interactive prompt.
fn get_password() -> CliResult<String> {
    if let Ok(pwd) = std::env::var("NODALYNC_PASSWORD") {
        Ok(pwd)
    } else if crate::prompt::is_interactive() {
        crate::prompt::password("Enter password to unlock identity")
            .map_err(|e| CliError::User(format!("Failed to read password: {}", e)))
    } else {
        Err(CliError::User(
            "Set NODALYNC_PASSWORD or run interactively".into(),
        ))
    }
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

/// Node context containing all initialized components.
pub struct NodeContext {
    /// Operations interface.
    pub ops: DefaultNodeOperations,
    /// Settlement interface.
    pub settlement: Arc<dyn Settlement>,
    /// Network node (optional, for network operations).
    pub network: Option<Arc<NetworkNode>>,
    /// Configuration.
    pub config: CliConfig,
}

impl NodeContext {
    /// Initialize node context for local-only operations.
    ///
    /// Does not start networking. Use this for commands like `list`, `versions`.
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

        // Create settlement (mock for now)
        let settlement: Arc<dyn Settlement> =
            Arc::new(MockSettlement::with_balance(AccountId::simple(1), 0));

        // Create operations without network
        let ops = DefaultNodeOperations::with_defaults(state, peer_id);

        Ok(Self {
            ops,
            settlement,
            network: None,
            config,
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
            let password = get_password()?;
            state.identity.load(&password)?
        } else {
            // For local-only, we don't need the private key
            (PrivateKey::from_bytes([0u8; 32]), PublicKey::from_bytes([0u8; 32]))
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
            let node = NetworkNode::with_keypair(private_key.clone(), public_key, libp2p_keypair, net_config).await?;
            Some(Arc::new(node))
        } else {
            None
        };

        // Create settlement
        let settlement: Arc<dyn Settlement> = Arc::new(MockSettlement::with_balance(
            AccountId::simple(1),
            1_000_000_000,
        ));

        // Create operations with optional network
        let ops = if let Some(ref net) = network {
            DefaultNodeOperations::with_network(
                state,
                nodalync_valid::DefaultValidator::new(),
                nodalync_ops::RuleBasedExtractor::new(),
                OpsConfig::default(),
                peer_id,
                Arc::clone(net) as Arc<dyn Network>,
            )
        } else {
            DefaultNodeOperations::with_defaults(state, peer_id)
        };

        Ok(Self {
            ops,
            settlement,
            network,
            config,
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
