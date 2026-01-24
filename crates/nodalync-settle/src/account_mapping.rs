//! Account mapping for PeerId to Hedera AccountId.

use std::collections::HashMap;

use nodalync_crypto::PeerId;
use serde::{Deserialize, Serialize};

use crate::error::{SettleError, SettleResult};
use crate::types::AccountId;

/// Bidirectional mapping between PeerIds and Hedera AccountIds.
///
/// This mapping is essential for settlement because:
/// - Off-chain, we use PeerIds for peer identification
/// - On-chain, we need Hedera AccountIds for token transfers
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AccountMapper {
    /// PeerId -> AccountId mapping
    peer_to_account: HashMap<[u8; 20], AccountId>,
    /// AccountId -> PeerId mapping (for reverse lookups)
    account_to_peer: HashMap<AccountId, [u8; 20]>,
}

impl AccountMapper {
    /// Create a new empty account mapper.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a mapping between a PeerId and an AccountId.
    ///
    /// If either the PeerId or AccountId was previously mapped,
    /// the old mapping is replaced.
    pub fn register(&mut self, peer_id: &PeerId, account_id: AccountId) {
        // Remove any existing mappings
        if let Some(old_account) = self.peer_to_account.get(&peer_id.0) {
            self.account_to_peer.remove(old_account);
        }
        if let Some(old_peer) = self.account_to_peer.get(&account_id) {
            self.peer_to_account.remove(old_peer);
        }

        // Add new mapping
        self.peer_to_account.insert(peer_id.0, account_id);
        self.account_to_peer.insert(account_id, peer_id.0);
    }

    /// Get the AccountId for a PeerId.
    pub fn get_account(&self, peer_id: &PeerId) -> Option<AccountId> {
        self.peer_to_account.get(&peer_id.0).copied()
    }

    /// Get the AccountId for a PeerId, or return an error.
    pub fn require_account(&self, peer_id: &PeerId) -> SettleResult<AccountId> {
        self.get_account(peer_id)
            .ok_or_else(|| SettleError::account_not_found(format!("{}", peer_id)))
    }

    /// Get the PeerId for an AccountId.
    pub fn get_peer(&self, account_id: &AccountId) -> Option<PeerId> {
        self.account_to_peer
            .get(account_id)
            .map(|bytes| PeerId::from_bytes(*bytes))
    }

    /// Remove a mapping by PeerId.
    pub fn remove_by_peer(&mut self, peer_id: &PeerId) -> Option<AccountId> {
        if let Some(account_id) = self.peer_to_account.remove(&peer_id.0) {
            self.account_to_peer.remove(&account_id);
            Some(account_id)
        } else {
            None
        }
    }

    /// Remove a mapping by AccountId.
    pub fn remove_by_account(&mut self, account_id: &AccountId) -> Option<PeerId> {
        if let Some(peer_bytes) = self.account_to_peer.remove(account_id) {
            self.peer_to_account.remove(&peer_bytes);
            Some(PeerId::from_bytes(peer_bytes))
        } else {
            None
        }
    }

    /// Check if a PeerId has a registered account.
    pub fn has_account(&self, peer_id: &PeerId) -> bool {
        self.peer_to_account.contains_key(&peer_id.0)
    }

    /// Get the number of registered mappings.
    pub fn len(&self) -> usize {
        self.peer_to_account.len()
    }

    /// Check if the mapper is empty.
    pub fn is_empty(&self) -> bool {
        self.peer_to_account.is_empty()
    }

    /// Get all registered PeerIds.
    pub fn peer_ids(&self) -> impl Iterator<Item = PeerId> + '_ {
        self.peer_to_account
            .keys()
            .map(|bytes| PeerId::from_bytes(*bytes))
    }

    /// Get all registered AccountIds.
    pub fn account_ids(&self) -> impl Iterator<Item = &AccountId> + '_ {
        self.account_to_peer.keys()
    }

    /// Clear all mappings.
    pub fn clear(&mut self) {
        self.peer_to_account.clear();
        self.account_to_peer.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nodalync_crypto::{generate_identity, peer_id_from_public_key};

    fn test_peer_id() -> PeerId {
        let (_, public_key) = generate_identity();
        peer_id_from_public_key(&public_key)
    }

    #[test]
    fn test_register_and_get() {
        let mut mapper = AccountMapper::new();
        let peer = test_peer_id();
        let account = AccountId::simple(12345);

        mapper.register(&peer, account);

        assert_eq!(mapper.get_account(&peer), Some(account));
        assert_eq!(mapper.get_peer(&account), Some(peer));
    }

    #[test]
    fn test_require_account() {
        let mut mapper = AccountMapper::new();
        let peer = test_peer_id();
        let unregistered = test_peer_id();
        let account = AccountId::simple(12345);

        mapper.register(&peer, account);

        assert!(mapper.require_account(&peer).is_ok());
        assert!(mapper.require_account(&unregistered).is_err());
    }

    #[test]
    fn test_register_replaces_old_mapping() {
        let mut mapper = AccountMapper::new();
        let peer = test_peer_id();
        let account1 = AccountId::simple(11111);
        let account2 = AccountId::simple(22222);

        mapper.register(&peer, account1);
        mapper.register(&peer, account2);

        assert_eq!(mapper.get_account(&peer), Some(account2));
        assert!(mapper.get_peer(&account1).is_none());
    }

    #[test]
    fn test_remove_by_peer() {
        let mut mapper = AccountMapper::new();
        let peer = test_peer_id();
        let account = AccountId::simple(12345);

        mapper.register(&peer, account);
        let removed = mapper.remove_by_peer(&peer);

        assert_eq!(removed, Some(account));
        assert!(mapper.get_account(&peer).is_none());
        assert!(mapper.get_peer(&account).is_none());
    }

    #[test]
    fn test_remove_by_account() {
        let mut mapper = AccountMapper::new();
        let peer = test_peer_id();
        let account = AccountId::simple(12345);

        mapper.register(&peer, account);
        let removed = mapper.remove_by_account(&account);

        assert_eq!(removed, Some(peer));
        assert!(mapper.get_account(&peer).is_none());
        assert!(mapper.get_peer(&account).is_none());
    }

    #[test]
    fn test_has_account() {
        let mut mapper = AccountMapper::new();
        let peer = test_peer_id();
        let account = AccountId::simple(12345);

        assert!(!mapper.has_account(&peer));
        mapper.register(&peer, account);
        assert!(mapper.has_account(&peer));
    }

    #[test]
    fn test_len_and_is_empty() {
        let mut mapper = AccountMapper::new();
        assert!(mapper.is_empty());
        assert_eq!(mapper.len(), 0);

        let peer1 = test_peer_id();
        let peer2 = test_peer_id();

        mapper.register(&peer1, AccountId::simple(11111));
        assert_eq!(mapper.len(), 1);

        mapper.register(&peer2, AccountId::simple(22222));
        assert_eq!(mapper.len(), 2);
        assert!(!mapper.is_empty());
    }

    #[test]
    fn test_clear() {
        let mut mapper = AccountMapper::new();
        let peer = test_peer_id();

        mapper.register(&peer, AccountId::simple(12345));
        mapper.clear();

        assert!(mapper.is_empty());
    }

    #[test]
    fn test_iterators() {
        let mut mapper = AccountMapper::new();
        let peer1 = test_peer_id();
        let peer2 = test_peer_id();
        let account1 = AccountId::simple(11111);
        let account2 = AccountId::simple(22222);

        mapper.register(&peer1, account1);
        mapper.register(&peer2, account2);

        let peers: Vec<_> = mapper.peer_ids().collect();
        assert_eq!(peers.len(), 2);

        let accounts: Vec<_> = mapper.account_ids().collect();
        assert_eq!(accounts.len(), 2);
    }
}
