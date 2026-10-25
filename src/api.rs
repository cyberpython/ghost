//! Read-only HTTP API over a `ghost` captures DB, plus optional static SPA serving.

use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::http::header::HeaderValue;
use axum::response::{IntoResponse, Json, Response};
use axum::routing::get;
use axum::Router;
use serde::Deserialize;
use serde_json::{json, Value};
use std::net::SocketAddr;
use std::path::Path as StdPath;
use tower_http::services::{ServeDir, ServeFile};

type R = Result<Json<Value>, StatusCode>;

// ── CLI args ────────────────────────────────────────────────────────────────

#[derive(clap::Parser)]
#[command(about = "read-only HTTP API over a ghost captures DB, plus the static SPA (web/dist)")]
#[command(after_help = "The captures DB is created by the proxy (`ghost --db …`); \
 ghost-api only reads it.")]
pub struct Args {
    #[arg(long)] pub db: String,
    #[arg(long, default_value = "127.0.0.1:8090")] pub listen: SocketAddr,
    #[arg(long, default_value = "web/dist")] pub static_dir: std::path::PathBuf,
}

// ── Read-only pool ──────────────────────────────────────────────────────────

pub async fn open_read_only(path: &str) -> anyhow::Result<sqlx::sqlite::SqlitePool> {
    // A read-only SQLite open can never create a missing file (SQLITE_CANTOPEN,
    // code 14) — check up front and tell the user what to do about it.
    if !StdPath::new(path).exists() {
        anyhow::bail!(
            "captures DB \"{path}\" not found — the proxy creates it; run \
             `ghost --db {path} ...` first (then re-run ghost-api with the same --db path)"
        );
    }
    let opts = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(path)
        .read_only(true);
    Ok(sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(8)
        .connect_with(opts)
        .await?)
}

// ── Row structs (field names == column names, required by sqlx::FromRow) ─────

#[derive(sqlx::FromRow)]
pub struct ListRow {
    pub id: i64,
    pub created_at: String,
    pub total_ms: i64,
    pub client_addr: String,
    pub request_method: String,
    pub request_target: String,
    pub upstream_path: String,
    pub upstream_query: Option<String>,
    pub status_code: Option<i32>,
    pub error: Option<String>,
    pub request_body_len: i64,
    pub request_body_truncated: i64,
    pub response_body_len: i64,
    pub response_body_truncated: i64,
}

#[derive(sqlx::FromRow)]
pub struct CaptureRow {
    pub id: i64,
    pub created_at: String,
    pub total_ms: i64,
    pub client_addr: String,
    pub request_version: String,
    pub request_method: String,
    pub request_target: String,
    pub request_headers: String,
    pub request_body: Option<Vec<u8>>,
    pub request_body_len: i64,
    pub request_body_truncated: i64,
    pub request_body_sha256: Option<String>,
    pub upstream_scheme: String,
    pub upstream_authority: String,
    pub upstream_path: String,
    pub upstream_query: Option<String>,
    pub response_version: Option<String>,
    pub status_code: Option<i32>,
    pub response_headers: String,
    pub response_body: Option<Vec<u8>>,
    pub response_body_len: i64,
    pub response_body_truncated: i64,
    pub response_body_sha256: Option<String>,
    pub error: Option<String>,
}

// Both body sets fetched in one row; the handler picks by `part`.
#[derive(sqlx::FromRow)]
struct BodyRow {
    request_body: Option<Vec<u8>>,
    request_body_len: i64,
    request_body_truncated: i64,
    response_body: Option<Vec<u8>>,
    response_body_len: i64,
    response_body_truncated: i64,
}

// ── Query struct (only serde dep — for Deserialize on this one struct) ───────

#[derive(Deserialize)]
pub struct ListQuery {
    pub method: Option<String>,
    pub status: Option<String>,
    pub path: Option<String>,
    pub client_addr: Option<String>,
    pub since: Option<String>,
    pub until: Option<String>,
    pub error: Option<String>,
    pub limit: Option<u64>,
    pub offset: Option<u64>,
}

#[derive(Deserialize)]
pub struct BodyQuery {
    pub part: Option<String>,
}

/// Normalize the status filter: "ERR"/"err"/"error" (case-insensitive) means
/// `status_code IS NULL` (failed/proxy errors have no HTTP status).
fn normalize_status(raw: Option<&str>) -> (Option<&str>, bool) {
    match raw {
        Some(s) if s.eq_ignore_ascii_case("err") || s.eq_ignore_ascii_case("error") => (None, true),
        Some(s) => (Some(s), false),
        None => (None, false),
    }
}

// ── Body view helper ────────────────────────────────────────────────────────

// ── Decode-on-read (FR-019) ────────────────────────────────────────────

/// Bounded inflate for the *view* only: stored bytes stay compressed on
/// disk (FR-004/FR-010 semantics); the decoded result never exceeds this
/// cap, so a decompression bomb falls back to the raw view instead of
/// exploding RAM. View-path only — there is no decode-on-write.
const MAX_DECODE_BYTES: usize = 8 * 1024 * 1024;

/// Inflate `bytes` for the view. Accepts a single `gzip`/`deflate` coding
/// (`deflate` = zlib per RFC 9110, with a raw-deflate retry since some
/// servers emit that); anything else — multi-coding lists, `br`, unknown —
/// is a silent no-op: the raw view, exactly as before this FR.
fn decode_body_view(bytes: &[u8], encoding: &str) -> Option<(Vec<u8>, &'static str)> {
    let enc = encoding.trim().to_ascii_lowercase();
    if enc.contains(',') {
        return None;
    }
    match enc.as_str() {
        "gzip" => {
            bounded_inflate(&mut flate2::read::MultiGzDecoder::new(bytes)).map(|b| (b, "gzip"))
        }
        "deflate" => bounded_inflate(&mut flate2::read::ZlibDecoder::new(bytes))
            .or_else(|| bounded_inflate(&mut flate2::read::DeflateDecoder::new(bytes)))
            .map(|b| (b, "deflate")),
        _ => None,
    }
}

fn bounded_inflate<R: std::io::Read>(r: &mut R) -> Option<Vec<u8>> {
    use std::io::Read as _;
    let mut out = Vec::new();
    r.take((MAX_DECODE_BYTES + 1) as u64)
        .read_to_end(&mut out)
        .ok()?;
    if out.len() > MAX_DECODE_BYTES {
        return None;
    }
    Some(out)
}

/// Pull the first value of a header out of the stored `[{name,value}]` JSON.
fn header_value(headers_json: &str, name: &str) -> Option<String> {
    let arr = serde_json::from_str::<Value>(headers_json).ok()?;
    arr.as_array()?
        .iter()
        .find(|h| {
            h.get("name")
                .and_then(|v| v.as_str())
                .is_some_and(|n| n.eq_ignore_ascii_case(name))
        })
        .and_then(|h| h.get("value").and_then(|v| v.as_str()))
        .map(|s| s.to_string())
}

fn body_view(
    bytes: Option<Vec<u8>>,
    truncated: i64,
    sha: Option<String>,
    encoding: Option<&str>,
) -> Value {
    let raw = bytes.unwrap_or_default();
    let (bytes, decoded_via) = match encoding {
        Some(e) => match decode_body_view(&raw, e) {
            Some((b, enc)) => (b, Some(enc)),
            None => (raw, None),
        },
        None => (raw, None),
    };
    let stored_len = bytes.len() as i64;
    let is_utf8 = std::str::from_utf8(&bytes).is_ok();
    let too_big = stored_len > 1024 * 1024;
    let text = if is_utf8 && !too_big {
        Some(String::from_utf8(bytes.to_vec()).unwrap())
    } else {
        None
    };
    let text_truncated = text.is_some() && truncated == 1;
    let binary = !is_utf8;
    let hex_preview = if stored_len > 0 {
        let n = bytes.len().min(1024);
        Some(bytes[..n]
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<Vec<_>>()
            .join(" "))
    } else {
        None
    };
    json!({
        "text": text,
        "text_truncated": text_truncated,
        "binary": binary,
        "hex_preview": hex_preview,
        "len": stored_len,
        "sha256": sha,
        "truncated": truncated == 1,
        "decoded_encoding": decoded_via,
    })
}

// ── Handlers ────────────────────────────────────────────────────────────────

/// Build the WHERE clause for the list filters. Shared by the JSON list
/// endpoint, the HAR export, and `/api/stats` so all three always agree on
/// what "the current filter set" means.
fn where_sql(q: &ListQuery) -> String {
    // Normalize status filter: "ERR"/"err"/"error" → match NULL status_code
    let (status_raw, is_err) = normalize_status(q.status.as_deref());

    // Build status filter in the correct position (2nd in WHERE clause)
    let status_filter = if is_err {
        "status_code IS NULL".into()
    } else if let Some(s) = status_raw {
        format!("status_code LIKE {} || '%'", s.replace('\'', "''"))
    } else {
        "1=1".into()
    };

    // Build WHERE clause with filters in the correct order matching bind order
    let method_filter = q.method.as_deref().map(|m| {
        let m_lower = m.to_lowercase().replace('\'', "''");
        format!("instr(LOWER(request_method), '{}') > 0", m_lower)
    });
    let path_filter = q.path.as_deref().map(|p| {
        let p_lower = p.to_lowercase().replace('\'', "''");
        format!("instr(LOWER(upstream_path), '{}') > 0", p_lower)
    });
    let client_filter = q.client_addr.as_deref().map(|c| {
        let c_lower = c.to_lowercase().replace('\'', "''");
        format!("instr(LOWER(client_addr), '{}') > 0", c_lower)
    });
    let since_filter = q.since.as_deref().map(|s| format!("created_at >= '{}'", s.replace('\'', "''")));
    let until_filter = q.until.as_deref().map(|u| format!("created_at <= '{}'", u.replace('\'', "''")));
    let error_filter = q.error.as_deref().map(|e| {
        let e_lower = e.to_lowercase().replace('\'', "''");
        format!("instr(LOWER(error), '{}') > 0", e_lower)
    });

    [
        method_filter.as_deref(),
        Some(status_filter.as_str()),
        path_filter.as_deref(),
        client_filter.as_deref(),
        since_filter.as_deref(),
        until_filter.as_deref(),
        error_filter.as_deref(),
    ]
    .iter()
    .filter_map(|x| *x)
    .collect::<Vec<_>>()
    .join("\n  AND ")
}

async fn list(
    State(pool): State<sqlx::sqlite::SqlitePool>,
    Query(q): Query<ListQuery>,
) -> R {
    let limit = q.limit.unwrap_or(50).min(500);
    let offset = q.offset.unwrap_or(0);
    let where_sql = where_sql(&q);

    let count_sql = format!("SELECT COUNT(*) FROM captures WHERE {}", where_sql);
    let count: i64 = sqlx::query_scalar(&count_sql)
        .fetch_one(&pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let rows_sql = format!(
        "SELECT id, created_at, total_ms, client_addr, request_method, request_target, \
         upstream_path, upstream_query, status_code, error, \
         request_body_len, request_body_truncated, response_body_len, response_body_truncated \
         FROM captures WHERE {} ORDER BY id DESC LIMIT ? OFFSET ?",
        where_sql
    );

    let rows: Vec<ListRow> = sqlx::query_as(&rows_sql)
        .bind(limit as i64)
        .bind(offset as i64)
        .fetch_all(&pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let captures: Vec<Value> = rows
        .into_iter()
        .map(|r| {
            json!({
                "id": r.id,
                "created_at": r.created_at,
                "total_ms": r.total_ms,
                "client_addr": r.client_addr,
                "request_method": r.request_method,
                "request_target": r.request_target,
                "upstream_path": r.upstream_path,
                "upstream_query": r.upstream_query,
                "status_code": r.status_code,
                "error": r.error,
                "request_body_len": r.request_body_len,
                "request_body_truncated": r.request_body_truncated,
                "response_body_len": r.response_body_len,
                "response_body_truncated": r.response_body_truncated,
            })
        })
        .collect();

    Ok(Json(json!({ "count": count, "captures": captures })))
}

async fn detail(
    Path(id): Path<u64>,
    State(pool): State<sqlx::sqlite::SqlitePool>,
) -> R {
    let row: CaptureRow = sqlx::query_as("SELECT * FROM captures WHERE id = ?")
        .bind(id as i64)
        .fetch_one(&pool)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    let request_headers =
        serde_json::from_str::<Value>(&row.request_headers)
            .unwrap_or(Value::Array(vec![]));
    let response_headers =
        serde_json::from_str::<Value>(&row.response_headers)
            .unwrap_or(Value::Array(vec![]));
    let req_encoding = header_value(&row.request_headers, "content-encoding");
    let resp_encoding = header_value(&row.response_headers, "content-encoding");

    Ok(Json(json!({
        "id": row.id,
        "created_at": row.created_at,
        "total_ms": row.total_ms,
        "client_addr": row.client_addr,
        "request_version": row.request_version,
        "request_method": row.request_method,
        "request_target": row.request_target,
        "request_headers": request_headers,
        "request_body": body_view(
            row.request_body,
            row.request_body_truncated,
            row.request_body_sha256,
            req_encoding.as_deref(),
        ),
        "upstream_scheme": row.upstream_scheme,
        "upstream_authority": row.upstream_authority,
        "upstream_path": row.upstream_path,
        "upstream_query": row.upstream_query,
        "response_version": row.response_version,
        "status_code": row.status_code,
        "response_headers": response_headers,
        "response_body": body_view(
            row.response_body,
            row.response_body_truncated,
            row.response_body_sha256,
            resp_encoding.as_deref(),
        ),
        "error": row.error,
    })))
}

// ── HAR 1.2 export ──────────────────────────────────────────────────────────

/// Export the current filter set as a HAR 1.2 log (attachment download).
async fn har_list(
    State(pool): State<sqlx::sqlite::SqlitePool>,
    Query(q): Query<ListQuery>,
) -> Result<Response, StatusCode> {
    let limit = q.limit.unwrap_or(50).min(500);
    let offset = q.offset.unwrap_or(0);
    let where_sql = where_sql(&q);

    let rows_sql = format!(
        "SELECT * FROM captures WHERE {} ORDER BY id DESC LIMIT ? OFFSET ?",
        where_sql
    );
    let rows: Vec<CaptureRow> = sqlx::query_as(&rows_sql)
        .bind(limit as i64)
        .bind(offset as i64)
        .fetch_all(&pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let log = crate::har::build_log(&rows);
    let body = serde_json::to_vec(&log).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut res = (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        )],
        body,
    )
        .into_response();
    res.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static(r#"attachment; filename="captures.har""#),
    );
    Ok(res)
}

/// Export a single capture as a HAR 1.2 log (attachment download).
async fn har_detail(
    Path(id): Path<u64>,
    State(pool): State<sqlx::sqlite::SqlitePool>,
) -> Result<Response, StatusCode> {
    let row: CaptureRow = sqlx::query_as("SELECT * FROM captures WHERE id = ?")
        .bind(id as i64)
        .fetch_one(&pool)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    let log = crate::har::build_log(std::slice::from_ref(&row));
    let body = serde_json::to_vec(&log).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut res = (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        )],
        body,
    )
        .into_response();
    res.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!(r#"attachment; filename="capture-{id}.har""#))
            .expect("valid content-disposition"),
    );
    Ok(res)
}

// Raw stored body bytes (for downloading binary bodies the SPA can't render).
async fn raw_body(
    Path(id): Path<u64>,
    Query(q): Query<BodyQuery>,
    State(pool): State<sqlx::sqlite::SqlitePool>,
) -> Result<Response, StatusCode> {
    let is_request = match q.part.as_deref() {
        Some("request") => true,
        Some("response") => false,
        _ => return Err(StatusCode::BAD_REQUEST),
    };
    let row: BodyRow = sqlx::query_as(
        "SELECT request_body, request_body_len, request_body_truncated, \
                response_body, response_body_len, response_body_truncated \
         FROM captures WHERE id = ?",
    )
    .bind(id as i64)
    .fetch_one(&pool)
    .await
    .map_err(|_| StatusCode::NOT_FOUND)?;
    let (stored, len, truncated) = if is_request {
        (row.request_body, row.request_body_len, row.request_body_truncated)
    } else {
        (row.response_body, row.response_body_len, row.response_body_truncated)
    };
    let bytes: Vec<u8> = match stored {
        Some(b) if !b.is_empty() => b,
        _ => return Err(StatusCode::NOT_FOUND),
    };
    let label = if is_request { "request" } else { "response" };
    let content_length = bytes.len();
    // Vec<u8>::into_response() sets application/octet-stream; add the rest.
    let mut res = bytes.into_response();
    res.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!(r#"attachment; filename="capture-{id}-{label}""#))
            .expect("valid content-disposition"),
    );
    res.headers_mut().insert(header::CONTENT_LENGTH, HeaderValue::from(content_length));
    res.headers_mut().insert(
        header::HeaderName::from_static("x-body-truncated"),
        HeaderValue::from(truncated),
    );
    res.headers_mut().insert(
        header::HeaderName::from_static("x-body-original-length"),
        HeaderValue::from(len),
    );
    Ok(res)
}

// S/N adjacency for the detail view's Prev/Next (FR-039): the nearest lower
// and higher capture ids, resolved — never assumed id±1, gaps are expected
// (retention pruning, path filters, insert failures). Lenient for unknown ids
// (deep links): the nearest existing neighbors still come back; ends → null.
async fn adjacent(
    Path(id): Path<u64>,
    State(pool): State<sqlx::sqlite::SqlitePool>,
) -> R {
    let prev: Option<i64> =
        sqlx::query_scalar("SELECT MAX(id) FROM captures WHERE id < ?")
            .bind(id as i64)
            .fetch_one(&pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let next: Option<i64> =
        sqlx::query_scalar("SELECT MIN(id) FROM captures WHERE id > ?")
            .bind(id as i64)
            .fetch_one(&pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(json!({ "prev": prev, "next": next })))
}

async fn facets(
    State(pool): State<sqlx::sqlite::SqlitePool>,
) -> R {
    let methods: Vec<String> =
        sqlx::query_scalar("SELECT DISTINCT request_method FROM captures ORDER BY request_method ASC")
            .fetch_all(&pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let statuses: Vec<i32> =
        sqlx::query_scalar(
            "SELECT DISTINCT status_code FROM captures WHERE status_code IS NOT NULL ORDER BY status_code ASC",
        )
        .fetch_all(&pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let errors: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM captures WHERE error IS NOT NULL",
    )
    .fetch_one(&pool)
    .await
    .map(|r: i64| r)
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(json!({
        "methods": methods,
        "statuses": statuses,
        "errors": errors,
    })))
}

// ── Stats (FR-016 KPI panel + FR-018 requests/hour graph) ───────────────────

/// One cell of the method × status-class breakdown (FR-016).
#[derive(sqlx::FromRow)]
struct MethodStatusRow {
    request_method: String,
    status_class: String,
    count: i64,
}

/// One hourly bucket (FR-018): `hour` is the start of the hour, `count` the
/// requests in it. Only hours with data are returned (empty hours are gaps).
#[derive(sqlx::FromRow)]
struct HourlyRow {
    hour: String,
    count: i64,
}

/// `GET /api/stats`: the single aggregate endpoint behind both the KPI panel
/// (FR-016) and the requests/hour graph (FR-018). It takes the same filter set
/// as `/api/captures` and returns:
///   * `total` — captures in the window;
///   * `by_method_status` — the method × status-class breakdown (FR-016);
///   * `hourly` — requests-per-hour buckets, only hours with data (FR-018).
/// The graph's Total/Avg/Peak chips are computed client-side from `hourly`;
/// the endpoint itself is a pure aggregation over the active filter set.
async fn stats(
    State(pool): State<sqlx::sqlite::SqlitePool>,
    Query(q): Query<ListQuery>,
) -> R {
    let where_sql = where_sql(&q);

    let total: i64 = sqlx::query_scalar(&format!(
        "SELECT COUNT(*) FROM captures WHERE {}", where_sql))
        .fetch_one(&pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let ms_sql = format!(
        "SELECT request_method, \
         CASE WHEN status_code IS NULL THEN 'err' \
              WHEN status_code >= 500 THEN '5xx' \
              WHEN status_code >= 400 THEN '4xx' \
              WHEN status_code >= 300 THEN '3xx' \
              ELSE '2xx' END AS status_class, \
         COUNT(*) AS count \
         FROM captures WHERE {} \
         GROUP BY request_method, status_class \
         ORDER BY request_method ASC, status_class ASC",
        where_sql
    );
    let ms_rows: Vec<MethodStatusRow> = sqlx::query_as(&ms_sql)
        .fetch_all(&pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let hourly_sql = format!(
        "SELECT strftime('%Y-%m-%dT%H:00:00Z', created_at) AS hour, COUNT(*) AS count \
         FROM captures WHERE {} \
         GROUP BY hour ORDER BY hour ASC",
        where_sql
    );
    let hourly_rows: Vec<HourlyRow> = sqlx::query_as(&hourly_sql)
        .fetch_all(&pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let by_method_status: Vec<Value> = ms_rows
        .into_iter()
        .map(|r| json!({
            "method": r.request_method,
            "status_class": r.status_class,
            "count": r.count,
        }))
        .collect();
    let hourly: Vec<Value> = hourly_rows
        .into_iter()
        .map(|r| json!({ "hour": r.hour, "count": r.count }))
        .collect();

    Ok(Json(json!({
        "total": total,
        "by_method_status": by_method_status,
        "hourly": hourly,
    })))
}

// ── Router + run ────────────────────────────────────────────────────────────

pub fn build_router(pool: sqlx::sqlite::SqlitePool, static_dir: &StdPath) -> Router {
    let r = Router::new()
        .route("/api/captures", get(list))
        .route("/api/captures/{id}", get(detail))
        .route("/api/captures/{id}/body", get(raw_body))
        .route("/api/export/captures", get(har_list))
        .route("/api/export/captures/{id}", get(har_detail))
        .route("/api/captures/{id}/adjacent", get(adjacent))
        .route("/api/facets", get(facets))
        .route("/api/stats", get(stats))
        .with_state(pool);
    if static_dir.exists() {
        let index = static_dir.join("index.html");
        // SPA fallback: unknown paths (e.g. /captures/3) serve index.html with 200
        // so deep links and refreshes work. `not_found_service` would force 404.
        let spa = ServeDir::new(static_dir).fallback(ServeFile::new(index));
        r.fallback_service(spa)
    } else {
        tracing::warn!("static dir missing; serving API only");
        r
    }
}

pub async fn run(args: Args) -> anyhow::Result<()> {
    let pool = open_read_only(&args.db).await?;
    let router = build_router(pool, args.static_dir.as_path());
    let listener = tokio::net::TcpListener::bind(args.listen).await?;
    tracing::info!(%args.listen, db = %args.db, "ghost-api listening");
    axum::serve(listener, router).await?;
    Ok(())
}
