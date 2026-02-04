//! Peer store-backed public key lookup.
//!
//! Bridges `SqlitePeerStore` with the `PublicKeyLookup` trait from `nodalync-valid`,
//! enabling signature verification against keys stored in the peer database.

use nodalync_crypto::{PeerId, PublicKey};
use nodalync_store::{NodeState, SqlitePeerStore};
use nodalync_valid::PublicKeyLookup;

/// Public key lookup backed by the SQLite peer store.
///
/// Wraps a `SqlitePeerStore` to implement `PublicKeyLookup` for use in
/// validators and signature verification. Returns `None` for unknown peers
/// or peers with all-zero (unset) public keys.
pub struct PeerStoreKeyLookup {
    peers: SqlitePeerStore,
}

impl PeerStoreKeyLookup {
    /// Create a new lookup from a `NodeState`'s shared database connection.
    pub fn from_state(state: &NodeState) -> Self {
        Self {
            peers: SqlitePeerStore::new(state.connection()),
        }
    }
}

impl PublicKeyLookup for PeerStoreKeyLookup {
    fn lookup(&self, peer_id: &PeerId) -> Option<PublicKey> {
        use nodalync_store::PeerStore;

        self.peers
            .get(peer_id)
            .ok()
            .flatten()
            .map(|info| info.public_key)
            .filter(|pk| pk.0 != [0u8; 32])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nodalync_crypto::{generate_identity, peer_id_from_public_key};
    use nodalync_store::{NodeStateConfig, PeerInfo, PeerStore};
    use tempfile::TempDir;

    fn setup() -> (PeerStoreKeyLookup, NodeState, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let config = NodeStateConfig::new(temp_dir.path());
        let state = NodeState::open(config).unwrap();
        let lookup = PeerStoreKeyLookup::from_state(&state);
        (lookup, state, temp_dir)
    }

    #[test]
    fn test_lookup_returns_none_for_unknown_peer() {
        let (lookup, _state, _temp) = setup();
        let (_, public_key) = generate_identity();
        let peer_id = peer_id_from_public_key(&public_key);

        assert!(lookup.lookup(&peer_id).is_none());
    }

    #[test]
    fn test_lookup_returns_pubkey_after_registration() {
        let (lookup, mut state, _temp) = setup();
        let (_, public_key) = generate_identity();
        let peer_id = peer_id_from_public_key(&public_key);

        // Register peer
        let peer_info = PeerInfo::new(peer_id, public_key, vec![], 1000);
        state.peers.upsert(&peer_info).unwrap();

        // Lookup should return the public key
        let result = lookup.lookup(&peer_id);
        assert!(result.is_some());
        assert_eq!(result.unwrap(), public_key);
    }

    #[test]
    fn test_lookup_returns_none_for_zero_pubkey() {
        let (lookup, mut state, _temp) = setup();
        let (_, public_key) = generate_identity();
        let peer_id = peer_id_from_public_key(&public_key);

        // Register peer with all-zero public key (unset)
        let zero_key = PublicKey::from_bytes([0u8; 32]);
        let peer_info = PeerInfo::new(peer_id, zero_key, vec![], 1000);
        state.peers.upsert(&peer_info).unwrap();

        // Lookup should return None for zero key
        assert!(lookup.lookup(&peer_id).is_none());
    }

    #[test]
    fn test_implements_public_key_lookup_trait() {
        // Compile-time verification that PeerStoreKeyLookup implements PublicKeyLookup
        fn assert_impl<T: PublicKeyLookup>(_: &T) {}
        let (lookup, _state, _temp) = setup();
        assert_impl(&lookup);
    }
}
