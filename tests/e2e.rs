//! End-to-end test: spawn a real origin + the proxy on ephemeral ports with a
//! temp SQLite file, drive raw-socket requests through, and assert that
//! headers/bytes arrive byte-for-byte and are stored faithfully.

use bytes::Bytes;
use ghost::url_join::UpstreamBase;
use ghost::{db, proxy};
use http_body_util::{BodyExt, Full, Limited, combinators::BoxBody};
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use sha2::{Digest, Sha256};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

type Res = Result<Response<BoxBody<Bytes, hyper::Error>>, hyper::Error>;

/// Origin that echoes what it received (JSON body) so we can assert on it.
async fn origin_handle(req: Request<Incoming>) -> Res {
    let method = req.method().clone();
    let target = req
        .uri()
        .path_and_query()
        .map(|s| s.to_string())
        .unwrap_or_default();
    // FR-003 amendment pin: was the request-target absolute-form on the wire?
    let absolute_target = req.uri().scheme_str().is_some();
    let headers: Vec<String> = req
        .headers()
        .iter()
        .map(|(n, v)| format!("{}: {}", n.as_str(), v.to_str().unwrap_or("")))
        .collect();
    let body: Bytes = match Limited::new(req.into_body(), 10 * 1024 * 1024)
        .collect()
        .await
    {
        Ok(c) => c.to_bytes(),
        Err(_) => Bytes::new(),
    };
    let json = serde_json::json!({
        "method": method.to_string(),
        "target": target,
        "absolute_target": absolute_target,
        "headers": headers,
        "body": String::from_utf8_lossy(&body).to_string(),
    });
    let mut resp = Response::new(
        Full::new(Bytes::from(json.to_string().into_bytes()))
            .map_err(|_n| unreachable!())
            .boxed(),
    );
    *resp.status_mut() = StatusCode::OK;
    resp.headers_mut().insert(
        http::HeaderName::from_static("x-origin"),
        http::HeaderValue::from_static("yes"),
    );
    Ok(resp)
}

async fn origin_serve(listener: TcpListener) {
    loop {
        let (stream, _peer) = listener.accept().await.unwrap();
        let io = TokioIo::new(stream);
        tokio::spawn(async move {
            let svc = service_fn(|req: Request<Incoming>| Box::pin(origin_handle(req)));
            let _ = http1::Builder::new()
                .preserve_header_case(true)
                .title_case_headers(true)
                .serve_connection(io, svc)
                .with_upgrades()
                .await;
        });
    }
}

async fn wait_for_listen(addr: &str) {
    for _ in 0..200 {
        if TcpStream::connect(addr).await.is_ok() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("proxy did not start listening");
}

fn find_headers_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n")
}

/// Read from the stream until we have the full status line, headers and body.
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
            _ => unreachable!(),
        }
    }
    buf
}

/// Split a raw HTTP response into (status line, ordered header pairs, body).
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
            // OWS after the colon is not part of the value (RFC 9110 §5.5).
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

async fn col_text(pool: &sqlx::sqlite::SqlitePool, id: i64, col: &str) -> Option<String> {
    let v: Option<String> =
        sqlx::query_scalar(&format!("SELECT {col} FROM captures WHERE id = {id}"))
            .fetch_one(pool)
            .await
            .unwrap();
    v
}

async fn col_i64(pool: &sqlx::sqlite::SqlitePool, id: i64, col: &str) -> Option<i64> {
    let v: Option<i64> = sqlx::query_scalar(&format!("SELECT {col} FROM captures WHERE id = {id}"))
        .fetch_one(pool)
        .await
        .unwrap();
    v
}

async fn col_blob(pool: &sqlx::sqlite::SqlitePool, id: i64, col: &str) -> Option<Vec<u8>> {
    let v: Option<Vec<u8>> =
        sqlx::query_scalar(&format!("SELECT {col} FROM captures WHERE id = {id}"))
            .fetch_one(pool)
            .await
            .unwrap();
    v
}

async fn col_id(pool: &sqlx::sqlite::SqlitePool, where_clause: &str) -> i64 {
    let v: Option<i64> =
        sqlx::query_scalar(&format!("SELECT id FROM captures WHERE {where_clause}"))
            .fetch_one(pool)
            .await
            .unwrap();
    v.unwrap()
}

#[tokio::test]
async fn transparent_e2e() {
    // Origin on an ephemeral port.
    let origin_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin_port = origin_listener.local_addr().unwrap().port();
    let origin_task = tokio::spawn(origin_serve(origin_listener));

    // Proxy on an ephemeral port, temp DB.
    let proxy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_port = proxy_listener.local_addr().unwrap().port();
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("t.db");
    let pool = db::init(db_path.to_str().unwrap()).await.unwrap();
    let upstream = UpstreamBase::parse(&format!("http://127.0.0.1:{origin_port}/app")).unwrap();
    let state = proxy::State {
        db: pool.clone(),
        upstream,
        max_body_bytes: 100 * 1024 * 1024,
    };
    let proxy_task = tokio::spawn(proxy::serve(proxy_listener, state));

    let proxy_addr = format!("127.0.0.1:{proxy_port}");
    wait_for_listen(&proxy_addr).await;

    // --- Successful POST through the proxy -------------------------------
    let mut s = TcpStream::connect(&proxy_addr).await.unwrap();
    let req = "POST /endpoint/path?q=1 HTTP/1.1\r\n\
              Host: example.client\r\n\
              X-Test: abc\r\n\
              X-Other: 1\r\n\
              X-Dup: a\r\n\
              X-Dup: b\r\n\
              Content-Length: 10\r\n\
              \r\n\
              hello-body";
    s.write_all(req.as_bytes()).await.unwrap();
    let raw = read_full_response(&mut s).await;
    let (status_line, resp_headers, body) = parse_response(&raw);
    assert!(
        status_line.starts_with("HTTP/1.1 200"),
        "got: {status_line}"
    );

    let body_str = std::str::from_utf8(&body).unwrap();
    let echo: serde_json::Value = serde_json::from_str(body_str).unwrap();
    assert_eq!(echo["target"], "/app/endpoint/path?q=1");
    // FR-003 amendment: the upstream receives an origin-form request-target
    // (path?query), not the resolved absolute URI.
    assert_eq!(echo["absolute_target"], false);
    assert_eq!(echo["method"], "POST");
    assert_eq!(echo["body"], "hello-body");
    let hdrs: Vec<String> = echo["headers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    // Exact header sequence the origin must see, in order: values byte-for-byte,
    // names in their canonical lowercase form.
    assert_eq!(
        hdrs,
        vec![
            "host: example.client",
            "x-test: abc",
            "x-other: 1",
            "x-dup: a",
            "x-dup: b",
            "content-length: 10",
        ]
    );

    // Response relayed to the client: the origin's headers (plus the `Date` the
    // test origin's hyper auto-adds), then the normalised CL — all lowercase.
    let (date_headers, rest): (Vec<_>, Vec<_>) = resp_headers
        .into_iter()
        .partition(|(n, _)| n.as_str() == "date");
    assert!(
        !date_headers.is_empty(),
        "relayed response missing Date header"
    );
    let expected_resp_headers = vec![
        ("x-origin".to_string(), "yes".to_string()),
        ("content-length".to_string(), body.len().to_string()),
    ];
    assert_eq!(rest, expected_resp_headers);

    // --- CONNECT must be rejected with 405 --------------------------------
    let mut s2 = TcpStream::connect(&proxy_addr).await.unwrap();
    s2.write_all(b"CONNECT 10.0.0.1:80 HTTP/1.1\r\nHost: 10.0.0.1:80\r\n\r\n")
        .await
        .unwrap();
    let raw2 = read_full_response(&mut s2).await;
    let (status2, _, _) = parse_response(&raw2);
    assert!(status2.starts_with("HTTP/1.1 405"), "got: {status2}");

    // --- Upstream down must yield 502 -------------------------------------
    origin_task.abort();
    let mut s3 = TcpStream::connect(&proxy_addr).await.unwrap();
    s3.write_all(b"GET /nope HTTP/1.1\r\nHost: example.client\r\n\r\n")
        .await
        .unwrap();
    let raw3 = read_full_response(&mut s3).await;
    let (status3, _, _) = parse_response(&raw3);
    assert!(status3.starts_with("HTTP/1.1 502"), "got: {status3}");

    // --- Let the async inserts land, then assert the DB -------------------
    let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(10);
    loop {
        let c: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM captures")
            .fetch_one(&pool)
            .await
            .unwrap();
        if c >= 3 {
            break;
        }
        if tokio::time::Instant::now() > deadline {
            panic!("timed out waiting for 3 rows");
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    let post_id = col_id(&pool, "request_method='POST' AND status_code=200").await;
    let conn_id = col_id(&pool, "request_method='CONNECT'").await;
    let get_id = col_id(&pool, "request_method='GET' AND status_code IS NULL").await;

    // POST row: stored request headers, in original order, names lowercase.
    let req_headers = col_text(&pool, post_id, "request_headers").await.unwrap();
    let rh: Vec<serde_json::Value> = serde_json::from_str(&req_headers).unwrap();
    let names: Vec<&str> = rh.iter().map(|v| v["name"].as_str().unwrap()).collect();
    assert_eq!(
        names,
        vec![
            "host",
            "x-test",
            "x-other",
            "x-dup",
            "x-dup",
            "content-length"
        ]
    );
    let values: Vec<&str> = rh.iter().map(|v| v["value"].as_str().unwrap()).collect();
    assert_eq!(values, vec!["example.client", "abc", "1", "a", "b", "10"]);

    // POST row: bodies byte-exact + hashes.
    assert_eq!(
        col_blob(&pool, post_id, "request_body").await.unwrap(),
        b"hello-body"
    );
    assert_eq!(col_i64(&pool, post_id, "request_body_len").await, Some(10));
    assert_eq!(
        col_text(&pool, post_id, "request_body_sha256")
            .await
            .unwrap(),
        sha256_hex(b"hello-body")
    );
    assert_eq!(col_i64(&pool, post_id, "status_code").await, Some(200));
    assert_eq!(
        col_text(&pool, post_id, "upstream_path").await.unwrap(),
        "/app/endpoint/path"
    );
    assert_eq!(
        col_text(&pool, post_id, "upstream_query").await.unwrap(),
        "q=1".to_string()
    );
    assert_eq!(
        col_blob(&pool, post_id, "response_body").await.unwrap(),
        body
    );
    assert_eq!(
        col_text(&pool, post_id, "response_body_sha256")
            .await
            .unwrap(),
        sha256_hex(&body)
    );
    // No error on success.
    assert!(col_text(&pool, post_id, "error").await.is_none());

    // CONNECT row: recorded, no response, error set, no body.
    assert_eq!(col_i64(&pool, conn_id, "status_code").await, None);
    assert!(
        !col_text(&pool, conn_id, "error")
            .await
            .unwrap_or_default()
            .is_empty()
    );
    assert!(col_blob(&pool, conn_id, "request_body").await.is_none());

    // GET (502) row: recorded, no response, error set.
    assert_eq!(col_i64(&pool, get_id, "status_code").await, None);
    assert!(
        !col_text(&pool, get_id, "error")
            .await
            .unwrap_or_default()
            .is_empty()
    );

    // Clean up.
    proxy_task.abort();
}

/// Sensitive values (auth header, cookie, query param, JSON body field) are
/// masked in the *stored* row while the relayed bytes stay intact.
#[tokio::test]
async fn masking_e2e() {
    let origin_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin_port = origin_listener.local_addr().unwrap().port();
    let origin_task = tokio::spawn(origin_serve(origin_listener));

    let proxy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_port = proxy_listener.local_addr().unwrap().port();
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("t.db");
    let pool = db::init(db_path.to_str().unwrap()).await.unwrap();
    let upstream = UpstreamBase::parse(&format!("http://127.0.0.1:{origin_port}/app")).unwrap();
    let state = proxy::State {
        db: pool.clone(),
        upstream,
        max_body_bytes: 100 * 1024 * 1024,
    };
    let proxy_task = tokio::spawn(proxy::serve(proxy_listener, state));

    let proxy_addr = format!("127.0.0.1:{proxy_port}");
    wait_for_listen(&proxy_addr).await;

    // --- Sensitive POST through the proxy -----------------------------------
    let body = r#"{"password":"bodypass123","safe":"keepme"}"#;
    let mut s = TcpStream::connect(&proxy_addr).await.unwrap();
    let req = format!(
        "POST /sensitive?api_key=querysecret789 HTTP/1.1\r\n\
         Host: example.client\r\n\
         Authorization: Bearer supersecretkey123\r\n\
         Cookie: sessionid=cookiesecret456\r\n\
         Content-Type: application/json\r\n\
         Content-Length: {}\r\n\
         \r\n\
         {body}",
        body.len()
    );
    s.write_all(req.as_bytes()).await.unwrap();
    let raw = read_full_response(&mut s).await;
    let (status_line, _, resp_body) = parse_response(&raw);
    assert!(
        status_line.starts_with("HTTP/1.1 200"),
        "got: {status_line}"
    );

    // Relay intact: the origin saw the original, unmasked bytes.
    let echo: serde_json::Value =
        serde_json::from_str(&String::from_utf8(resp_body).unwrap()).unwrap();
    assert_eq!(echo["target"], "/app/sensitive?api_key=querysecret789");
    assert_eq!(echo["body"], body);
    let hdrs: Vec<String> = echo["headers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert!(hdrs.contains(&"authorization: Bearer supersecretkey123".to_string()));
    assert!(hdrs.contains(&"cookie: sessionid=cookiesecret456".to_string()));

    // --- Let the async insert land, then assert the DB ---------------------
    let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(10);
    loop {
        let c: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM captures")
            .fetch_one(&pool)
            .await
            .unwrap();
        if c >= 1 {
            break;
        }
        if tokio::time::Instant::now() > deadline {
            panic!("timed out waiting for row");
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    let id = col_id(&pool, "request_method='POST' AND status_code=200").await;

    // Stored request headers: sensitive values masked, others unchanged.
    let req_headers = col_text(&pool, id, "request_headers").await.unwrap();
    let rh: Vec<serde_json::Value> = serde_json::from_str(&req_headers).unwrap();
    let val = |name: &str| -> String {
        rh.iter()
            .find(|v| v["name"].as_str() == Some(name))
            .map(|v| v["value"].as_str().unwrap().to_string())
            .unwrap()
    };
    assert_eq!(val("authorization"), "Bea***23");
    assert_eq!(val("cookie"), "ses***56");
    assert_eq!(val("content-type"), "application/json"); // not sensitive
    assert!(!req_headers.contains("supersecretkey123"));
    assert!(!req_headers.contains("cookiesecret456"));

    // Stored target + query: the param is masked.
    assert_eq!(
        col_text(&pool, id, "request_target").await.unwrap(),
        "/sensitive?api_key=que***89"
    );
    assert_eq!(
        col_text(&pool, id, "upstream_query").await.unwrap(),
        "api_key=que***89"
    );

    // Stored request body: password masked, `safe` intact.
    let body_bytes = col_blob(&pool, id, "request_body").await.unwrap();
    assert_eq!(
        String::from_utf8(body_bytes).unwrap(),
        r#"{"password":"bod***23","safe":"keepme"}"#
    );

    // Clean up.
    proxy_task.abort();
    origin_task.abort();
}
