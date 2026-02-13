//! Tauri IPC commands for content discovery, search, and query.
//!
//! These commands expose the Nodalync protocol's content discovery
//! and query flows to the React frontend. They cover:
//! - Network search (local + cached + peer)
//! - Content preview (metadata + L1 summary)
//! - Content query (full retrieval with payment)
//! - Unpublish
//! - Version history

use std::sync::Arc;
use nodalync_crypto::Hash;
use nodalync_types::ContentType;
use serde::{Deserialize, Serialize};
use tauri::State;
use tokio::sync::Mutex;
use tracing::info;

use crate::fee_commands::{self, FeeConfig, TransactionStatus};
use crate::protocol::ProtocolState;

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Parse a hex string into a Hash.
fn parse_hash(hex: &str) -> Result<Hash, String> {
    let hex = hex.trim();
    if hex.len() != 64 {
        return Err(format!(
            "Invalid hash length: expected 64 hex chars, got {}",
            hex.len()
        ));
    }
    let bytes: Vec<u8> = (0..64)
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16))
        .collect::<Result<Vec<u8>, _>>()
        .map_err(|e| format!("Invalid hex: {}", e))?;
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Ok(Hash(arr))
}

// ─── Response Types ──────────────────────────────────────────────────────────

/// Search result returned to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub hash: String,
    pub title: String,
    pub content_type: String,
    pub price: u64,
    pub owner: String,
    pub mention_count: u32,
    pub primary_topics: Vec<String>,
    pub summary: String,
    pub total_queries: u64,
    pub source: String, // "local", "cached", "peer"
}

/// Preview response for the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewResult {
    pub hash: String,
    pub title: String,
    pub content_type: String,
    pub size: u64,
    pub price: u64,
    pub visibility: String,
    pub owner: String,
    pub mention_count: u32,
    pub primary_topics: Vec<String>,
    pub summary: String,
    pub version: u32,
    /// If content is remote, the provider's libp2p peer ID for dialing.
    pub provider_peer_id: Option<String>,
}

/// Full query response for the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResult {
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
}

/// Version info for the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionItem {
    pub hash: String,
    pub number: u32,
    pub timestamp: u64,
    pub visibility: String,
    pub price: u64,
}

// ─── Search Command ──────────────────────────────────────────────────────────

/// Search the network for content matching a query.
///
/// Combines results from local manifests, cached announcements, and
/// connected peers via the SEARCH protocol. Results are deduplicated.
#[tauri::command]
pub async fn search_network(
    query: String,
    content_type: Option<String>,
    limit: Option<u32>,
    protocol: State<'_, Arc<Mutex<Option<ProtocolState>>>>,
) -> Result<Vec<SearchResult>, String> {
    let mut guard = protocol.lock().await;
    let state = guard
        .as_mut()
        .ok_or("Node not initialized — unlock first")?;

    let ct = content_type.and_then(|s| match s.to_lowercase().as_str() {
        "l0" | "original" => Some(ContentType::L0),
        "l1" | "summary" => Some(ContentType::L1),
        "l2" | "graph" => Some(ContentType::L2),
        _ => None,
    });

    let max_results = limit.unwrap_or(20).min(100);

    info!("Network search: query='{}', limit={}", query, max_results);

    let results = state
        .ops
        .search_network(&query, ct, max_results)
        .await
        .map_err(|e| format!("Search failed: {}", e))?;

    let items: Vec<SearchResult> = results
        .into_iter()
        .map(|r| SearchResult {
            hash: r.hash.to_string(),
            title: r.title,
            content_type: format!("{:?}", r.content_type),
            price: r.price,
            owner: r.owner.to_string(),
            mention_count: r.l1_summary.mention_count,
            primary_topics: r.l1_summary.primary_topics.clone(),
            summary: r.l1_summary.summary.clone(),
            total_queries: r.total_queries,
            source: r.source.to_string(),
        })
        .collect();

    info!("Search returned {} results", items.len());
    Ok(items)
}

// ─── Preview Command ─────────────────────────────────────────────────────────

/// Preview content metadata and L1 summary without retrieving full content.
///
/// Works for local content, cached announcements, and DHT lookups.
/// Returns enough information for the UI to display a content card.
#[tauri::command]
pub async fn preview_content(
    hash: String,
    protocol: State<'_, Arc<Mutex<Option<ProtocolState>>>>,
) -> Result<PreviewResult, String> {
    let mut guard = protocol.lock().await;
    let state = guard
        .as_mut()
        .ok_or("Node not initialized — unlock first")?;

    let hash_parsed = parse_hash(&hash)?;

    let preview = state
        .ops
        .preview_content(&hash_parsed)
        .await
        .map_err(|e| format!("Preview failed: {}", e))?;

    Ok(PreviewResult {
        hash: preview.manifest.hash.to_string(),
        title: preview.manifest.metadata.title.clone(),
        content_type: format!("{:?}", preview.manifest.content_type),
        size: preview.manifest.metadata.content_size,
        price: preview.manifest.economics.price,
        visibility: format!("{:?}", preview.manifest.visibility),
        owner: preview.manifest.owner.to_string(),
        mention_count: preview.l1_summary.mention_count,
        primary_topics: preview.l1_summary.primary_topics.clone(),
        summary: preview.l1_summary.summary.clone(),
        version: preview.manifest.version.number,
        provider_peer_id: preview.provider_peer_id,
    })
}

// ─── Query Command ───────────────────────────────────────────────────────────

/// Query and retrieve full content with payment.
///
/// For own content: returns immediately (no payment).
/// For remote content: sends payment via channel and fetches from network.
/// Content is cached locally after successful retrieval.
#[tauri::command]
pub async fn query_content(
    hash: String,
    payment_amount: Option<f64>,
    protocol: State<'_, Arc<Mutex<Option<ProtocolState>>>>,
) -> Result<QueryResult, String> {
    let mut guard = protocol.lock().await;
    let state = guard
        .as_mut()
        .ok_or("Node not initialized — unlock first")?;

    let hash_parsed = parse_hash(&hash)?;

    // Convert NDL to tinybars
    let amount = payment_amount
        .map(|p| (p * 100_000_000.0) as u64)
        .unwrap_or(0);

    // Load fee config and calculate app fee
    let fee_config = FeeConfig::load(&state.data_dir);
    let (app_fee, _total) = fee_commands::calculate_app_fee(amount, fee_config.rate);

    info!(
        "Querying content: hash={}, content_cost={}, app_fee={}, total={}",
        hash, amount, app_fee, amount + app_fee
    );

    let response = state
        .ops
        .query_content(&hash_parsed, amount, None)
        .await
        .map_err(|e| format!("Query failed: {}", e))?;

    // Try to decode content as UTF-8 text
    let content_text = String::from_utf8(response.content.clone()).ok();

    // Record the transaction with fee breakdown
    let tx_status = if amount == 0 {
        TransactionStatus::Free
    } else {
        TransactionStatus::Pending
    };

    let tx_record = fee_commands::record_transaction(
        &state.data_dir,
        &hash,
        &response.manifest.metadata.title,
        response.receipt.amount,
        app_fee,
        &response.manifest.owner.to_string(),
        tx_status,
    );

    let transaction_id = tx_record.ok().map(|r| r.id);

    Ok(QueryResult {
        hash: response.manifest.hash.to_string(),
        title: response.manifest.metadata.title.clone(),
        content_type: format!("{:?}", response.manifest.content_type),
        content_text,
        content_size: response.content.len() as u64,
        price_paid: response.receipt.amount,
        app_fee,
        total_cost: response.receipt.amount + app_fee,
        receipt_id: response.receipt.payment_id.to_string(),
        transaction_id,
    })
}

// ─── Unpublish Command ───────────────────────────────────────────────────────

/// Unpublish content from the network.
///
/// Sets visibility to Private and removes from DHT.
/// Content remains on the local node but is no longer discoverable.
#[tauri::command]
pub async fn unpublish_content(
    hash: String,
    protocol: State<'_, Arc<Mutex<Option<ProtocolState>>>>,
) -> Result<(), String> {
    let mut guard = protocol.lock().await;
    let state = guard
        .as_mut()
        .ok_or("Node not initialized — unlock first")?;

    let hash_parsed = parse_hash(&hash)?;

    state
        .ops
        .unpublish_content(&hash_parsed)
        .await
        .map_err(|e| format!("Unpublish failed: {}", e))?;

    info!("Unpublished content: {}", hash);
    Ok(())
}

// ─── Version History Command ─────────────────────────────────────────────────

/// Get version history for content.
///
/// Returns all versions of a content item (using the root hash).
#[tauri::command]
pub async fn get_content_versions(
    hash: String,
    protocol: State<'_, Arc<Mutex<Option<ProtocolState>>>>,
) -> Result<Vec<VersionItem>, String> {
    let guard = protocol.lock().await;
    let state = guard
        .as_ref()
        .ok_or("Node not initialized — unlock first")?;

    let hash_parsed = parse_hash(&hash)?;

    let versions = state
        .ops
        .get_content_versions(&hash_parsed)
        .map_err(|e| format!("Failed to get versions: {}", e))?;

    let items: Vec<VersionItem> = versions
        .into_iter()
        .map(|v| VersionItem {
            hash: v.hash.to_string(),
            number: v.number,
            timestamp: v.timestamp,
            visibility: format!("{:?}", v.visibility),
            price: v.price,
        })
        .collect();

    Ok(items)
}
