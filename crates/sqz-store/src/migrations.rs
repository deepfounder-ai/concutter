use crate::db::StoreError;

const MIGRATION_001: &str = include_str!("../../../migrations/001_initial.sql");

/// Run all pending migrations against the given connection.
///
/// A `schema_version` table is created (if it does not already exist) to track
/// which migrations have been applied.  Each migration is applied inside a
/// transaction so that a partial failure leaves the database unchanged.
pub fn run_migrations(conn: &rusqlite::Connection) -> Result<(), StoreError> {
    // Ensure the version-tracking table exists.
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_version (
            version INTEGER PRIMARY KEY,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        );"
    ).map_err(|e| StoreError::Sqlite(e.to_string()))?;

    // Check the current version.
    let current_version: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_version",
            [],
            |row| row.get(0),
        )
        .map_err(|e| StoreError::Sqlite(e.to_string()))?;

    // Migration 1
    if current_version < 1 {
        tracing::info!("applying migration 001_initial");
        conn.execute_batch(MIGRATION_001)
            .map_err(|e| StoreError::Sqlite(e.to_string()))?;
        conn.execute(
            "INSERT INTO schema_version (version) VALUES (?1)",
            rusqlite::params![1],
        )
        .map_err(|e| StoreError::Sqlite(e.to_string()))?;
    }

    Ok(())
}
