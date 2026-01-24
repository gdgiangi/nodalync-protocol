//! Node context for CLI operations.

use std::sync::Arc;

use nodalync_crypto::PeerId;
use nodalync_net::{Network, NetworkConfig, NetworkNode};
use nodalync_ops::{DefaultNodeOperations, OpsConfig};
use nodalync_settle::{AccountId, MockSettlement, Settlement};
use nodalync_store::{NodeState, NodeStateConfig};

use crate::config::CliConfig;
use crate::error::{CliError, CliResult};

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

        // Create network node
        let network = if config.network.enabled {
            let mut net_config = NetworkConfig::default();

            // Parse listen addresses
            for addr_str in &config.network.listen_addresses {
                if let Ok(addr) = addr_str.parse() {
                    net_config.listen_addresses.push(addr);
                }
            }

            let node = NetworkNode::new(net_config).await?;
            Some(Arc::new(node))
        } else {
            None
        };

        // Create settlement
        let settlement: Arc<dyn Settlement> =
            Arc::new(MockSettlement::with_balance(AccountId::simple(1), 1_000_000_000));

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
        let hex_str = std::str::from_utf8(chunk)
            .map_err(|_| CliError::InvalidHash(hash_str.to_string()))?;
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
