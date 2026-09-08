//! A local, human-authored synthesis workspace backed by protocol L3 provenance.
//! Sources remain untouched; saving never starts networking or publishes content.

use base64::{engine::general_purpose::STANDARD, Engine as _};
use nodalync_crypto::{content_hash, Hash};
use nodalync_ops::DefaultNodeOperations;
use nodalync_store::ContentStore;
use nodalync_types::{ContentType, Metadata, MAX_DESCRIPTION_LENGTH, MAX_TITLE_LENGTH};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;
use tauri::State;
use tokio::sync::Mutex;

use crate::protocol::ProtocolState;
use crate::publish_commands::parse_hash;

const PAGE_LIMIT: u32 = 40;
const MAX_BODY_BYTES: usize = 256 * 1024;
const FOOTER_PREFIX: &str = "\n<!-- nodalync-studio-synthesis-v1: ";

#[derive(Debug, Clone, Serialize)]
pub struct SynthesisSourceItem {
    pub hash: String,
    pub title: String,
    pub description: Option<String>,
    pub size: u64,
    pub content_type: String,
    pub created_at: u64,
    pub available_locally: bool,
}

#[derive(Debug, Serialize)]
pub struct SynthesisSourcePage {
    pub items: Vec<SynthesisSourceItem>,
    pub total: u64,
    pub offset: u32,
    pub limit: u32,
    pub has_more: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynthesisSourceReference {
    pub hash: String,
    pub title: String,
    pub label: String,
    pub available_locally: bool,
}

#[derive(Debug, Serialize)]
pub struct SynthesisDetails {
    pub hash: String,
    pub title: String,
    pub question: String,
    pub body: String,
    pub text: String,
    pub content_type: String,
    pub visibility: String,
    pub sources: Vec<SynthesisSourceReference>,
    pub root_source_count: usize,
    pub provenance_depth: u32,
    pub created_at: u64,
}

/// This envelope preserves the editor fields without making the metadata store
/// carry arbitrary JSON. Its hashes are checked against the protocol manifest.
#[derive(Debug, Serialize, Deserialize)]
struct SynthesisDocument {
    title: String,
    question: String,
    body: String,
    source_hashes: Vec<String>,
}

#[tauri::command]
pub async fn list_synthesis_sources(
    query: Option<String>,
    offset: Option<u32>,
    limit: Option<u32>,
    protocol: State<'_, Arc<Mutex<Option<ProtocolState>>>>,
) -> Result<SynthesisSourcePage, String> {
    let guard = protocol.lock().await;
    let state = guard.as_ref().ok_or("Unlock your workspace first.")?;
    source_page(&state.ops, query.as_deref(), offset, limit)
}

fn source_page(
    ops: &DefaultNodeOperations,
    query: Option<&str>,
    offset: Option<u32>,
    limit: Option<u32>,
) -> Result<SynthesisSourcePage, String> {
    let query = query.unwrap_or_default().trim();
    if query.len() > 200 {
        return Err("Keep the source search under 200 bytes.".into());
    }
    let offset = offset.unwrap_or(0);
    let limit = limit.unwrap_or(24).clamp(1, PAGE_LIMIT);
    // Treat user '%' and '_' characters literally, not as SQL wildcards.
    let escaped = query
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    let pattern = format!("%{}%", escaped);
    let connection = ops.state().connection();
    let connection = connection.lock().map_err(|error| error.to_string())?;
    let total: u64 = connection
        .query_row(
            "SELECT COUNT(*) FROM manifests WHERE content_type = 0 AND title LIKE ?1 ESCAPE '\\'",
            [&pattern],
            |row| row.get(0),
        )
        .map_err(|error| format!("Couldn’t count sources: {error}"))?;
    // Query only this page's lightweight fields. No graph or manifest collection
    // is materialized, and the hash tie-breaker keeps paging deterministic.
    let mut statement = connection
        .prepare(
            "SELECT hash, title, description, content_size, created_at FROM manifests
         WHERE content_type = 0 AND title LIKE ?1 ESCAPE '\\'
         ORDER BY created_at DESC, hash ASC LIMIT ?2 OFFSET ?3",
        )
        .map_err(|error| format!("Couldn’t find sources: {error}"))?;
    let rows = statement
        .query_map((&pattern, limit, offset), |row| {
            let hash: Vec<u8> = row.get(0)?;
            Ok(SynthesisSourceItem {
                hash: hex::encode(hash),
                title: row.get(1)?,
                description: row.get(2)?,
                size: row.get(3)?,
                content_type: "L0".into(),
                created_at: row.get(4)?,
                available_locally: false,
            })
        })
        .map_err(|error| format!("Couldn’t read source results: {error}"))?;
    let mut items = Vec::with_capacity(limit as usize);
    for row in rows {
        let mut item = row.map_err(|error| format!("Couldn’t read source result: {error}"))?;
        item.available_locally = parse_hash(&item.hash)
            .map(|hash| ops.state().content.exists(&hash))
            .unwrap_or(false);
        items.push(item);
    }
    Ok(SynthesisSourcePage {
        has_more: u64::from(offset) + (items.len() as u64) < total,
        items,
        total,
        offset,
        limit,
    })
}

#[tauri::command]
pub async fn save_synthesis(
    title: String,
    question: String,
    body: String,
    source_hashes: Vec<String>,
    protocol: State<'_, Arc<Mutex<Option<ProtocolState>>>>,
) -> Result<SynthesisDetails, String> {
    let mut guard = protocol.lock().await;
    let state = guard.as_mut().ok_or("Unlock your workspace first.")?;
    save_document(&mut state.ops, title, question, body, source_hashes)
}

fn save_document(
    ops: &mut DefaultNodeOperations,
    title: String,
    question: String,
    body: String,
    source_hashes: Vec<String>,
) -> Result<SynthesisDetails, String> {
    let title = title.trim().to_string();
    let question = question.trim().to_string();
    if title.is_empty() || title.len() > MAX_TITLE_LENGTH {
        return Err(format!(
            "Give this idea a title of 1–{MAX_TITLE_LENGTH} bytes."
        ));
    }
    if question.is_empty() || question.len() > MAX_DESCRIPTION_LENGTH {
        return Err(format!(
            "Write a guiding question of 1–{MAX_DESCRIPTION_LENGTH} bytes."
        ));
    }
    if body.trim().is_empty() || body.len() > MAX_BODY_BYTES {
        return Err("Write your new idea before saving (up to 256 KiB).".into());
    }
    if !(2..=12).contains(&source_hashes.len()) {
        return Err("Choose between 2 and 12 original sources for this synthesis.".into());
    }

    let mut seen = HashSet::new();
    let mut hashes = Vec::with_capacity(source_hashes.len());
    let mut source_titles = Vec::with_capacity(source_hashes.len());
    // Validate the complete source selection before asking the protocol to write.
    for raw in source_hashes {
        let hash = parse_hash(&raw)?;
        if !seen.insert(hash) {
            return Err("Each original source can only be selected once.".into());
        }
        let manifest = ops
            .get_content_manifest(&hash)
            .map_err(|error| format!("Couldn’t read source: {error}"))?
            .ok_or_else(|| format!("Source is no longer in this workspace: {hash}"))?;
        if manifest.content_type != ContentType::L0 {
            return Err(
                "This workspace synthesizes original sources (L0). Choose an original note.".into(),
            );
        }
        let source = ops
            .state()
            .content
            .load(&hash)
            .map_err(|error| format!("Couldn’t read original source: {error}"))?
            .ok_or_else(|| {
                format!(
                    "Source is not stored on this device: {}",
                    manifest.metadata.title
                )
            })?;
        if std::str::from_utf8(&source).is_err() {
            return Err(format!(
                "Source is not readable UTF-8 text: {}",
                manifest.metadata.title
            ));
        }
        if content_hash(&source) != hash {
            return Err(format!(
                "Source content failed its hash check: {}",
                manifest.metadata.title
            ));
        }
        source_titles.push(manifest.metadata.title);
        hashes.push(hash);
    }

    let document = SynthesisDocument {
        title: title.clone(),
        question: question.clone(),
        body,
        source_hashes: hashes.iter().map(ToString::to_string).collect(),
    };
    let mut text = format!(
        "# {}\n\n## Guiding question\n\n{}\n\n## New idea\n\n{}\n\n## Sources\n",
        document.title, document.question, document.body
    );
    for (index, (hash, source_title)) in hashes.iter().zip(source_titles).enumerate() {
        let label = source_title.replace(['\n', '\r'], " ");
        text.push_str(&format!("\n[S{}] {} — `{}`\n", index + 1, label, hash));
    }
    let encoded =
        STANDARD.encode(serde_json::to_vec(&document).map_err(|error| error.to_string())?);
    text.push_str(&format!("{FOOTER_PREFIX}{encoded} -->\n"));
    let metadata = Metadata::new(title, text.len() as u64)
        .with_description(question)
        .with_mime_type("text/markdown");
    // The protocol validates ownership/access and carries every upstream root.
    // It creates private L3 content; it performs no publishing or network calls.
    let hash = ops
        .derive_content(&hashes, text.as_bytes(), metadata)
        .map_err(|error| format!("Couldn’t save synthesis: {error}"))?;
    synthesis_details(ops, hash)
}

#[tauri::command]
pub async fn get_synthesis_details(
    hash: String,
    protocol: State<'_, Arc<Mutex<Option<ProtocolState>>>>,
) -> Result<SynthesisDetails, String> {
    let guard = protocol.lock().await;
    let state = guard.as_ref().ok_or("Unlock your workspace first.")?;
    synthesis_details(&state.ops, parse_hash(&hash)?)
}

fn synthesis_details(ops: &DefaultNodeOperations, hash: Hash) -> Result<SynthesisDetails, String> {
    let manifest = ops
        .get_content_manifest(&hash)
        .map_err(|error| format!("Couldn’t read synthesis: {error}"))?
        .ok_or("This synthesis is no longer in the workspace.")?;
    if manifest.content_type != ContentType::L3 {
        return Err("This content is an original source, not a saved synthesis.".into());
    }
    let bytes = ops
        .state()
        .content
        .load(&hash)
        .map_err(|error| format!("Couldn’t open synthesis: {error}"))?
        .ok_or("This synthesis is not stored on this device.")?;
    let text = String::from_utf8(bytes).map_err(|_| "This synthesis is not UTF-8 text.")?;
    let document = text
        .rsplit_once(FOOTER_PREFIX)
        .and_then(|(_, suffix)| suffix.trim_end().strip_suffix(" -->"))
        .and_then(|encoded| STANDARD.decode(encoded).ok())
        .and_then(|json| serde_json::from_slice::<SynthesisDocument>(&json).ok())
        .filter(|document| {
            document.title == manifest.metadata.title
                && document.source_hashes
                    == manifest
                        .provenance
                        .derived_from
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
        });
    let mut sources = Vec::with_capacity(manifest.provenance.derived_from.len());
    for (index, source_hash) in manifest.provenance.derived_from.iter().enumerate() {
        let source = ops
            .get_content_manifest(source_hash)
            .map_err(|error| format!("Couldn’t read source provenance: {error}"))?;
        sources.push(SynthesisSourceReference {
            hash: source_hash.to_string(),
            title: source
                .map(|source| source.metadata.title)
                .unwrap_or_else(|| "Original source unavailable".into()),
            label: format!("S{}", index + 1),
            available_locally: ops.state().content.exists(source_hash),
        });
    }
    let (question, body) = document
        .map(|document| (document.question, document.body))
        .unwrap_or_else(|| {
            (
                manifest.metadata.description.clone().unwrap_or_default(),
                text.clone(),
            )
        });
    Ok(SynthesisDetails {
        hash: hash.to_string(),
        title: manifest.metadata.title,
        question,
        body,
        text,
        content_type: "L3".into(),
        visibility: format!("{:?}", manifest.visibility),
        sources,
        root_source_count: manifest.provenance.root_l0l1.len(),
        provenance_depth: manifest.provenance.depth,
        created_at: manifest.created_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use nodalync_crypto::{generate_identity, peer_id_from_public_key};
    use nodalync_store::{ManifestFilter, ManifestStore, NodeState, NodeStateConfig};
    use nodalync_types::{Provenance, Version, Visibility};
    use tempfile::TempDir;

    fn workspace() -> (DefaultNodeOperations, TempDir) {
        let dir = TempDir::new().unwrap();
        let (_, public_key) = generate_identity();
        let ops = DefaultNodeOperations::with_defaults(
            NodeState::open(NodeStateConfig::new(dir.path())).unwrap(),
            peer_id_from_public_key(&public_key),
        );
        (ops, dir)
    }

    fn original(ops: &mut DefaultNodeOperations, title: &str, text: &[u8]) -> Hash {
        ops.create_content(text, Metadata::new(title, text.len() as u64))
            .unwrap()
    }

    #[test]
    fn saves_real_private_l3_with_exact_editor_fields_and_originals_intact() {
        let (mut ops, dir) = workspace();
        let first = original(
            &mut ops,
            "Field notes",
            b"An orchard shares nutrients through fungal networks.",
        );
        let second = original(
            &mut ops,
            "Engineering notes",
            b"Distributed caches retain frequently requested records.",
        );
        let before = [first, second].map(|hash| ops.get_content_manifest(&hash).unwrap().unwrap());
        let body = "  Design a cache with local exchange rules. [S1] [S2]\n\nTest resource sharing before central coordination.\n";
        let saved = save_document(
            &mut ops,
            "A cooperative cache".into(),
            "What can distributed systems learn from ecosystems?".into(),
            body.into(),
            vec![first.to_string(), second.to_string()],
        )
        .unwrap();
        assert_eq!(saved.body, body);
        assert_eq!(saved.visibility, "Private");
        assert_eq!(saved.content_type, "L3");
        assert_eq!(saved.provenance_depth, 1);
        assert_eq!(saved.root_source_count, 2);
        assert_eq!(
            saved
                .sources
                .iter()
                .map(|source| source.hash.clone())
                .collect::<Vec<_>>(),
            vec![first.to_string(), second.to_string()]
        );
        let reopened = synthesis_details(&ops, parse_hash(&saved.hash).unwrap()).unwrap();
        assert_eq!(reopened.body, body);
        assert_eq!(reopened.question, saved.question);
        let manifest = ops
            .get_content_manifest(&parse_hash(&saved.hash).unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(manifest.visibility, Visibility::Private);
        assert_eq!(manifest.provenance.derived_from, vec![first, second]);
        for (hash, original_manifest) in [first, second].into_iter().zip(before) {
            assert_eq!(
                ops.get_content_manifest(&hash).unwrap().unwrap(),
                original_manifest
            );
            assert_eq!(
                content_hash(&ops.state().content.load(&hash).unwrap().unwrap()),
                hash
            );
        }
        let retry = save_document(
            &mut ops,
            saved.title,
            saved.question,
            body.into(),
            vec![first.to_string(), second.to_string()],
        )
        .unwrap();
        assert_eq!(saved.hash, retry.hash);
        assert_eq!(
            ops.state()
                .manifests
                .list(ManifestFilter::new())
                .unwrap()
                .len(),
            3
        );
        // A fresh protocol store can reconstruct the idea after application exit.
        let peer_id = ops.peer_id();
        drop(ops);
        let reopened_ops = DefaultNodeOperations::with_defaults(
            NodeState::open(NodeStateConfig::new(dir.path())).unwrap(),
            peer_id,
        );
        let restored = synthesis_details(&reopened_ops, parse_hash(&saved.hash).unwrap()).unwrap();
        assert_eq!(restored.body, body);
        assert_eq!(restored.sources.len(), 2);
    }

    #[test]
    fn rejects_invalid_missing_duplicate_and_binary_sources_before_writing() {
        let (mut ops, _dir) = workspace();
        let first = original(&mut ops, "Source", b"Readable source");
        let binary = original(&mut ops, "Binary", &[0xff, 0xfe]);
        for sources in [
            vec![first.to_string()],
            vec![first.to_string(), first.to_string()],
            vec![first.to_string(), "bad hash".into()],
            vec![first.to_string(), content_hash(b"missing").to_string()],
            vec![first.to_string(), binary.to_string()],
        ] {
            assert!(save_document(
                &mut ops,
                "Idea".into(),
                "Question?".into(),
                "Idea body".into(),
                sources
            )
            .is_err());
        }
        assert_eq!(
            ops.state()
                .manifests
                .list(ManifestFilter::new())
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn source_pages_remain_bounded_and_exact_at_ten_thousand_records() {
        let (mut ops, _dir) = workspace();
        let template_hash = original(&mut ops, "Template", b"Template source");
        let template = ops.get_content_manifest(&template_hash).unwrap().unwrap();
        let connection = ops.state().connection();
        connection.lock().unwrap().execute_batch("BEGIN").unwrap();
        for index in 0..10_000 {
            let mut manifest = template.clone();
            manifest.hash = content_hash(format!("Synthetic source {index}").as_bytes());
            manifest.metadata.title = if index % 10 == 0 {
                format!("Orchard {index}")
            } else {
                format!("Systems {index}")
            };
            manifest.provenance = Provenance::new_l0(manifest.hash, manifest.owner);
            manifest.version = Version::new_v1(manifest.hash, manifest.created_at);
            ops.state_mut().manifests.store(&manifest).unwrap();
        }
        connection.lock().unwrap().execute_batch("COMMIT").unwrap();
        let first = source_page(&ops, Some("Orchard"), Some(0), Some(1000)).unwrap();
        assert_eq!(first.total, 1000);
        assert_eq!(first.items.len(), PAGE_LIMIT as usize);
        assert!(first.has_more);
        let second =
            source_page(&ops, Some("Orchard"), Some(PAGE_LIMIT), Some(PAGE_LIMIT)).unwrap();
        assert!(!second
            .items
            .iter()
            .any(|source| first.items.iter().any(|other| source.hash == other.hash)));
        let last = source_page(&ops, Some("Orchard"), Some(990), Some(40)).unwrap();
        assert_eq!(last.items.len(), 10);
        assert!(!last.has_more);
        assert_eq!(source_page(&ops, None, None, None).unwrap().total, 10_001);
        assert_eq!(source_page(&ops, Some("%"), None, None).unwrap().total, 0);
        assert_eq!(source_page(&ops, Some("_"), None, None).unwrap().total, 0);
    }
}
