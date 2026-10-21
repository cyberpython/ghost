//! The transparent capture proxy.
//!
//! For each accepted connection, hyper parses HTTP/1.0/1.1 requests. Each
//! request is:
//!   1. buffered (with a cap) so it can be stored,
//!   2. re-issued to the fixed upstream with only the request target remapped,
//!   3. its response buffered, and
//!   4. the response relayed to the client,
//!   5. the whole exchange stored in SQLite.
//!
//! Transparency: every header value is forwarded byte-for-byte, in its
//! original order, with duplicates preserved. Header *names* are emitted in
//! their canonical lowercase form, both on the wire and in the database (field
//! names are case-insensitive per RFC 9110). Only framing headers
//! (`Content-Length`, `Transfer-Encoding`) are normalised to a plain
//! `Content-Length`, and the request target is remapped onto the upstream
//! prefix. `Host` is set only when the client omitted it. No body bytes are
//! ever altered **on the wire** — what the client and the upstream exchange
//! is byte-for-byte.
//!
//! The **stored** copy is different: before a row is written, sensitive
//! fields are masked (see [`crate::redact`]). Sensitive header *values*,
//! query params, and sensitive JSON/form body fields are replaced with a
//! fixed `***` mask, so the SQLite file (and the read-only API + SPA that
//! serve it) never holds a raw secret. The stored `*_body_sha256` still
//! hashes the original on-wire bytes, so it no longer verifies the stored
//! blob when masking or truncation changed it.

use bytes::Bytes;
use http::header::{CONTENT_LENGTH, CONTENT_TYPE, HOST};
use http::{HeaderMap, HeaderValue, Method, Version};
use http_body_util::{BodyExt, Full, LengthLimitError, Limited, combinators::BoxBody};
use hyper::StatusCode;
use hyper::body::Incoming;
use hyper::client::conn::http1 as client_http1;
use hyper::server::conn::http1 as server_http1;
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::TokioIo;
use sha2::{Digest, Sha256};
use std::time::Instant;
use tokio::net::{TcpListener, TcpStream};
use tracing::{debug, error};

use crate::db::{self, Capture};
use crate::redact;
use crate::url_join::{UpstreamBase, split_target};

/// Per-connection immutable configuration, shared by all requests.
#[derive(Clone)]
pub struct State {
    pub db: sqlx::sqlite::SqlitePool,
    pub upstream: UpstreamBase,
    /// Max bytes of a body retained for storage (the stored copy is truncated
    /// here; relaying is always in full).
    pub max_body_bytes: usize,
    /// FR-011a: header names masked before storage; a CLI override replaces
    /// the compiled-in default list wholesale (never merges with it).
    pub sensitive_headers: Vec<String>,
}

/// Accept-and-serve loop. Runs until the process is shutting down.
pub async fn serve(listener: TcpListener, state: State) -> Result<(), anyhow::Error> {
    loop {
        let (stream, peer) = listener.accept().await?;
        let io = TokioIo::new(stream);
        let st = state.clone();
        tokio::task::spawn(async move {
            let service = service_fn(move |req: Request<Incoming>| {
                let st = st.clone();
                Box::pin(async move { proxy(req, st, peer).await })
            });
            // No case preservation or title-casing: relays go out with
            // canonical lowercase names, exactly as stored.
            if let Err(err) = server_http1::Builder::new()
                .serve_connection(io, service)
                .with_upgrades()
                .await
            {
                error!("serve connection error: {err}");
            }
        });
    }
}

/// Per-request handler (runs on one accepted connection).
async fn proxy(
    req: Request<Incoming>,
    st: State,
    peer: std::net::SocketAddr,
) -> Result<Response<BoxBody<Bytes, hyper::Error>>, hyper::Error> {
    let started = Instant::now();
    let client_addr = peer.to_string();

    // Snapshot request metadata before the request is consumed.
    let method = req.method().clone();
    let req_version = version_str(req.version());
    let target = req
        .uri()
        .path_and_query()
        .map(|s| s.to_string())
        .unwrap_or_default();
    let client_headers = req.headers().clone(); // original order; names are canonical lowercase
    let (up_uri, up_path, up_query) = st.upstream.resolve(&target);
    let req_body = req.into_body();

    // Reject CONNECT outright (no TLS, no tunnel).
    if method == Method::CONNECT {
        let mut resp = Response::new(full_err("CONNECT is not supported by this proxy"));
        *resp.status_mut() = StatusCode::METHOD_NOT_ALLOWED;
        let cap = build_capture(
            &st,
            &client_addr,
            started,
            &method,
            &target,
            req_version,
            &client_headers,
            None,
            0,
            1,
            &up_path,
            up_query.as_deref(),
            None,
            None,
            &HeaderMap::new(),
            &[],
            0,
            0,
            Some("CONNECT rejected (HTTP-only proxy)".into()),
        );
        spawn_insert(st.db.clone(), cap);
        return Ok(resp);
    }

    // Buffer the request body (this is what makes 413 vs relay).
    let req_bytes: Bytes = match Limited::new(req_body, st.max_body_bytes).collect().await {
        Ok(c) => c.to_bytes(),
        Err(e) => {
            let (status, msg) = if e.downcast_ref::<LengthLimitError>().is_some() {
                (
                    StatusCode::PAYLOAD_TOO_LARGE,
                    "request body exceeds the storage limit",
                )
            } else {
                (StatusCode::BAD_REQUEST, "failed to read request body")
            };
            let mut resp = Response::new(full_err(msg));
            *resp.status_mut() = status;
            let cap = build_capture(
                &st,
                &client_addr,
                started,
                &method,
                &target,
                req_version,
                &client_headers,
                None,
                st.max_body_bytes as i64,
                1,
                &up_path,
                up_query.as_deref(),
                None,
                None,
                &HeaderMap::new(),
                &[],
                0,
                0,
                Some(format!("{msg} (rejected)")),
            );
            spawn_insert(st.db.clone(), cap);
            return Ok(resp);
        }
    };

    // Build the upstream request: same method, remapped target, headers as-is
    // except framing, body bytes identical.
    let mut up_req = Request::new(Full::new(req_bytes.clone()));
    *up_req.method_mut() = method.clone();
    *up_req.uri_mut() = up_uri;
    {
        let h = up_req.headers_mut();
        for (name, value) in client_headers.iter() {
            if name.as_str().eq_ignore_ascii_case("content-length")
                || name.as_str().eq_ignore_ascii_case("transfer-encoding")
            {
                continue;
            }
            h.append(name, value.clone());
        }
        if !h.contains_key(&HOST) {
            h.insert(
                HOST,
                HeaderValue::try_from(st.upstream.authority.as_str()).unwrap(),
            );
        }
        h.insert(
            CONTENT_LENGTH,
            HeaderValue::try_from(req_bytes.len().to_string()).unwrap(),
        );
    }

    // Connect to the upstream (one connection per exchange; the tool values
    // byte-faithfulness and simplicity over connection reuse).
    let resp = match TcpStream::connect(st.upstream.authority.as_str()).await {
        Ok(stream) => {
            let io = TokioIo::new(stream);
            // Canonical lowercase names on the wire, matching the stored record.
            let (mut sender, conn) = match client_http1::Builder::new().handshake(io).await {
                Ok(h) => h,
                Err(e) => {
                    return bad_gateway(
                        &st,
                        started,
                        &client_addr,
                        &method,
                        &target,
                        req_version,
                        &client_headers,
                        &req_bytes,
                        &up_path,
                        up_query.as_deref(),
                        &e.to_string(),
                    );
                }
            };
            tokio::spawn(async move {
                if let Err(e) = conn.await {
                    error!("upstream connection error: {e}");
                }
            });
            match sender.send_request(up_req).await {
                Ok(r) => r,
                Err(e) => {
                    return bad_gateway(
                        &st,
                        started,
                        &client_addr,
                        &method,
                        &target,
                        req_version,
                        &client_headers,
                        &req_bytes,
                        &up_path,
                        up_query.as_deref(),
                        &e.to_string(),
                    );
                }
            }
        }
        Err(e) => {
            return bad_gateway(
                &st,
                started,
                &client_addr,
                &method,
                &target,
                req_version,
                &client_headers,
                &req_bytes,
                &up_path,
                up_query.as_deref(),
                &e.to_string(),
            );
        }
    };

    let resp_status = resp.status();
    let resp_headers = resp.headers().clone(); // preserves order + case
    let resp_version = version_str(resp.version());
    let resp_body = resp.into_body();

    // Buffer the full response so it can be relayed in full and stored.
    let resp_bytes: Bytes = match Limited::new(resp_body, usize::MAX).collect().await {
        Ok(c) => c.to_bytes(),
        Err(e) => {
            error!("failed to read upstream response body: {e}");
            return bad_gateway(
                &st,
                started,
                &client_addr,
                &method,
                &target,
                req_version,
                &client_headers,
                &req_bytes,
                &up_path,
                up_query.as_deref(),
                &e.to_string(),
            );
        }
    };

    // Relay to the client: status + headers as-is except framing, body identical.
    let mut client_resp = Response::new(full_err(resp_bytes.clone()));
    *client_resp.status_mut() = resp_status;
    {
        let h = client_resp.headers_mut();
        for (name, value) in resp_headers.iter() {
            if name.as_str().eq_ignore_ascii_case("content-length")
                || name.as_str().eq_ignore_ascii_case("transfer-encoding")
            {
                continue;
            }
            h.append(name, value.clone());
        }
        h.insert(
            CONTENT_LENGTH,
            HeaderValue::try_from(resp_bytes.len().to_string()).unwrap(),
        );
    }

    let resp_trunc = if resp_bytes.len() > st.max_body_bytes {
        1
    } else {
        0
    };
    let cap = build_capture(
        &st,
        &client_addr,
        started,
        &method,
        &target,
        req_version,
        &client_headers,
        Some(&req_bytes),
        req_bytes.len() as i64,
        0,
        &up_path,
        up_query.as_deref(),
        Some(resp_status.as_u16()),
        Some(resp_version),
        &resp_headers,
        &resp_bytes,
        resp_bytes.len() as i64,
        resp_trunc,
        None,
    );
    spawn_insert(st.db.clone(), cap);

    Ok(client_resp)
}

/// Build a `502 Bad Gateway` response for an upstream failure and record it.
#[allow(clippy::too_many_arguments)]
fn bad_gateway(
    st: &State,
    started: Instant,
    client_addr: &str,
    method: &Method,
    target: &str,
    req_version: &str,
    client_headers: &HeaderMap<HeaderValue>,
    req_bytes: &Bytes,
    up_path: &str,
    up_query: Option<&str>,
    e: &str,
) -> Result<Response<BoxBody<Bytes, hyper::Error>>, hyper::Error> {
    let msg = format!("bad gateway: {e}");
    error!("{msg}");
    let mut resp = Response::new(full_err(Bytes::copy_from_slice(msg.as_bytes())));
    *resp.status_mut() = StatusCode::BAD_GATEWAY;
    let cap = build_capture(
        st,
        client_addr,
        started,
        method,
        target,
        req_version,
        client_headers,
        Some(req_bytes),
        req_bytes.len() as i64,
        0,
        up_path,
        up_query,
        None,
        None,
        &HeaderMap::new(),
        &[],
        0,
        0,
        Some(msg),
    );
    spawn_insert(st.db.clone(), cap);
    Ok(resp)
}

/// Assemble a `Capture` row from the parts gathered while handling a request.
#[allow(clippy::too_many_arguments)]
fn build_capture(
    st: &State,
    client_addr: &str,
    started: Instant,
    method: &Method,
    target: &str,
    req_version: &str,
    client_headers: &HeaderMap<HeaderValue>,
    req_body_full: Option<&Bytes>,
    req_body_len: i64,
    req_body_truncated: i64,
    up_path: &str,
    up_query: Option<&str>,
    resp_status: Option<u16>,
    resp_version: Option<&str>,
    resp_headers: &HeaderMap<HeaderValue>,
    resp_body_full: &[u8],
    resp_body_len: i64,
    resp_body_truncated: i64,
    error: Option<String>,
) -> Capture {
    // Mask the FULL body first, then truncate: a cap can land inside a
    // sensitive value, so masking the truncated copy would leak a raw prefix.
    // The SHA-256 still hashes the original on-wire bytes.
    let ct_req = client_headers
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok());
    let (req_stored, req_sha) = match req_body_full {
        Some(full) => {
            let masked = redact::mask_body(full, ct_req);
            let src = masked.as_deref().unwrap_or(full);
            let cap = src.len().min(st.max_body_bytes);
            (Some(src[..cap].to_vec()), Some(sha256_hex(full)))
        }
        None => (None, None),
    };
    let ct_resp = resp_headers.get(CONTENT_TYPE).and_then(|v| v.to_str().ok());
    let n = (resp_body_len as usize).min(resp_body_full.len());
    let full = &resp_body_full[0..n];
    let masked = redact::mask_body(full, ct_resp);
    let src = masked.as_deref().unwrap_or(full);
    let cap = src.len().min(st.max_body_bytes);
    let (resp_stored, resp_sha) = (
        Some(src[..cap].to_vec()),
        if !full.is_empty() {
            Some(sha256_hex(full))
        } else {
            None
        },
    );
    // Store the request target with sensitive query params masked.
    let (tp, tq) = split_target(target);
    let request_target = match tq {
        Some(q) => format!("{tp}?{}", redact::mask_query(q)),
        None => target.to_string(),
    };
    Capture {
        created_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        total_ms: started.elapsed().as_millis() as i64,
        client_addr: client_addr.to_string(),
        request_version: req_version.to_string(),
        request_method: method.to_string(),
        request_target,
        request_headers: headers_to_json(&redact::mask_headers_with(client_headers, &st.sensitive_headers)),
        request_body: req_stored,
        request_body_len: req_body_len,
        request_body_truncated: req_body_truncated,
        request_body_sha256: req_sha,
        upstream_scheme: st.upstream.scheme.clone(),
        upstream_authority: st.upstream.authority.clone(),
        upstream_path: up_path.to_string(),
        upstream_query: up_query.map(redact::mask_query),
        response_version: resp_version.map(|s| s.to_string()),
        status_code: resp_status.map(|s| s as i32),
        response_headers: headers_to_json(&redact::mask_headers_with(resp_headers, &st.sensitive_headers)),
        response_body: resp_stored,
        response_body_len: resp_body_len,
        response_body_truncated: resp_body_truncated,
        response_body_sha256: resp_sha,
        error,
    }
}

fn spawn_insert(db: sqlx::sqlite::SqlitePool, c: Capture) {
    tokio::task::spawn(async move {
        match db::insert(&db, &c).await {
            Ok(id) => debug!("stored capture id={id}"),
            Err(e) => error!("failed to store capture: {e}"),
        }
    });
}

/// Serialise (name, value) pairs as a JSON array of `{"name","value"}`.
fn headers_to_json(items: &[(String, String)]) -> String {
    use serde_json::json;

    let arr: Vec<serde_json::Value> = items
        .iter()
        .map(|(name, value)| json!({ "name": name, "value": value }))
        .collect();
    serde_json::to_string(&arr).unwrap_or_else(|_| "[]".to_string())
}

fn version_str(v: Version) -> &'static str {
    if v == Version::HTTP_09 {
        "0.9"
    } else if v == Version::HTTP_10 {
        "1.0"
    } else if v == Version::HTTP_11 {
        "1.1"
    } else if v == Version::HTTP_2 {
        "2.0"
    } else if v == Version::HTTP_3 {
        "3.0"
    } else {
        "unknown"
    }
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

fn full_err<T: Into<Bytes>>(chunk: T) -> BoxBody<Bytes, hyper::Error> {
    Full::new(chunk.into())
        .map_err(|never| match never {})
        .boxed()
}
