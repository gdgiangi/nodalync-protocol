// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use nodalync_graph::L2GraphDB;
use std::sync::Mutex;
use tauri::Manager;
use tracing::info;

mod graph_commands;

use graph_commands::*;

/// Resolve the graph database path.
/// Checks (in order): env var, default location relative to exe, fallback.
fn resolve_db_path() -> String {
    // 1. Environment variable override
    if let Ok(path) = std::env::var("NODALYNC_GRAPH_DB") {
        return path;
    }

    // 2. Well-known location: repo root
    let repo_db = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent() // apps/desktop
        .and_then(|p| p.parent()) // apps
        .and_then(|p| p.parent()) // repo root
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

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(move |app| {
            info!("Setting up Tauri application");
            app.manage(Mutex::new(graph_db));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_graph_data,
            get_subgraph,
            search_entities,
            get_graph_stats,
            get_context,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
