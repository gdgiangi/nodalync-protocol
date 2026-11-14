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
    let deposit_tinybars = channel_deposit_tinybars(deposit_hbar)?;

    // Initialize context with network and bootstrap to connect to the network
    let mut ctx = NodeContext::connected(config).await?;

    // Check if this is a libp2p peer ID (12D3KooW...)
    if peer_id_str.starts_with("12D3KooW") {
        let output = open_channel_via_libp2p(&mut ctx, peer_id_str, deposit_tinybars).await?;
        return Ok(output.render(format));
    }

    let output = open_channel_via_nodalync(&mut ctx, peer_id_str, deposit_tinybars).await?;
    Ok(output.render(format))
}

/// Open a channel with a peer given by Nodalync peer ID (hex or base58).
async fn open_channel_via_nodalync(
    ctx: &mut NodeContext,
    peer_id_str: &str,
    deposit_tinybars: u64,
) -> CliResult<ChannelOutput> {
    // Parse Nodalync peer ID from hex or base58 string
    let peer_id = parse_peer_id(peer_id_str)?;

    // Open the channel
    let channel = ctx
        .ops
        .open_payment_channel(&peer_id, deposit_tinybars)
        .await?;

    Ok(opened_channel_output(&channel, peer_id_str.to_string()))
}

/// Validate the minimum deposit and convert HBAR to tinybars (1 HBAR = 100_000_000 tinybars).
fn channel_deposit_tinybars(deposit_hbar: f64) -> CliResult<u64> {
    if deposit_hbar < MIN_CHANNEL_DEPOSIT_HBAR {
        return Err(CliError::User(format!(
            "Minimum channel deposit is {} HBAR, got {}",
            MIN_CHANNEL_DEPOSIT_HBAR, deposit_hbar
        )));
    }
    Ok((deposit_hbar * 100_000_000.0) as u64)
}

/// Open a channel with a peer given by libp2p peer ID.
async fn open_channel_via_libp2p(
    ctx: &mut NodeContext,
    peer_id_str: &str,
    deposit_tinybars: u64,
) -> CliResult<ChannelOutput> {
    // Parse libp2p peer ID
    let libp2p_peer: nodalync_net::PeerId = peer_id_str
        .parse()
        .map_err(|e| CliError::User(format!("Invalid libp2p peer ID: {}", e)))?;

    // Open channel using libp2p peer ID
    let (channel, nodalync_peer_id) = ctx
        .ops
        .open_payment_channel_to_libp2p(libp2p_peer, deposit_tinybars)
        .await?;

    Ok(opened_channel_output(
        &channel,
        nodalync_crypto::peer_id_to_string(&nodalync_peer_id),
    ))
}

fn opened_channel_output(channel: &nodalync_types::Channel, peer_id: String) -> ChannelOutput {
    ChannelOutput {
        channel_id: channel.channel_id.to_string(),
        peer_id,
        state: format!("{:?}", channel.state),
        my_balance: channel.my_balance,
        their_balance: channel.their_balance,
        operation: "opened".to_string(),
        transaction_id: channel.funding_tx_id.clone(),
    }
}

/// A channel's ID and balances before it is closed or disputed.
struct ChannelSnapshot {
    channel_id: String,
    my_balance: u64,
    their_balance: u64,
}

impl ChannelSnapshot {
    fn of(channel: &nodalync_types::Channel) -> Self {
        Self {
            channel_id: channel.channel_id.to_string(),
            my_balance: channel.my_balance,
            their_balance: channel.their_balance,
        }
    }
}

/// The channel with `peer_id`, or a user error `missing` when there is none.
fn require_channel(
    ctx: &NodeContext,
    peer_id: &PeerId,
    missing: &str,
) -> CliResult<nodalync_types::Channel> {
    ctx.ops
        .get_payment_channel(peer_id)?
        .ok_or_else(|| CliError::User(missing.into()))
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

    // Initialize context with network and bootstrap to connect to the network
    let mut ctx = NodeContext::connected(config).await?;

    // Get channel info before closing
    let snapshot = ChannelSnapshot::of(&require_channel(
        &ctx,
        &peer_id,
        "No channel exists with this peer",
    )?);

    // Try cooperative close with signature
    let result = ctx
        .ops
        .close_payment_channel(&peer_id, &ctx.private_key)
        .await?;

    render_close_result(result, snapshot, peer_id_str, format)
}

/// Report the outcome of a cooperative close.
fn render_close_result(
    result: CloseResult,
    snapshot: ChannelSnapshot,
    peer_id_str: &str,
    format: OutputFormat,
) -> CliResult<String> {
    let closed = |operation: &str, final_balances: (u64, u64), transaction_id| ChannelOutput {
        channel_id: snapshot.channel_id.clone(),
        peer_id: peer_id_str.to_string(),
        state: "Closed".to_string(),
        my_balance: final_balances.0,
        their_balance: final_balances.1,
        operation: operation.to_string(),
        transaction_id,
    };
    match result {
        CloseResult::Success {
            transaction_id,
            final_balances,
        } => Ok(closed("closed (on-chain)", final_balances, Some(transaction_id)).render(format)),
        CloseResult::SuccessOffChain { final_balances } => {
            Ok(closed("closed (off-chain)", final_balances, None).render(format))
        }
        CloseResult::PeerUnresponsive { suggestion } => Ok(peer_unresponsive_message(
            &snapshot,
            peer_id_str,
            &suggestion,
            format,
        )),
        CloseResult::OnChainFailed { error } => Err(CliError::User(format!(
            "On-chain close failed: {}. Try using 'nodalync dispute-channel {}' instead.",
            error, peer_id_str
        ))),
    }
}

/// Report a cooperative close the peer did not answer, with the suggestion to dispute.
fn peer_unresponsive_message(
    snapshot: &ChannelSnapshot,
    peer_id_str: &str,
    suggestion: &str,
    format: OutputFormat,
) -> String {
    if format == OutputFormat::Json {
        return serde_json::json!({
            "status": "peer_unresponsive",
            "channel_id": snapshot.channel_id,
            "peer_id": peer_id_str,
            "my_balance": snapshot.my_balance,
            "their_balance": snapshot.their_balance,
            "suggestion": suggestion
        })
        .to_string();
    }
    format!(
        "Cooperative close failed: peer unresponsive\n\n\
        Channel: {}\n\
        Your balance: {} tinybars\n\
        Their balance: {} tinybars\n\n\
        {}",
        snapshot.channel_id, snapshot.my_balance, snapshot.their_balance, suggestion
    )
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

    // Get channel info; it must not already be disputed
    let snapshot = ChannelSnapshot::of(&require_disputable_channel(&ctx, &peer_id)?);

    // Initiate dispute
    let tx_id = ctx
        .ops
        .dispute_payment_channel(&peer_id, &ctx.private_key)
        .await?;

    Ok(dispute_message(&snapshot, &tx_id, peer_id_str, format))
}

/// The channel with `peer_id`, which must not already have a pending dispute.
fn require_disputable_channel(
    ctx: &NodeContext,
    peer_id: &PeerId,
) -> CliResult<nodalync_types::Channel> {
    let channel = require_channel(ctx, peer_id, "No channel exists with this peer")?;

    // Check if already has pending dispute
    if channel.pending_dispute.is_some() {
        return Err(CliError::User(
            "Channel already has a pending dispute. Use 'nodalync resolve-dispute' when the waiting period is over.".into()
        ));
    }
    Ok(channel)
}

/// Report a dispute initiated on-chain.
fn dispute_message(
    snapshot: &ChannelSnapshot,
    tx_id: &str,
    peer_id_str: &str,
    format: OutputFormat,
) -> String {
    if format == OutputFormat::Json {
        return serde_json::json!({
            "status": "dispute_initiated",
            "channel_id": snapshot.channel_id,
            "peer_id": peer_id_str,
            "transaction_id": tx_id,
            "my_balance": snapshot.my_balance,
            "their_balance": snapshot.their_balance,
            "waiting_period": "24 hours"
        })
        .to_string();
    }
    format!(
        "Dispute initiated\n\n\
        Channel: {}\n\
        Transaction: {}\n\
        Your balance: {} tinybars\n\
        Their balance: {} tinybars\n\n\
        Waiting period: 24 hours\n\
        Run 'nodalync resolve-dispute {}' after the waiting period to finalize.",
        snapshot.channel_id, tx_id, snapshot.my_balance, snapshot.their_balance, peer_id_str
    )
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
    check_dispute_resolvable(&ctx, &peer_id)?;

    // Get channel info
    let snapshot = ChannelSnapshot::of(&require_channel(&ctx, &peer_id, "Channel not found")?);

    // Resolve the dispute
    let tx_id = ctx.ops.resolve_dispute(&peer_id).await?;

    Ok(resolution_message(&snapshot, &tx_id, peer_id_str, format))
}

/// A dispute must be pending and its waiting period over.
fn check_dispute_resolvable(ctx: &NodeContext, peer_id: &PeerId) -> CliResult<()> {
    match ctx.ops.get_pending_dispute_status(peer_id)? {
        None => Err(CliError::User("No pending dispute for this channel".into())),
        Some((_, false, remaining_ms)) => {
            let hours = remaining_ms as f64 / (60.0 * 60.0 * 1000.0);
            Err(CliError::User(format!(
                "Dispute waiting period not yet complete. {:.1} hours remaining.",
                hours
            )))
        }
        // Can resolve
        Some((_, true, _)) => Ok(()),
    }
}

/// Report a resolved dispute.
fn resolution_message(
    snapshot: &ChannelSnapshot,
    tx_id: &str,
    peer_id_str: &str,
    format: OutputFormat,
) -> String {
    if format == OutputFormat::Json {
        return serde_json::json!({
            "status": "dispute_resolved",
            "channel_id": snapshot.channel_id,
            "peer_id": peer_id_str,
            "transaction_id": tx_id,
            "final_balance_ours": snapshot.my_balance,
            "final_balance_theirs": snapshot.their_balance
        })
        .to_string();
    }
    format!(
        "Dispute resolved\n\n\
        Channel: {}\n\
        Transaction: {}\n\
        Your final balance: {} tinybars\n\
        Their final balance: {} tinybars\n\n\
        Channel is now closed.",
        snapshot.channel_id, tx_id, snapshot.my_balance, snapshot.their_balance
    )
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
        bytes[i] = decode_hex_pair(chunk)?;
    }

    Ok(PeerId(bytes))
}

/// Decode two hex characters into a byte.
fn decode_hex_pair(chunk: &[u8]) -> CliResult<u8> {
    let hex_pair =
        std::str::from_utf8(chunk).map_err(|_| CliError::User("Invalid peer ID format".into()))?;
    u8::from_str_radix(hex_pair, 16)
        .map_err(|_| CliError::User(format!("Invalid hex in peer ID: {}", hex_pair)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channel_messages() {
        let snapshot = ChannelSnapshot {
            channel_id: "ch1".into(),
            my_balance: 70,
            their_balance: 30,
        };
        assert_eq!(
            peer_unresponsive_message(&snapshot, "ndl1peer", "Use dispute", OutputFormat::Human),
            "Cooperative close failed: peer unresponsive\n\nChannel: ch1\nYour balance: 70 tinybars\nTheir balance: 30 tinybars\n\nUse dispute"
        );
        let json: serde_json::Value = serde_json::from_str(&dispute_message(
            &snapshot,
            "tx9",
            "ndl1peer",
            OutputFormat::Json,
        ))
        .unwrap();
        assert_eq!(json["status"], "dispute_initiated");
        assert_eq!(json["transaction_id"], "tx9");
        assert!(
            resolution_message(&snapshot, "tx9", "p", OutputFormat::Human)
                .ends_with("Channel is now closed.")
        );
        assert!(channel_deposit_tinybars(99.0).is_err());
        assert_eq!(channel_deposit_tinybars(100.0).unwrap(), 10_000_000_000);
    }

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
