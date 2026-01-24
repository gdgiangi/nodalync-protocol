//! SQLite-based manifest storage.
//!
//! This module implements manifest storage using SQLite for efficient
//! querying and filtering.

use rusqlite::{params, Connection, OptionalExtension};
use std::sync::{Arc, Mutex};

use nodalync_crypto::{Hash, PeerId, Timestamp};
use nodalync_types::{
    AccessControl, ContentType, Currency, Economics, Manifest, Metadata, Provenance, Version,
    Visibility,
};

use crate::error::{Result, StoreError};
use crate::traits::ManifestStore;
use crate::types::ManifestFilter;

/// SQLite-based manifest store.
pub struct SqliteManifestStore {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteManifestStore {
    /// Create a new manifest store with the given database connection.
    pub fn new(conn: Arc<Mutex<Connection>>) -> Self {
        Self { conn }
    }

    /// Serialize a manifest to SQL row values.
    fn serialize_manifest(
        manifest: &Manifest,
    ) -> Result<(
        Vec<u8>,                  // hash
        u8,                       // content_type
        Vec<u8>,                  // owner
        u32,                      // version_number
        Option<Vec<u8>>,          // version_previous
        Vec<u8>,                  // version_root
        Timestamp,                // version_timestamp
        u8,                       // visibility
        String,                   // title
        Option<String>,           // description
        Option<String>,           // tags (JSON)
        u64,                      // content_size
        Option<String>,           // mime_type
        u64,                      // price
        u64,                      // total_queries
        u64,                      // total_revenue
        String,                   // access_control (JSON)
        String,                   // provenance (JSON)
        Timestamp,                // created_at
        Timestamp,                // updated_at
    )> {
        let hash = manifest.hash.0.to_vec();
        let content_type = manifest.content_type as u8;
        let owner = manifest.owner.0.to_vec();
        let version_number = manifest.version.number;
        let version_previous = manifest.version.previous.map(|h| h.0.to_vec());
        let version_root = manifest.version.root.0.to_vec();
        let version_timestamp = manifest.version.timestamp;
        let visibility = manifest.visibility as u8;
        let title = manifest.metadata.title.clone();
        let description = manifest.metadata.description.clone();
        let tags = if manifest.metadata.tags.is_empty() {
            None
        } else {
            Some(serde_json::to_string(&manifest.metadata.tags)?)
        };
        let content_size = manifest.metadata.content_size;
        let mime_type = manifest.metadata.mime_type.clone();
        let price = manifest.economics.price;
        let total_queries = manifest.economics.total_queries;
        let total_revenue = manifest.economics.total_revenue;
        let access_control = serde_json::to_string(&manifest.access)?;
        let provenance = serde_json::to_string(&manifest.provenance)?;
        let created_at = manifest.created_at;
        let updated_at = manifest.updated_at;

        Ok((
            hash,
            content_type,
            owner,
            version_number,
            version_previous,
            version_root,
            version_timestamp,
            visibility,
            title,
            description,
            tags,
            content_size,
            mime_type,
            price,
            total_queries,
            total_revenue,
            access_control,
            provenance,
            created_at,
            updated_at,
        ))
    }

    /// Deserialize a manifest from a database row.
    fn deserialize_row(row: &rusqlite::Row) -> rusqlite::Result<Manifest> {
        let hash_bytes: Vec<u8> = row.get(0)?;
        let content_type_u8: u8 = row.get(1)?;
        let owner_bytes: Vec<u8> = row.get(2)?;
        let version_number: u32 = row.get(3)?;
        let version_previous_bytes: Option<Vec<u8>> = row.get(4)?;
        let version_root_bytes: Vec<u8> = row.get(5)?;
        let version_timestamp: Timestamp = row.get(6)?;
        let visibility_u8: u8 = row.get(7)?;
        let title: String = row.get(8)?;
        let description: Option<String> = row.get(9)?;
        let tags_json: Option<String> = row.get(10)?;
        let content_size: u64 = row.get(11)?;
        let mime_type: Option<String> = row.get(12)?;
        let price: u64 = row.get(13)?;
        let total_queries: u64 = row.get(14)?;
        let total_revenue: u64 = row.get(15)?;
        let access_control_json: String = row.get(16)?;
        let provenance_json: String = row.get(17)?;
        let created_at: Timestamp = row.get(18)?;
        let updated_at: Timestamp = row.get(19)?;

        // Convert bytes to types
        let hash = bytes_to_hash(&hash_bytes);
        let owner = bytes_to_peer_id(&owner_bytes);
        let version_previous = version_previous_bytes.map(|b| bytes_to_hash(&b));
        let version_root = bytes_to_hash(&version_root_bytes);

        let content_type = match content_type_u8 {
            0 => ContentType::L0,
            1 => ContentType::L1,
            2 => ContentType::L2,
            3 => ContentType::L3,
            _ => ContentType::L0, // Default fallback
        };

        let visibility = match visibility_u8 {
            0 => Visibility::Private,
            1 => Visibility::Unlisted,
            2 => Visibility::Shared,
            _ => Visibility::Private, // Default fallback
        };

        let tags: Vec<String> = tags_json
            .map(|j| serde_json::from_str(&j).unwrap_or_default())
            .unwrap_or_default();

        let access: AccessControl =
            serde_json::from_str(&access_control_json).unwrap_or_default();
        let provenance: Provenance =
            serde_json::from_str(&provenance_json).unwrap_or_default();

        Ok(Manifest {
            hash,
            content_type,
            owner,
            version: Version {
                number: version_number,
                previous: version_previous,
                root: version_root,
                timestamp: version_timestamp,
            },
            visibility,
            access,
            metadata: Metadata {
                title,
                description,
                tags,
                content_size,
                mime_type,
            },
            economics: Economics {
                price,
                currency: Currency::NDL,
                total_queries,
                total_revenue,
            },
            provenance,
            created_at,
            updated_at,
        })
    }
}

impl ManifestStore for SqliteManifestStore {
    fn store(&mut self, manifest: &Manifest) -> Result<()> {
        let (
            hash,
            content_type,
            owner,
            version_number,
            version_previous,
            version_root,
            version_timestamp,
            visibility,
            title,
            description,
            tags,
            content_size,
            mime_type,
            price,
            total_queries,
            total_revenue,
            access_control,
            provenance,
            created_at,
            updated_at,
        ) = Self::serialize_manifest(manifest)?;

        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR IGNORE INTO manifests (
                hash, content_type, owner, version_number, version_previous,
                version_root, version_timestamp, visibility, title, description,
                tags, content_size, mime_type, price, total_queries,
                total_revenue, access_control, provenance, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
                      ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)",
            params![
                hash,
                content_type,
                owner,
                version_number,
                version_previous,
                version_root,
                version_timestamp,
                visibility,
                title,
                description,
                tags,
                content_size,
                mime_type,
                price,
                total_queries,
                total_revenue,
                access_control,
                provenance,
                created_at,
                updated_at,
            ],
        )?;

        Ok(())
    }

    fn load(&self, hash: &Hash) -> Result<Option<Manifest>> {
        let conn = self.conn.lock().unwrap();
        let hash_bytes = hash.0.to_vec();

        let manifest = conn
            .query_row(
                "SELECT hash, content_type, owner, version_number, version_previous,
                        version_root, version_timestamp, visibility, title, description,
                        tags, content_size, mime_type, price, total_queries,
                        total_revenue, access_control, provenance, created_at, updated_at
                 FROM manifests WHERE hash = ?1",
                [hash_bytes],
                Self::deserialize_row,
            )
            .optional()?;

        Ok(manifest)
    }

    fn update(&mut self, manifest: &Manifest) -> Result<()> {
        let (
            hash,
            content_type,
            owner,
            version_number,
            version_previous,
            version_root,
            version_timestamp,
            visibility,
            title,
            description,
            tags,
            content_size,
            mime_type,
            price,
            total_queries,
            total_revenue,
            access_control,
            provenance,
            _created_at, // Don't update created_at
            updated_at,
        ) = Self::serialize_manifest(manifest)?;

        let conn = self.conn.lock().unwrap();
        let rows_affected = conn.execute(
            "UPDATE manifests SET
                content_type = ?2, owner = ?3, version_number = ?4, version_previous = ?5,
                version_root = ?6, version_timestamp = ?7, visibility = ?8, title = ?9,
                description = ?10, tags = ?11, content_size = ?12, mime_type = ?13,
                price = ?14, total_queries = ?15, total_revenue = ?16,
                access_control = ?17, provenance = ?18, updated_at = ?19
             WHERE hash = ?1",
            params![
                hash,
                content_type,
                owner,
                version_number,
                version_previous,
                version_root,
                version_timestamp,
                visibility,
                title,
                description,
                tags,
                content_size,
                mime_type,
                price,
                total_queries,
                total_revenue,
                access_control,
                provenance,
                updated_at,
            ],
        )?;

        if rows_affected == 0 {
            return Err(StoreError::ManifestNotFound(manifest.hash));
        }

        Ok(())
    }

    fn delete(&mut self, hash: &Hash) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let hash_bytes = hash.0.to_vec();
        conn.execute("DELETE FROM manifests WHERE hash = ?1", [hash_bytes])?;
        Ok(())
    }

    fn list(&self, filter: ManifestFilter) -> Result<Vec<Manifest>> {
        let conn = self.conn.lock().unwrap();

        let mut sql = String::from(
            "SELECT hash, content_type, owner, version_number, version_previous,
                    version_root, version_timestamp, visibility, title, description,
                    tags, content_size, mime_type, price, total_queries,
                    total_revenue, access_control, provenance, created_at, updated_at
             FROM manifests WHERE 1=1",
        );

        let mut params_values: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        let mut param_idx = 1;

        if let Some(visibility) = filter.visibility {
            sql.push_str(&format!(" AND visibility = ?{}", param_idx));
            params_values.push(Box::new(visibility as u8));
            param_idx += 1;
        }

        if let Some(content_type) = filter.content_type {
            sql.push_str(&format!(" AND content_type = ?{}", param_idx));
            params_values.push(Box::new(content_type as u8));
            param_idx += 1;
        }

        if let Some(created_after) = filter.created_after {
            sql.push_str(&format!(" AND created_at >= ?{}", param_idx));
            params_values.push(Box::new(created_after as i64));
            param_idx += 1;
        }

        if let Some(created_before) = filter.created_before {
            sql.push_str(&format!(" AND created_at <= ?{}", param_idx));
            params_values.push(Box::new(created_before as i64));
            param_idx += 1;
        }

        if let Some(owner) = filter.owner {
            sql.push_str(&format!(" AND owner = ?{}", param_idx));
            params_values.push(Box::new(owner.0.to_vec()));
            // param_idx is not incremented here as it's the last filter
        }

        sql.push_str(" ORDER BY created_at DESC");

        if let Some(limit) = filter.limit {
            sql.push_str(&format!(" LIMIT {}", limit));
        }

        if let Some(offset) = filter.offset {
            sql.push_str(&format!(" OFFSET {}", offset));
        }

        let params_refs: Vec<&dyn rusqlite::ToSql> =
            params_values.iter().map(|p| p.as_ref()).collect();

        let mut stmt = conn.prepare(&sql)?;
        let manifests = stmt
            .query_map(params_refs.as_slice(), Self::deserialize_row)?
            .filter_map(|r| r.ok())
            .collect();

        Ok(manifests)
    }

    fn get_versions(&self, version_root: &Hash) -> Result<Vec<Manifest>> {
        let conn = self.conn.lock().unwrap();
        let root_bytes = version_root.0.to_vec();

        let mut stmt = conn.prepare(
            "SELECT hash, content_type, owner, version_number, version_previous,
                    version_root, version_timestamp, visibility, title, description,
                    tags, content_size, mime_type, price, total_queries,
                    total_revenue, access_control, provenance, created_at, updated_at
             FROM manifests WHERE version_root = ?1 ORDER BY version_number ASC",
        )?;

        let manifests = stmt
            .query_map([root_bytes], Self::deserialize_row)?
            .filter_map(|r| r.ok())
            .collect();

        Ok(manifests)
    }
}

/// Convert bytes to Hash.
fn bytes_to_hash(bytes: &[u8]) -> Hash {
    let mut arr = [0u8; 32];
    if bytes.len() >= 32 {
        arr.copy_from_slice(&bytes[..32]);
    }
    Hash(arr)
}

/// Convert bytes to PeerId.
fn bytes_to_peer_id(bytes: &[u8]) -> PeerId {
    let mut arr = [0u8; 20];
    if bytes.len() >= 20 {
        arr.copy_from_slice(&bytes[..20]);
    }
    PeerId::from_bytes(arr)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::initialize_schema;
    use nodalync_crypto::{content_hash, generate_identity, peer_id_from_public_key};
    use rusqlite::Connection;

    fn setup_store() -> SqliteManifestStore {
        let conn = Connection::open_in_memory().unwrap();
        initialize_schema(&conn).unwrap();
        SqliteManifestStore::new(Arc::new(Mutex::new(conn)))
    }

    fn test_manifest() -> Manifest {
        let (_, public_key) = generate_identity();
        let owner = peer_id_from_public_key(&public_key);
        let hash = content_hash(b"test content");
        let metadata = Metadata::new("Test", 100);
        Manifest::new_l0(hash, owner, metadata, 1234567890)
    }

    #[test]
    fn test_store_and_load() {
        let mut store = setup_store();
        let manifest = test_manifest();

        store.store(&manifest).unwrap();

        let loaded = store.load(&manifest.hash).unwrap();
        assert!(loaded.is_some());

        let loaded = loaded.unwrap();
        assert_eq!(loaded.hash, manifest.hash);
        assert_eq!(loaded.owner, manifest.owner);
        assert_eq!(loaded.content_type, manifest.content_type);
        assert_eq!(loaded.metadata.title, manifest.metadata.title);
    }

    #[test]
    fn test_store_idempotent() {
        let mut store = setup_store();
        let manifest = test_manifest();

        store.store(&manifest).unwrap();
        store.store(&manifest).unwrap(); // Should not error

        let loaded = store.load(&manifest.hash).unwrap();
        assert!(loaded.is_some());
    }

    #[test]
    fn test_load_nonexistent() {
        let store = setup_store();
        let hash = content_hash(b"nonexistent");

        let result = store.load(&hash).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_update() {
        let mut store = setup_store();
        let mut manifest = test_manifest();

        store.store(&manifest).unwrap();

        manifest.visibility = Visibility::Shared;
        manifest.economics.price = 1000;
        manifest.updated_at = 9999999999;

        store.update(&manifest).unwrap();

        let loaded = store.load(&manifest.hash).unwrap().unwrap();
        assert_eq!(loaded.visibility, Visibility::Shared);
        assert_eq!(loaded.economics.price, 1000);
    }

    #[test]
    fn test_update_nonexistent() {
        let mut store = setup_store();
        let manifest = test_manifest();

        let result = store.update(&manifest);
        assert!(matches!(result, Err(StoreError::ManifestNotFound(_))));
    }

    #[test]
    fn test_delete() {
        let mut store = setup_store();
        let manifest = test_manifest();

        store.store(&manifest).unwrap();
        store.delete(&manifest.hash).unwrap();

        let loaded = store.load(&manifest.hash).unwrap();
        assert!(loaded.is_none());
    }

    #[test]
    fn test_list_no_filter() {
        let mut store = setup_store();

        let m1 = test_manifest();
        let mut m2 = test_manifest();
        m2.metadata.title = "Second".to_string();

        // Create different hashes
        let (_, pk2) = generate_identity();
        m2.hash = content_hash(b"different content");
        m2.owner = peer_id_from_public_key(&pk2);

        store.store(&m1).unwrap();
        store.store(&m2).unwrap();

        let manifests = store.list(ManifestFilter::new()).unwrap();
        assert_eq!(manifests.len(), 2);
    }

    #[test]
    fn test_list_with_visibility_filter() {
        let mut store = setup_store();

        let mut m1 = test_manifest();
        m1.visibility = Visibility::Private;

        let mut m2 = test_manifest();
        m2.hash = content_hash(b"public content");
        m2.visibility = Visibility::Shared;

        store.store(&m1).unwrap();
        store.store(&m2).unwrap();

        let filter = ManifestFilter::new().with_visibility(Visibility::Shared);
        let manifests = store.list(filter).unwrap();
        assert_eq!(manifests.len(), 1);
        assert_eq!(manifests[0].visibility, Visibility::Shared);
    }

    #[test]
    fn test_list_with_limit() {
        let mut store = setup_store();

        for i in 0..5 {
            let mut m = test_manifest();
            m.hash = content_hash(&format!("content{}", i).into_bytes());
            store.store(&m).unwrap();
        }

        let filter = ManifestFilter::new().limit(3);
        let manifests = store.list(filter).unwrap();
        assert_eq!(manifests.len(), 3);
    }

    #[test]
    fn test_get_versions() {
        let mut store = setup_store();

        // Create v1
        let mut m1 = test_manifest();
        let version_root = m1.hash;
        m1.version = Version::new_v1(m1.hash, 1000);

        store.store(&m1).unwrap();

        // Create v2 with same root
        let mut m2 = test_manifest();
        m2.hash = content_hash(b"version 2 content");
        m2.version = Version {
            number: 2,
            previous: Some(m1.hash),
            root: version_root,
            timestamp: 2000,
        };

        store.store(&m2).unwrap();

        let versions = store.get_versions(&version_root).unwrap();
        assert_eq!(versions.len(), 2);
        assert_eq!(versions[0].version.number, 1);
        assert_eq!(versions[1].version.number, 2);
    }

    #[test]
    fn test_manifest_with_metadata() {
        let mut store = setup_store();

        let (_, public_key) = generate_identity();
        let owner = peer_id_from_public_key(&public_key);
        let hash = content_hash(b"rich content");
        let metadata = Metadata::new("Rich Title", 500)
            .with_description("A detailed description")
            .with_tags(vec!["tag1".to_string(), "tag2".to_string()])
            .with_mime_type("text/plain");

        let manifest = Manifest::new_l0(hash, owner, metadata, 1234567890);
        store.store(&manifest).unwrap();

        let loaded = store.load(&hash).unwrap().unwrap();
        assert_eq!(loaded.metadata.title, "Rich Title");
        assert_eq!(
            loaded.metadata.description,
            Some("A detailed description".to_string())
        );
        assert_eq!(loaded.metadata.tags, vec!["tag1", "tag2"]);
        assert_eq!(loaded.metadata.mime_type, Some("text/plain".to_string()));
    }
}
