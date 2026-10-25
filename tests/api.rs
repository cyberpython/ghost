//! Integration test: seed a temp SQLite DB, run a real `ghost::api` server,
//! drive raw-socket GETs, and assert filters, ordering, body views, facets.
//!
//! Uses raw sockets (reuses helpers from e2e.rs) — zero new runtime deps.

use ghost::{api, db};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tempfile;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

// ── Raw-socket helpers copied from tests/e2e.rs ─────────────────────────────

fn find_headers_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n")
}

async fn read_full_response(stream: &mut TcpStream) -> Vec<u8> {
    let mut buf: Vec<u8> = Vec::new();
    let mut tmp = [0u8; 8192];
    loop {
        let done = match find_headers_end(&buf) {
            None => false,
            Some(head_len) => {
                let head_total = head_len + 4;
                let head = std::str::from_utf8(&buf[..head_len]).unwrap();
                let mut cl = 0usize;
                for line in head.split("\r\n").skip(1) {
                    if let Some((k, v)) = line.split_once(':') {
                        if k.eq_ignore_ascii_case("content-length") {
                            cl = v.trim().parse::<usize>().unwrap_or(0);
                        }
                    }
                }
                buf.len() >= head_total + cl
            }
        };
        if done {
            break;
        }
        match stream.read(&mut tmp).await {
            Ok(0) | Err(_) => break,
            Ok(n) => buf.extend_from_slice(&tmp[..n]),
        }
    }
    buf
}

fn parse_response(buf: &[u8]) -> (String, Vec<(String, String)>, Vec<u8>) {
    let head_len = find_headers_end(buf).expect("no header terminator");
    let head_total = head_len + 4;
    let head = std::str::from_utf8(&buf[..head_len]).unwrap();
    let mut lines = head.split("\r\n");
    let status_line = lines.next().unwrap().to_string();
    let mut headers = Vec::new();
    let mut cl = 0usize;
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            headers.push((k.to_string(), v.trim().to_string()));
            if k.eq_ignore_ascii_case("content-length") {
                cl = v.trim().parse::<usize>().unwrap_or(0);
            }
        }
    }
    let body = buf[head_total..head_total + cl].to_vec();
    (status_line, headers, body)
}

fn sha256_hex(b: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(b);
    let d = h.finalize();
    let mut s = String::new();
    for byte in d {
        s.push_str(&format!("{:02x}", byte));
    }
    s
}

// ── Seed helper ─────────────────────────────────────────────────────────────

fn sha256_str(b: &[u8]) -> String {
    sha256_hex(b)
}

fn make_row(
    method: &str,
    status: Option<i32>,
    path: &str,
    client: &str,
    ts: &str,
    req_body: Option<Vec<u8>>,
    resp_body: Option<Vec<u8>>,
    error: Option<&str>,
) -> ghost::db::Capture {
    let req_headers = serde_json::json!([
        {"name": "host", "value": "example.org"},
        {"name": "x-dup", "value": "a"},
        {"name": "x-dup", "value": "b"},
    ])
    .to_string();
    let resp_headers = if resp_body.is_some() {
        serde_json::json!([{"name": "content-type", "value": "application/json"}]).to_string()
    } else {
        serde_json::json!([]).to_string()
    };
    ghost::db::Capture {
        created_at: ts.to_string(),
        total_ms: 1,
        client_addr: client.to_string(),
        request_version: "1.1".to_string(),
        request_method: method.to_string(),
        request_target: path.to_string(),
        request_headers: req_headers,
        request_body: req_body.clone(),
        request_body_len: req_body.as_ref().map_or(0, |b| b.len() as i64),
        request_body_truncated: 0,
        request_body_sha256: req_body.as_ref().map(|b| sha256_hex(&b[..])),
        upstream_scheme: "http".to_string(),
        upstream_authority: "127.0.0.1:1".to_string(),
        upstream_path: path.to_string(),
        upstream_query: None,
        response_version: status.map(|_| "1.1".to_string()),
        status_code: status,
        response_headers: resp_headers.clone(),
        response_body: resp_body.clone(),
        response_body_len: resp_body.as_ref().map_or(0, |b| b.len() as i64),
        response_body_truncated: 0,
        response_body_sha256: resp_body.as_ref().map(|b| sha256_hex(&b[..])),
        error: error.map(|s| s.to_string()),
    }
}

fn gzip(b: &[u8]) -> Vec<u8> {
    use std::io::Write as _;
    let mut e = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    e.write_all(b).unwrap();
    e.finish().unwrap()
}

fn zlib(b: &[u8]) -> Vec<u8> {
    use std::io::Write as _;
    let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    e.write_all(b).unwrap();
    e.finish().unwrap()
}

// ── Raw-GET helper ──────────────────────────────────────────────────────────

async fn get(port: u16, target: &str) -> (String, String) {
    let mut s = TcpStream::connect(format!("127.0.0.1:{port}")).await.unwrap();
    let req = format!("GET {target} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n");
    s.write_all(req.as_bytes()).await.unwrap();
    let raw = read_full_response(&mut s).await;
    let (sl, _, body) = parse_response(&raw);
    (sl, String::from_utf8(body).unwrap())
}

/// Same as `get`, but keeps the raw body bytes (for binary assertions).
async fn get_raw(port: u16, target: &str) -> (String, Vec<(String, String)>, Vec<u8>) {
    let mut s = TcpStream::connect(format!("127.0.0.1:{port}")).await.unwrap();
    let req = format!("GET {target} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n");
    s.write_all(req.as_bytes()).await.unwrap();
    let raw = read_full_response(&mut s).await;
    parse_response(&raw)
}

fn hdr<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}

// ── Test ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn api_integration() {
    let temp = tempfile::tempdir().unwrap();
    let dbpath = temp.path().join("t.db");
    let dbstr = dbpath.to_str().unwrap();

    // Seed DB
    let pool = db::init(dbstr).await.unwrap();
    let r1 = make_row("GET", Some(200), "/app/a", "10.0.0.1", "2024-01-01T00:00:00Z",
        Some(b"{\"k\":1}".to_vec()), Some(b"{\"ok\":true}".to_vec()), None);
    let r2 = make_row("GET", Some(404), "/app/b", "10.0.0.2", "2024-01-02T00:00:00Z",
        None, None, None);
    let r3 = make_row("POST", None, "/app/c", "10.0.0.3", "2024-01-03T00:00:00Z",
        Some(vec![0xFF, 0xFE, 0x00, 0x01]), None, Some("boom"));
    let r4 = make_row("CONNECT", None, "/app/d", "10.0.0.4", "2024-01-04T00:00:00Z",
        None, None, Some("CONNECT rejected"));
    let r5 = make_row("GET", Some(200), "/app/a", "10.0.0.5", "2024-01-05T00:00:00Z",
        None, None, None);
    db::insert(&pool, &r1).await.unwrap(); // id=1
    db::insert(&pool, &r2).await.unwrap(); // id=2
    db::insert(&pool, &r3).await.unwrap(); // id=3
    db::insert(&pool, &r4).await.unwrap(); // id=4
    db::insert(&pool, &r5).await.unwrap(); // id=5

    // Start server
    let listen = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listen.local_addr().unwrap().port();
    let router = api::build_router(
        api::open_read_only(dbstr).await.unwrap(),
        &temp.path().join("nope"),
    );
    let srv = tokio::spawn(async move {
        axum::serve(listen, router).await.unwrap();
    });

    // ── List tests ─────────────────────────────────────────────────────────
    // GET /api/captures
    {
        let (sl, body) = get(port, "/api/captures").await;
        assert!(sl.starts_with("HTTP/1.1 200"));
        let j: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(j["count"], 5);
        assert_eq!(j["captures"].as_array().unwrap().len(), 5);
        let ids: Vec<i64> = j["captures"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["id"].as_i64().unwrap())
            .collect();
        assert_eq!(ids, vec![5, 4, 3, 2, 1]);
    }

    // method=GET → 3
    {
        let (sl, body) = get(port, "/api/captures?method=GET").await;
        assert!(sl.starts_with("HTTP/1.1 200"));
        let j: Value = serde_json::from_str(&body).unwrap();
        let ids: Vec<i64> = j["captures"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["id"].as_i64().unwrap())
            .collect();
        assert_eq!(ids, vec![5, 2, 1]);
    }

    // status=200 → 2
    {
        let (sl, body) = get(port, "/api/captures?status=200").await;
        let j: Value = serde_json::from_str(&body).unwrap();
        let ids: Vec<i64> = j["captures"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["id"].as_i64().unwrap())
            .collect();
        assert_eq!(ids, vec![5, 1]);
    }

    // status=404 → 1 (id 2)
    {
        let (sl, body) = get(port, "/api/captures?status=404").await;
        let j: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(j["captures"].as_array().unwrap().len(), 1);
        assert_eq!(j["captures"][0]["id"], 2);
    }

    // path=/app/a → 2 (ids 1, 5)
    {
        let (sl, body) = get(port, "/api/captures?path=/app/a").await;
        let j: Value = serde_json::from_str(&body).unwrap();
        let ids: Vec<i64> = j["captures"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["id"].as_i64().unwrap())
            .collect();
        assert_eq!(ids, vec![5, 1]);
    }

    // client_addr=10.0.0.1 → 1 (id 1)
    {
        let (sl, body) = get(port, "/api/captures?client_addr=10.0.0.1").await;
        let j: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(j["captures"].as_array().unwrap().len(), 1);
        assert_eq!(j["captures"][0]["id"], 1);
    }

    // since=... → 3 (ids 3, 4, 5)
    {
        let (sl, body) = get(
            port,
            "/api/captures?since=2024-01-03T00:00:00Z",
        )
        .await;
        let j: Value = serde_json::from_str(&body).unwrap();
        let ids: Vec<i64> = j["captures"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["id"].as_i64().unwrap())
            .collect();
        assert_eq!(ids, vec![5, 4, 3]);
    }

    // until=... → 2 (ids 1, 2)
    {
        let (sl, body) = get(
            port,
            "/api/captures?until=2024-01-02T00:00:00Z",
        )
        .await;
        let j: Value = serde_json::from_str(&body).unwrap();
        let ids: Vec<i64> = j["captures"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["id"].as_i64().unwrap())
            .collect();
        assert_eq!(ids, vec![2, 1]);
    }

    // Combined: method=GET&status=200 → 2 (ids 1, 5)
    {
        let (sl, body) = get(
            port,
            "/api/captures?method=GET&status=200",
        )
        .await;
        let j: Value = serde_json::from_str(&body).unwrap();
        let ids: Vec<i64> = j["captures"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["id"].as_i64().unwrap())
            .collect();
        assert_eq!(ids, vec![5, 1]);
    }

    // limit=2
    {
        let (sl, body) = get(port, "/api/captures?limit=2").await;
        let j: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(j["count"], 5);
        assert_eq!(j["captures"].as_array().unwrap().len(), 2);
        let ids: Vec<i64> = j["captures"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["id"].as_i64().unwrap())
            .collect();
        assert_eq!(ids, vec![5, 4]);
    }

    // offset=3 (default limit 50)
    {
        let (sl, body) = get(port, "/api/captures?offset=3").await;
        let j: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(j["count"], 5);
        let ids: Vec<i64> = j["captures"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["id"].as_i64().unwrap())
            .collect();
        assert_eq!(ids, vec![2, 1]);
    }

    // limit=1
    {
        let (sl, body) = get(port, "/api/captures?limit=1").await;
        let j: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(j["captures"].as_array().unwrap().len(), 1);
        assert_eq!(j["captures"][0]["id"], 5);
    }

    // ── Detail tests ───────────────────────────────────────────────────────

    // Detail id=1
    {
        let (sl, body) = get(port, "/api/captures/1").await;
        assert!(sl.starts_with("HTTP/1.1 200"));
        let j: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(j["request_body"]["text"], "{\"k\":1}");
        assert_eq!(j["request_body"]["binary"], false);
        assert_eq!(j["request_body"]["truncated"], false);
        assert_eq!(j["request_body"]["len"], 7);
        assert_eq!(j["request_body"]["sha256"], sha256_hex(b"{\"k\":1}"));
        assert_eq!(j["response_body"]["text"], "{\"ok\":true}");
        let hdrs = j["request_headers"].as_array().unwrap();
        assert_eq!(hdrs.len(), 3);
        let names: Vec<&str> = hdrs.iter().map(|h| h["name"].as_str().unwrap()).collect();
        assert_eq!(names, vec!["host", "x-dup", "x-dup"]);
        let vals: Vec<&str> = hdrs.iter().map(|h| h["value"].as_str().unwrap()).collect();
        assert_eq!(vals, vec!["example.org", "a", "b"]);
        assert_eq!(j["status_code"], 200);
    }

    // Detail id=3 (binary body, null status, error)
    {
        let (sl, body) = get(port, "/api/captures/3").await;
        let j: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(j["status_code"], Value::Null);
        assert_eq!(j["error"], "boom");
        assert_eq!(j["request_body"]["binary"], true);
        assert_eq!(j["request_body"]["text"], Value::Null);
        let hex = j["request_body"]["hex_preview"].as_str().unwrap();
        assert!(hex.starts_with("ff fe 00 01"));
    }

    // Detail id=4 (error)
    {
        let (sl, body) = get(port, "/api/captures/4").await;
        let j: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(j["error"], "CONNECT rejected");
    }

    // Detail id=9999 → 404
    {
        let (sl, _) = get(port, "/api/captures/9999").await;
        assert!(sl.starts_with("HTTP/1.1 404"));
    }

    // ── Raw body endpoint ───────────────────────────────────────────────
    // Row 1: request body, exact bytes + download headers.
    {
        let (sl, hs, body) = get_raw(port, "/api/captures/1/body?part=request").await;
        assert!(sl.starts_with("HTTP/1.1 200"));
        assert_eq!(body, b"{\"k\":1}");
        assert_eq!(hdr(&hs, "content-type"), Some("application/octet-stream"));
        assert!(hdr(&hs, "content-disposition").unwrap().contains("attachment; filename=\"capture-1-request\""));
        assert_eq!(hdr(&hs, "x-body-truncated"), Some("0"));
        assert_eq!(hdr(&hs, "x-body-original-length"), Some("7"));
    }
    // Row 1: response body.
    {
        let (sl, _, body) = get_raw(port, "/api/captures/1/body?part=response").await;
        assert!(sl.starts_with("HTTP/1.1 200"));
        assert_eq!(body, b"{\"ok\":true}");
    }
    // Row 3: binary request body round-trips byte-for-byte.
    {
        let (sl, _, body) = get_raw(port, "/api/captures/3/body?part=request").await;
        assert!(sl.starts_with("HTTP/1.1 200"));
        assert_eq!(body, vec![0xFF, 0xFE, 0x00, 0x01]);
    }
    // Row 3 has no response body → 404.
    {
        let (sl, _, _) = get_raw(port, "/api/captures/3/body?part=response").await;
        assert!(sl.starts_with("HTTP/1.1 404"));
    }
    // Unknown capture → 404.
    {
        let (sl, _, _) = get_raw(port, "/api/captures/9999/body?part=request").await;
        assert!(sl.starts_with("HTTP/1.1 404"));
    }
    // Invalid part → 400.
    {
        let (sl, _, _) = get_raw(port, "/api/captures/1/body?part=bogus").await;
        assert!(sl.starts_with("HTTP/1.1 400"));
    }

    // ── Facets ─────────────────────────────────────────────────────────────

    {
        let (sl, body) = get(port, "/api/facets").await;
        assert!(sl.starts_with("HTTP/1.1 200"));
        let j: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(j["methods"], serde_json::json!(["CONNECT", "GET", "POST"]));
        assert_eq!(j["statuses"], serde_json::json!([200, 404]));
        assert_eq!(j["errors"], 2);
    }

    // ── Stats (FR-016 KPI panel + FR-018 requests/hour graph) ─────────────

    // Unfiltered: total, method × status-class breakdown, hourly buckets.
    {
        let (sl, body) = get(port, "/api/stats").await;
        assert!(sl.starts_with("HTTP/1.1 200"));
        let j: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(j["total"], 5);
        let ms = j["by_method_status"].as_array().unwrap();
        assert_eq!(ms.len(), 4);
        assert_eq!(ms[0]["method"], "CONNECT");
        assert_eq!(ms[0]["status_class"], "err");
        assert_eq!(ms[0]["count"], 1);
        assert_eq!(ms[1]["method"], "GET");
        assert_eq!(ms[1]["status_class"], "2xx");
        assert_eq!(ms[1]["count"], 2);
        assert_eq!(ms[2]["method"], "GET");
        assert_eq!(ms[2]["status_class"], "4xx");
        assert_eq!(ms[2]["count"], 1);
        assert_eq!(ms[3]["method"], "POST");
        assert_eq!(ms[3]["status_class"], "err");
        assert_eq!(ms[3]["count"], 1);
        // Only hours with data — no fabricated zeros.
        let hourly = j["hourly"].as_array().unwrap();
        assert_eq!(hourly.len(), 5);
        assert_eq!(hourly[0]["hour"], "2024-01-01T00:00:00Z");
        assert_eq!(hourly[0]["count"], 1);
        assert_eq!(hourly[4]["hour"], "2024-01-05T00:00:00Z");
        assert_eq!(hourly[4]["count"], 1);
    }

    // method=GET → only the GET rows, in both the breakdown and the buckets.
    {
        let (sl, body) = get(port, "/api/stats?method=GET").await;
        assert!(sl.starts_with("HTTP/1.1 200"));
        let j: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(j["total"], 3);
        let ms = j["by_method_status"].as_array().unwrap();
        assert_eq!(ms.len(), 2);
        assert_eq!(ms[0]["method"], "GET");
        assert_eq!(ms[0]["status_class"], "2xx");
        assert_eq!(ms[0]["count"], 2);
        assert_eq!(ms[1]["method"], "GET");
        assert_eq!(ms[1]["status_class"], "4xx");
        assert_eq!(ms[1]["count"], 1);
        let hourly = j["hourly"].as_array().unwrap();
        assert_eq!(hourly.len(), 3);
        assert_eq!(hourly[0]["hour"], "2024-01-01T00:00:00Z");
        assert_eq!(hourly[2]["hour"], "2024-01-05T00:00:00Z");
    }

    // since=2024-01-03T00:00:00Z → ids 3, 4, 5 (POST, CONNECT, GET).
    {
        let (sl, body) = get(port, "/api/stats?since=2024-01-03T00:00:00Z").await;
        assert!(sl.starts_with("HTTP/1.1 200"));
        let j: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(j["total"], 3);
        let ms = j["by_method_status"].as_array().unwrap();
        assert_eq!(ms.len(), 3);
        assert_eq!(ms[0]["method"], "CONNECT");
        assert_eq!(ms[0]["status_class"], "err");
        assert_eq!(ms[1]["method"], "GET");
        assert_eq!(ms[1]["status_class"], "2xx");
        assert_eq!(ms[2]["method"], "POST");
        assert_eq!(ms[2]["status_class"], "err");
        let hourly = j["hourly"].as_array().unwrap();
        assert_eq!(hourly.len(), 3);
        assert_eq!(hourly[0]["hour"], "2024-01-03T00:00:00Z");
    }

    // until=2024-01-02T00:00:00Z → ids 1, 2 (both GET).
    {
        let (sl, body) = get(port, "/api/stats?until=2024-01-02T00:00:00Z").await;
        assert!(sl.starts_with("HTTP/1.1 200"));
        let j: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(j["total"], 2);
        let ms = j["by_method_status"].as_array().unwrap();
        assert_eq!(ms.len(), 2);
        assert_eq!(ms[0]["method"], "GET");
        assert_eq!(ms[0]["status_class"], "2xx");
        assert_eq!(ms[0]["count"], 1);
        assert_eq!(ms[1]["method"], "GET");
        assert_eq!(ms[1]["status_class"], "4xx");
        assert_eq!(ms[1]["count"], 1);
        let hourly = j["hourly"].as_array().unwrap();
        assert_eq!(hourly.len(), 2);
        assert_eq!(hourly[0]["hour"], "2024-01-01T00:00:00Z");
        assert_eq!(hourly[1]["hour"], "2024-01-02T00:00:00Z");
    }

    // ── HAR export ────────────────────────────────────────────────────────

    // Single capture: /api/export/captures/1
    {
        let (sl, hs, body) = get_raw(port, "/api/export/captures/1").await;
        assert!(sl.starts_with("HTTP/1.1 200"));
        assert_eq!(
            hdr(&hs, "content-type"),
            Some("application/json")
        );
        assert!(hdr(&hs, "content-disposition")
            .unwrap()
            .contains(r#"attachment; filename="capture-1.har""#));
        let j: Value = serde_json::from_slice(&body).unwrap();
        // HAR 1.2 wraps everything in a top-level `log` object.
        assert_eq!(j["log"]["version"], "1.2");
        assert_eq!(j["log"]["creator"]["name"], "ghost");
        assert_eq!(j["log"]["entries"].as_array().unwrap().len(), 1);
        let e = &j["log"]["entries"][0];
        assert_eq!(e["startedDateTime"], "2024-01-01T00:00:00Z");
        assert_eq!(e["request"]["method"], "GET");
        assert_eq!(e["request"]["url"], "http://127.0.0.1:1/app/a");
        assert_eq!(e["request"]["httpVersion"], "HTTP/1.1");
        // Text request body inlined as UTF-8 text (no base64).
        assert_eq!(e["request"]["postData"]["text"], "{\"k\":1}");
        assert!(e["request"]["postData"].get("encoding").is_none());
        // Response: status, statusText, text body.
        assert_eq!(e["response"]["status"], 200);
        assert_eq!(e["response"]["statusText"], "OK");
        assert_eq!(e["response"]["content"]["text"], "{\"ok\":true}");
        assert_eq!(e["response"]["content"]["mimeType"], "application/json");
        // Headers pass through as [{name, value}].
        let hdrs = e["request"]["headers"].as_array().unwrap();
        let names: Vec<&str> = hdrs.iter().map(|h| h["name"].as_str().unwrap()).collect();
        assert_eq!(names, vec!["host", "x-dup", "x-dup"]);
        assert!(e.get("error").is_none());
        assert!(e["response"].get("_error").is_none());
    }

    // Binary request body round-trips base64: [0xFF, 0xFE, 0x00, 0x01] → "//4AAQ==".
    {
        let (_, _, body) = get_raw(port, "/api/export/captures/3").await;
        let j: Value = serde_json::from_slice(&body).unwrap();
        let e = &j["log"]["entries"][0];
        assert_eq!(e["request"]["postData"]["text"], "//4AAQ==");
        assert_eq!(e["request"]["postData"]["encoding"], "base64");
        // Failed exchange: no status → 0, error as statusText.
        assert_eq!(e["response"]["status"], 0);
        assert_eq!(e["response"]["statusText"], "boom");
        assert_eq!(e["response"]["_error"], "boom");
        assert_eq!(e["response"]["headers"].as_array().unwrap().len(), 0);
        assert_eq!(e["timings"]["wait"], -1);
        assert_eq!(e["comment"], "boom");
    }

    // Filter set: method=GET → ids 5, 2, 1 (same order as /api/captures).
    {
        let (sl, hs, body) = get_raw(port, "/api/export/captures?method=GET").await;
        assert!(sl.starts_with("HTTP/1.1 200"));
        assert!(hdr(&hs, "content-disposition")
            .unwrap()
            .contains(r#"attachment; filename="captures.har""#));
        let j: Value = serde_json::from_slice(&body).unwrap();
        let urls: Vec<&str> = j["log"]["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["request"]["url"].as_str().unwrap())
            .collect();
        assert_eq!(urls, vec![
            "http://127.0.0.1:1/app/a",
            "http://127.0.0.1:1/app/b",
            "http://127.0.0.1:1/app/a",
        ]);
    }

    // Unknown capture → 404.
    {
        let (sl, _, _) = get_raw(port, "/api/export/captures/9999").await;
        assert!(sl.starts_with("HTTP/1.1 404"));
    }

    // ── Adjacency (FR-039) ──────────────────────────────────────────────

    // Middle id: both neighbors resolved.
    {
        let (sl, body) = get(port, "/api/captures/3/adjacent").await;
        assert!(sl.starts_with("HTTP/1.1 200"));
        let j: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(j["prev"], 2);
        assert_eq!(j["next"], 4);
    }
    // First id: prev is null.
    {
        let (sl, body) = get(port, "/api/captures/1/adjacent").await;
        let j: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(j["prev"], Value::Null);
        assert_eq!(j["next"], 2);
    }
    // Last id: next is null.
    {
        let (sl, body) = get(port, "/api/captures/5/adjacent").await;
        let j: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(j["prev"], 4);
        assert_eq!(j["next"], Value::Null);
    }
    // Unknown id: lenient — nearest existing neighbors, no 404.
    {
        let (sl, body) = get(port, "/api/captures/9999/adjacent").await;
        assert!(sl.starts_with("HTTP/1.1 200"));
        let j: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(j["prev"], 5);
        assert_eq!(j["next"], Value::Null);
    }

    // ── Cleanup ────────────────────────────────────────────────────────────
    srv.abort();
}

/// FR-019 decode-on-read: the view (and only the view) inflates a single
/// gzip/deflate coding; stored bytes and the raw body endpoint stay
/// compressed; bombs and garbage fall back to the raw view silently.
#[tokio::test]
async fn decode_on_read() {
    let temp = tempfile::tempdir().unwrap();
    let dbpath = temp.path().join("t.db");
    let dbstr = dbpath.to_str().unwrap();
    let pool = db::init(dbstr).await.unwrap();

    // id=1: gzip on both sides of the exchange.
    let gz_req = gzip(b"{\"gz\":true}");
    let gz_resp = gzip(b"{\"ok\":2}");
    let mut r1 = make_row(
        "POST",
        Some(200),
        "/gz",
        "10.0.0.9",
        "2024-01-06T00:00:00Z",
        Some(gz_req.clone()),
        Some(gz_resp),
        None,
    );
    r1.request_headers = serde_json::json!([
        {"name": "host", "value": "example.org"},
        {"name": "content-encoding", "value": "gzip"},
        {"name": "content-type", "value": "application/json"},
    ])
    .to_string();
    r1.response_headers = serde_json::json!([
        {"name": "content-encoding", "value": "gzip"},
        {"name": "content-type", "value": "application/json"},
    ])
    .to_string();
    db::insert(&pool, &r1).await.unwrap();

    // id=2: lying header — binary garbage with content-encoding: gzip.
    let mut r2 = make_row(
        "POST",
        Some(200),
        "/corrupt",
        "10.0.0.9",
        "2024-01-06T00:00:01Z",
        Some(vec![0xff, 0x00, 0x01, 0x8b]),
        None,
        None,
    );
    r2.request_headers = serde_json::json!([
        {"name": "content-encoding", "value": "gzip"},
    ])
    .to_string();
    db::insert(&pool, &r2).await.unwrap();

    // id=3: multi-coding list → outside the accepted subset → raw view.
    let mut r3 = make_row(
        "POST",
        Some(200),
        "/multi",
        "10.0.0.9",
        "2024-01-06T00:00:02Z",
        Some(gzip(b"[1,2]")),
        None,
        None,
    );
    r3.request_headers = serde_json::json!([
        {"name": "content-encoding", "value": "gzip, br"},
    ])
    .to_string();
    db::insert(&pool, &r3).await.unwrap();

    // id=4: decompression bomb — deflate expanding past the 8 MiB view cap.
    let mut r4 = make_row(
        "POST",
        Some(200),
        "/bomb",
        "10.0.0.9",
        "2024-01-06T00:00:03Z",
        Some(zlib(&vec![0u8; 9 * 1024 * 1024])),
        None,
        None,
    );
    r4.request_headers = serde_json::json!([
        {"name": "content-encoding", "value": "deflate"},
    ])
    .to_string();
    db::insert(&pool, &r4).await.unwrap();

    let listen = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listen.local_addr().unwrap().port();
    let router = api::build_router(
        api::open_read_only(dbstr).await.unwrap(),
        &temp.path().join("nope"),
    );
    let srv = tokio::spawn(async move {
        axum::serve(listen, router).await.unwrap();
    });

    // id=1: both sides decoded, JSON detected, decoded lengths reported.
    {
        let (_, body) = get(port, "/api/captures/1").await;
        let j: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(j["request_body"]["text"], "{\"gz\":true}");
        assert_eq!(j["request_body"]["decoded_encoding"], "gzip");
        assert_eq!(j["request_body"]["len"], 11);
        assert_eq!(j["response_body"]["text"], "{\"ok\":2}");
        assert_eq!(j["response_body"]["decoded_encoding"], "gzip");
    }
    // Raw endpoint: still the compressed wire bytes.
    {
        let (_, _, body) = get_raw(port, "/api/captures/1/body?part=request").await;
        assert_eq!(body, gz_req);
    }
    // id=2 corrupt: silent raw fallback (binary view, no decode claim).
    {
        let (_, body) = get(port, "/api/captures/2").await;
        let j: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(j["request_body"]["text"], Value::Null);
        assert_eq!(j["request_body"]["binary"], true);
        assert_eq!(j["request_body"]["decoded_encoding"], Value::Null);
    }
    // id=3 multi-coding: raw fallback.
    {
        let (_, body) = get(port, "/api/captures/3").await;
        let j: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(j["request_body"]["decoded_encoding"], Value::Null);
        assert_eq!(j["request_body"]["binary"], true);
    }
    // id=4 bomb: over-cap → raw fallback, view stays cheap.
    {
        let (_, body) = get(port, "/api/captures/4").await;
        let j: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(j["request_body"]["decoded_encoding"], Value::Null);
        assert_eq!(j["request_body"]["binary"], true);
    }

    srv.abort();
}
