// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use nodalync_graph::L2GraphDB;
use std::sync::{Arc, Mutex as StdMutex};
use tauri::Manager;
use tokio::sync::Mutex as TokioMutex;
use tracing::info;

mod channel_commands;
mod discovery_commands;
mod event_loop;
mod fee_commands;
mod graph_commands;
mod health_monitor;
mod hip991_commands;
mod invite;
mod network_commands;
mod peer_store;
mod protocol;
mod publish_commands;
mod seed_store;
mod synthesis_commands;

use channel_commands::*;
use discovery_commands::*;
use fee_commands::*;
use graph_commands::*;
use hip991_commands::*;
use network_commands::*;
use publish_commands::*;
use synthesis_commands::*;

/// Use an explicit graph database when requested, otherwise keep it with node data.
fn resolve_db_path() -> std::path::PathBuf {
    graph_db_path(
        std::env::var_os("NODALYNC_GRAPH_DB"),
        &protocol::ProtocolState::default_data_dir(),
    )
}

fn graph_db_path(
    override_path: Option<std::ffi::OsString>,
    data_dir: &std::path::Path,
) -> std::path::PathBuf {
    override_path
        .filter(|path| !path.is_empty())
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| data_dir.join("studio").join("knowledge.db"))
}

fn main() {
    tracing_subscriber::fmt::init();

    let db_path = resolve_db_path();
    info!("Nodalync Studio starting — DB: {}", db_path.display());
    if let Some(parent) = db_path.parent().filter(|path| !path.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).expect("Failed to create graph database directory");
    }

    let graph_db = L2GraphDB::new(&db_path).expect("Failed to open graph database");
    info!("Graph database opened successfully");

    // Protocol state starts as None — user must init or unlock.
    // Wrapped in Arc so the network event loop can hold a clone.
    let protocol_state: Arc<TokioMutex<Option<protocol::ProtocolState>>> =
        Arc::new(TokioMutex::new(None));

    // Event loop handle — populated when the network starts, cleared on stop
    let event_loop_handle: TokioMutex<Option<event_loop::EventLoopHandle>> = TokioMutex::new(None);

    // Health monitor handle — populated when the network starts
    let health_monitor_handle: TokioMutex<Option<health_monitor::HealthMonitorHandle>> =
        TokioMutex::new(None);

    // Shared health state — read by get_network_health IPC command
    let shared_health = health_monitor::new_shared_health();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
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
            // Source-backed, human-authored private L3 content.
            list_synthesis_sources,
            save_synthesis,
            get_synthesis_details,
            // Protocol commands (Phase 2 — publish flow)
            check_identity,
            init_node,
            unlock_node,
            get_identity,
            publish_file,
            publish_text,
            list_content,
            get_content_details,
            read_content_text,
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
            // HIP-991 commands (D2 — native on-chain fee via HCS topics)
            get_hip991_status,
            configure_hip991,
            create_fee_topic,
            submit_to_topic,
            get_topic_revenue,
            get_topic_details,
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
            // Content import (L0 add without network publish)
            add_content,
            add_text_content,
            // Connection invites
            generate_invite,
            accept_invite,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod startup_tests {
    use super::graph_db_path;
    use std::path::Path;

    #[test]
    fn default_graph_database_stays_inside_node_data() {
        let data_dir = Path::new("isolated-node");
        assert_eq!(
            graph_db_path(None, data_dir),
            data_dir.join("studio").join("knowledge.db")
        );
        assert_eq!(
            graph_db_path(Some("".into()), data_dir),
            data_dir.join("studio").join("knowledge.db")
        );
    }

    #[test]
    fn explicit_graph_database_is_preserved() {
        assert_eq!(
            graph_db_path(Some("chosen.db".into()), Path::new("isolated-node")),
            Path::new("chosen.db")
        );
    }
}
