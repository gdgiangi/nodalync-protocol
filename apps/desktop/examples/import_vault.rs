//! Offline, append-only Markdown import into an existing desktop profile.
//! Stop Studio first. Set NODALYNC_IMPORT_PASSWORD in the environment, then run:
//! cargo run --locked --manifest-path apps/desktop/Cargo.toml --example import_vault -- \
//!   --vault /path/to/vault --notes-dir /path/to/vault/Nodes --data-dir /path/to/profile
//! Originals are read only; imports stay private. No network is initialized.

#[allow(dead_code)]
#[path = "../rust-src/protocol.rs"]
mod protocol;

use anyhow::{bail, Context, Result};
use chrono::Utc;
use nodalync_crypto::{content_hash, Hash};
use nodalync_graph::{
    entity_extraction::{
        entity_from_node_path, parse_frontmatter, relationships_from_frontmatter,
        relationships_from_wikilinks,
    },
    Entity, L2GraphDB, Relationship,
};
use nodalync_store::ContentStore;
use nodalync_types::{ContentType, Metadata, Visibility};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize)]
struct ImportRecord {
    imported_at: String,
    path: PathBuf,
    relative_path: PathBuf,
    title: String,
    hash: Option<String>,
    previous_hash: Option<String>,
    size: Option<u64>,
    outcome: String,
    duplicate_in_batch: bool,
    error: Option<String>,
    indexing_error: Option<String>,
}

#[derive(Debug, Default, Serialize)]
struct ImportReport {
    files: usize,
    imported: usize,
    unchanged: usize,
    updated_paths: usize,
    duplicate_files: usize,
    failed: usize,
    indexing_failed: usize,
    unique_hashes: usize,
    relationships_added: usize,
    unresolved_links: usize,
    ledger: PathBuf,
}

struct Note {
    record: usize,
    text: String,
    hash: Hash,
    entity_id: String,
    content_id: String,
}

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let (mut vault, mut notes, mut profile) = (None, None, None);
    while let Some(flag) = args.next() {
        if flag == "--help" || flag == "-h" {
            println!("import_vault --vault PATH [--notes-dir PATH] --data-dir PATH\nRequires an existing profile and NODALYNC_IMPORT_PASSWORD. Stop Studio first. Never publishes or modifies vault files.");
            return Ok(());
        }
        let value = PathBuf::from(args.next().context("Each option requires a path")?);
        match flag.as_str() {
            "--vault" => vault = Some(value),
            "--notes-dir" => notes = Some(value),
            "--data-dir" => profile = Some(value),
            _ => bail!("Unknown option: {flag}"),
        }
    }
    let vault = vault.context("Pass --vault PATH")?.canonicalize()?;
    let notes = notes
        .unwrap_or_else(|| {
            let nodes = vault.join("Nodes");
            if nodes.is_dir() {
                nodes
            } else {
                vault.clone()
            }
        })
        .canonicalize()?;
    let profile = profile.context("Pass --data-dir PATH")?.canonicalize()?;
    if !vault.is_dir() || !notes.is_dir() || !notes.starts_with(&vault) {
        bail!("The notes directory must be a directory inside the vault");
    }
    if profile.starts_with(&vault) || vault.starts_with(&profile) {
        bail!("The profile and vault must be separate directories");
    }
    if !protocol::ProtocolState::identity_exists(&profile) {
        bail!("The profile must already contain an identity; this importer never creates one");
    }
    let password = std::env::var("NODALYNC_IMPORT_PASSWORD")
        .context("Set NODALYNC_IMPORT_PASSWORD in the environment")?;
    let mut state = protocol::ProtocolState::open(&profile, &password)?;
    drop(password);
    let studio = profile.join("studio");
    fs::create_dir_all(&studio)?;
    let graph = L2GraphDB::new(studio.join("knowledge.db"))?;
    let report = import_vault(
        &mut state,
        &graph,
        &vault,
        &notes,
        &studio.join("vault-imports.jsonl"),
    )?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if report.failed > 0 || report.indexing_failed > 0 {
        bail!("Import completed with failures; inspect the private import ledger before retrying");
    }
    Ok(())
}

fn collect_markdown(directory: &Path, paths: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') || kind.is_symlink() {
            continue;
        }
        if kind.is_dir() {
            if ![
                "node_modules",
                "target",
                "dist",
                "build",
                "Templates",
                "Scripts",
                "vector_db",
            ]
            .contains(&name.as_ref())
            {
                collect_markdown(&entry.path(), paths)?;
            }
        } else if kind.is_file()
            && entry
                .path()
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
        {
            paths.push(entry.path());
        }
    }
    Ok(())
}

fn import_vault(
    state: &mut protocol::ProtocolState,
    graph: &L2GraphDB,
    vault: &Path,
    notes_dir: &Path,
    ledger: &Path,
) -> Result<ImportReport> {
    let mut previous = HashMap::new();
    if ledger.exists() {
        for line in fs::read_to_string(ledger)?
            .lines()
            .filter(|line| !line.trim().is_empty())
        {
            let record: ImportRecord = serde_json::from_str(line)
                .context("Existing import ledger is invalid; it has not been overwritten")?;
            if let Some(hash) = record.hash {
                previous.insert(record.path, hash);
            }
        }
    }
    let mut paths = Vec::new();
    collect_markdown(notes_dir, &mut paths)?;
    paths.sort();
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut output = options
        .open(ledger)
        .context("Could not open the private import ledger")?;
    let mut report = ImportReport {
        files: paths.len(),
        ledger: ledger.to_path_buf(),
        ..Default::default()
    };
    let mut records = Vec::with_capacity(paths.len());
    let mut notes = Vec::new();
    let mut hashes = HashSet::new();
    for path in paths {
        let title = path
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_string();
        let relative_path = path.strip_prefix(vault)?.to_path_buf();
        let mut record = ImportRecord {
            imported_at: Utc::now().to_rfc3339(),
            path: path.clone(),
            relative_path,
            title,
            hash: None,
            previous_hash: previous.get(&path).cloned(),
            size: None,
            outcome: "failed".into(),
            duplicate_in_batch: false,
            error: None,
            indexing_error: None,
        };
        let imported = (|| -> Result<Note> {
            let bytes = fs::read(&path)?;
            record.size = Some(bytes.len() as u64);
            let text = std::str::from_utf8(&bytes)
                .context("Markdown is not valid UTF-8")?
                .to_string();
            let hash = content_hash(&bytes);
            let existing = state.ops.get_content_manifest(&hash)?;
            if let Some(manifest) = existing {
                if manifest.content_type != ContentType::L0
                    || manifest.visibility != Visibility::Private
                {
                    bail!("This content hash already belongs to non-private or non-L0 content; refusing to change it");
                }
                let stored = state.ops.state().content.load(&hash)?.context(
                    "Existing manifest is missing its content; refusing to overwrite it",
                )?;
                if stored != bytes {
                    bail!("Existing stored bytes do not match the original; refusing to overwrite them");
                }
                record.outcome = "unchanged".into();
                report.unchanged += 1;
            } else {
                state.ops.create_content(
                    &bytes,
                    Metadata::new(&record.title, bytes.len() as u64)
                        .with_mime_type("text/markdown")
                        .with_description(format!("Obsidian: {}", record.relative_path.display())),
                )?;
                record.outcome = "imported".into();
                report.imported += 1;
            }
            record.hash = Some(hash.to_string());
            record.duplicate_in_batch = !hashes.insert(hash);
            if record.duplicate_in_batch {
                report.duplicate_files += 1;
            }
            if record
                .previous_hash
                .as_ref()
                .is_some_and(|previous| previous != &hash.to_string())
            {
                report.updated_paths += 1;
            }
            let content_id = match graph.content_hash_exists(&hash.to_string())? {
                Some(id) => id,
                None => graph.register_content(&hash.to_string(), "L0")?,
            };
            let entity_id = format!("vault:{}", content_hash(path.to_string_lossy().as_bytes()));
            let metadata = serde_json::json!({"source_file": path, "vault_relative_path": record.relative_path, "extraction": "vault_import"}).to_string();
            if graph.find_entity_by_id(&entity_id)?.is_none() {
                let entity_type = entity_from_node_path(&path, vault)
                    .map(|(_, kind)| kind)
                    .unwrap_or_else(|| "Document".into());
                graph.upsert_entity(&Entity {
                    id: entity_id.clone(),
                    canonical_label: record.title.clone(),
                    entity_type,
                    description: Some(format!("Obsidian: {}", record.relative_path.display())),
                    confidence: 1.0,
                    first_seen: Utc::now(),
                    last_updated: Utc::now(),
                    source_count: 0,
                    metadata_json: Some(metadata),
                    aliases: vec![],
                })?;
            }
            link_source(graph, &entity_id, &content_id)?;
            Ok(Note {
                record: records.len(),
                text,
                hash,
                entity_id,
                content_id,
            })
        })();
        match imported {
            Ok(note) => notes.push(note),
            Err(error) => {
                if record.hash.is_some() {
                    record.indexing_error = Some(error.to_string());
                    report.indexing_failed += 1;
                } else {
                    record.error = Some(error.to_string());
                    report.failed += 1;
                }
            }
        }
        records.push(record);
    }
    // Build the complete note registry before extracting mentions/link targets.
    let mut labels: HashMap<String, Vec<String>> = HashMap::new();
    for note in &notes {
        labels
            .entry(records[note.record].title.to_lowercase())
            .or_default()
            .push(note.entity_id.clone());
    }
    for note in &notes {
        let indexed = (|| -> Result<()> {
            let summary = state.ops.extract_l1_summary(&note.hash)?;
            let mut seen = HashSet::new();
            for name in summary
                .preview_mentions
                .iter()
                .flat_map(|mention| &mention.entities)
                .chain(&summary.primary_topics)
            {
                let name = name.trim();
                if name.len() < 2 || !seen.insert(name.to_lowercase()) {
                    continue;
                }
                let entity = match graph.find_entity(name)? {
                    Some(entity) => entity,
                    None => {
                        let entity = Entity { id: graph.next_entity_id()?, canonical_label: name.into(), entity_type: "Concept".into(),
                            description: None, confidence: 0.6, first_seen: Utc::now(), last_updated: Utc::now(), source_count: 0,
                            metadata_json: Some(serde_json::json!({"auto_extracted": true,"needs_review": true,"source_content": note.hash.to_string()}).to_string()), aliases: vec![] };
                        graph.upsert_entity(&entity)?;
                        entity
                    }
                };
                link_source(graph, &entity.id, &note.content_id)?;
            }
            let mut links = relationships_from_wikilinks(&records[note.record].title, &note.text);
            if let Some(frontmatter) = parse_frontmatter(&note.text) {
                links.extend(relationships_from_frontmatter(
                    &records[note.record].title,
                    &frontmatter,
                ));
            }
            for link in links {
                // Resolve only unambiguous note names; never guess at missing targets.
                let Some(targets) = labels.get(&link.object_label.to_lowercase()) else {
                    report.unresolved_links += 1;
                    continue;
                };
                if targets.len() != 1 {
                    report.unresolved_links += 1;
                    continue;
                }
                let relationship = Relationship { id: graph.next_relationship_id()?, subject_id: note.entity_id.clone(),
                    predicate: link.predicate, object_type: "entity".into(), object_value: targets[0].clone(),
                    confidence: link.confidence, extracted_at: Utc::now(),
                    metadata_json: Some(serde_json::json!({"source": link.source, "source_content": note.hash.to_string()}).to_string()) };
                if graph.add_relationship(&relationship)? {
                    report.relationships_added += 1;
                }
            }
            Ok(())
        })();
        if let Err(error) = indexed {
            records[note.record].indexing_error = Some(error.to_string());
            report.indexing_failed += 1;
        }
    }
    for record in &records {
        serde_json::to_writer(&mut output, record)?;
        output.write_all(b"\n")?;
    }
    output.sync_all()?;
    report.unique_hashes = hashes.len();
    Ok(report)
}

fn link_source(graph: &L2GraphDB, entity_id: &str, content_id: &str) -> Result<()> {
    let transaction = graph.connection().unchecked_transaction()?;
    let added = transaction.execute("INSERT OR IGNORE INTO entity_sources (entity_id, content_id, added_at) VALUES (?1, ?2, ?3)", (entity_id, content_id, Utc::now().timestamp()))?;
    if added > 0 {
        transaction.execute(
            "UPDATE entities SET source_count = source_count + 1, last_updated = ?1 WHERE id = ?2",
            (Utc::now().timestamp(), entity_id),
        )?;
    }
    transaction.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use nodalync_store::{ManifestFilter, ManifestStore};

    #[test]
    fn imports_exact_private_originals_with_paths_dedup_updates_and_matching_graph_hashes(
    ) -> Result<()> {
        let temp = tempfile::tempdir()?;
        let vault = temp.path().join("vault");
        let notes = vault.join("Nodes/Ideas");
        fs::create_dir_all(&notes)?;
        let original =
            b"---\r\ntitle: Alpha\r\n---\r\n# Alpha\r\n\r\nCaf\xc3\xa9 [[Beta]] remains exact.\r\n";
        fs::write(notes.join("Alpha.md"), original)?;
        fs::write(notes.join("Duplicate.md"), original)?;
        fs::write(
            notes.join("Beta.md"),
            b"# Beta\n\nA second original about Knowledge.\n",
        )?;
        fs::create_dir_all(notes.join(".hidden"))?;
        fs::write(notes.join(".hidden/Ignore.md"), b"not imported")?;
        let profile = temp.path().join("profile");
        let mut state = protocol::ProtocolState::init(&profile, "synthetic-import-test")?;
        fs::create_dir_all(profile.join("studio"))?;
        let graph = L2GraphDB::new(profile.join("studio/knowledge.db"))?;
        let ledger = profile.join("studio/vault-imports.jsonl");
        let first = import_vault(&mut state, &graph, &vault, &notes, &ledger)?;
        assert_eq!(
            (
                first.files,
                first.imported,
                first.unchanged,
                first.duplicate_files,
                first.unique_hashes,
                first.failed,
                first.indexing_failed
            ),
            (3, 2, 1, 1, 2, 0, 0)
        );
        assert!(first.relationships_added >= 1);
        let hash = content_hash(original);
        assert_eq!(fs::read(notes.join("Alpha.md"))?, original);
        assert_eq!(state.ops.state().content.load(&hash)?.unwrap(), original);
        let manifests = state.ops.state().manifests.list(ManifestFilter::new())?;
        assert!(manifests
            .iter()
            .all(|manifest| manifest.visibility == Visibility::Private
                && manifest.content_type == ContentType::L0));
        assert_eq!(
            state
                .ops
                .get_content_manifest(&hash)?
                .unwrap()
                .metadata
                .description
                .as_deref(),
            Some("Obsidian: Nodes/Ideas/Alpha.md")
        );
        let records: Vec<ImportRecord> = fs::read_to_string(&ledger)?
            .lines()
            .map(serde_json::from_str)
            .collect::<std::result::Result<_, _>>()?;
        assert_eq!(records.len(), 3);
        assert_eq!(
            records
                .iter()
                .filter(|record| record.hash.as_deref() == Some(&hash.to_string()))
                .count(),
            2
        );
        assert!(records.iter().all(|record| record.path.starts_with(&vault)));
        let stats = graph.get_stats()?;
        let again = import_vault(&mut state, &graph, &vault, &notes, &ledger)?;
        assert_eq!(
            (
                again.imported,
                again.unchanged,
                again.updated_paths,
                again.relationships_added
            ),
            (0, 3, 0, 0)
        );
        assert_eq!(
            graph.get_stats()?.get("relationships"),
            stats.get("relationships")
        );
        let entity = format!(
            "vault:{}",
            content_hash(notes.join("Alpha.md").to_string_lossy().as_bytes())
        );
        assert_eq!(graph.find_entity_by_id(&entity)?.unwrap().source_count, 1);
        assert_eq!(graph.get_entity_sources(&entity)?.len(), 1);
        let links = graph.get_entity_sources(&entity)?;
        let registered: String = graph.connection().query_row(
            "SELECT current_hash FROM content_registry WHERE content_id = ?1",
            [&links[0].content_id],
            |row| row.get(0),
        )?;
        assert_eq!(registered, hash.to_string());
        fs::write(
            notes.join("Alpha.md"),
            b"# Alpha revised\n\nThe earlier original remains preserved.\n",
        )?;
        fs::write(notes.join("Invalid.md"), [0xff])?;
        let changed = import_vault(&mut state, &graph, &vault, &notes, &ledger)?;
        assert_eq!(
            (changed.imported, changed.updated_paths, changed.failed),
            (1, 1, 1)
        );
        assert_eq!(state.ops.state().content.load(&hash)?.unwrap(), original);
        assert_eq!(fs::read(notes.join("Invalid.md"))?, [0xff]);
        assert!(state.network.is_none());
        Ok(())
    }
}
