//! SQLite storage of every captured exchange.
//!
//! The proxy is the single writer; WAL mode lets a later read-only process
//! (e.g. the future SPA's API) read the same file concurrently without
//! blocking the proxy.

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions};

/// One row in the `captures` table: a single client<->upstream exchange.
#[derive(Debug, Clone)]
pub struct Capture {
    pub created_at: String,
    pub total_ms: i64,
    pub client_addr: String,

    pub request_version: String,
    pub request_method: String,
    /// The request target; sensitive query params are masked.
    pub request_target: String,
    /// JSON array of `{"name":..,"value":..}` in original header order;
    /// values masked for sensitive names.
    pub request_headers: String,
    /// Stored body bytes (may be truncated to the cap); sensitive fields
    /// masked.
    pub request_body: Option<Vec<u8>>,
    pub request_body_len: i64,
    pub request_body_truncated: i64,
    /// SHA-256 hex of the full in-memory request body, if we had it; hashes
    /// the original on-wire bytes, which may differ from the stored (masked)
    /// blob.
    pub request_body_sha256: Option<String>,

    pub upstream_scheme: String,
    pub upstream_authority: String,
    pub upstream_path: String,
    /// The upstream query string; sensitive params masked.
    pub upstream_query: Option<String>,

    pub response_version: Option<String>,
    pub status_code: Option<i32>,
    /// JSON array of `{"name":..,"value":..}` in original header order;
    /// values masked for sensitive names.
    pub response_headers: String,
    /// Stored body bytes (may be truncated to the cap); sensitive fields
    /// masked.
    pub response_body: Option<Vec<u8>>,
    pub response_body_len: i64,
    pub response_body_truncated: i64,
    /// SHA-256 hex of the full in-memory response body, if we had it; hashes
    /// the original on-wire bytes, which may differ from the stored (masked)
    /// blob.
    pub response_body_sha256: Option<String>,

    /// NULL on success; a human-readable reason for a failed exchange.
    pub error: Option<String>,
}

pub async fn init(path: &str) -> anyhow::Result<SqlitePool> {
    let opts = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .busy_timeout(std::time::Duration::from_secs(5));
    let pool = SqlitePoolOptions::new()
        .max_connections(8)
        .connect_with(opts)
        .await?;

    // Schema. `IF NOT EXISTS` keeps re-runs idempotent.
    const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS captures (
    id                       INTEGER PRIMARY KEY AUTOINCREMENT,
    created_at               TEXT    NOT NULL,
    total_ms                 INTEGER NOT NULL,
    client_addr              TEXT    NOT NULL,

    request_version          TEXT    NOT NULL,
    request_method           TEXT    NOT NULL,
    request_target           TEXT    NOT NULL,
    request_headers          TEXT    NOT NULL,
    request_body             BLOB,
    request_body_len        INTEGER NOT NULL,
    request_body_truncated  INTEGER NOT NULL,
    request_body_sha256     TEXT,

    upstream_scheme          TEXT    NOT NULL,
    upstream_authority       TEXT    NOT NULL,
    upstream_path            TEXT    NOT NULL,
    upstream_query           TEXT,

    response_version         TEXT,
    status_code              INTEGER,
    response_headers         TEXT    NOT NULL,
    response_body            BLOB,
    response_body_len       INTEGER NOT NULL,
    response_body_truncated INTEGER NOT NULL,
    response_body_sha256     TEXT,

    error                    TEXT
);
CREATE INDEX IF NOT EXISTS idx_captures_created ON captures(created_at);
CREATE INDEX IF NOT EXISTS idx_captures_method ON captures(request_method);
CREATE INDEX IF NOT EXISTS idx_captures_status ON captures(status_code);
CREATE INDEX IF NOT EXISTS idx_captures_path   ON captures(upstream_path);
"#;
    sqlx::raw_sql(SCHEMA).execute(&pool).await?;
    Ok(pool)
}

pub async fn insert(pool: &SqlitePool, c: &Capture) -> Result<i64, sqlx::Error> {
    let mut q = sqlx::query(
        "INSERT INTO captures (
            created_at, total_ms, client_addr,
            request_version, request_method, request_target, request_headers,
            request_body, request_body_len, request_body_truncated, request_body_sha256,
            upstream_scheme, upstream_authority, upstream_path, upstream_query,
            response_version, status_code, response_headers,
            response_body, response_body_len, response_body_truncated, response_body_sha256,
            error
        ) VALUES (
            ?, ?, ?,
            ?, ?, ?, ?,
            ?, ?, ?, ?,
            ?, ?, ?, ?,
            ?, ?, ?,
            ?, ?, ?, ?,
            ?
        )",
    );
    q = q
        .bind(&c.created_at)
        .bind(c.total_ms)
        .bind(&c.client_addr)
        .bind(&c.request_version)
        .bind(&c.request_method)
        .bind(&c.request_target)
        .bind(&c.request_headers)
        .bind(c.request_body.as_deref())
        .bind(c.request_body_len)
        .bind(c.request_body_truncated)
        .bind(&c.request_body_sha256)
        .bind(&c.upstream_scheme)
        .bind(&c.upstream_authority)
        .bind(&c.upstream_path)
        .bind(&c.upstream_query)
        .bind(&c.response_version)
        .bind(c.status_code)
        .bind(&c.response_headers)
        .bind(c.response_body.as_deref())
        .bind(c.response_body_len)
        .bind(c.response_body_truncated)
        .bind(&c.response_body_sha256)
        .bind(&c.error);
    let res = q.execute(pool).await?;
    Ok(res.last_insert_rowid())
}

/// Delete captures whose `created_at` is older than `days` days.
/// Returns the number of rows deleted.
///
/// After deletion a `wal_checkpoint(TRUNCATE)` is run to reclaim disk space.
pub async fn prune(pool: &SqlitePool, days: u64) -> Result<u64, sqlx::Error> {
    let cutoff = chrono::Utc::now()
        - chrono::Duration::days(days as i64);
    let cutoff_str = cutoff.to_rfc3339();

    let result = sqlx::query("DELETE FROM captures WHERE created_at < ?1")
        .bind(&cutoff_str)
        .execute(pool)
        .await?;

    // Reclaim disk space after bulk deletes.
    sqlx::raw_sql("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(pool)
        .await?;

    Ok(result.rows_affected())
}
