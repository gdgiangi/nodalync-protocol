//! Connection invite system for Nodalync Studio.
//!
//! Generates and parses shareable invite strings that encode everything
//! needed to connect to a peer: peer ID, addresses, and optional metadata.
//!
//! This is the primary D3 onboarding mechanism before public seed nodes
//! are deployed. A running node generates an invite string; a new user
//! pastes it into their app to connect.
//!
//! ## Invite Format
//!
//! **Compact:** `nodalync://connect/<peer_id>@<multiaddr>`
//! **Full (JSON-encoded, base64):** `nodalync://connect/<base64_json>`
//!
//! The full format supports multiple addresses and metadata:
//! ```json
//! {
//!   "v": 1,
//!   "pid": "12D3KooW...",
//!   "addrs": ["/ip4/.../tcp/9000", "/ip4/.../tcp/9001"],
//!   "name": "Alice's Node",
//!   "proto": "0.7.1"
//! }
//! ```

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::{Deserialize, Serialize};

/// Protocol prefix for invite URLs.
const INVITE_PREFIX: &str = "nodalync://connect/";

/// Current invite format version.
const INVITE_VERSION: u8 = 1;

/// Parsed invite data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InviteData {
    /// Invite format version.
    pub v: u8,
    /// libp2p PeerId string.
    pub pid: String,
    /// Multiaddresses to try connecting to.
    pub addrs: Vec<String>,
    /// Optional human-readable node name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Protocol version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proto: Option<String>,
}

/// Generate a compact invite string (single address).
///
/// Format: `nodalync://connect/<peer_id>@<multiaddr>`
pub fn generate_compact_invite(peer_id: &str, address: &str) -> String {
    format!("{}{}{}{}", INVITE_PREFIX, peer_id, "@", address)
}

/// Generate a full invite string (multiple addresses, metadata).
///
/// Format: `nodalync://connect/<base64_json>`
pub fn generate_full_invite(
    peer_id: &str,
    addresses: Vec<String>,
    name: Option<String>,
) -> Result<String, String> {
    if addresses.is_empty() {
        return Err("At least one address is required".to_string());
    }

    let data = InviteData {
        v: INVITE_VERSION,
        pid: peer_id.to_string(),
        addrs: addresses,
        name,
        proto: Some("0.7.1".to_string()),
    };

    let json = serde_json::to_string(&data).map_err(|e| format!("Serialize error: {}", e))?;
    let encoded = URL_SAFE_NO_PAD.encode(json.as_bytes());

    Ok(format!("{}{}", INVITE_PREFIX, encoded))
}

/// Parse an invite string into its components.
///
/// Accepts both compact and full formats.
pub fn parse_invite(invite: &str) -> Result<InviteData, String> {
    let trimmed = invite.trim();

    // Strip prefix (case-insensitive)
    let payload = if let Some(p) = trimmed.strip_prefix(INVITE_PREFIX) {
        p
    } else if let Some(p) = trimmed.strip_prefix("nodalync://Connect/") {
        p
    } else {
        // Try without prefix (user might paste just the payload)
        trimmed
    };

    // Try compact format first: peer_id@multiaddr
    // Detect by looking for '@' followed by '/' (multiaddr always starts with /)
    if let Some(at_pos) = payload.find('@') {
        let after_at = &payload[at_pos + 1..];
        if after_at.starts_with('/') {
            let peer_id = &payload[..at_pos];
            let address = after_at;

            // Validate peer_id — if this fails, fall through to base64 path
            if let Ok(_pid) = peer_id.parse::<nodalync_net::PeerId>() {
                // Validate address
                address
                    .parse::<nodalync_net::Multiaddr>()
                    .map_err(|e| format!("Invalid address: {}", e))?;

                return Ok(InviteData {
                    v: 1,
                    pid: peer_id.to_string(),
                    addrs: vec![address.to_string()],
                    name: None,
                    proto: None,
                });
            }
        }
    }

    // Try full format: base64-encoded JSON
    let decoded = URL_SAFE_NO_PAD
        .decode(payload.as_bytes())
        .map_err(|e| format!("Invalid invite format: {}", e))?;

    let data: InviteData =
        serde_json::from_slice(&decoded).map_err(|e| format!("Invalid invite data: {}", e))?;

    // Validate version
    if data.v > INVITE_VERSION {
        return Err(format!(
            "Invite version {} not supported (max: {}). Update Nodalync Studio.",
            data.v, INVITE_VERSION
        ));
    }

    // Validate peer_id
    data.pid
        .parse::<nodalync_net::PeerId>()
        .map_err(|e| format!("Invalid peer ID in invite: {}", e))?;

    // Validate at least one address
    if data.addrs.is_empty() {
        return Err("Invite contains no addresses".to_string());
    }

    for addr in &data.addrs {
        addr.parse::<nodalync_net::Multiaddr>()
            .map_err(|e| format!("Invalid address in invite: {}", e))?;
    }

    Ok(data)
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn test_peer_id() -> String {
        nodalync_net::PeerId::random().to_string()
    }

    #[test]
    fn test_compact_invite_roundtrip() {
        let pid = test_peer_id();
        let addr = "/ip4/192.168.1.5/tcp/9000";

        let invite = generate_compact_invite(&pid, addr);
        assert!(invite.starts_with(INVITE_PREFIX));

        let parsed = parse_invite(&invite).unwrap();
        assert_eq!(parsed.pid, pid);
        assert_eq!(parsed.addrs, vec![addr.to_string()]);
    }

    #[test]
    fn test_full_invite_roundtrip() {
        let pid = test_peer_id();
        let addrs = vec![
            "/ip4/192.168.1.5/tcp/9000".to_string(),
            "/ip4/10.0.0.1/tcp/9001".to_string(),
        ];

        let invite = generate_full_invite(&pid, addrs.clone(), Some("Test Node".to_string()))
            .unwrap();
        assert!(invite.starts_with(INVITE_PREFIX));

        let parsed = parse_invite(&invite).unwrap();
        assert_eq!(parsed.pid, pid);
        assert_eq!(parsed.addrs, addrs);
        assert_eq!(parsed.name.as_deref(), Some("Test Node"));
        assert_eq!(parsed.proto.as_deref(), Some("0.7.1"));
    }

    #[test]
    fn test_parse_without_prefix() {
        let pid = test_peer_id();
        let addr = "/ip4/1.2.3.4/tcp/9000";

        // Compact without prefix
        let raw = format!("{}@{}", pid, addr);
        let parsed = parse_invite(&raw).unwrap();
        assert_eq!(parsed.pid, pid);
    }

    #[test]
    fn test_parse_invalid_peer_id() {
        let result = parse_invite("nodalync://connect/invalid@/ip4/1.2.3.4/tcp/9000");
        // "invalid" doesn't start with 12D3KooW or Qm, so it tries base64 path
        assert!(result.is_err());
    }

    #[test]
    fn test_full_invite_no_addresses_fails() {
        let pid = test_peer_id();
        let result = generate_full_invite(&pid, vec![], None);
        assert!(result.is_err());
    }

    #[test]
    fn test_compact_invite_with_dns_addr() {
        let pid = test_peer_id();
        let addr = "/dns4/seed1.nodalync.io/tcp/9000";

        let invite = generate_compact_invite(&pid, addr);
        let parsed = parse_invite(&invite).unwrap();
        assert_eq!(parsed.addrs[0], addr);
    }

    #[test]
    fn test_invite_whitespace_trimmed() {
        let pid = test_peer_id();
        let addr = "/ip4/1.2.3.4/tcp/9000";

        let invite = format!("  nodalync://connect/{}@{}  \n", pid, addr);
        let parsed = parse_invite(&invite).unwrap();
        assert_eq!(parsed.pid, pid);
    }
}
