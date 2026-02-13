//! Persistent peer store for Nodalync Studio.
//!
//! Saves known peers to disk so the node can reconnect on restart.
//! Peers are stored as a JSON file in the node's data directory.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{info, warn};

/// A known peer that we've successfully connected to.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnownPeer {
    /// libp2p peer ID as a string.
    pub peer_id: String,
    /// Multiaddresses we've seen this peer at.
    pub addresses: Vec<String>,
    /// Optional Nodalync peer ID (if we've learned it).
    pub nodalync_id: Option<String>,
    /// Last time we successfully connected.
    pub last_seen: DateTime<Utc>,
    /// How many times we've successfully connected.
    pub connection_count: u32,
    /// Whether this peer was manually added (vs. auto-discovered).
    pub manual: bool,
}

/// Persistent store for known peers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerStore {
    /// Known peers indexed by libp2p peer ID.
    pub peers: HashMap<String, KnownPeer>,
    /// Last time the store was updated.
    pub updated_at: DateTime<Utc>,
}

impl PeerStore {
    /// Create a new empty peer store.
    pub fn new() -> Self {
        Self {
            peers: HashMap::new(),
            updated_at: Utc::now(),
        }
    }

    /// Load peer store from disk. Returns empty store if file doesn't exist.
    pub fn load(data_dir: &Path) -> Self {
        let path = Self::file_path(data_dir);
        match std::fs::read_to_string(&path) {
            Ok(json) => match serde_json::from_str(&json) {
                Ok(store) => {
                    info!("Loaded {} known peers from {}", 
                        Self::count(&store), path.display());
                    store
                }
                Err(e) => {
                    warn!("Failed to parse peer store: {}. Starting fresh.", e);
                    Self::new()
                }
            },
            Err(_) => {
                info!("No peer store found at {}. Starting fresh.", path.display());
                Self::new()
            }
        }
    }

    /// Save peer store to disk.
    pub fn save(&mut self, data_dir: &Path) -> Result<(), String> {
        self.updated_at = Utc::now();
        let path = Self::file_path(data_dir);

        // Ensure directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create peer store dir: {}", e))?;
        }

        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize peer store: {}", e))?;
        std::fs::write(&path, json)
            .map_err(|e| format!("Failed to write peer store: {}", e))?;

        info!("Saved {} known peers to {}", self.peers.len(), path.display());
        Ok(())
    }

    /// Record a peer we've connected to.
    pub fn record_peer(
        &mut self,
        peer_id: &str,
        addresses: Vec<String>,
        nodalync_id: Option<String>,
        manual: bool,
    ) {
        let entry = self.peers.entry(peer_id.to_string()).or_insert_with(|| {
            KnownPeer {
                peer_id: peer_id.to_string(),
                addresses: Vec::new(),
                nodalync_id: None,
                last_seen: Utc::now(),
                connection_count: 0,
                manual,
            }
        });

        // Update addresses (merge, don't replace)
        for addr in addresses {
            if !entry.addresses.contains(&addr) {
                entry.addresses.push(addr);
            }
        }

        // Update nodalync ID if we have one
        if nodalync_id.is_some() {
            entry.nodalync_id = nodalync_id;
        }

        entry.last_seen = Utc::now();
        entry.connection_count += 1;
    }

    /// Get bootstrap entries: peers with addresses, sorted by most recently seen.
    ///
    /// Returns (peer_id_str, first_address_str) pairs suitable for
    /// parsing into libp2p types.
    pub fn bootstrap_entries(&self, max: usize) -> Vec<(&str, &str)> {
        let mut entries: Vec<&KnownPeer> = self
            .peers
            .values()
            .filter(|p| !p.addresses.is_empty())
            .collect();

        // Sort by: manual first, then by last_seen descending
        entries.sort_by(|a, b| {
            b.manual
                .cmp(&a.manual)
                .then_with(|| b.last_seen.cmp(&a.last_seen))
        });

        entries
            .into_iter()
            .take(max)
            .map(|p| (p.peer_id.as_str(), p.addresses[0].as_str()))
            .collect()
    }

    /// Prune peers not seen in the given number of days.
    pub fn prune_stale(&mut self, max_age_days: i64) {
        let cutoff = Utc::now() - chrono::Duration::days(max_age_days);
        let before = self.peers.len();
        self.peers.retain(|_, p| p.last_seen > cutoff || p.manual);
        let removed = before - self.peers.len();
        if removed > 0 {
            info!("Pruned {} stale peers (older than {} days)", removed, max_age_days);
        }
    }

    fn file_path(data_dir: &Path) -> PathBuf {
        data_dir.join("peers.json")
    }

    fn count(store: &Self) -> usize {
        store.peers.len()
    }
}

impl Default for PeerStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_new_store_is_empty() {
        let store = PeerStore::new();
        assert!(store.peers.is_empty());
    }

    #[test]
    fn test_record_and_retrieve_peer() {
        let mut store = PeerStore::new();
        store.record_peer(
            "12D3KooWTest123",
            vec!["/ip4/192.168.1.5/tcp/9000".to_string()],
            None,
            false,
        );

        assert_eq!(store.peers.len(), 1);
        let entries = store.bootstrap_entries(10);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, "12D3KooWTest123");
        assert_eq!(entries[0].1, "/ip4/192.168.1.5/tcp/9000");
    }

    #[test]
    fn test_save_and_load() {
        let tmp = TempDir::new().unwrap();
        let mut store = PeerStore::new();
        store.record_peer(
            "12D3KooWTest456",
            vec!["/ip4/10.0.0.1/tcp/4001".to_string()],
            Some("nodalync-peer-1".to_string()),
            true,
        );

        store.save(tmp.path()).unwrap();

        let loaded = PeerStore::load(tmp.path());
        assert_eq!(loaded.peers.len(), 1);
        let peer = loaded.peers.get("12D3KooWTest456").unwrap();
        assert_eq!(peer.nodalync_id.as_deref(), Some("nodalync-peer-1"));
        assert!(peer.manual);
    }

    #[test]
    fn test_merge_addresses() {
        let mut store = PeerStore::new();
        store.record_peer(
            "peer1",
            vec!["/ip4/1.2.3.4/tcp/9000".to_string()],
            None,
            false,
        );
        store.record_peer(
            "peer1",
            vec!["/ip4/5.6.7.8/tcp/9000".to_string()],
            None,
            false,
        );

        let peer = store.peers.get("peer1").unwrap();
        assert_eq!(peer.addresses.len(), 2);
        assert_eq!(peer.connection_count, 2);
    }

    #[test]
    fn test_prune_stale() {
        let mut store = PeerStore::new();
        store.record_peer("fresh", vec!["/ip4/1.1.1.1/tcp/1".to_string()], None, false);

        // Manually set an old peer
        store.peers.insert(
            "stale".to_string(),
            KnownPeer {
                peer_id: "stale".to_string(),
                addresses: vec!["/ip4/2.2.2.2/tcp/2".to_string()],
                nodalync_id: None,
                last_seen: Utc::now() - chrono::Duration::days(31),
                connection_count: 1,
                manual: false,
            },
        );

        store.prune_stale(30);
        assert_eq!(store.peers.len(), 1);
        assert!(store.peers.contains_key("fresh"));
    }

    #[test]
    fn test_manual_peers_survive_prune() {
        let mut store = PeerStore::new();
        store.peers.insert(
            "manual-old".to_string(),
            KnownPeer {
                peer_id: "manual-old".to_string(),
                addresses: vec!["/ip4/3.3.3.3/tcp/3".to_string()],
                nodalync_id: None,
                last_seen: Utc::now() - chrono::Duration::days(100),
                connection_count: 1,
                manual: true,
            },
        );

        store.prune_stale(30);
        assert_eq!(store.peers.len(), 1); // manual peers survive
    }
}
