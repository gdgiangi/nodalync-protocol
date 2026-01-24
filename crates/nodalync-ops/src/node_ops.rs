//! Main NodeOperations implementation.
//!
//! This module provides the `NodeOperations` struct that implements
//! the `Operations` trait, orchestrating all protocol functionality.

use std::sync::Arc;

use nodalync_crypto::{PeerId, Timestamp};
use nodalync_net::Network;
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
}

impl<V, E> NodeOperations<V, E>
where
    V: Validator,
    E: L1Extractor,
{
    /// Create new NodeOperations with the given components (no network).
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
}

/// Default NodeOperations with DefaultValidator and RuleBasedExtractor.
pub type DefaultNodeOperations =
    NodeOperations<nodalync_valid::DefaultValidator, crate::extraction::RuleBasedExtractor>;

impl DefaultNodeOperations {
    /// Create a new NodeOperations with default validator and extractor (no network).
    pub fn with_defaults(state: NodeState, peer_id: PeerId) -> Self {
        Self::new(
            state,
            nodalync_valid::DefaultValidator::new(),
            crate::extraction::RuleBasedExtractor::new(),
            OpsConfig::default(),
            peer_id,
        )
    }

    /// Create with custom configuration (no network).
    pub fn with_config(state: NodeState, peer_id: PeerId, config: OpsConfig) -> Self {
        Self::new(
            state,
            nodalync_valid::DefaultValidator::new(),
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
        Self::with_network(
            state,
            nodalync_valid::DefaultValidator::new(),
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
        Self::with_network(
            state,
            nodalync_valid::DefaultValidator::new(),
            crate::extraction::RuleBasedExtractor::new(),
            config,
            peer_id,
            network,
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
}
