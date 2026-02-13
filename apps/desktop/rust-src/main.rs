// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use nodalync_graph::L2GraphDB;
use std::sync::{Arc, Mutex as StdMutex};
use tokio::sync::Mutex as TokioMutex;
use tauri::Manager;
use tracing::info;

mod channel_commands;
mod discovery_commands;
mod event_loop;
mod fee_commands;
mod graph_commands;
mod health_monitor;
mod network_commands;
mod peer_store;
mod protocol;
mod publish_commands;
mod seed_store;

use channel_commands::*;
use discovery_commands::*;
use fee_commands::*;
use graph_commands::*;
use network_commands::*;
use publish_commands::*;

/// Resolve the graph database path.
/// Checks (in order): env var, default location relative to exe, fallback.
fn resolve_db_path() -> String {
    // 1. Environment variable override
    if let Ok(path) = std::env::var("NODALYNC_GRAPH_DB") {
        return path;
    }

    // 2. Well-known location: repo root
    // CARGO_MANIFEST_DIR = .../nodalync-protocol/apps/desktop
    // .parent() → .../nodalync-protocol/apps
    // .parent() → .../nodalync-protocol  (repo root)
    let repo_db = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent() // apps/desktop → apps
        .and_then(|p| p.parent()) // apps → repo root
        .map(|p| p.join("obsidian_l2_graph.db"));

    if let Some(path) = repo_db {
        if path.exists() {
            return path.to_string_lossy().to_string();
        }
    }

    // 3. Fallback: current directory
    "obsidian_l2_graph.db".to_string()
}

fn main() {
    tracing_subscriber::fmt::init();

    let db_path = resolve_db_path();
    info!("Nodalync Studio starting — DB: {}", db_path);

    let graph_db = L2GraphDB::new(&db_path).expect("Failed to open graph database");
    info!("Graph database opened successfully");

    // Protocol state starts as None — user must init or unlock.
    // Wrapped in Arc so the network event loop can hold a clone.
    let protocol_state: Arc<TokioMutex<Option<protocol::ProtocolState>>> =
        Arc::new(TokioMutex::new(None));

    // Event loop handle — populated when the network starts, cleared on stop
    let event_loop_handle: TokioMutex<Option<event_loop::EventLoopHandle>> =
        TokioMutex::new(None);

    // Health monitor handle — populated when the network starts
    let health_monitor_handle: TokioMutex<Option<health_monitor::HealthMonitorHandle>> =
        TokioMutex::new(None);

    // Shared health state — read by get_network_health IPC command
    let shared_health = health_monitor::new_shared_health();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(move |app| {
            info!("Setting up Tauri application");
            app.manage(StdMutex::new(graph_db));
            app.manage(protocol_state);
            app.manage(event_loop_handle);
            app.manage(health_monitor_handle);
            app.manage(shared_health);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // L2 Graph commands (Phase 1)
            get_graph_data,
            get_subgraph,
            search_entities,
            get_graph_stats,
            get_context,
            // L1 Extraction pipeline (bridges L0 → L1 → L2)
            extract_mentions,
            // L3 Synthesis commands
            create_l3_summary,
            get_l3_summaries,
            get_entity_content_links,
            // Protocol commands (Phase 2 — publish flow)
            check_identity,
            init_node,
            unlock_node,
            get_identity,
            publish_file,
            publish_text,
            list_content,
            get_content_details,
            delete_content,
            get_node_status,
            start_network,
            stop_network,
            get_peers,
            // Discovery commands (Phase 2 — content discovery)
            search_network,
            preview_content,
            query_content,
            unpublish_content,
            get_content_versions,
            // Network commands (Phase 2 — peering)
            get_network_info,
            start_network_configured,
            dial_peer,
            // Fee commands (D2 — application-level fee)
            get_fee_config,
            set_fee_rate,
            get_transaction_history,
            get_fee_quote,
            // Peer persistence commands
            auto_start_network,
            save_known_peers,
            get_known_peers,
            add_known_peer,
            // Network maintenance
            reannounce_content,
            // NAT traversal status
            get_nat_status,
            // Health monitor
            get_network_health,
            // Seed node management
            get_seed_nodes,
            add_seed_node,
            remove_seed_node,
            // Network diagnostics
            diagnose_network,
            // Resource management
            get_resource_stats,
            // Channel management
            open_channel,
            close_channel,
            list_channels,
            get_channel,
            check_channel,
            auto_open_and_query,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
