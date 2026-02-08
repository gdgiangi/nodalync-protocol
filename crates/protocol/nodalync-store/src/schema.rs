//! SQL schema initialization.
//!
//! This module defines the database schema for SQLite storage.

use rusqlite::Connection;

use crate::error::Result;

/// Schema version for migration tracking.
pub const SCHEMA_VERSION: u32 = 4;

/// Initialize the database schema.
///
/// Creates all tables and indexes if they don't exist.
/// This function is idempotent - calling it multiple times is safe.
pub fn initialize_schema(conn: &Connection) -> Result<()> {
    // Create schema version table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS schema_version (
            version INTEGER PRIMARY KEY
        )",
        [],
    )?;

    // Check current version
    let current_version: Option<u32> = conn
        .query_row("SELECT version FROM schema_version LIMIT 1", [], |row| {
            row.get(0)
        })
        .ok();

    match current_version {
        None => {
            // Fresh database - create all tables
            create_tables(conn)?;
            conn.execute(
                "INSERT INTO schema_version (version) VALUES (?1)",
                [SCHEMA_VERSION],
            )?;
        }
        Some(version) if version < SCHEMA_VERSION => {
            // Apply migrations
            migrate_schema(conn, version)?;
            conn.execute("UPDATE schema_version SET version = ?1", [SCHEMA_VERSION])?;
        }
        Some(_) => {
            // Current version is up to date
        }
    }

    Ok(())
}

/// Apply schema migrations from the given version to the current version.
fn migrate_schema(conn: &Connection, from_version: u32) -> Result<()> {
    // Migration from version 1 to 2: Add pending_close and pending_dispute columns to channels
    if from_version < 2 {
        // Add pending_close column (stores JSON)
        if let Err(e) = conn.execute("ALTER TABLE channels ADD COLUMN pending_close TEXT", []) {
            if !e.to_string().contains("duplicate column") {
                tracing::warn!(error = %e, "Failed to add pending_close column to channels");
            }
        }

        // Add pending_dispute column (stores JSON)
        if let Err(e) = conn.execute("ALTER TABLE channels ADD COLUMN pending_dispute TEXT", []) {
            if !e.to_string().contains("duplicate column") {
                tracing::warn!(error = %e, "Failed to add pending_dispute column to channels");
            }
        }
    }

    // Migration from version 2 to 3: Add funding_tx_id column to channels
    if from_version < 3 {
        if let Err(e) = conn.execute("ALTER TABLE channels ADD COLUMN funding_tx_id TEXT", []) {
            if !e.to_string().contains("duplicate column") {
                tracing::warn!(error = %e, "Failed to add funding_tx_id column to channels");
            }
        }
    }

    // Migration from version 3 to 4: Add UNIQUE constraint on settlement_queue(payment_id, recipient)
    if from_version < 4 {
        // Deduplicate existing rows before adding unique constraint
        conn.execute(
            "DELETE FROM settlement_queue WHERE rowid NOT IN (
                SELECT MIN(rowid) FROM settlement_queue GROUP BY payment_id, recipient
            )",
            [],
        )?;
        // Drop old non-unique index and create unique one
        conn.execute("DROP INDEX IF EXISTS idx_settlement_queue_payment_id", [])?;
        conn.execute(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_settlement_queue_payment_id ON settlement_queue(payment_id, recipient)",
            [],
        )?;
    }

    Ok(())
}

/// Create all database tables.
fn create_tables(conn: &Connection) -> Result<()> {
    // Manifests table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS manifests (
            hash BLOB PRIMARY KEY,
            content_type INTEGER NOT NULL,
            owner BLOB NOT NULL,
            version_number INTEGER NOT NULL,
            version_previous BLOB,
            version_root BLOB NOT NULL,
            version_timestamp INTEGER NOT NULL,
            visibility INTEGER NOT NULL,
            title TEXT NOT NULL,
            description TEXT,
            tags TEXT,
            content_size INTEGER NOT NULL,
            mime_type TEXT,
            price INTEGER NOT NULL,
            total_queries INTEGER NOT NULL DEFAULT 0,
            total_revenue INTEGER NOT NULL DEFAULT 0,
            access_control TEXT NOT NULL,
            provenance TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        )",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_manifests_visibility ON manifests(visibility)",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_manifests_version_root ON manifests(version_root)",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_manifests_created ON manifests(created_at)",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_manifests_owner ON manifests(owner)",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_manifests_content_type ON manifests(content_type)",
        [],
    )?;

    // Provenance forward edges table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS derived_from (
            content_hash BLOB NOT NULL,
            source_hash BLOB NOT NULL,
            PRIMARY KEY (content_hash, source_hash)
        )",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_derived_from_source ON derived_from(source_hash)",
        [],
    )?;

    // Cached flattened roots table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS root_cache (
            content_hash BLOB NOT NULL,
            root_hash BLOB NOT NULL,
            owner BLOB NOT NULL,
            visibility INTEGER NOT NULL,
            weight INTEGER NOT NULL DEFAULT 1,
            PRIMARY KEY (content_hash, root_hash)
        )",
        [],
    )?;

    // Payment channels table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS channels (
            peer_id BLOB PRIMARY KEY,
            channel_id BLOB NOT NULL,
            state INTEGER NOT NULL,
            my_balance INTEGER NOT NULL,
            their_balance INTEGER NOT NULL,
            nonce INTEGER NOT NULL,
            last_update INTEGER NOT NULL,
            pending_close TEXT,
            pending_dispute TEXT,
            funding_tx_id TEXT
        )",
        [],
    )?;

    // Pending payments table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS payments (
            id BLOB PRIMARY KEY,
            channel_peer BLOB NOT NULL,
            channel_id BLOB NOT NULL,
            amount INTEGER NOT NULL,
            recipient BLOB NOT NULL,
            query_hash BLOB NOT NULL,
            provenance TEXT NOT NULL,
            timestamp INTEGER NOT NULL,
            signature BLOB NOT NULL,
            settled INTEGER NOT NULL DEFAULT 0
        )",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_payments_channel ON payments(channel_peer)",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_payments_settled ON payments(settled)",
        [],
    )?;

    // Peers table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS peers (
            peer_id BLOB PRIMARY KEY,
            public_key BLOB NOT NULL,
            addresses TEXT NOT NULL,
            last_seen INTEGER NOT NULL,
            reputation INTEGER NOT NULL DEFAULT 0
        )",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_peers_last_seen ON peers(last_seen)",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_peers_reputation ON peers(reputation)",
        [],
    )?;

    // Cache metadata table (content stored on filesystem)
    conn.execute(
        "CREATE TABLE IF NOT EXISTS cache (
            hash BLOB PRIMARY KEY,
            source_peer BLOB NOT NULL,
            queried_at INTEGER NOT NULL,
            size_bytes INTEGER NOT NULL,
            payment_receipt TEXT NOT NULL
        )",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_cache_queried ON cache(queried_at)",
        [],
    )?;

    // Settlement queue table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS settlement_queue (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            payment_id BLOB NOT NULL,
            recipient BLOB NOT NULL,
            amount INTEGER NOT NULL,
            source_hash BLOB NOT NULL,
            queued_at INTEGER NOT NULL,
            settled INTEGER NOT NULL DEFAULT 0,
            batch_id BLOB
        )",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_settlement_queue_recipient ON settlement_queue(recipient)",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_settlement_queue_settled ON settlement_queue(settled)",
        [],
    )?;

    conn.execute(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_settlement_queue_payment_id ON settlement_queue(payment_id, recipient)",
        [],
    )?;

    // Settlement metadata table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS settlement_meta (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        )",
        [],
    )?;

    // L1 summaries table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS l1_summaries (
            l0_hash BLOB PRIMARY KEY,
            mention_count INTEGER NOT NULL,
            preview_mentions TEXT NOT NULL,
            primary_topics TEXT NOT NULL,
            summary TEXT NOT NULL
        )",
        [],
    )?;

    // Announcements table (content discovered from network)
    conn.execute(
        "CREATE TABLE IF NOT EXISTS announcements (
            hash BLOB PRIMARY KEY,
            content_type INTEGER NOT NULL,
            title TEXT NOT NULL,
            l1_summary TEXT NOT NULL,
            price INTEGER NOT NULL,
            addresses TEXT NOT NULL,
            received_at INTEGER NOT NULL,
            publisher_peer_id TEXT
        )",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_announcements_received ON announcements(received_at)",
        [],
    )?;

    // Migration: Add publisher_peer_id column if it doesn't exist (for existing DBs)
    // SQLite doesn't have IF NOT EXISTS for ALTER TABLE, so we check first
    let has_publisher_peer_id: bool = conn
        .prepare("SELECT publisher_peer_id FROM announcements LIMIT 1")
        .is_ok();
    if !has_publisher_peer_id {
        if let Err(e) = conn.execute(
            "ALTER TABLE announcements ADD COLUMN publisher_peer_id TEXT",
            [],
        ) {
            // Column may already exist from a concurrent migration - only warn for unexpected errors
            if !e.to_string().contains("duplicate column") {
                tracing::warn!(error = %e, "Failed to add publisher_peer_id column to announcements");
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn test_initialize_schema() {
        let conn = Connection::open_in_memory().unwrap();
        let result = initialize_schema(&conn);
        assert!(result.is_ok());
    }

    #[test]
    fn test_initialize_schema_idempotent() {
        let conn = Connection::open_in_memory().unwrap();

        // First initialization
        initialize_schema(&conn).unwrap();

        // Second initialization should succeed
        let result = initialize_schema(&conn);
        assert!(result.is_ok());
    }

    #[test]
    fn test_tables_exist() {
        let conn = Connection::open_in_memory().unwrap();
        initialize_schema(&conn).unwrap();

        // Verify tables exist by querying their structure
        let tables = [
            "manifests",
            "derived_from",
            "root_cache",
            "channels",
            "payments",
            "peers",
            "cache",
            "settlement_queue",
            "settlement_meta",
            "l1_summaries",
        ];

        for table in tables {
            let exists: i32 = conn
                .query_row(
                    &format!(
                        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='{}'",
                        table
                    ),
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(exists, 1, "Table {} should exist", table);
        }
    }

    #[test]
    fn test_schema_version() {
        let conn = Connection::open_in_memory().unwrap();
        initialize_schema(&conn).unwrap();

        let version: u32 = conn
            .query_row("SELECT version FROM schema_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION);
    }

    #[test]
    fn test_migration_v3_to_v4_dedup_settlement_queue() {
        let conn = Connection::open_in_memory().unwrap();

        // Create v3 schema manually
        conn.execute(
            "CREATE TABLE IF NOT EXISTS schema_version (version INTEGER PRIMARY KEY)",
            [],
        )
        .unwrap();
        conn.execute("INSERT INTO schema_version (version) VALUES (3)", [])
            .unwrap();

        // Create channels table (required by older migrations but already at v3)
        conn.execute(
            "CREATE TABLE IF NOT EXISTS channels (
                peer_id BLOB PRIMARY KEY,
                channel_id BLOB NOT NULL,
                state INTEGER NOT NULL,
                my_balance INTEGER NOT NULL,
                their_balance INTEGER NOT NULL,
                nonce INTEGER NOT NULL,
                last_update INTEGER NOT NULL,
                pending_close TEXT,
                pending_dispute TEXT,
                funding_tx_id TEXT
            )",
            [],
        )
        .unwrap();

        // Create settlement_queue table with non-unique index (v3 schema)
        conn.execute(
            "CREATE TABLE IF NOT EXISTS settlement_queue (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                payment_id BLOB NOT NULL,
                recipient BLOB NOT NULL,
                amount INTEGER NOT NULL,
                source_hash BLOB NOT NULL,
                queued_at INTEGER NOT NULL,
                settled INTEGER NOT NULL DEFAULT 0,
                batch_id BLOB
            )",
            [],
        )
        .unwrap();
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_settlement_queue_payment_id ON settlement_queue(payment_id)",
            [],
        )
        .unwrap();

        // Insert duplicate rows
        let payment_id = vec![1u8; 32];
        let recipient = vec![2u8; 20];
        conn.execute(
            "INSERT INTO settlement_queue (payment_id, recipient, amount, source_hash, queued_at, settled)
             VALUES (?1, ?2, 100, ?1, 1000, 0)",
            rusqlite::params![payment_id, recipient],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO settlement_queue (payment_id, recipient, amount, source_hash, queued_at, settled)
             VALUES (?1, ?2, 100, ?1, 1001, 0)",
            rusqlite::params![payment_id, recipient],
        )
        .unwrap();

        // Verify duplicates exist
        let count: i32 = conn
            .query_row("SELECT COUNT(*) FROM settlement_queue", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 2);

        // Run migration
        initialize_schema(&conn).unwrap();

        // Verify version was bumped
        let version: u32 = conn
            .query_row("SELECT version FROM schema_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION);

        // Verify duplicates were removed
        let count: i32 = conn
            .query_row("SELECT COUNT(*) FROM settlement_queue", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1, "Migration should deduplicate settlement_queue rows");

        // Verify unique index prevents new duplicates
        let result = conn.execute(
            "INSERT INTO settlement_queue (payment_id, recipient, amount, source_hash, queued_at, settled)
             VALUES (?1, ?2, 100, ?1, 1002, 0)",
            rusqlite::params![payment_id, recipient],
        );
        assert!(result.is_err(), "UNIQUE index should prevent duplicate payment_id+recipient");
    }

    #[test]
    fn test_migration_v2_to_v3() {
        let conn = Connection::open_in_memory().unwrap();

        // Simulate a v2 database by creating tables and setting version to 2
        conn.execute(
            "CREATE TABLE IF NOT EXISTS schema_version (version INTEGER PRIMARY KEY)",
            [],
        )
        .unwrap();
        conn.execute("INSERT INTO schema_version (version) VALUES (2)", [])
            .unwrap();

        // Create the channels table WITHOUT funding_tx_id (v2 schema)
        conn.execute(
            "CREATE TABLE IF NOT EXISTS channels (
                peer_id BLOB PRIMARY KEY,
                channel_id BLOB NOT NULL,
                state INTEGER NOT NULL,
                my_balance INTEGER NOT NULL,
                their_balance INTEGER NOT NULL,
                nonce INTEGER NOT NULL,
                last_update INTEGER NOT NULL,
                pending_close TEXT,
                pending_dispute TEXT
            )",
            [],
        )
        .unwrap();

        // Create settlement_queue table (needed for v3→v4 migration)
        conn.execute(
            "CREATE TABLE IF NOT EXISTS settlement_queue (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                payment_id BLOB NOT NULL,
                recipient BLOB NOT NULL,
                amount INTEGER NOT NULL,
                source_hash BLOB NOT NULL,
                queued_at INTEGER NOT NULL,
                settled INTEGER NOT NULL DEFAULT 0,
                batch_id BLOB
            )",
            [],
        )
        .unwrap();
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_settlement_queue_payment_id ON settlement_queue(payment_id)",
            [],
        )
        .unwrap();

        // Run migration
        initialize_schema(&conn).unwrap();

        // Verify version was bumped
        let version: u32 = conn
            .query_row("SELECT version FROM schema_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION);

        // Verify funding_tx_id column exists by querying table_info
        let has_column: bool = conn
            .prepare("PRAGMA table_info(channels)")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .filter_map(|r| r.ok())
            .any(|name| name == "funding_tx_id");
        assert!(
            has_column,
            "funding_tx_id column should exist after migration"
        );
    }
}
