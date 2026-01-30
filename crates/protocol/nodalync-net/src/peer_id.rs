//! Peer ID mapping between libp2p and Nodalync.
//!
//! This module provides bidirectional mapping between libp2p PeerIds
//! and Nodalync PeerIds. The mapping is populated when peers exchange
//! PeerInfoPayload messages.

use nodalync_crypto::{PeerId as NodalyncPeerId, PublicKey};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Bidirectional mapper between libp2p PeerId and Nodalync PeerId.
///
/// The mapping is established when peers exchange PeerInfoPayload messages,
/// which contain their Nodalync public key. This allows us to:
/// 1. Look up a Nodalync PeerId given a libp2p PeerId
/// 2. Look up a libp2p PeerId given a Nodalync PeerId
#[derive(Debug, Clone)]
pub struct PeerIdMapper {
    /// libp2p PeerId -> Nodalync PeerId
    to_nodalync: Arc<RwLock<HashMap<libp2p::PeerId, NodalyncPeerId>>>,
    /// Nodalync PeerId -> libp2p PeerId
    to_libp2p: Arc<RwLock<HashMap<NodalyncPeerId, libp2p::PeerId>>>,
    /// libp2p PeerId -> Nodalync PublicKey (for signature verification)
    public_keys: Arc<RwLock<HashMap<libp2p::PeerId, PublicKey>>>,
}

impl Default for PeerIdMapper {
    fn default() -> Self {
        Self::new()
    }
}

impl PeerIdMapper {
    /// Create a new empty mapper.
    pub fn new() -> Self {
        Self {
            to_nodalync: Arc::new(RwLock::new(HashMap::new())),
            to_libp2p: Arc::new(RwLock::new(HashMap::new())),
            public_keys: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a mapping between libp2p and Nodalync peer IDs.
    ///
    /// This should be called when receiving a PeerInfoPayload from a peer.
    pub fn register(
        &self,
        libp2p_peer: libp2p::PeerId,
        nodalync_peer: NodalyncPeerId,
        public_key: PublicKey,
    ) {
        if let Ok(mut to_nodalync) = self.to_nodalync.write() {
            to_nodalync.insert(libp2p_peer, nodalync_peer);
        }
        if let Ok(mut to_libp2p) = self.to_libp2p.write() {
            to_libp2p.insert(nodalync_peer, libp2p_peer);
        }
        if let Ok(mut public_keys) = self.public_keys.write() {
            public_keys.insert(libp2p_peer, public_key);
        }
    }

    /// Remove a peer mapping.
    ///
    /// This should be called when a peer disconnects.
    pub fn unregister(&self, libp2p_peer: &libp2p::PeerId) {
        let nodalync_peer = self.to_nodalync(libp2p_peer);

        if let Ok(mut to_nodalync) = self.to_nodalync.write() {
            to_nodalync.remove(libp2p_peer);
        }
        if let Some(nodalync) = nodalync_peer {
            if let Ok(mut to_libp2p) = self.to_libp2p.write() {
                to_libp2p.remove(&nodalync);
            }
        }
        if let Ok(mut public_keys) = self.public_keys.write() {
            public_keys.remove(libp2p_peer);
        }
    }

    /// Get the Nodalync PeerId for a libp2p PeerId.
    pub fn to_nodalync(&self, libp2p_peer: &libp2p::PeerId) -> Option<NodalyncPeerId> {
        self.to_nodalync
            .read()
            .ok()
            .and_then(|map| map.get(libp2p_peer).copied())
    }

    /// Get the libp2p PeerId for a Nodalync PeerId.
    pub fn to_libp2p(&self, nodalync_peer: &NodalyncPeerId) -> Option<libp2p::PeerId> {
        self.to_libp2p
            .read()
            .ok()
            .and_then(|map| map.get(nodalync_peer).copied())
    }

    /// Get the public key for a libp2p PeerId.
    pub fn public_key(&self, libp2p_peer: &libp2p::PeerId) -> Option<PublicKey> {
        self.public_keys
            .read()
            .ok()
            .and_then(|map| map.get(libp2p_peer).copied())
    }

    /// Check if a libp2p peer is registered.
    pub fn is_registered(&self, libp2p_peer: &libp2p::PeerId) -> bool {
        self.to_nodalync
            .read()
            .map(|map| map.contains_key(libp2p_peer))
            .unwrap_or(false)
    }

    /// Get the number of registered peers.
    pub fn len(&self) -> usize {
        self.to_nodalync.read().map(|map| map.len()).unwrap_or(0)
    }

    /// Check if there are no registered peers.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get all registered libp2p peer IDs.
    pub fn libp2p_peers(&self) -> Vec<libp2p::PeerId> {
        self.to_nodalync
            .read()
            .map(|map| map.keys().copied().collect())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nodalync_crypto::{generate_identity, peer_id_from_public_key};

    #[test]
    fn test_register_and_lookup() {
        let mapper = PeerIdMapper::new();

        let libp2p_peer = libp2p::PeerId::random();
        let (_, public_key) = generate_identity();
        let nodalync_peer = peer_id_from_public_key(&public_key);

        mapper.register(libp2p_peer, nodalync_peer, public_key);

        assert!(mapper.is_registered(&libp2p_peer));
        assert_eq!(mapper.to_nodalync(&libp2p_peer), Some(nodalync_peer));
        assert_eq!(mapper.to_libp2p(&nodalync_peer), Some(libp2p_peer));
        assert_eq!(mapper.public_key(&libp2p_peer), Some(public_key));
    }

    #[test]
    fn test_unregister() {
        let mapper = PeerIdMapper::new();

        let libp2p_peer = libp2p::PeerId::random();
        let (_, public_key) = generate_identity();
        let nodalync_peer = peer_id_from_public_key(&public_key);

        mapper.register(libp2p_peer, nodalync_peer, public_key);
        assert!(mapper.is_registered(&libp2p_peer));

        mapper.unregister(&libp2p_peer);
        assert!(!mapper.is_registered(&libp2p_peer));
        assert!(mapper.to_nodalync(&libp2p_peer).is_none());
        assert!(mapper.to_libp2p(&nodalync_peer).is_none());
    }

    #[test]
    fn test_unknown_peer() {
        let mapper = PeerIdMapper::new();
        let unknown_peer = libp2p::PeerId::random();

        assert!(!mapper.is_registered(&unknown_peer));
        assert!(mapper.to_nodalync(&unknown_peer).is_none());
    }

    #[test]
    fn test_len_and_is_empty() {
        let mapper = PeerIdMapper::new();
        assert!(mapper.is_empty());
        assert_eq!(mapper.len(), 0);

        let libp2p_peer = libp2p::PeerId::random();
        let (_, public_key) = generate_identity();
        let nodalync_peer = peer_id_from_public_key(&public_key);

        mapper.register(libp2p_peer, nodalync_peer, public_key);
        assert!(!mapper.is_empty());
        assert_eq!(mapper.len(), 1);
    }

    #[test]
    fn test_clone() {
        let mapper = PeerIdMapper::new();

        let libp2p_peer = libp2p::PeerId::random();
        let (_, public_key) = generate_identity();
        let nodalync_peer = peer_id_from_public_key(&public_key);

        mapper.register(libp2p_peer, nodalync_peer, public_key);

        // Clone shares the same underlying data
        let mapper2 = mapper.clone();
        assert!(mapper2.is_registered(&libp2p_peer));
    }
}
