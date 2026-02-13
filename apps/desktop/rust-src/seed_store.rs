//! Seed node configuration for Nodalync Studio.
//!
//! Manages a list of well-known seed nodes that enable first-time network
//! discovery. Without seed nodes, a fresh install can only find peers via
//! mDNS (LAN-only), which blocks D3 (external users joining the network).
//!
//! Discovery priority:
//! 1. **Hardcoded testnet seeds** — compiled into the binary
//! 2. **User-configured seeds** — from seeds.json in data dir
//! 3. **Known peers** — from peers.json (existing PeerStore)
//! 4. **mDNS** — LAN-only fallback

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

// ─── Hardcoded Testnet Seeds ─────────────────────────────────────────────────

/// Nodalync testnet seed nodes.
///
/// These are well-known public nodes that new installations connect to first.
/// Update these when deploying new seed infrastructure.
///
/// Format: (peer_id, multiaddr)
/// - PeerId: libp2p PeerId string (e.g. "12D3KooW...")
/// - Addr: Multiaddr (e.g. "/ip4/x.x.x.x/tcp/9000" or "/dns4/seed1.nodalync.io/tcp/9000")
///
/// IMPORTANT: Keep this list current. Stale seeds degrade first-run experience.
const TESTNET_SEEDS: &[(&str, &str)] = &[
    // Placeholder — replace with actual deployed seed nodes.
    // These will be populated when the first public seed is deployed.
    // Format: ("12D3KooW...", "/dns4/seed1.nodalync.io/tcp/9000"),
];

/// Protocol version for seed handshake compatibility.
pub const SEED_PROTOCOL_VERSION: &str = "nodalync/0.7.1";

// ─── Seed Node Types ─────────────────────────────────────────────────────────

/// A seed node entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeedNode {
    /// libp2p PeerId string.
    pub peer_id: String,
    /// Multiaddress (can be IP or DNS-based).
    pub address: String,
    /// Human-readable label (e.g. "Nodalync Testnet Seed 1").
    pub label: Option<String>,
    /// Source of this seed entry.
    pub source: SeedSource,
    /// When this seed was added or last verified.
    pub added_at: DateTime<Utc>,
    /// Whether this seed is currently enabled.
    pub enabled: bool,
}

/// Where a seed node entry came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SeedSource {
    /// Hardcoded in the binary (testnet seeds).
    Builtin,
    /// Added by the user manually.
    User,
    /// Discovered via DNS TXT records.
    Dns,
    /// Shared by another peer.
    PeerExchange,
}

// ─── Seed Store ──────────────────────────────────────────────────────────────

/// Persistent store for seed nodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeedStore {
    /// All known seed nodes.
    pub seeds: Vec<SeedNode>,
    /// Protocol version this seed list is for.
    pub protocol_version: String,
    /// Last time the store was updated.
    pub updated_at: DateTime<Utc>,
}

impl SeedStore {
    /// Create a new seed store with builtin testnet seeds.
    pub fn new() -> Self {
        let mut store = Self {
            seeds: Vec::new(),
            protocol_version: SEED_PROTOCOL_VERSION.to_string(),
            updated_at: Utc::now(),
        };
        store.load_builtin_seeds();
        store
    }

    /// Load seed store from disk, merging with builtin seeds.
    pub fn load(data_dir: &Path) -> Self {
        let path = Self::file_path(data_dir);
        let mut store = match std::fs::read_to_string(&path) {
            Ok(json) => match serde_json::from_str::<SeedStore>(&json) {
                Ok(s) => {
                    info!("Loaded {} seeds from {}", s.seeds.len(), path.display());
                    s
                }
                Err(e) => {
                    warn!("Failed to parse seed store: {}. Starting fresh.", e);
                    Self {
                        seeds: Vec::new(),
                        protocol_version: SEED_PROTOCOL_VERSION.to_string(),
                        updated_at: Utc::now(),
                    }
                }
            },
            Err(_) => {
                debug!("No seed store at {}. Using defaults.", path.display());
                Self {
                    seeds: Vec::new(),
                    protocol_version: SEED_PROTOCOL_VERSION.to_string(),
                    updated_at: Utc::now(),
                }
            }
        };

        // Always merge builtin seeds (in case binary was updated with new seeds)
        store.load_builtin_seeds();
        store
    }

    /// Save seed store to disk.
    pub fn save(&mut self, data_dir: &Path) -> Result<(), String> {
        self.updated_at = Utc::now();
        let path = Self::file_path(data_dir);

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create seed store dir: {}", e))?;
        }

        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize seed store: {}", e))?;
        std::fs::write(&path, json)
            .map_err(|e| format!("Failed to write seed store: {}", e))?;

        info!("Saved {} seeds to {}", self.seeds.len(), path.display());
        Ok(())
    }

    /// Add a user-configured seed node.
    pub fn add_seed(
        &mut self,
        peer_id: String,
        address: String,
        label: Option<String>,
    ) -> Result<(), String> {
        // Validate peer_id format
        peer_id
            .parse::<nodalync_net::PeerId>()
            .map_err(|e| format!("Invalid peer ID: {}", e))?;

        // Validate multiaddr format
        address
            .parse::<nodalync_net::Multiaddr>()
            .map_err(|e| format!("Invalid address: {}", e))?;

        // Check for duplicates
        if self.seeds.iter().any(|s| s.peer_id == peer_id) {
            // Update existing
            if let Some(seed) = self.seeds.iter_mut().find(|s| s.peer_id == peer_id) {
                seed.address = address;
                seed.label = label.or(seed.label.clone());
                seed.enabled = true;
                info!("Updated seed: {}", seed.peer_id);
            }
        } else {
            self.seeds.push(SeedNode {
                peer_id: peer_id.clone(),
                address,
                label,
                source: SeedSource::User,
                added_at: Utc::now(),
                enabled: true,
            });
            info!("Added user seed: {}", peer_id);
        }

        Ok(())
    }

    /// Remove a seed node by peer_id. Builtin seeds can't be removed, only disabled.
    pub fn remove_seed(&mut self, peer_id: &str) -> Result<(), String> {
        if let Some(seed) = self.seeds.iter_mut().find(|s| s.peer_id == peer_id) {
            if seed.source == SeedSource::Builtin {
                seed.enabled = false;
                info!("Disabled builtin seed: {}", peer_id);
            } else {
                self.seeds.retain(|s| s.peer_id != peer_id);
                info!("Removed seed: {}", peer_id);
            }
            Ok(())
        } else {
            Err(format!("Seed not found: {}", peer_id))
        }
    }

    /// Get all enabled seeds as (peer_id, address) pairs for bootstrap.
    pub fn bootstrap_entries(&self) -> Vec<(&str, &str)> {
        self.seeds
            .iter()
            .filter(|s| s.enabled)
            .map(|s| (s.peer_id.as_str(), s.address.as_str()))
            .collect()
    }

    /// Get count of enabled seeds.
    pub fn enabled_count(&self) -> usize {
        self.seeds.iter().filter(|s| s.enabled).count()
    }

    /// Merge builtin seeds, preserving user modifications.
    fn load_builtin_seeds(&mut self) {
        for (peer_id, addr) in TESTNET_SEEDS {
            if !self.seeds.iter().any(|s| s.peer_id == *peer_id) {
                self.seeds.push(SeedNode {
                    peer_id: peer_id.to_string(),
                    address: addr.to_string(),
                    label: Some("Nodalync Testnet Seed".to_string()),
                    source: SeedSource::Builtin,
                    added_at: Utc::now(),
                    enabled: true,
                });
            }
        }
    }

    fn file_path(data_dir: &Path) -> PathBuf {
        data_dir.join("seeds.json")
    }
}

impl Default for SeedStore {
    fn default() -> Self {
        Self::new()
    }
}

// ─── DNS-based Seed Discovery ────────────────────────────────────────────────

/// DNS domain for seed node TXT records.
///
/// TXT records should be in format: "peer_id=12D3KooW...;addr=/ip4/.../tcp/9000"
#[allow(dead_code)]
const DNS_SEED_DOMAIN: &str = "_nodalync-seeds._tcp.nodalync.io";

/// Attempt to discover seed nodes via DNS TXT records.
///
/// This is a fallback when hardcoded seeds are stale or unavailable.
/// Returns any seeds found, or empty vec on failure.
#[allow(dead_code)]
pub async fn discover_seeds_dns() -> Vec<SeedNode> {
    // Use tokio's DNS resolver
    let resolver = match tokio::net::lookup_host(format!("{}:0", DNS_SEED_DOMAIN)).await {
        Ok(_) => {
            info!("DNS seed domain resolved, checking TXT records");
        }
        Err(e) => {
            debug!("DNS seed discovery unavailable: {} (expected before deployment)", e);
            return Vec::new();
        }
    };

    // For TXT record lookup, we'd need a proper DNS crate like trust-dns.
    // For now, log that DNS discovery was attempted and return empty.
    // This will be wired up when seed infrastructure is deployed.
    debug!("DNS seed discovery: TXT lookup not yet implemented for {}", DNS_SEED_DOMAIN);
    let _ = resolver;
    Vec::new()
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_new_store_has_builtin_seeds() {
        let store = SeedStore::new();
        // Builtin seeds are currently empty placeholders
        assert_eq!(store.seeds.len(), TESTNET_SEEDS.len());
        assert_eq!(store.protocol_version, SEED_PROTOCOL_VERSION);
    }

    #[test]
    fn test_add_user_seed() {
        let mut store = SeedStore::new();
        let peer_id = nodalync_net::PeerId::random().to_string();

        store
            .add_seed(
                peer_id.clone(),
                "/ip4/1.2.3.4/tcp/9000".to_string(),
                Some("My seed".to_string()),
            )
            .unwrap();

        assert_eq!(store.seeds.len(), TESTNET_SEEDS.len() + 1);
        let seed = store.seeds.iter().find(|s| s.peer_id == peer_id).unwrap();
        assert_eq!(seed.source, SeedSource::User);
        assert!(seed.enabled);
    }

    #[test]
    fn test_add_duplicate_updates() {
        let mut store = SeedStore::new();
        let peer_id = nodalync_net::PeerId::random().to_string();

        store
            .add_seed(peer_id.clone(), "/ip4/1.1.1.1/tcp/9000".to_string(), None)
            .unwrap();
        store
            .add_seed(
                peer_id.clone(),
                "/ip4/2.2.2.2/tcp/9000".to_string(),
                Some("Updated".to_string()),
            )
            .unwrap();

        // Should not duplicate
        let matching: Vec<_> = store.seeds.iter().filter(|s| s.peer_id == peer_id).collect();
        assert_eq!(matching.len(), 1);
        assert_eq!(matching[0].address, "/ip4/2.2.2.2/tcp/9000");
        assert_eq!(matching[0].label.as_deref(), Some("Updated"));
    }

    #[test]
    fn test_invalid_peer_id_rejected() {
        let mut store = SeedStore::new();
        let result = store.add_seed(
            "not-a-peer-id".to_string(),
            "/ip4/1.2.3.4/tcp/9000".to_string(),
            None,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_address_rejected() {
        let mut store = SeedStore::new();
        let peer_id = nodalync_net::PeerId::random().to_string();
        let result = store.add_seed(peer_id, "not-an-address".to_string(), None);
        assert!(result.is_err());
    }

    #[test]
    fn test_remove_user_seed() {
        let mut store = SeedStore::new();
        let peer_id = nodalync_net::PeerId::random().to_string();

        store
            .add_seed(peer_id.clone(), "/ip4/1.2.3.4/tcp/9000".to_string(), None)
            .unwrap();
        store.remove_seed(&peer_id).unwrap();

        assert!(!store.seeds.iter().any(|s| s.peer_id == peer_id));
    }

    #[test]
    fn test_bootstrap_entries_only_enabled() {
        let mut store = SeedStore::new();
        let pid1 = nodalync_net::PeerId::random().to_string();
        let pid2 = nodalync_net::PeerId::random().to_string();

        store
            .add_seed(pid1.clone(), "/ip4/1.1.1.1/tcp/9000".to_string(), None)
            .unwrap();
        store
            .add_seed(pid2.clone(), "/ip4/2.2.2.2/tcp/9000".to_string(), None)
            .unwrap();

        // Disable one
        store
            .seeds
            .iter_mut()
            .find(|s| s.peer_id == pid2)
            .unwrap()
            .enabled = false;

        let entries = store.bootstrap_entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, pid1);
    }

    #[test]
    fn test_save_and_load() {
        let tmp = TempDir::new().unwrap();
        let peer_id = nodalync_net::PeerId::random().to_string();

        let mut store = SeedStore::new();
        store
            .add_seed(
                peer_id.clone(),
                "/ip4/5.5.5.5/tcp/9000".to_string(),
                Some("Test seed".to_string()),
            )
            .unwrap();
        store.save(tmp.path()).unwrap();

        let loaded = SeedStore::load(tmp.path());
        let seed = loaded.seeds.iter().find(|s| s.peer_id == peer_id).unwrap();
        assert_eq!(seed.label.as_deref(), Some("Test seed"));
        assert_eq!(seed.source, SeedSource::User);
    }

    #[test]
    fn test_enabled_count() {
        let mut store = SeedStore::new();
        let pid1 = nodalync_net::PeerId::random().to_string();
        let pid2 = nodalync_net::PeerId::random().to_string();

        store
            .add_seed(pid1, "/ip4/1.1.1.1/tcp/9000".to_string(), None)
            .unwrap();
        store
            .add_seed(pid2.clone(), "/ip4/2.2.2.2/tcp/9000".to_string(), None)
            .unwrap();

        assert_eq!(store.enabled_count(), 2);

        store
            .seeds
            .iter_mut()
            .find(|s| s.peer_id == pid2)
            .unwrap()
            .enabled = false;

        assert_eq!(store.enabled_count(), 1);
    }
}
