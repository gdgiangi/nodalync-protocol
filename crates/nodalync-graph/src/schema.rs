use rusqlite::Connection;
use anyhow::Result;

/// Create all necessary tables for the L2 Graph layer
/// Based on desktop-app-spec.md Section 4.2 SQLite Schema
pub fn create_tables(conn: &Connection) -> Result<()> {
    // Content Registry - stable IDs for content that may be edited
    conn.execute(
        "CREATE TABLE IF NOT EXISTS content_registry (
            content_id TEXT PRIMARY KEY,
            current_hash BLOB NOT NULL,
            content_type TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            deleted_at INTEGER
        )",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_content_current_hash ON content_registry(current_hash)",
        [],
    )?;

    // Content Versions - track edit history
    conn.execute(
        "CREATE TABLE IF NOT EXISTS content_versions (
            hash BLOB PRIMARY KEY,
            content_id TEXT NOT NULL REFERENCES content_registry(content_id),
            previous_hash BLOB,
            version_number INTEGER NOT NULL,
            created_at INTEGER NOT NULL
        )",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_versions_content_id ON content_versions(content_id)",
        [],
    )?;

    // Entities Table - Source of Truth for L2 graph
    conn.execute(
        "CREATE TABLE IF NOT EXISTS entities (
            id TEXT PRIMARY KEY,
            canonical_label TEXT NOT NULL,
            entity_type TEXT NOT NULL,
            description TEXT,
            confidence REAL NOT NULL DEFAULT 0.8,
            first_seen INTEGER NOT NULL,
            last_updated INTEGER NOT NULL,
            source_count INTEGER NOT NULL DEFAULT 1,
            metadata_json TEXT
        )",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_entities_label ON entities(canonical_label)",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_entities_type ON entities(entity_type)",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_entities_updated ON entities(last_updated)",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_entities_source_count ON entities(source_count)",
        [],
    )?;

    // Entity Aliases Table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS entity_aliases (
            entity_id TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
            alias TEXT NOT NULL,
            PRIMARY KEY (entity_id, alias)
        )",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_aliases_alias ON entity_aliases(alias)",
        [],
    )?;

    // Relationships Table - Source of Truth for L2 graph
    conn.execute(
        "CREATE TABLE IF NOT EXISTS relationships (
            id TEXT PRIMARY KEY,
            subject_id TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
            predicate TEXT NOT NULL,
            object_type TEXT NOT NULL,
            object_value TEXT NOT NULL,
            confidence REAL NOT NULL DEFAULT 0.8,
            extracted_at INTEGER NOT NULL,
            metadata_json TEXT
        )",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_rel_subject ON relationships(subject_id)",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_rel_predicate ON relationships(predicate)",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_rel_object ON relationships(object_type, object_value)",
        [],
    )?;

    // Entity-Source Links - uses stable content_id
    conn.execute(
        "CREATE TABLE IF NOT EXISTS entity_sources (
            entity_id TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
            content_id TEXT NOT NULL REFERENCES content_registry(content_id),
            l1_mention_id TEXT,
            added_at INTEGER NOT NULL,
            PRIMARY KEY (entity_id, content_id)
        )",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_entity_sources_content ON entity_sources(content_id)",
        [],
    )?;

    // Relationship-Source Links - uses stable content_id
    conn.execute(
        "CREATE TABLE IF NOT EXISTS relationship_sources (
            relationship_id TEXT NOT NULL REFERENCES relationships(id) ON DELETE CASCADE,
            content_id TEXT NOT NULL REFERENCES content_registry(content_id),
            l1_mention_id TEXT,
            added_at INTEGER NOT NULL,
            PRIMARY KEY (relationship_id, content_id)
        )",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_rel_sources_content ON relationship_sources(content_id)",
        [],
    )?;

    // Review Queue - for conflicts and ambiguities
    conn.execute(
        "CREATE TABLE IF NOT EXISTS review_queue (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            item_type TEXT NOT NULL,
            priority INTEGER NOT NULL DEFAULT 0,
            entity_ids TEXT,
            content_id TEXT,
            reason TEXT NOT NULL,
            suggested_action TEXT,
            status TEXT NOT NULL DEFAULT 'pending',
            created_at INTEGER NOT NULL,
            resolved_at INTEGER
        )",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_review_status ON review_queue(status)",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_review_priority ON review_queue(priority DESC)",
        [],
    )?;

    // AI Processing Jobs - for async processing
    conn.execute(
        "CREATE TABLE IF NOT EXISTS ai_jobs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            job_type TEXT NOT NULL,
            content_id TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'queued',
            progress INTEGER DEFAULT 0,
            result_json TEXT,
            error_message TEXT,
            created_at INTEGER NOT NULL,
            started_at INTEGER,
            completed_at INTEGER,
            depends_on INTEGER,
            FOREIGN KEY (depends_on) REFERENCES ai_jobs(id)
        )",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_jobs_status ON ai_jobs(status, created_at)",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_jobs_content ON ai_jobs(content_id)",
        [],
    )?;

    // ID Counters - for entity/relationship ID generation
    conn.execute(
        "CREATE TABLE IF NOT EXISTS id_counters (
            counter_name TEXT PRIMARY KEY,
            next_value INTEGER NOT NULL DEFAULT 1
        )",
        [],
    )?;

    // Settings table - for configuration
    conn.execute(
        "CREATE TABLE IF NOT EXISTS settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL,
            encrypted INTEGER NOT NULL DEFAULT 0
        )",
        [],
    )?;

    Ok(())
}

/// Initialize counter values if they don't exist
pub fn initialize_counters(conn: &Connection) -> Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO id_counters (counter_name, next_value) VALUES ('entity', 1)",
        [],
    )?;
    
    conn.execute(
        "INSERT OR IGNORE INTO id_counters (counter_name, next_value) VALUES ('relationship', 1)",
        [],
    )?;

    Ok(())
}

/// Check if the database schema is up to date
pub fn check_schema_version(conn: &Connection) -> Result<u32> {
    match conn.query_row(
        "SELECT value FROM settings WHERE key = 'schema_version'",
        [],
        |row| row.get::<_, String>(0)
    ) {
        Ok(version_str) => Ok(version_str.parse().unwrap_or(0)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(0),
        Err(e) => Err(e.into()),
    }
}

/// Set the schema version
pub fn set_schema_version(conn: &Connection, version: u32) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO settings (key, value, encrypted) VALUES ('schema_version', ?1, 0)",
        [version.to_string()],
    )?;
    Ok(())
}

pub const CURRENT_SCHEMA_VERSION: u32 = 1;