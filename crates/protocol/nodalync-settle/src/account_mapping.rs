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
    /// AccountId -> EVM address cache (40-char hex, no 0x prefix)
    #[serde(default)]
    account_to_evm: HashMap<AccountId, String>,
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

    /// Get the cached EVM address for an AccountId.
    pub fn get_evm_address(&self, account: &AccountId) -> Option<&str> {
        self.account_to_evm.get(account).map(|s| s.as_str())
    }

    /// Cache an EVM address for an AccountId.
    pub fn set_evm_address(&mut self, account: AccountId, evm_address: String) {
        self.account_to_evm.insert(account, evm_address);
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
            self.account_to_evm.remove(&account_id);
            Some(account_id)
        } else {
            None
        }
    }

    /// Remove a mapping by AccountId.
    pub fn remove_by_account(&mut self, account_id: &AccountId) -> Option<PeerId> {
        if let Some(peer_bytes) = self.account_to_peer.remove(account_id) {
            self.peer_to_account.remove(&peer_bytes);
            self.account_to_evm.remove(account_id);
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
        self.account_to_evm.clear();
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

    #[test]
    fn test_mapping_overwrite() {
        let mut mapper = AccountMapper::new();
        let peer = test_peer_id();
        let account_old = AccountId::simple(11111);
        let account_new = AccountId::simple(22222);

        mapper.register(&peer, account_old);
        assert_eq!(mapper.get_account(&peer), Some(account_old));
        assert_eq!(mapper.get_peer(&account_old), Some(peer));

        // Overwrite with a new account
        mapper.register(&peer, account_new);

        // New mapping should be active
        assert_eq!(mapper.get_account(&peer), Some(account_new));
        assert_eq!(mapper.get_peer(&account_new), Some(peer));

        // Old account should no longer resolve to this peer
        assert!(mapper.get_peer(&account_old).is_none());

        // Total mapping count stays at 1
        assert_eq!(mapper.len(), 1);
    }

    #[test]
    fn test_mapping_concurrent_access() {
        let mut mapper = AccountMapper::new();

        let peer1 = test_peer_id();
        let peer2 = test_peer_id();
        let peer3 = test_peer_id();
        let account1 = AccountId::simple(10001);
        let account2 = AccountId::simple(10002);
        let account3 = AccountId::simple(10003);

        mapper.register(&peer1, account1);
        mapper.register(&peer2, account2);
        mapper.register(&peer3, account3);

        assert_eq!(mapper.len(), 3);

        // All mappings should be independently accessible
        assert_eq!(mapper.get_account(&peer1), Some(account1));
        assert_eq!(mapper.get_account(&peer2), Some(account2));
        assert_eq!(mapper.get_account(&peer3), Some(account3));

        // Reverse lookups all work
        assert_eq!(mapper.get_peer(&account1), Some(peer1));
        assert_eq!(mapper.get_peer(&account2), Some(peer2));
        assert_eq!(mapper.get_peer(&account3), Some(peer3));

        // Removing one does not affect the others
        mapper.remove_by_peer(&peer2);
        assert_eq!(mapper.len(), 2);
        assert!(mapper.get_account(&peer2).is_none());
        assert_eq!(mapper.get_account(&peer1), Some(account1));
        assert_eq!(mapper.get_account(&peer3), Some(account3));
    }

    #[test]
    fn test_evm_address_cache() {
        let mut mapper = AccountMapper::new();
        let account = AccountId::simple(12345);

        assert!(mapper.get_evm_address(&account).is_none());

        mapper.set_evm_address(
            account,
            "89cd3ec89e55e1211b2d1969ab9d73c06ce85e5d".to_string(),
        );

        assert_eq!(
            mapper.get_evm_address(&account),
            Some("89cd3ec89e55e1211b2d1969ab9d73c06ce85e5d")
        );
    }

    #[test]
    fn test_evm_address_cleared_on_remove_by_peer() {
        let mut mapper = AccountMapper::new();
        let peer = test_peer_id();
        let account = AccountId::simple(12345);

        mapper.register(&peer, account);
        mapper.set_evm_address(
            account,
            "abcdef1234567890abcdef1234567890abcdef12".to_string(),
        );

        mapper.remove_by_peer(&peer);

        assert!(mapper.get_evm_address(&account).is_none());
    }

    #[test]
    fn test_evm_address_cleared_on_remove_by_account() {
        let mut mapper = AccountMapper::new();
        let peer = test_peer_id();
        let account = AccountId::simple(12345);

        mapper.register(&peer, account);
        mapper.set_evm_address(
            account,
            "abcdef1234567890abcdef1234567890abcdef12".to_string(),
        );

        mapper.remove_by_account(&account);

        assert!(mapper.get_evm_address(&account).is_none());
    }

    #[test]
    fn test_evm_address_cleared_on_clear() {
        let mut mapper = AccountMapper::new();
        let account = AccountId::simple(12345);

        mapper.set_evm_address(
            account,
            "abcdef1234567890abcdef1234567890abcdef12".to_string(),
        );

        mapper.clear();

        assert!(mapper.get_evm_address(&account).is_none());
    }
}
