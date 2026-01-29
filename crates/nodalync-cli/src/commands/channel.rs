//! Payment channel management commands.

use nodalync_crypto::PeerId;
use nodalync_store::ChannelStore;

use crate::config::CliConfig;
use crate::context::NodeContext;
use crate::error::{CliError, CliResult};
use crate::output::{ChannelListOutput, ChannelOutput, ChannelSummary, OutputFormat, Render};

/// Open a payment channel with a peer.
pub async fn open_channel(
    config: CliConfig,
    format: OutputFormat,
    peer_id_str: &str,
    deposit_hbar: f64,
) -> CliResult<String> {
    // Parse peer ID from hex string
    let peer_id = parse_peer_id(peer_id_str)?;

    // Convert HBAR to tinybars (1 HBAR = 100_000_000 tinybars)
    let deposit_tinybars = (deposit_hbar * 100_000_000.0) as u64;

    // Initialize context with network
    let mut ctx = NodeContext::with_network(config).await?;

    // Open the channel
    let channel = ctx
        .ops
        .open_payment_channel(&peer_id, deposit_tinybars)
        .await?;

    let output = ChannelOutput {
        channel_id: channel.channel_id.to_string(),
        peer_id: peer_id_str.to_string(),
        state: format!("{:?}", channel.state),
        my_balance: channel.my_balance,
        their_balance: channel.their_balance,
        operation: "opened".to_string(),
    };

    Ok(output.render(format))
}

/// Close a payment channel with a peer.
pub async fn close_channel(
    config: CliConfig,
    format: OutputFormat,
    peer_id_str: &str,
) -> CliResult<String> {
    // Parse peer ID from hex string
    let peer_id = parse_peer_id(peer_id_str)?;

    // Initialize context with network
    let mut ctx = NodeContext::with_network(config).await?;

    // Get channel info before closing
    let channel = ctx
        .ops
        .get_payment_channel(&peer_id)?
        .ok_or_else(|| CliError::User("No channel exists with this peer".into()))?;

    let channel_id = channel.channel_id.to_string();
    let my_balance = channel.my_balance;
    let their_balance = channel.their_balance;

    // Close the channel
    ctx.ops.close_payment_channel(&peer_id).await?;

    let output = ChannelOutput {
        channel_id,
        peer_id: peer_id_str.to_string(),
        state: "Closed".to_string(),
        my_balance,
        their_balance,
        operation: "closed".to_string(),
    };

    Ok(output.render(format))
}

/// List all payment channels.
pub fn list_channels(config: CliConfig, format: OutputFormat) -> CliResult<String> {
    // Use local context (no network needed for listing)
    let ctx = NodeContext::local(config)?;

    // Get all open channels using list_open
    let channels = ctx.ops.state.channels.list_open()?;

    let summaries: Vec<ChannelSummary> = channels
        .into_iter()
        .map(|(peer_id, c)| ChannelSummary {
            channel_id: c.channel_id.to_string(),
            peer_id: peer_id.to_string(),
            state: format!("{:?}", c.state),
            my_balance: c.my_balance,
            their_balance: c.their_balance,
            pending_payments: c.pending_payments.len() as u32,
        })
        .collect();

    let total = summaries.len();
    let open_count = summaries.iter().filter(|c| c.state == "Open").count();

    let output = ChannelListOutput {
        channels: summaries,
        total,
        open_count,
    };

    Ok(output.render(format))
}

/// Parse a peer ID from a hex string.
///
/// Peer IDs are 20 bytes, displayed as 40 hex characters.
fn parse_peer_id(s: &str) -> CliResult<PeerId> {
    // Remove optional "ndl" prefix if present
    let hex_str = s.strip_prefix("ndl").unwrap_or(s);

    // Must be 40 hex characters (20 bytes)
    if hex_str.len() != 40 {
        return Err(CliError::User(format!(
            "Peer ID must be 40 hex characters (20 bytes), got {} characters",
            hex_str.len()
        )));
    }

    let mut bytes = [0u8; 20];
    for (i, chunk) in hex_str.as_bytes().chunks(2).enumerate() {
        if i >= 20 {
            return Err(CliError::User("Invalid peer ID format".into()));
        }
        let hex_pair = std::str::from_utf8(chunk)
            .map_err(|_| CliError::User("Invalid peer ID format".into()))?;
        bytes[i] = u8::from_str_radix(hex_pair, 16)
            .map_err(|_| CliError::User(format!("Invalid hex in peer ID: {}", hex_pair)))?;
    }

    Ok(PeerId(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_peer_id_hex() {
        let hex = "0102030405060708090a0b0c0d0e0f1011121314";
        let result = parse_peer_id(hex);
        assert!(result.is_ok());
        let peer_id = result.unwrap();
        assert_eq!(peer_id.0[0], 0x01);
        assert_eq!(peer_id.0[19], 0x14);
    }

    #[test]
    fn test_parse_peer_id_with_prefix() {
        let hex = "ndl0102030405060708090a0b0c0d0e0f1011121314";
        let result = parse_peer_id(hex);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_peer_id_invalid() {
        let result = parse_peer_id("invalid");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_peer_id_wrong_length() {
        let result = parse_peer_id("0102030405");
        assert!(result.is_err());
    }
}
