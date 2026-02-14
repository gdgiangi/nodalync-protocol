//! Main NodeOperations implementation.
//!
//! This module provides the `NodeOperations` struct that implements
//! the `Operations` trait, orchestrating all protocol functionality.

use std::sync::Arc;

use nodalync_crypto::{PeerId, PrivateKey, Timestamp};
use nodalync_net::Network;
use nodalync_settle::Settlement;
use nodalync_store::NodeState;
use nodalync_valid::Validator;

use crate::config::OpsConfig;
use crate::extraction::L1Extractor;

/// Main operations implementation.
///
/// `NodeOperations` is the primary implementation of the `Operations` trait.
/// It is generic over:
/// - `V`: The validator implementation
/// - `E`: The L1 extractor implementation
///
/// This allows for different validation and extraction strategies while
/// maintaining the same core operation logic.
///
/// When `network` is `Some`, operations will use P2P networking for DHT lookups,
/// content queries, and channel messaging. When `None`, operations fall back to
/// local-only mode (useful for testing or offline operation).
pub struct NodeOperations<V, E>
where
    V: Validator,
    E: L1Extractor,
{
    /// Node state containing all storage components.
    pub state: NodeState,
    /// Validator for content, provenance, and payment validation.
    pub validator: V,
    /// L1 extractor for mention extraction.
    pub extractor: E,
    /// Operations configuration.
    pub config: OpsConfig,
    /// This node's peer ID.
    peer_id: PeerId,
    /// Optional network for P2P operations.
    ///
    /// When `Some`, enables DHT announce/lookup, network queries, and channel messaging.
    /// When `None`, operations are local-only.
    network: Option<Arc<dyn Network>>,
    /// Optional settlement for on-chain payment operations.
    ///
    /// When `Some`, enables Hedera settlement for payment batches.
    /// When `None`, settlement batches are only processed locally.
    settlement: Option<Arc<dyn Settlement>>,
    /// Optional private key for signing payments and channel operations.
    ///
    /// Required for paid queries - without this, only free content can be queried.
    private_key: Option<PrivateKey>,
    /// Timestamp of the last auto-deposit for rate limiting.
    ///
    /// Used to prevent rapid deposits from malicious channel open spam.
    /// This is a global cooldown (not per-peer) for simplicity.
    last_auto_deposit: Option<std::time::Instant>,
    /// Node start time for uptime tracking.
    node_start: std::time::Instant,
}

impl<V, E> NodeOperations<V, E>
where
    V: Validator,
    E: L1Extractor,
{
    /// Create new NodeOperations with the given components (no network, no settlement).
    pub fn new(
        state: NodeState,
        validator: V,
        extractor: E,
        config: OpsConfig,
        peer_id: PeerId,
    ) -> Self {
        Self {
            state,
            validator,
            extractor,
            config,
            peer_id,
            network: None,
            settlement: None,
            private_key: None,
            last_auto_deposit: None,
            node_start: std::time::Instant::now(),
        }
    }

    /// Create new NodeOperations with a network for P2P operations.
    pub fn with_network(
        state: NodeState,
        validator: V,
        extractor: E,
        config: OpsConfig,
        peer_id: PeerId,
        network: Arc<dyn Network>,
    ) -> Self {
        Self {
            state,
            validator,
            extractor,
            config,
            peer_id,
            network: Some(network),
            settlement: None,
            private_key: None,
            last_auto_deposit: None,
            node_start: std::time::Instant::now(),
        }
    }

    /// Create new NodeOperations with a settlement for on-chain operations.
    pub fn with_settlement(
        state: NodeState,
        validator: V,
        extractor: E,
        config: OpsConfig,
        peer_id: PeerId,
        settlement: Arc<dyn Settlement>,
    ) -> Self {
        Self {
            state,
            validator,
            extractor,
            config,
            peer_id,
            network: None,
            settlement: Some(settlement),
            private_key: None,
            last_auto_deposit: None,
            node_start: std::time::Instant::now(),
        }
    }

    /// Create new NodeOperations with both network and settlement.
    pub fn with_network_and_settlement(
        state: NodeState,
        validator: V,
        extractor: E,
        config: OpsConfig,
        peer_id: PeerId,
        network: Arc<dyn Network>,
        settlement: Arc<dyn Settlement>,
    ) -> Self {
        Self {
            state,
            validator,
            extractor,
            config,
            peer_id,
            network: Some(network),
            settlement: Some(settlement),
            private_key: None,
            last_auto_deposit: None,
            node_start: std::time::Instant::now(),
        }
    }

    /// Get the node's peer ID.
    pub fn peer_id(&self) -> PeerId {
        self.peer_id
    }

    /// Get the operations configuration.
    pub fn config(&self) -> &OpsConfig {
        &self.config
    }

    /// Get a reference to the node state.
    pub fn state(&self) -> &NodeState {
        &self.state
    }

    /// Get a mutable reference to the node state.
    pub fn state_mut(&mut self) -> &mut NodeState {
        &mut self.state
    }

    /// Get a reference to the network (if available).
    pub fn network(&self) -> Option<&Arc<dyn Network>> {
        self.network.as_ref()
    }

    /// Check if network is available.
    pub fn has_network(&self) -> bool {
        self.network.is_some()
    }

    /// Set the network for P2P operations.
    pub fn set_network(&mut self, network: Arc<dyn Network>) {
        self.network = Some(network);
    }

    /// Remove the network (switch to local-only mode).
    pub fn clear_network(&mut self) {
        self.network = None;
    }

    /// Get a reference to the settlement (if available).
    pub fn settlement(&self) -> Option<&Arc<dyn Settlement>> {
        self.settlement.as_ref()
    }

    /// Check if settlement is available.
    pub fn has_settlement(&self) -> bool {
        self.settlement.is_some()
    }

    /// Set the settlement for on-chain operations.
    pub fn set_settlement(&mut self, settlement: Arc<dyn Settlement>) {
        self.settlement = Some(settlement);
    }

    /// Remove the settlement (switch to local-only mode).
    pub fn clear_settlement(&mut self) {
        self.settlement = None;
    }

    /// Get a reference to the private key (if available).
    pub fn private_key(&self) -> Option<&PrivateKey> {
        self.private_key.as_ref()
    }

    /// Check if private key is available.
    pub fn has_private_key(&self) -> bool {
        self.private_key.is_some()
    }

    /// Set the private key for signing payments.
    pub fn set_private_key(&mut self, private_key: PrivateKey) {
        self.private_key = Some(private_key);
    }

    /// Remove the private key.
    pub fn clear_private_key(&mut self) {
        self.private_key = None;
    }

    /// Mark that an auto-deposit was just performed.
    ///
    /// This sets the cooldown timestamp to prevent rapid deposits.
    pub fn mark_auto_deposit(&mut self) {
        self.last_auto_deposit = Some(std::time::Instant::now());
    }

    /// Get the timestamp of the last auto-deposit.
    pub fn last_auto_deposit(&self) -> Option<std::time::Instant> {
        self.last_auto_deposit
    }

    /// Get node uptime in seconds.
    pub fn uptime_seconds(&self) -> u64 {
        self.node_start.elapsed().as_secs()
    }

    /// Check if an auto-deposit is allowed based on the cooldown.
    ///
    /// Returns true if no deposit has been made, or if the cooldown has elapsed.
    pub fn can_auto_deposit(&self) -> bool {
        match self.last_auto_deposit {
            None => true,
            Some(last) => {
                let cooldown =
                    std::time::Duration::from_secs(self.config.channel.auto_deposit_cooldown_secs);
                last.elapsed() >= cooldown
            }
        }
    }
}

/// Default NodeOperations with DefaultValidator (using PeerStoreKeyLookup) and RuleBasedExtractor.
pub type DefaultNodeOperations = NodeOperations<
    nodalync_valid::DefaultValidator<
        crate::peer_key_lookup::PeerStoreKeyLookup,
        nodalync_valid::NoopBondChecker,
    >,
    crate::extraction::RuleBasedExtractor,
>;

/// Helper to create a validator with PeerStoreKeyLookup from a NodeState.
fn create_default_validator(
    state: &NodeState,
) -> nodalync_valid::DefaultValidator<
    crate::peer_key_lookup::PeerStoreKeyLookup,
    nodalync_valid::NoopBondChecker,
> {
    let key_lookup = crate::peer_key_lookup::PeerStoreKeyLookup::from_state(state);
    nodalync_valid::DefaultValidator::with_dependencies(
        nodalync_valid::ValidatorConfig::default(),
        key_lookup,
        nodalync_valid::NoopBondChecker,
    )
}

impl DefaultNodeOperations {
    /// Create a new NodeOperations with default validator and extractor (no network).
    pub fn with_defaults(state: NodeState, peer_id: PeerId) -> Self {
        let validator = create_default_validator(&state);
        Self::new(
            state,
            validator,
            crate::extraction::RuleBasedExtractor::new(),
            OpsConfig::default(),
            peer_id,
        )
    }

    /// Create with custom configuration (no network).
    pub fn with_config(state: NodeState, peer_id: PeerId, config: OpsConfig) -> Self {
        let validator = create_default_validator(&state);
        Self::new(
            state,
            validator,
            crate::extraction::RuleBasedExtractor::new(),
            config,
            peer_id,
        )
    }

    /// Create with default validator/extractor and a network.
    pub fn with_defaults_and_network(
        state: NodeState,
        peer_id: PeerId,
        network: Arc<dyn Network>,
    ) -> Self {
        let validator = create_default_validator(&state);
        Self::with_network(
            state,
            validator,
            crate::extraction::RuleBasedExtractor::new(),
            OpsConfig::default(),
            peer_id,
            network,
        )
    }

    /// Create with custom configuration and a network.
    pub fn with_config_and_network(
        state: NodeState,
        peer_id: PeerId,
        config: OpsConfig,
        network: Arc<dyn Network>,
    ) -> Self {
        let validator = create_default_validator(&state);
        Self::with_network(
            state,
            validator,
            crate::extraction::RuleBasedExtractor::new(),
            config,
            peer_id,
            network,
        )
    }

    /// Create with default validator/extractor and a settlement.
    pub fn with_defaults_and_settlement(
        state: NodeState,
        peer_id: PeerId,
        settlement: Arc<dyn Settlement>,
    ) -> Self {
        let validator = create_default_validator(&state);
        Self::with_settlement(
            state,
            validator,
            crate::extraction::RuleBasedExtractor::new(),
            OpsConfig::default(),
            peer_id,
            settlement,
        )
    }

    /// Create with custom configuration and a settlement.
    pub fn with_config_and_settlement(
        state: NodeState,
        peer_id: PeerId,
        config: OpsConfig,
        settlement: Arc<dyn Settlement>,
    ) -> Self {
        let validator = create_default_validator(&state);
        Self::with_settlement(
            state,
            validator,
            crate::extraction::RuleBasedExtractor::new(),
            config,
            peer_id,
            settlement,
        )
    }

    /// Create with default validator/extractor, network, and settlement.
    pub fn with_defaults_network_and_settlement(
        state: NodeState,
        peer_id: PeerId,
        network: Arc<dyn Network>,
        settlement: Arc<dyn Settlement>,
    ) -> Self {
        let validator = create_default_validator(&state);
        Self::with_network_and_settlement(
            state,
            validator,
            crate::extraction::RuleBasedExtractor::new(),
            OpsConfig::default(),
            peer_id,
            network,
            settlement,
        )
    }

    /// Create with custom configuration, network, and settlement.
    pub fn with_config_network_and_settlement(
        state: NodeState,
        peer_id: PeerId,
        config: OpsConfig,
        network: Arc<dyn Network>,
        settlement: Arc<dyn Settlement>,
    ) -> Self {
        let validator = create_default_validator(&state);
        Self::with_network_and_settlement(
            state,
            validator,
            crate::extraction::RuleBasedExtractor::new(),
            config,
            peer_id,
            network,
            settlement,
        )
    }
}

/// Get current timestamp in milliseconds since Unix epoch.
pub fn current_timestamp() -> Timestamp {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("Time went backwards")
        .as_millis() as Timestamp
}

#[cfg(test)]
mod tests {
    use super::*;
    use nodalync_crypto::{generate_identity, peer_id_from_public_key};
    use nodalync_store::NodeStateConfig;
    use tempfile::TempDir;

    fn create_test_node_ops() -> (DefaultNodeOperations, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let config = NodeStateConfig::new(temp_dir.path());
        let state = NodeState::open(config).unwrap();

        let (_, public_key) = generate_identity();
        let peer_id = peer_id_from_public_key(&public_key);

        let ops = DefaultNodeOperations::with_defaults(state, peer_id);
        (ops, temp_dir)
    }

    #[test]
    fn test_create_node_ops() {
        let (ops, _temp) = create_test_node_ops();
        assert_eq!(ops.config().max_preview_mentions, 5);
    }

    #[test]
    fn test_peer_id() {
        let (ops, _temp) = create_test_node_ops();
        let peer_id = ops.peer_id();
        assert!(peer_id.0.iter().any(|&b| b != 0)); // Non-zero peer ID
    }

    #[test]
    fn test_current_timestamp() {
        let ts = current_timestamp();
        // Should be a reasonable timestamp (after 2020)
        assert!(ts > 1577836800000); // Jan 1, 2020
    }

    #[test]
    fn test_set_clear_network() {
        use nodalync_test_utils::MockNetwork;

        let (mut ops, _temp) = create_test_node_ops();

        // Initially no network
        assert!(!ops.has_network());

        // Set network
        let mock_net = MockNetwork::new();
        ops.set_network(Arc::new(mock_net));
        assert!(ops.has_network());
        assert!(ops.network().is_some());

        // Clear network
        ops.clear_network();
        assert!(!ops.has_network());
        assert!(ops.network().is_none());
    }

    #[test]
    fn test_set_clear_settlement() {
        use nodalync_test_utils::MockSettlement;

        let (mut ops, _temp) = create_test_node_ops();

        // Initially no settlement
        assert!(!ops.has_settlement());

        // Set settlement
        let mock_settle = MockSettlement::new();
        ops.set_settlement(Arc::new(mock_settle));
        assert!(ops.has_settlement());
        assert!(ops.settlement().is_some());

        // Clear settlement
        ops.clear_settlement();
        assert!(!ops.has_settlement());
        assert!(ops.settlement().is_none());
    }

    #[test]
    fn test_set_clear_private_key() {
        let (mut ops, _temp) = create_test_node_ops();

        // Initially no private key
        assert!(!ops.has_private_key());
        assert!(ops.private_key().is_none());

        // Set private key
        let (private_key, _) = generate_identity();
        ops.set_private_key(private_key);
        assert!(ops.has_private_key());
        assert!(ops.private_key().is_some());

        // Clear private key
        ops.clear_private_key();
        assert!(!ops.has_private_key());
        assert!(ops.private_key().is_none());
    }

    #[test]
    fn test_has_network_and_settlement() {
        use nodalync_test_utils::{MockNetwork, MockSettlement};

        let (mut ops, _temp) = create_test_node_ops();

        // Initially neither
        assert!(!ops.has_network());
        assert!(!ops.has_settlement());

        // Set both
        ops.set_network(Arc::new(MockNetwork::new()));
        ops.set_settlement(Arc::new(MockSettlement::new()));
        assert!(ops.has_network());
        assert!(ops.has_settlement());

        // Clear one, verify the other persists
        ops.clear_network();
        assert!(!ops.has_network());
        assert!(ops.has_settlement());
    }
}
