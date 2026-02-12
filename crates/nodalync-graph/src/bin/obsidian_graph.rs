use anyhow::{Context, Result};
use chrono::Utc;
use clap::{Parser, Subcommand};
use nodalync_graph::{
    entity_extraction::{
        entity_from_node_path, parse_frontmatter,
        relationships_from_frontmatter, relationships_from_wikilinks,
        should_exclude_dir, VaultRelationship,
    },
    Entity, L2GraphDB, Relationship,
};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Parser)]
#[command(name = "obsidian-graph")]
#[command(about = "L2 Graph SQLite layer for Obsidian vault knowledge extraction")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize the graph database
    Init {
        /// Path to SQLite database file
        #[arg(short, long, default_value = "obsidian_graph.db")]
        database: PathBuf,
    },
    /// Scan Obsidian vault and extract entities
    Scan {
        /// Path to Obsidian vault directory
        #[arg(short, long)]
        vault_path: PathBuf,
        /// Path to SQLite database file
        #[arg(short, long, default_value = "obsidian_graph.db")]
        database: PathBuf,
        /// Force re-scan of all files (clear DB first)
        #[arg(long)]
        force: bool,
    },
    /// Query the graph database
    Query {
        /// Path to SQLite database file
        #[arg(short, long, default_value = "obsidian_graph.db")]
        database: PathBuf,
        #[command(subcommand)]
        query_type: QueryCommands,
    },
    /// Get database statistics
    Stats {
        /// Path to SQLite database file
        #[arg(short, long, default_value = "obsidian_graph.db")]
        database: PathBuf,
    },
}

#[derive(Subcommand)]
enum QueryCommands {
    /// Get subgraph around an entity
    Subgraph {
        /// Entity ID or label to center on
        entity: String,
        /// Maximum number of hops
        #[arg(short = 'n', long, default_value = "2")]
        max_hops: u32,
        /// Maximum number of results
        #[arg(short, long, default_value = "50")]
        max_results: u32,
    },
    /// Search for entities
    Search {
        /// Search query
        query: String,
        /// Maximum number of results
        #[arg(short, long, default_value = "20")]
        limit: u32,
    },
    /// Get focused context for agents
    Context {
        /// Context query
        query: String,
        /// Maximum number of entities
        #[arg(short, long, default_value = "10")]
        max_entities: u32,
    },
    /// List entities by type
    List {
        /// Entity type to list (Person, Organization, Product, etc.)
        entity_type: String,
        /// Maximum number of results
        #[arg(short, long, default_value = "20")]
        limit: u32,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init { database } => {
            println!("Initializing L2 Graph database at: {}", database.display());
            let _db =
                L2GraphDB::new(&database).context("Failed to initialize database")?;
            println!("✅ Database initialized successfully");
        }

        Commands::Scan {
            vault_path,
            database,
            force,
        } => {
            println!("Scanning Obsidian vault: {}", vault_path.display());
            println!("Database: {}", database.display());
            scan_vault(&vault_path, &database, force)
                .context("Failed to scan vault")?;
        }

        Commands::Query {
            database,
            query_type,
        } => {
            let db =
                L2GraphDB::new(&database).context("Failed to open database")?;

            match query_type {
                QueryCommands::Subgraph {
                    entity,
                    max_hops,
                    max_results,
                } => query_subgraph(&db, &entity, max_hops, max_results)?,
                QueryCommands::Search { query, limit } => {
                    search_entities(&db, &query, limit)?
                }
                QueryCommands::Context { query, max_entities } => {
                    get_context(&db, &query, max_entities)?
                }
                QueryCommands::List { entity_type, limit } => {
                    list_entities(&db, &entity_type, limit)?
                }
            }
        }

        Commands::Stats { database } => {
            let db =
                L2GraphDB::new(&database).context("Failed to open database")?;
            show_stats(&db)?;
        }
    }

    Ok(())
}

// ─── SCAN ────────────────────────────────────────────────────────────────────

fn scan_vault(vault_path: &Path, database_path: &Path, force: bool) -> Result<()> {
    if !vault_path.exists() {
        return Err(anyhow::anyhow!(
            "Vault path does not exist: {}",
            vault_path.display()
        ));
    }

    let db = L2GraphDB::new(database_path)?;

    if force {
        println!("🗑️  Force mode — clearing existing data...");
        db.clear_all()?;
    }

    // ── Phase 1: Register all .md files as content ──────────────────────────
    println!("\n📄 Phase 1: Registering content files...");
    let mut file_registry: HashMap<PathBuf, String> = HashMap::new(); // path → content_id
    let mut total_files = 0u32;
    let mut skipped_hash = 0u32;

    for entry in walk_vault(vault_path) {
        let path = entry.path().to_path_buf();
        if !path.is_file() || path.extension().map_or(true, |e| e != "md") {
            continue;
        }

        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("⚠️  Could not read {}: {}", path.display(), e);
                continue;
            }
        };

        let hash = sha256_hex(&content);

        // Content-hash dedup: skip if unchanged
        if let Ok(Some(existing_id)) = db.content_hash_exists(&hash) {
            file_registry.insert(path, existing_id);
            skipped_hash += 1;
            continue;
        }

        let content_id = db.register_content(&hash, "L0")?;
        file_registry.insert(path, content_id);
        total_files += 1;
    }
    println!(
        "   Registered {} new files ({} unchanged, skipped)",
        total_files, skipped_hash
    );

    // ── Phase 2: Build entity registry from Nodes/ ──────────────────────────
    println!("\n🏷️  Phase 2: Extracting entities from Nodes/ structure...");

    // label_lower → entity_id
    let mut entity_map: HashMap<String, String> = HashMap::new();
    let mut entity_count = 0u32;

    for (path, content_id) in &file_registry {
        if let Some((label, entity_type)) = entity_from_node_path(path, vault_path) {
            let label_lower = label.to_lowercase();

            if let Some(existing_id) = entity_map.get(&label_lower) {
                // Entity already registered, link to additional source
                db.link_entity_source(existing_id, content_id)?;
                db.increment_source_count(existing_id)?;
                continue;
            }

            let content = fs::read_to_string(path).unwrap_or_default();
            let fm = parse_frontmatter(&content);

            // Determine entity type: frontmatter `type` overrides folder-based
            let final_type = fm
                .as_ref()
                .and_then(|f| f.note_type.as_deref())
                .map(|t| capitalize_type(t))
                .unwrap_or(entity_type.clone());

            // Build description from role and first paragraph
            let description = build_description(&content, fm.as_ref());

            let entity_id = db.next_entity_id()?;
            let now = Utc::now();

            let mut aliases = Vec::new();
            // Add tags as searchable aliases
            if let Some(ref f) = fm {
                if let Some(ref tags) = f.tags {
                    for tag in tags {
                        aliases.push(tag.clone());
                    }
                }
            }

            let entity = Entity {
                id: entity_id.clone(),
                canonical_label: label.clone(),
                entity_type: final_type,
                description,
                confidence: 1.0, // vault-sourced = highest confidence
                first_seen: now,
                last_updated: now,
                source_count: 1,
                metadata_json: Some(
                    serde_json::json!({
                        "source_file": path.to_string_lossy(),
                        "extraction": "vault_structure",
                        "status": fm.as_ref().and_then(|f| f.status.clone()),
                        "role": fm.as_ref().and_then(|f| f.role.clone()),
                    })
                    .to_string(),
                ),
                aliases,
            };

            db.upsert_entity(&entity)?;
            db.link_entity_source(&entity_id, content_id)?;
            entity_map.insert(label_lower, entity_id);
            entity_count += 1;
        }
    }
    println!("   Extracted {} entities from Nodes/", entity_count);

    // ── Phase 3: Extract relationships from frontmatter ─────────────────────
    println!("\n🔗 Phase 3: Extracting frontmatter relationships...");
    let mut rel_count = 0u32;

    for (path, _content_id) in &file_registry {
        if entity_from_node_path(path, vault_path).is_none() {
            continue; // Only Nodes/ files have frontmatter entities
        }

        let label = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_string();
        let content = fs::read_to_string(path).unwrap_or_default();

        if let Some(fm) = parse_frontmatter(&content) {
            let rels = relationships_from_frontmatter(&label, &fm);
            for rel in rels {
                if store_relationship(&db, &rel, &entity_map)? {
                    rel_count += 1;
                }
            }
        }
    }
    println!("   Extracted {} frontmatter relationships", rel_count);

    // ── Phase 4: Extract wiki-link relationships from ALL files ─────────────
    println!("\n🔗 Phase 4: Extracting wiki-link relationships...");
    let mut wikilink_rel_count = 0u32;

    for (path, _content_id) in &file_registry {
        let content = fs::read_to_string(path).unwrap_or_default();

        // Determine the "source entity" for this file
        let source_label = if let Some((label, _)) = entity_from_node_path(path, vault_path) {
            label
        } else {
            // For non-Nodes files, use filename as source label
            path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_string()
        };

        if source_label.is_empty() {
            continue;
        }

        // Only create wiki-link relationships where the source entity exists in our map
        let source_lower = source_label.to_lowercase();
        if !entity_map.contains_key(&source_lower) {
            // Non-Node file: skip wikilink relationship extraction
            // (source entity doesn't exist in graph)
            continue;
        }

        let rels = relationships_from_wikilinks(&source_label, &content);
        for rel in rels {
            if store_relationship(&db, &rel, &entity_map)? {
                wikilink_rel_count += 1;
            }
        }
    }
    println!(
        "   Extracted {} wiki-link relationships",
        wikilink_rel_count
    );

    // ── Phase 5: Dedup entities ─────────────────────────────────────────────
    println!("\n🧹 Phase 5: Deduplicating entities...");
    // Already handled by entity_map keyed on label_lower in Phase 2

    // ── Done ────────────────────────────────────────────────────────────────
    println!("\n✅ Scan complete!");
    show_stats(&db)?;

    Ok(())
}

/// Walk the vault, excluding hidden and unwanted directories
fn walk_vault(vault_path: &Path) -> impl Iterator<Item = walkdir::DirEntry> {
    WalkDir::new(vault_path)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_str().unwrap_or("");
            if e.file_type().is_dir() {
                !should_exclude_dir(name)
            } else {
                true
            }
        })
        .filter_map(|e| e.ok())
}

/// Store a VaultRelationship in the database, resolving labels to entity IDs.
/// Returns true if the relationship was stored (both endpoints exist).
fn store_relationship(
    db: &L2GraphDB,
    rel: &VaultRelationship,
    entity_map: &HashMap<String, String>,
) -> Result<bool> {
    let subject_lower = rel.subject_label.to_lowercase();
    let object_lower = rel.object_label.to_lowercase();

    let subject_id = match entity_map.get(&subject_lower) {
        Some(id) => id.clone(),
        None => return Ok(false),
    };
    let object_id = match entity_map.get(&object_lower) {
        Some(id) => id.clone(),
        None => return Ok(false), // Only store relationships where both entities are known
    };

    let rel_id = db.next_relationship_id()?;
    let now = Utc::now();

    let relationship = Relationship {
        id: rel_id,
        subject_id,
        predicate: rel.predicate.clone(),
        object_type: "entity".to_string(),
        object_value: object_id,
        confidence: rel.confidence,
        extracted_at: now,
        metadata_json: Some(
            serde_json::json!({
                "source": rel.source,
                "subject_label": rel.subject_label,
                "object_label": rel.object_label,
            })
            .to_string(),
        ),
    };

    let was_new = db.add_relationship(&relationship)?;
    Ok(was_new)
}

/// Capitalize a frontmatter type value ("person" → "Person")
/// Also strips wiki-link syntax: "[[Asset]]" → "Asset"
fn capitalize_type(t: &str) -> String {
    // Strip wiki-link brackets if present
    let cleaned = t.trim()
        .trim_start_matches("[[")
        .trim_end_matches("]]")
        .trim()
        .trim_matches('"')
        .trim();
    let mut chars = cleaned.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// Build a description from role + first meaningful paragraph
fn build_description(content: &str, fm: Option<&nodalync_graph::entity_extraction::Frontmatter>) -> Option<String> {
    let mut parts = Vec::new();

    if let Some(f) = fm {
        if let Some(ref role) = f.role {
            parts.push(role.clone());
        }
        if let Some(ref status) = f.status {
            parts.push(format!("Status: {}", status));
        }
    }

    // Extract first real paragraph from body
    let body = strip_frontmatter_simple(content);
    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("```") {
            continue;
        }
        if trimmed.len() > 20 {
            let snippet = if trimmed.len() > 200 {
                format!("{}...", safe_truncate(trimmed, 200))
            } else {
                trimmed.to_string()
            };
            parts.push(snippet);
            break;
        }
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" | "))
    }
}

fn strip_frontmatter_simple(content: &str) -> &str {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return content;
    }
    let after = &trimmed[3..];
    if let Some(pos) = after.find("\n---") {
        let start = pos + 4;
        if start < after.len() {
            return &after[start..];
        }
    }
    content
}

/// Truncate a string to at most `max_bytes` without splitting a multi-byte character.
fn safe_truncate(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

fn sha256_hex(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

// ─── QUERY COMMANDS ──────────────────────────────────────────────────────────

fn query_subgraph(
    db: &L2GraphDB,
    entity_query: &str,
    max_hops: u32,
    max_results: u32,
) -> Result<()> {
    println!("🔍 Searching for entity: {}", entity_query);

    let entity = if entity_query.starts_with('e')
        && entity_query[1..].parse::<u32>().is_ok()
    {
        db.find_entity(entity_query)?
    } else {
        db.find_entity(entity_query)?
    };

    let center_entity = match entity {
        Some(e) => e,
        None => {
            let search_results = db.search_entities(entity_query, 1)?;
            match search_results.first() {
                Some(e) => e.clone(),
                None => {
                    println!("❌ No entity found matching: {}", entity_query);
                    return Ok(());
                }
            }
        }
    };

    println!(
        "🎯 Found entity: {} ({})",
        center_entity.canonical_label, center_entity.id
    );

    let subgraph = db.get_subgraph(&center_entity.id, max_hops, max_results)?;

    println!("\n📊 Subgraph Results:");
    println!(
        "   Center: {} ({})",
        subgraph.center_entity.canonical_label, subgraph.center_entity.entity_type
    );
    println!(
        "   Connected entities: {}",
        subgraph.connected_entities.len()
    );
    println!("   Relationships: {}", subgraph.relationships.len());
    println!("   Sources: {}", subgraph.sources.len());

    if !subgraph.connected_entities.is_empty() {
        println!("\n🔗 Connected Entities:");
        for entity in &subgraph.connected_entities {
            println!(
                "   • {} ({}) - {} sources",
                entity.canonical_label, entity.entity_type, entity.source_count
            );
        }
    }

    if !subgraph.relationships.is_empty() {
        println!("\n🔀 Relationships:");
        for rel in &subgraph.relationships {
            // Resolve entity IDs to labels for display
            let subject_label = resolve_label(db, &rel.subject_id);
            let object_label = if rel.object_type == "entity" {
                resolve_label(db, &rel.object_value)
            } else {
                rel.object_value.clone()
            };
            println!(
                "   • {} —[{}]→ {}",
                subject_label, rel.predicate, object_label
            );
        }
    }

    Ok(())
}

fn search_entities(db: &L2GraphDB, query: &str, limit: u32) -> Result<()> {
    println!("🔍 Searching entities for: {}", query);

    let results = db.search_entities(query, limit)?;

    if results.is_empty() {
        println!("❌ No entities found matching: {}", query);
        return Ok(());
    }

    println!("\n📊 Found {} entities:", results.len());
    for entity in results {
        println!(
            "   • {} ({}) - {} sources, confidence: {:.2}",
            entity.canonical_label, entity.entity_type, entity.source_count, entity.confidence
        );

        if !entity.aliases.is_empty() {
            println!("     Tags: {}", entity.aliases.join(", "));
        }

        if let Some(desc) = &entity.description {
            let short_desc = if desc.len() > 120 {
                format!("{}...", safe_truncate(desc, 120))
            } else {
                desc.clone()
            };
            println!("     {}", short_desc);
        }
    }

    Ok(())
}

fn get_context(db: &L2GraphDB, query: &str, max_entities: u32) -> Result<()> {
    println!("🤖 Getting context for: {}", query);

    let context = db.get_context(query, max_entities)?;

    println!("\n📊 Context Results:");
    println!("   Query: {}", context.query);
    println!(
        "   Relevant entities: {}",
        context.relevant_entities.len()
    );
    println!(
        "   Relevant relationships: {}",
        context.relevant_relationships.len()
    );
    println!("   Confidence score: {:.2}", context.confidence_score);

    if !context.relevant_entities.is_empty() {
        println!("\n🎯 Relevant Entities:");
        for entity in &context.relevant_entities {
            println!(
                "   • {} ({}) - {} sources",
                entity.canonical_label, entity.entity_type, entity.source_count
            );
            if let Some(desc) = &entity.description {
                let short = if desc.len() > 100 {
                    format!("{}...", safe_truncate(desc, 100))
                } else {
                    desc.clone()
                };
                println!("     {}", short);
            }
        }
    }

    if !context.relevant_relationships.is_empty() {
        println!("\n🔀 Relevant Relationships:");
        for rel in &context.relevant_relationships {
            let subject = resolve_label(db, &rel.subject_id);
            let object = if rel.object_type == "entity" {
                resolve_label(db, &rel.object_value)
            } else {
                rel.object_value.clone()
            };
            println!("   • {} —[{}]→ {}", subject, rel.predicate, object);
        }
    }

    // JSON for agent consumption
    println!("\n📋 JSON Output:");
    println!("{}", serde_json::to_string_pretty(&context)?);

    Ok(())
}

fn list_entities(db: &L2GraphDB, entity_type: &str, limit: u32) -> Result<()> {
    println!("📋 Listing {} entities (limit: {})", entity_type, limit);

    let entities = db.list_entities_by_type(entity_type, limit)?;

    if entities.is_empty() {
        println!("❌ No entities found of type: {}", entity_type);
        return Ok(());
    }

    println!("\n📊 Found {} entities:", entities.len());
    for entity in entities {
        println!(
            "   • {} - {} sources, updated: {}",
            entity.canonical_label,
            entity.source_count,
            entity.last_updated.format("%Y-%m-%d")
        );

        if let Some(desc) = &entity.description {
            let short_desc = if desc.len() > 80 {
                format!("{}...", safe_truncate(desc, 80))
            } else {
                desc.clone()
            };
            println!("     {}", short_desc);
        }
    }

    Ok(())
}

fn show_stats(db: &L2GraphDB) -> Result<()> {
    let stats = db.get_stats()?;

    println!("\n📊 Database Statistics:");
    // Print top-level stats first
    for key in &["entities", "relationships", "content_items"] {
        if let Some(value) = stats.get(*key) {
            println!("   {}: {}", key, value);
        }
    }
    // Print type breakdown
    let mut type_stats: Vec<_> = stats
        .iter()
        .filter(|(k, _)| k.starts_with("  type:"))
        .collect();
    type_stats.sort_by(|a, b| b.1.cmp(a.1));
    if !type_stats.is_empty() {
        println!("   Entity types:");
        for (key, value) in type_stats {
            println!("     {}: {}", key.trim(), value);
        }
    }

    Ok(())
}

/// Resolve an entity ID to its label, falling back to the ID itself
fn resolve_label(db: &L2GraphDB, entity_id: &str) -> String {
    db.find_entity(entity_id)
        .ok()
        .flatten()
        .map(|e| e.canonical_label)
        .unwrap_or_else(|| entity_id.to_string())
}
