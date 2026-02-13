//! Tauri IPC commands for payment channel management.
//!
//! These commands expose the Nodalync protocol's payment channel operations
//! to the React frontend. Without channels, paid content queries fail.
//!
//! Flow:
//! 1. User discovers paid content via search_network
//! 2. User calls get_fee_quote to see cost
//! 3. User calls open_channel (if none exists with that peer)
//! 4. User calls query_content — payment goes through the channel
//! 5. Optionally: close_channel when done

use std::sync::Arc;
use nodalync_store::ChannelStore;
use serde::{Deserialize, Serialize};
use tauri::State;
use tokio::sync::Mutex;
use tracing::info;

use crate::protocol::ProtocolState;

// ─── Response Types ──────────────────────────────────────────────────────────

/// Channel info returned to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelInfo {
    pub channel_id: String,
    pub peer_id: String,
    pub libp2p_peer_id: Option<String>,
    pub state: String,
    pub my_balance: u64,
    pub their_balance: u64,
    pub my_balance_hbar: f64,
    pub their_balance_hbar: f64,
    pub nonce: u64,
    pub pending_payments: u32,
    pub has_pending_close: bool,
    pub has_pending_dispute: bool,
    pub funding_tx_id: Option<String>,
    pub last_update: u64,
}

/// Result of opening a channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenChannelResult {
    pub channel: ChannelInfo,
    pub nodalync_peer_id: String,
}

/// Result of closing a channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloseChannelResult {
    pub status: String,
    pub channel_id: String,
    pub my_final_balance: u64,
    pub their_final_balance: u64,
    pub transaction_id: Option<String>,
    pub message: Option<String>,
}

/// Summary of all channels.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelListResult {
    pub channels: Vec<ChannelInfo>,
    pub total: usize,
    pub open_count: usize,
    pub total_deposited: u64,
    pub total_deposited_hbar: f64,
}

/// Result of auto_open_and_query — seamless paid content retrieval.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoQueryResult {
    pub hash: String,
    pub title: String,
    pub content_type: String,
    pub content_text: Option<String>,
    pub content_size: u64,
    pub price_paid: u64,
    pub app_fee: u64,
    pub total_cost: u64,
    pub receipt_id: String,
    pub transaction_id: Option<String>,
    pub channel_opened: bool,
    pub channel_id: Option<String>,
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

const TINYBARS_PER_HBAR: f64 = 100_000_000.0;

fn tinybars_to_hbar(tinybars: u64) -> f64 {
    tinybars as f64 / TINYBARS_PER_HBAR
}

fn hbar_to_tinybars(hbar: f64) -> u64 {
    (hbar * TINYBARS_PER_HBAR) as u64
}

fn channel_to_info(
    peer_id: &nodalync_crypto::PeerId,
    channel: &nodalync_types::Channel,
    libp2p_peer_id: Option<String>,
) -> ChannelInfo {
    ChannelInfo {
        channel_id: channel.channel_id.to_string(),
        peer_id: nodalync_crypto::peer_id_to_string(peer_id),
        libp2p_peer_id,
        state: format!("{:?}", channel.state),
        my_balance: channel.my_balance,
        their_balance: channel.their_balance,
        my_balance_hbar: tinybars_to_hbar(channel.my_balance),
        their_balance_hbar: tinybars_to_hbar(channel.their_balance),
        nonce: channel.nonce,
        pending_payments: channel.pending_payments.len() as u32,
        has_pending_close: channel.pending_close.is_some(),
        has_pending_dispute: channel.pending_dispute.is_some(),
        funding_tx_id: channel.funding_tx_id.clone(),
        last_update: channel.last_update,
    }
}

/// Parse a Nodalync peer ID from hex or base58.
fn parse_nodalync_peer_id(s: &str) -> Result<nodalync_crypto::PeerId, String> {
    // Try base58 first
    if s.starts_with("ndl1") {
        return nodalync_crypto::peer_id_from_string(s)
            .map_err(|e| format!("Invalid peer ID: {}", e));
    }

    // Strip optional ndl prefix
    let hex_str = s.strip_prefix("ndl").unwrap_or(s);

    if hex_str.len() != 40 {
        return Err(format!(
            "Peer ID must be base58 (ndl1...) or 40 hex chars, got {} chars",
            hex_str.len()
        ));
    }

    let mut bytes = [0u8; 20];
    for (i, chunk) in hex_str.as_bytes().chunks(2).enumerate() {
        if i >= 20 {
            return Err("Invalid peer ID format".into());
        }
        let hex_pair =
            std::str::from_utf8(chunk).map_err(|_| "Invalid peer ID format".to_string())?;
        bytes[i] = u8::from_str_radix(hex_pair, 16)
            .map_err(|_| format!("Invalid hex in peer ID: {}", hex_pair))?;
    }

    Ok(nodalync_crypto::PeerId(bytes))
}

// ─── IPC Commands ────────────────────────────────────────────────────────────

/// Open a payment channel with a peer.
///
/// Accepts either a libp2p peer ID (12D3KooW...) or a Nodalync peer ID (ndl1... or hex).
/// For libp2p peer IDs, the peer must be connected or dialable.
///
/// deposit_hbar: Amount to deposit in HBAR (e.g. 100.0)
#[tauri::command]
pub async fn open_channel(
    peer_id: String,
    deposit_hbar: f64,
    protocol: State<'_, Arc<Mutex<Option<ProtocolState>>>>,
) -> Result<OpenChannelResult, String> {
    let mut guard = protocol.lock().await;
    let state = guard
        .as_mut()
        .ok_or("Node not initialized — unlock first")?;

    if deposit_hbar < 1.0 {
        return Err("Minimum deposit is 1.0 HBAR".into());
    }

    let deposit_tinybars = hbar_to_tinybars(deposit_hbar);

    info!(
        "Opening channel with peer: {}, deposit: {} HBAR ({} tinybars)",
        peer_id, deposit_hbar, deposit_tinybars
    );

    // Check if this is a libp2p peer ID (12D3KooW...)
    if peer_id.starts_with("12D3KooW") {
        let libp2p_peer: nodalync_net::PeerId = peer_id
            .parse()
            .map_err(|e| format!("Invalid libp2p peer ID: {}", e))?;

        let (channel, nodalync_peer_id) = state
            .ops
            .open_payment_channel_to_libp2p(libp2p_peer, deposit_tinybars)
            .await
            .map_err(|e| format!("Failed to open channel: {}", e))?;

        let info = channel_to_info(
            &nodalync_peer_id,
            &channel,
            Some(peer_id),
        );

        info!(
            "Channel opened: {} with peer {}",
            info.channel_id,
            nodalync_crypto::peer_id_to_string(&nodalync_peer_id)
        );

        return Ok(OpenChannelResult {
            nodalync_peer_id: nodalync_crypto::peer_id_to_string(&nodalync_peer_id),
            channel: info,
        });
    }

    // Nodalync peer ID
    let nodalync_peer = parse_nodalync_peer_id(&peer_id)?;

    let channel = state
        .ops
        .open_payment_channel(&nodalync_peer, deposit_tinybars)
        .await
        .map_err(|e| format!("Failed to open channel: {}", e))?;

    let info = channel_to_info(&nodalync_peer, &channel, None);

    info!("Channel opened: {} with peer {}", info.channel_id, peer_id);

    Ok(OpenChannelResult {
        nodalync_peer_id: peer_id,
        channel: info,
    })
}

/// Close a payment channel cooperatively.
///
/// Attempts cooperative close with the peer. If the peer is unresponsive,
/// returns a suggestion to use dispute_channel instead.
#[tauri::command]
pub async fn close_channel(
    peer_id: String,
    protocol: State<'_, Arc<Mutex<Option<ProtocolState>>>>,
) -> Result<CloseChannelResult, String> {
    let mut guard = protocol.lock().await;
    let state = guard
        .as_mut()
        .ok_or("Node not initialized — unlock first")?;

    let nodalync_peer = parse_nodalync_peer_id(&peer_id)?;

    // Get private key for signing the close
    let private_key = state
        .ops
        .private_key()
        .cloned()
        .ok_or("Private key required for channel close")?;

    info!("Closing channel with peer: {}", peer_id);

    let result = state
        .ops
        .close_payment_channel(&nodalync_peer, &private_key)
        .await
        .map_err(|e| format!("Failed to close channel: {}", e))?;

    use nodalync_ops::CloseResult;
    match result {
        CloseResult::Success {
            transaction_id,
            final_balances,
        } => Ok(CloseChannelResult {
            status: "closed".into(),
            channel_id: String::new(), // Could look up, but channel is closed
            my_final_balance: final_balances.0,
            their_final_balance: final_balances.1,
            transaction_id: Some(transaction_id),
            message: Some("Channel closed on-chain".into()),
        }),
        CloseResult::SuccessOffChain { final_balances } => Ok(CloseChannelResult {
            status: "closed_offchain".into(),
            channel_id: String::new(),
            my_final_balance: final_balances.0,
            their_final_balance: final_balances.1,
            transaction_id: None,
            message: Some("Channel closed (off-chain)".into()),
        }),
        CloseResult::PeerUnresponsive { suggestion } => Ok(CloseChannelResult {
            status: "peer_unresponsive".into(),
            channel_id: String::new(),
            my_final_balance: 0,
            their_final_balance: 0,
            transaction_id: None,
            message: Some(suggestion),
        }),
        CloseResult::OnChainFailed { error } => Ok(CloseChannelResult {
            status: "on_chain_failed".into(),
            channel_id: String::new(),
            my_final_balance: 0,
            their_final_balance: 0,
            transaction_id: None,
            message: Some(format!("On-chain close failed: {}", error)),
        }),
    }
}

/// List all payment channels.
#[tauri::command]
pub async fn list_channels(
    protocol: State<'_, Arc<Mutex<Option<ProtocolState>>>>,
) -> Result<ChannelListResult, String> {
    let guard = protocol.lock().await;
    let state = guard
        .as_ref()
        .ok_or("Node not initialized — unlock first")?;

    let channels = state
        .ops
        .state
        .channels
        .list_open()
        .map_err(|e| format!("Failed to list channels: {}", e))?;

    let infos: Vec<ChannelInfo> = channels
        .iter()
        .map(|(peer_id, channel)| {
            // Try to find libp2p peer ID from network mapping
            let libp2p_id = state
                .ops
                .network()
                .and_then(|n| n.libp2p_peer_id(peer_id))
                .map(|p| p.to_string());
            channel_to_info(peer_id, channel, libp2p_id)
        })
        .collect();

    let open_count = infos.iter().filter(|c| c.state == "Open").count();
    let total_deposited: u64 = infos.iter().map(|c| c.my_balance).sum();

    info!("Listed {} channels ({} open)", infos.len(), open_count);

    Ok(ChannelListResult {
        total: infos.len(),
        open_count,
        total_deposited,
        total_deposited_hbar: tinybars_to_hbar(total_deposited),
        channels: infos,
    })
}

/// Get details for a specific channel by peer ID.
#[tauri::command]
pub async fn get_channel(
    peer_id: String,
    protocol: State<'_, Arc<Mutex<Option<ProtocolState>>>>,
) -> Result<Option<ChannelInfo>, String> {
    let guard = protocol.lock().await;
    let state = guard
        .as_ref()
        .ok_or("Node not initialized — unlock first")?;

    let nodalync_peer = parse_nodalync_peer_id(&peer_id)?;

    let channel = state
        .ops
        .get_payment_channel(&nodalync_peer)
        .map_err(|e| format!("Failed to get channel: {}", e))?;

    Ok(channel.map(|ch| {
        let libp2p_id = state
            .ops
            .network()
            .and_then(|n| n.libp2p_peer_id(&nodalync_peer))
            .map(|p| p.to_string());
        channel_to_info(&nodalync_peer, &ch, libp2p_id)
    }))
}

/// Check if we have an open channel with a peer (by libp2p or Nodalync peer ID).
///
/// Returns the channel info if it exists and is open, null otherwise.
/// Use this before query_content to check if a channel needs to be opened.
#[tauri::command]
pub async fn check_channel(
    peer_id: String,
    protocol: State<'_, Arc<Mutex<Option<ProtocolState>>>>,
) -> Result<Option<ChannelInfo>, String> {
    let guard = protocol.lock().await;
    let state = guard
        .as_ref()
        .ok_or("Node not initialized — unlock first")?;

    // If libp2p peer ID, resolve to Nodalync peer ID
    let nodalync_peer = if peer_id.starts_with("12D3KooW") {
        let libp2p_peer: nodalync_net::PeerId = peer_id
            .parse()
            .map_err(|e| format!("Invalid libp2p peer ID: {}", e))?;

        // Look up Nodalync peer ID from network mapping
        match state.ops.network() {
            Some(network) => match network.nodalync_peer_id(&libp2p_peer) {
                Some(npid) if npid != nodalync_crypto::UNKNOWN_PEER_ID => npid,
                _ => return Ok(None), // Unknown peer, no channel
            },
            None => return Ok(None),
        }
    } else {
        parse_nodalync_peer_id(&peer_id)?
    };

    let channel = state
        .ops
        .get_payment_channel(&nodalync_peer)
        .map_err(|e| format!("Failed to check channel: {}", e))?;

    Ok(channel.and_then(|ch| {
        if ch.is_open() {
            let libp2p_id = state
                .ops
                .network()
                .and_then(|n| n.libp2p_peer_id(&nodalync_peer))
                .map(|p| p.to_string());
            Some(channel_to_info(&nodalync_peer, &ch, libp2p_id))
        } else {
            None
        }
    }))
}

/// Auto-open a channel (if needed) and query content in one call.
///
/// This is the "just works" experience for D3 users:
/// 1. Checks if a channel exists with the content provider
/// 2. If not, opens one with the specified deposit
/// 3. Queries the content with payment
///
/// The frontend calls this instead of managing channels manually.
#[tauri::command]
pub async fn auto_open_and_query(
    hash: String,
    payment_amount: Option<f64>,
    deposit_hbar: Option<f64>,
    protocol: State<'_, Arc<Mutex<Option<ProtocolState>>>>,
) -> Result<AutoQueryResult, String> {
    let mut guard = protocol.lock().await;
    let state = guard
        .as_mut()
        .ok_or("Node not initialized — unlock first")?;

    let hash_hex = hash.trim();
    if hash_hex.len() != 64 {
        return Err(format!(
            "Invalid hash length: expected 64 hex chars, got {}",
            hash_hex.len()
        ));
    }
    let hash_bytes: Vec<u8> = (0..64)
        .step_by(2)
        .map(|i| u8::from_str_radix(&hash_hex[i..i + 2], 16))
        .collect::<Result<Vec<u8>, _>>()
        .map_err(|e| format!("Invalid hex: {}", e))?;
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&hash_bytes);
    let content_hash = nodalync_crypto::Hash(arr);

    // Convert NDL to tinybars
    let amount_tinybars = payment_amount
        .map(|p| (p * TINYBARS_PER_HBAR) as u64)
        .unwrap_or(0);

    let default_deposit = deposit_hbar.unwrap_or(100.0);
    let deposit_tinybars = hbar_to_tinybars(default_deposit);

    info!(
        "Auto-open-and-query: hash={}, payment={} tinybars, deposit={} HBAR",
        hash, amount_tinybars, default_deposit
    );

    // First attempt: try the query directly (may succeed if channel exists or content is free/local)
    let fee_config = crate::fee_commands::FeeConfig::load(&state.data_dir);
    let (app_fee, _) = crate::fee_commands::calculate_app_fee(amount_tinybars, fee_config.rate);

    match state
        .ops
        .query_content(&content_hash, amount_tinybars, None)
        .await
    {
        Ok(response) => {
            let content_text = String::from_utf8(response.content.clone()).ok();
            let tx_status = if amount_tinybars == 0 {
                crate::fee_commands::TransactionStatus::Free
            } else {
                crate::fee_commands::TransactionStatus::Pending
            };
            let tx_record = crate::fee_commands::record_transaction(
                &state.data_dir,
                &hash,
                &response.manifest.metadata.title,
                response.receipt.amount,
                app_fee,
                &response.manifest.owner.to_string(),
                tx_status,
            );

            return Ok(AutoQueryResult {
                hash: response.manifest.hash.to_string(),
                title: response.manifest.metadata.title.clone(),
                content_type: format!("{:?}", response.manifest.content_type),
                content_text,
                content_size: response.content.len() as u64,
                price_paid: response.receipt.amount,
                app_fee,
                total_cost: response.receipt.amount + app_fee,
                receipt_id: response.receipt.payment_id.to_string(),
                transaction_id: tx_record.ok().map(|r| r.id),
                channel_opened: false,
                channel_id: None,
            });
        }
        Err(nodalync_ops::OpsError::ChannelRequired) | Err(nodalync_ops::OpsError::ChannelRequiredWithPeerInfo { .. }) => {
            info!("Channel required — auto-opening with {} HBAR deposit", default_deposit);
        }
        Err(e) => {
            return Err(format!("Query failed: {}", e));
        }
    }

    // Need a channel. Get the provider's peer ID from the preview/announcement.
    let preview = state
        .ops
        .preview_content(&content_hash)
        .await
        .map_err(|e| format!("Failed to preview content for channel setup: {}", e))?;

    let provider_peer_id = preview.provider_peer_id.ok_or(
        "Cannot determine content provider's peer ID. Content may be unavailable.",
    )?;

    // Parse the libp2p peer ID
    let libp2p_peer: nodalync_net::PeerId = provider_peer_id
        .parse()
        .map_err(|e| format!("Invalid provider peer ID: {}", e))?;

    // Open channel with the provider
    let (channel, _nodalync_peer_id) = state
        .ops
        .open_payment_channel_to_libp2p(libp2p_peer, deposit_tinybars)
        .await
        .map_err(|e| format!("Failed to open channel: {}", e))?;

    let channel_id = channel.channel_id.to_string();
    info!("Channel opened: {}", channel_id);

    // Retry the query now that we have a channel
    let response = state
        .ops
        .query_content(&content_hash, amount_tinybars, None)
        .await
        .map_err(|e| format!("Query failed after opening channel: {}", e))?;

    let content_text = String::from_utf8(response.content.clone()).ok();
    let tx_status = if amount_tinybars == 0 {
        crate::fee_commands::TransactionStatus::Free
    } else {
        crate::fee_commands::TransactionStatus::Pending
    };
    let tx_record = crate::fee_commands::record_transaction(
        &state.data_dir,
        &hash,
        &response.manifest.metadata.title,
        response.receipt.amount,
        app_fee,
        &response.manifest.owner.to_string(),
        tx_status,
    );

    Ok(AutoQueryResult {
        hash: response.manifest.hash.to_string(),
        title: response.manifest.metadata.title.clone(),
        content_type: format!("{:?}", response.manifest.content_type),
        content_text,
        content_size: response.content.len() as u64,
        price_paid: response.receipt.amount,
        app_fee,
        total_cost: response.receipt.amount + app_fee,
        receipt_id: response.receipt.payment_id.to_string(),
        transaction_id: tx_record.ok().map(|r| r.id),
        channel_opened: true,
        channel_id: Some(channel_id),
    })
}
