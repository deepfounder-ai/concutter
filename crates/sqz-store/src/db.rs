use tokio_rusqlite::Connection;

use crate::migrations::run_migrations;
use crate::models::*;

// ---------------------------------------------------------------------------
// StoreError
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("SQLite error: {0}")]
    Sqlite(String),

    #[error("not found")]
    NotFound,

    #[error("{0}")]
    Other(String),
}

impl From<tokio_rusqlite::Error> for StoreError {
    fn from(err: tokio_rusqlite::Error) -> Self {
        StoreError::Sqlite(err.to_string())
    }
}

impl From<rusqlite::Error> for StoreError {
    fn from(err: rusqlite::Error) -> Self {
        StoreError::Sqlite(err.to_string())
    }
}

/// Helper: convert a `StoreError` to `StoreError::NotFound` when the message
/// indicates that no rows were returned.
fn not_found_or(err: StoreError) -> StoreError {
    match &err {
        StoreError::Sqlite(msg) if msg.contains("Query returned no rows") => StoreError::NotFound,
        _ => err,
    }
}

// ---------------------------------------------------------------------------
// Row-mapping helpers (avoids repeating the same 11-column extraction)
// ---------------------------------------------------------------------------

fn map_rule_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RuleRow> {
    Ok(RuleRow {
        id: row.get(0)?,
        pattern: row.get(1)?,
        replacement: row.get(2)?,
        layer: row.get(3)?,
        domain: row.get(4)?,
        confidence: row.get(5)?,
        samples: row.get(6)?,
        enabled: row.get::<_, bool>(7)?,
        priority: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

fn map_stat_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<CompressionStatRow> {
    Ok(CompressionStatRow {
        id: row.get(0)?,
        request_id: row.get(1)?,
        provider: row.get(2)?,
        model: row.get(3)?,
        domain_detected: row.get(4)?,
        original_tokens: row.get(5)?,
        compressed_tokens: row.get(6)?,
        compression_ratio: row.get(7)?,
        rules_applied: row.get(8)?,
        elapsed_us: row.get(9)?,
        created_at: row.get(10)?,
    })
}

fn map_experiment_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ExperimentRow> {
    Ok(ExperimentRow {
        id: row.get(0)?,
        rule_id: row.get(1)?,
        original_prompt: row.get(2)?,
        compressed_prompt: row.get(3)?,
        original_response: row.get(4)?,
        compressed_response: row.get(5)?,
        similarity_score: row.get(6)?,
        status: row.get(7)?,
        created_at: row.get(8)?,
        completed_at: row.get(9)?,
    })
}

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

pub struct Store {
    conn: Connection,
}

impl Store {
    // -----------------------------------------------------------------------
    // Construction
    // -----------------------------------------------------------------------

    /// Open (or create) a database at the given filesystem path, apply any
    /// outstanding migrations, and enable WAL mode.
    pub async fn new(path: &str) -> Result<Self, StoreError> {
        let conn = Connection::open(path).await?;

        conn.call(|conn| {
            run_migrations(conn).map_err(|e| {
                tokio_rusqlite::Error::Rusqlite(rusqlite::Error::ToSqlConversionFailure(
                    Box::new(e),
                ))
            })?;
            conn.execute_batch("PRAGMA journal_mode = WAL;")?;
            Ok(())
        })
        .await?;

        Ok(Store { conn })
    }

    /// Create an in-memory database (useful for tests).
    pub async fn new_in_memory() -> Result<Self, StoreError> {
        let conn = Connection::open_in_memory().await?;

        conn.call(|conn| {
            run_migrations(conn).map_err(|e| {
                tokio_rusqlite::Error::Rusqlite(rusqlite::Error::ToSqlConversionFailure(
                    Box::new(e),
                ))
            })?;
            Ok(())
        })
        .await?;

        Ok(Store { conn })
    }

    // -----------------------------------------------------------------------
    // Rules – CRUD
    // -----------------------------------------------------------------------

    /// List rules with optional filters, limit and offset.
    pub async fn list_rules(
        &self,
        filter_layer: Option<String>,
        filter_domain: Option<String>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<RuleRow>, StoreError> {
        self.conn
            .call(move |conn| {
                let mut sql = String::from(
                    "SELECT id, pattern, replacement, layer, domain, confidence, samples, \
                     enabled, priority, created_at, updated_at FROM rules WHERE 1=1",
                );
                let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

                if let Some(ref layer) = filter_layer {
                    sql.push_str(" AND layer = ?");
                    params.push(Box::new(layer.clone()));
                }
                if let Some(ref domain) = filter_domain {
                    sql.push_str(" AND domain = ?");
                    params.push(Box::new(domain.clone()));
                }
                sql.push_str(" ORDER BY priority DESC, created_at DESC LIMIT ? OFFSET ?");
                params.push(Box::new(limit));
                params.push(Box::new(offset));

                let param_refs: Vec<&dyn rusqlite::types::ToSql> =
                    params.iter().map(|p| p.as_ref()).collect();

                let mut stmt = conn.prepare(&sql)?;
                let rows = stmt
                    .query_map(param_refs.as_slice(), map_rule_row)?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            })
            .await
            .map_err(StoreError::from)
    }

    /// Retrieve a single rule by id.
    pub async fn get_rule(&self, id: &str) -> Result<RuleRow, StoreError> {
        let id = id.to_owned();
        self.conn
            .call(move |conn| {
                conn.query_row(
                    "SELECT id, pattern, replacement, layer, domain, confidence, samples, \
                     enabled, priority, created_at, updated_at FROM rules WHERE id = ?1",
                    rusqlite::params![id],
                    map_rule_row,
                )
                .map_err(tokio_rusqlite::Error::from)
            })
            .await
            .map_err(StoreError::from)
            .map_err(not_found_or)
    }

    /// Insert a new rule.
    pub async fn create_rule(&self, rule: &RuleRow) -> Result<(), StoreError> {
        let rule = rule.clone();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO rules (id, pattern, replacement, layer, domain, confidence, \
                     samples, enabled, priority, created_at, updated_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                    rusqlite::params![
                        rule.id,
                        rule.pattern,
                        rule.replacement,
                        rule.layer,
                        rule.domain,
                        rule.confidence,
                        rule.samples,
                        rule.enabled,
                        rule.priority,
                        rule.created_at,
                        rule.updated_at,
                    ],
                )?;
                Ok(())
            })
            .await
            .map_err(StoreError::from)
    }

    /// Update an existing rule (matched by id).
    pub async fn update_rule(&self, rule: &RuleRow) -> Result<(), StoreError> {
        let rule = rule.clone();
        self.conn
            .call(move |conn| {
                let updated = conn.execute(
                    "UPDATE rules SET pattern = ?1, replacement = ?2, layer = ?3, domain = ?4, \
                     confidence = ?5, samples = ?6, enabled = ?7, priority = ?8, \
                     updated_at = ?9 WHERE id = ?10",
                    rusqlite::params![
                        rule.pattern,
                        rule.replacement,
                        rule.layer,
                        rule.domain,
                        rule.confidence,
                        rule.samples,
                        rule.enabled,
                        rule.priority,
                        rule.updated_at,
                        rule.id,
                    ],
                )?;
                if updated == 0 {
                    return Err(tokio_rusqlite::Error::Rusqlite(
                        rusqlite::Error::QueryReturnedNoRows,
                    ));
                }
                Ok(())
            })
            .await
            .map_err(StoreError::from)
            .map_err(not_found_or)
    }

    /// Delete a rule by id.
    pub async fn delete_rule(&self, id: &str) -> Result<(), StoreError> {
        let id = id.to_owned();
        self.conn
            .call(move |conn| {
                let deleted =
                    conn.execute("DELETE FROM rules WHERE id = ?1", rusqlite::params![id])?;
                if deleted == 0 {
                    return Err(tokio_rusqlite::Error::Rusqlite(
                        rusqlite::Error::QueryReturnedNoRows,
                    ));
                }
                Ok(())
            })
            .await
            .map_err(StoreError::from)
            .map_err(not_found_or)
    }

    /// Return every enabled rule (for compressor rebuild).
    pub async fn get_all_enabled_rules(&self) -> Result<Vec<RuleRow>, StoreError> {
        self.conn
            .call(|conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, pattern, replacement, layer, domain, confidence, samples, \
                     enabled, priority, created_at, updated_at FROM rules WHERE enabled = 1 \
                     ORDER BY priority DESC",
                )?;
                let rows = stmt
                    .query_map([], map_rule_row)?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            })
            .await
            .map_err(StoreError::from)
    }

    /// Return rules where layer = 'learned'.
    pub async fn get_learned_rules(&self) -> Result<Vec<RuleRow>, StoreError> {
        self.conn
            .call(|conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, pattern, replacement, layer, domain, confidence, samples, \
                     enabled, priority, created_at, updated_at FROM rules WHERE layer = 'learned' \
                     ORDER BY confidence DESC",
                )?;
                let rows = stmt
                    .query_map([], map_rule_row)?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            })
            .await
            .map_err(StoreError::from)
    }

    // -----------------------------------------------------------------------
    // Compression stats
    // -----------------------------------------------------------------------

    /// Record a new compression stat entry.
    pub async fn record_compression_stat(
        &self,
        stat: &CompressionStatRow,
    ) -> Result<(), StoreError> {
        let stat = stat.clone();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO compression_stats (id, request_id, provider, model, \
                     domain_detected, original_tokens, compressed_tokens, compression_ratio, \
                     rules_applied, elapsed_us, created_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                    rusqlite::params![
                        stat.id,
                        stat.request_id,
                        stat.provider,
                        stat.model,
                        stat.domain_detected,
                        stat.original_tokens,
                        stat.compressed_tokens,
                        stat.compression_ratio,
                        stat.rules_applied,
                        stat.elapsed_us,
                        stat.created_at,
                    ],
                )?;
                Ok(())
            })
            .await
            .map_err(StoreError::from)
    }

    /// Compute an overview of compression statistics.
    pub async fn get_stats_overview(&self) -> Result<StatsOverview, StoreError> {
        self.conn
            .call(|conn| {
                let (total_requests, total_tokens_saved, avg_compression_ratio) = conn
                    .query_row(
                        "SELECT \
                            COUNT(*), \
                            COALESCE(SUM(original_tokens - compressed_tokens), 0), \
                            COALESCE(AVG(compression_ratio), 0.0) \
                         FROM compression_stats",
                        [],
                        |row| {
                            Ok((
                                row.get::<_, i64>(0)?,
                                row.get::<_, i64>(1)?,
                                row.get::<_, f64>(2)?,
                            ))
                        },
                    )?;

                let total_rules: i64 =
                    conn.query_row("SELECT COUNT(*) FROM rules", [], |row| row.get(0))?;

                let active_rules: i64 = conn.query_row(
                    "SELECT COUNT(*) FROM rules WHERE enabled = 1",
                    [],
                    |row| row.get(0),
                )?;

                Ok(StatsOverview {
                    total_requests,
                    total_tokens_saved,
                    avg_compression_ratio,
                    total_rules,
                    active_rules,
                })
            })
            .await
            .map_err(StoreError::from)
    }

    /// Retrieve recent compression stats with limit/offset pagination.
    pub async fn get_compression_stats(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<CompressionStatRow>, StoreError> {
        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, request_id, provider, model, domain_detected, original_tokens, \
                     compressed_tokens, compression_ratio, rules_applied, elapsed_us, created_at \
                     FROM compression_stats ORDER BY created_at DESC LIMIT ?1 OFFSET ?2",
                )?;
                let rows = stmt
                    .query_map(rusqlite::params![limit, offset], map_stat_row)?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            })
            .await
            .map_err(StoreError::from)
    }

    /// Increment `times_applied`, add `tokens_saved`, recompute the running
    /// average compression, and set `last_applied_at` to now.
    pub async fn update_rule_stats(
        &self,
        rule_id: &str,
        tokens_saved: i64,
    ) -> Result<(), StoreError> {
        let rule_id = rule_id.to_owned();
        self.conn
            .call(move |conn| {
                // Upsert into rule_stats: insert if missing, then update.
                conn.execute(
                    "INSERT INTO rule_stats (rule_id, times_applied, total_tokens_saved, \
                     avg_compression, last_applied_at) \
                     VALUES (?1, 0, 0, 0.0, NULL) \
                     ON CONFLICT(rule_id) DO NOTHING",
                    rusqlite::params![rule_id],
                )?;

                conn.execute(
                    "UPDATE rule_stats SET \
                        times_applied = times_applied + 1, \
                        total_tokens_saved = total_tokens_saved + ?1, \
                        avg_compression = CAST((total_tokens_saved + ?1) AS REAL) \
                            / (times_applied + 1), \
                        last_applied_at = datetime('now') \
                     WHERE rule_id = ?2",
                    rusqlite::params![tokens_saved, rule_id],
                )?;

                Ok(())
            })
            .await
            .map_err(StoreError::from)
    }

    // -----------------------------------------------------------------------
    // Experiments
    // -----------------------------------------------------------------------

    /// Create a new experiment record.
    pub async fn create_experiment(&self, exp: &ExperimentRow) -> Result<(), StoreError> {
        let exp = exp.clone();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO experiments (id, rule_id, original_prompt, compressed_prompt, \
                     original_response, compressed_response, similarity_score, status, \
                     created_at, completed_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                    rusqlite::params![
                        exp.id,
                        exp.rule_id,
                        exp.original_prompt,
                        exp.compressed_prompt,
                        exp.original_response,
                        exp.compressed_response,
                        exp.similarity_score,
                        exp.status,
                        exp.created_at,
                        exp.completed_at,
                    ],
                )?;
                Ok(())
            })
            .await
            .map_err(StoreError::from)
    }

    /// Update an existing experiment record.
    pub async fn update_experiment(&self, exp: &ExperimentRow) -> Result<(), StoreError> {
        let exp = exp.clone();
        self.conn
            .call(move |conn| {
                let updated = conn.execute(
                    "UPDATE experiments SET \
                        original_response = ?1, \
                        compressed_response = ?2, \
                        similarity_score = ?3, \
                        status = ?4, \
                        completed_at = ?5 \
                     WHERE id = ?6",
                    rusqlite::params![
                        exp.original_response,
                        exp.compressed_response,
                        exp.similarity_score,
                        exp.status,
                        exp.completed_at,
                        exp.id,
                    ],
                )?;
                if updated == 0 {
                    return Err(tokio_rusqlite::Error::Rusqlite(
                        rusqlite::Error::QueryReturnedNoRows,
                    ));
                }
                Ok(())
            })
            .await
            .map_err(StoreError::from)
            .map_err(not_found_or)
    }

    /// List experiments with limit/offset pagination.
    pub async fn list_experiments(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ExperimentRow>, StoreError> {
        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, rule_id, original_prompt, compressed_prompt, original_response, \
                     compressed_response, similarity_score, status, created_at, completed_at \
                     FROM experiments ORDER BY created_at DESC LIMIT ?1 OFFSET ?2",
                )?;
                let rows = stmt
                    .query_map(rusqlite::params![limit, offset], map_experiment_row)?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            })
            .await
            .map_err(StoreError::from)
    }

    /// Update a rule's confidence and sample count (typically after an
    /// experiment completes).
    pub async fn update_rule_confidence(
        &self,
        rule_id: &str,
        confidence: f64,
        samples: i64,
    ) -> Result<(), StoreError> {
        let rule_id = rule_id.to_owned();
        self.conn
            .call(move |conn| {
                let updated = conn.execute(
                    "UPDATE rules SET confidence = ?1, samples = ?2, \
                     updated_at = datetime('now') WHERE id = ?3",
                    rusqlite::params![confidence, samples, rule_id],
                )?;
                if updated == 0 {
                    return Err(tokio_rusqlite::Error::Rusqlite(
                        rusqlite::Error::QueryReturnedNoRows,
                    ));
                }
                Ok(())
            })
            .await
            .map_err(StoreError::from)
            .map_err(not_found_or)
    }
}
