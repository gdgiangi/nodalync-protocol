//! Payment channel management commands.

use nodalync_crypto::PeerId;
use nodalync_ops::CloseResult;
use nodalync_store::ChannelStore;

use crate::config::CliConfig;
use crate::context::NodeContext;
use crate::error::{CliError, CliResult};
use crate::output::{ChannelListOutput, ChannelOutput, ChannelSummary, OutputFormat, Render};

/// Minimum channel deposit in HBAR.
const MIN_CHANNEL_DEPOSIT_HBAR: f64 = 100.0;

/// Open a payment channel with a peer.
pub async fn open_channel(
    config: CliConfig,
    format: OutputFormat,
    peer_id_str: &str,
    deposit_hbar: f64,
) -> CliResult<String> {
    // Validate minimum deposit
    if deposit_hbar < MIN_CHANNEL_DEPOSIT_HBAR {
        return Err(CliError::User(format!(
            "Minimum channel deposit is {} HBAR, got {}",
            MIN_CHANNEL_DEPOSIT_HBAR, deposit_hbar
        )));
    }

    // Convert HBAR to tinybars (1 HBAR = 100_000_000 tinybars)
    let deposit_tinybars = (deposit_hbar * 100_000_000.0) as u64;

    // Initialize context with network
    let mut ctx = NodeContext::with_network(config).await?;

    // Bootstrap to connect to the network
    ctx.bootstrap().await?;

    // Check if this is a libp2p peer ID (12D3KooW...)
    if peer_id_str.starts_with("12D3KooW") {
        // Parse libp2p peer ID
        let libp2p_peer: nodalync_net::PeerId = peer_id_str
            .parse()
            .map_err(|e| CliError::User(format!("Invalid libp2p peer ID: {}", e)))?;

        // Open channel using libp2p peer ID
        let (channel, nodalync_peer_id) = ctx
            .ops
            .open_payment_channel_to_libp2p(libp2p_peer, deposit_tinybars)
            .await?;

        let output = ChannelOutput {
            channel_id: channel.channel_id.to_string(),
            peer_id: nodalync_crypto::peer_id_to_string(&nodalync_peer_id),
            state: format!("{:?}", channel.state),
            my_balance: channel.my_balance,
            their_balance: channel.their_balance,
            operation: "opened".to_string(),
            transaction_id: channel.funding_tx_id.clone(),
        };

        return Ok(output.render(format));
    }

    // Parse Nodalync peer ID from hex or base58 string
    let peer_id = parse_peer_id(peer_id_str)?;

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
        transaction_id: channel.funding_tx_id.clone(),
    };

    Ok(output.render(format))
}

/// Close a payment channel with a peer.
///
/// Attempts cooperative close first. If the peer is unresponsive,
/// returns a message suggesting to use dispute-channel instead.
pub async fn close_channel(
    config: CliConfig,
    format: OutputFormat,
    peer_id_str: &str,
) -> CliResult<String> {
    // Parse peer ID from hex string
    let peer_id = parse_peer_id(peer_id_str)?;

    // Initialize context with network
    let mut ctx = NodeContext::with_network(config).await?;

    // Bootstrap to connect to the network
    ctx.bootstrap().await?;

    // Get channel info before closing
    let channel = ctx
        .ops
        .get_payment_channel(&peer_id)?
        .ok_or_else(|| CliError::User("No channel exists with this peer".into()))?;

    let channel_id = channel.channel_id.to_string();
    let my_balance = channel.my_balance;
    let their_balance = channel.their_balance;

    // Try cooperative close with signature
    let result = ctx
        .ops
        .close_payment_channel(&peer_id, &ctx.private_key)
        .await?;

    match result {
        CloseResult::Success {
            transaction_id,
            final_balances,
        } => {
            let output = ChannelOutput {
                channel_id,
                peer_id: peer_id_str.to_string(),
                state: "Closed".to_string(),
                my_balance: final_balances.0,
                their_balance: final_balances.1,
                operation: "closed (on-chain)".to_string(),
                transaction_id: Some(transaction_id),
            };
            Ok(output.render(format))
        }
        CloseResult::SuccessOffChain { final_balances } => {
            let output = ChannelOutput {
                channel_id,
                peer_id: peer_id_str.to_string(),
                state: "Closed".to_string(),
                my_balance: final_balances.0,
                their_balance: final_balances.1,
                operation: "closed (off-chain)".to_string(),
                transaction_id: None,
            };
            Ok(output.render(format))
        }
        CloseResult::PeerUnresponsive { suggestion } => {
            // Return as formatted output based on format
            if format == OutputFormat::Json {
                Ok(serde_json::json!({
                    "status": "peer_unresponsive",
                    "channel_id": channel_id,
                    "peer_id": peer_id_str,
                    "my_balance": my_balance,
                    "their_balance": their_balance,
                    "suggestion": suggestion
                })
                .to_string())
            } else {
                Ok(format!(
                    "Cooperative close failed: peer unresponsive\n\n\
                    Channel: {}\n\
                    Your balance: {} tinybars\n\
                    Their balance: {} tinybars\n\n\
                    {}",
                    channel_id, my_balance, their_balance, suggestion
                ))
            }
        }
        CloseResult::OnChainFailed { error } => Err(CliError::User(format!(
            "On-chain close failed: {}. Try using 'nodalync dispute-channel {}' instead.",
            error, peer_id_str
        ))),
    }
}

/// Initiate a dispute-based channel close.
///
/// Use when the peer is unresponsive. Starts a 24-hour waiting period.
pub async fn dispute_channel(
    config: CliConfig,
    format: OutputFormat,
    peer_id_str: &str,
) -> CliResult<String> {
    // Parse peer ID
    let peer_id = parse_peer_id(peer_id_str)?;

    // Initialize context with network
    let mut ctx = NodeContext::with_network(config).await?;

    // Get channel info
    let channel = ctx
        .ops
        .get_payment_channel(&peer_id)?
        .ok_or_else(|| CliError::User("No channel exists with this peer".into()))?;

    // Check if already has pending dispute
    if channel.pending_dispute.is_some() {
        return Err(CliError::User(
            "Channel already has a pending dispute. Use 'nodalync resolve-dispute' when the waiting period is over.".into()
        ));
    }

    let channel_id = channel.channel_id.to_string();
    let my_balance = channel.my_balance;
    let their_balance = channel.their_balance;

    // Initiate dispute
    let tx_id = ctx
        .ops
        .dispute_payment_channel(&peer_id, &ctx.private_key)
        .await?;

    if format == OutputFormat::Json {
        Ok(serde_json::json!({
            "status": "dispute_initiated",
            "channel_id": channel_id,
            "peer_id": peer_id_str,
            "transaction_id": tx_id,
            "my_balance": my_balance,
            "their_balance": their_balance,
            "waiting_period": "24 hours"
        })
        .to_string())
    } else {
        Ok(format!(
            "Dispute initiated\n\n\
            Channel: {}\n\
            Transaction: {}\n\
            Your balance: {} tinybars\n\
            Their balance: {} tinybars\n\n\
            Waiting period: 24 hours\n\
            Run 'nodalync resolve-dispute {}' after the waiting period to finalize.",
            channel_id, tx_id, my_balance, their_balance, peer_id_str
        ))
    }
}

/// Resolve a channel dispute after the waiting period.
pub async fn resolve_dispute(
    config: CliConfig,
    format: OutputFormat,
    peer_id_str: &str,
) -> CliResult<String> {
    // Parse peer ID
    let peer_id = parse_peer_id(peer_id_str)?;

    // Initialize context
    let mut ctx = NodeContext::with_network(config).await?;

    // Check dispute status first
    let status = ctx.ops.get_pending_dispute_status(&peer_id)?;

    match status {
        None => {
            return Err(CliError::User("No pending dispute for this channel".into()));
        }
        Some((_, false, remaining_ms)) => {
            let hours = remaining_ms as f64 / (60.0 * 60.0 * 1000.0);
            return Err(CliError::User(format!(
                "Dispute waiting period not yet complete. {:.1} hours remaining.",
                hours
            )));
        }
        Some((_, true, _)) => {
            // Can resolve
        }
    }

    // Get channel info
    let channel = ctx
        .ops
        .get_payment_channel(&peer_id)?
        .ok_or_else(|| CliError::User("Channel not found".into()))?;

    let channel_id = channel.channel_id.to_string();
    let my_balance = channel.my_balance;
    let their_balance = channel.their_balance;

    // Resolve the dispute
    let tx_id = ctx.ops.resolve_dispute(&peer_id).await?;

    if format == OutputFormat::Json {
        Ok(serde_json::json!({
            "status": "dispute_resolved",
            "channel_id": channel_id,
            "peer_id": peer_id_str,
            "transaction_id": tx_id,
            "final_balance_ours": my_balance,
            "final_balance_theirs": their_balance
        })
        .to_string())
    } else {
        Ok(format!(
            "Dispute resolved\n\n\
            Channel: {}\n\
            Transaction: {}\n\
            Your final balance: {} tinybars\n\
            Their final balance: {} tinybars\n\n\
            Channel is now closed.",
            channel_id, tx_id, my_balance, their_balance
        ))
    }
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

/// Parse a peer ID from a string.
///
/// Accepts two formats:
/// - Base58 format: `ndl1...` (human-readable, e.g., `ndl13zE3otwfgopSgkT17R3yfhcT3sj8`)
/// - Hex format: 40 hex characters (e.g., `0102030405060708090a0b0c0d0e0f1011121314`)
fn parse_peer_id(s: &str) -> CliResult<PeerId> {
    // Try base58 format first (starts with "ndl1")
    if s.starts_with("ndl1") {
        return nodalync_crypto::peer_id_from_string(s)
            .map_err(|e| CliError::User(format!("Invalid peer ID: {}", e)));
    }

    // Try hex format (40 hex characters)
    let hex_str = s.strip_prefix("ndl").unwrap_or(s);

    if hex_str.len() != 40 {
        return Err(CliError::User(format!(
            "Peer ID must be base58 format (ndl1...) or 40 hex characters, got: {}",
            s
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
    fn test_parse_peer_id_with_ndl_prefix() {
        let hex = "ndl0102030405060708090a0b0c0d0e0f1011121314";
        let result = parse_peer_id(hex);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_peer_id_base58() {
        // Generate a valid base58 peer ID
        let (_, public_key) = nodalync_crypto::generate_identity();
        let peer_id = nodalync_crypto::peer_id_from_public_key(&public_key);
        let base58 = nodalync_crypto::peer_id_to_string(&peer_id);

        let result = parse_peer_id(&base58);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().0, peer_id.0);
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
