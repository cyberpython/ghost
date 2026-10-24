//! Retention-prune integration test: seed rows with writer-format
//! (`…Z`) timestamps, run `db::prune`, and assert old rows are deleted
//! while recent rows are kept. This also pins the `Z`-vs-`+00:00`
//! lexicographic comparability of the cutoff string empirically.

use ghost::db;
use tempfile;

fn make_row(ts: &str) -> ghost::db::Capture {
    let req_headers = serde_json::json!([{"name": "host", "value": "example.org"}]).to_string();
    ghost::db::Capture {
        created_at: ts.to_string(),
        total_ms: 1,
        client_addr: "10.0.0.1".to_string(),
        request_version: "1.1".to_string(),
        request_method: "GET".to_string(),
        request_target: "/app/x".to_string(),
        request_headers: req_headers,
        request_body: None,
        request_body_len: 0,
        request_body_truncated: 0,
        request_body_sha256: None,
        upstream_scheme: "http".to_string(),
        upstream_authority: "127.0.0.1:1".to_string(),
        upstream_path: "/app/x".to_string(),
        upstream_query: None,
        response_version: Some("1.1".to_string()),
        status_code: Some(200),
        response_headers: serde_json::json!([]).to_string(),
        response_body: None,
        response_body_len: 0,
        response_body_truncated: 0,
        response_body_sha256: None,
        error: None,
    }
}

#[tokio::test]
async fn prune_deletes_old_keeps_recent() {
    let temp = tempfile::tempdir().unwrap();
    let dbpath = temp.path().join("t.db");
    let dbstr = dbpath.to_str().unwrap();
    let pool = db::init(dbstr).await.unwrap();

    // Writer-format timestamps: `…Z`, whole seconds, as stored by the proxy.
    let old = chrono::Utc::now() - chrono::Duration::days(10);
    let recent = chrono::Utc::now() - chrono::Duration::days(1);
    let old_ts = old.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let recent_ts = recent.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);

    db::insert(&pool, &make_row(&old_ts)).await.unwrap(); // id=1, pruned
    db::insert(&pool, &make_row(&recent_ts)).await.unwrap(); // id=2, kept

    let pruned = db::prune(&pool, 7).await.unwrap();
    assert_eq!(pruned, 1, "only the 10-day-old row should be deleted");

    let rows: Vec<(i64, String)> =
        sqlx::query_as("SELECT id, created_at FROM captures").fetch_all(&pool).await.unwrap();
    assert_eq!(rows.len(), 1, "exactly one row should remain");
    assert_eq!(rows[0].0, 2, "the recent row (id=2) must survive");
    assert_eq!(rows[0].1, recent_ts);

    // A second prune with nothing to delete must be a no-op.
    assert_eq!(db::prune(&pool, 7).await.unwrap(), 0);
}

#[tokio::test]
async fn prune_zero_days_deletes_everything_older_than_now() {
    // `db::prune(0)` means "cutoff = now": every stored row is older than
    // now, so all of them go. (The CLI's "0 = disabled" is enforced in
    // `main.rs`, which never calls `prune` with 0.)
    let temp = tempfile::tempdir().unwrap();
    let dbpath = temp.path().join("t.db");
    let dbstr = dbpath.to_str().unwrap();
    let pool = db::init(dbstr).await.unwrap();

    let ts = chrono::Utc::now()
        - chrono::Duration::days(36500);
    let ts = ts.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db::insert(&pool, &make_row(&ts)).await.unwrap();

    assert_eq!(db::prune(&pool, 0).await.unwrap(), 1);
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM captures").fetch_one(&pool).await.unwrap();
    assert_eq!(count, 0);
}
