//! HAR 1.2 export: turn stored captures into a HAR log object.
//!
//! HAR is the lingua franca of HTTP tooling — a `.har` file opens directly in
//! Chrome DevTools (Network → Import HAR) and Charles. We emit one capture or
//! the current filter set as a HAR 1.2 log. Binary bodies are base64-encoded
//! in `content.text` with `encoding: "base64"` so they survive the JSON
//! round-trip byte-for-byte.

use base64::Engine;
use serde_json::{json, Value};

use crate::api::CaptureRow;

/// Build the HAR document for one or more captures.
///
/// The HAR 1.2 spec wraps everything in a top-level `log` object —
/// `{"log": {"version", "creator", "pages", "entries"}}`. Chrome DevTools
/// and Charles reject a file that puts those fields at the top level, so
/// the wrapper is mandatory, not cosmetic.
///
/// `rows` are in the order they should appear in the HAR (the API passes
/// them in `id DESC` order, matching the list view).
pub fn build_log(rows: &[CaptureRow]) -> Value {
    let entries: Vec<Value> = rows.iter().map(build_entry).collect();
    json!({
        "log": {
            "version": "1.2",
            "creator": {
                "name": "ghost",
                "version": env!("CARGO_PKG_VERSION"),
            },
            "pages": [],
            "entries": entries,
        }
    })
}

fn build_entry(row: &CaptureRow) -> Value {
    let (url, query_string) = split_url(row);
    let request = build_request(row, &url, &query_string);
    let response = build_response(row);
    // `cache` and `timings` are listed in the spec's entry object; Chrome
    // DevTools expects them present. We have no per-phase timing breakdown,
    // so every phase is -1 (the spec's "not available" sentinel) and `time`
    // carries the total. Failed exchanges (no HTTP status) get the same block.
    let timings = json!({
        "blocked": -1, "dns": -1, "connect": -1, "send": -1,
        "wait": -1, "receive": -1,
    });
    let mut entry = json!({
        "startedDateTime": row.created_at,
        "time": row.total_ms,
        "request": request,
        "response": response,
        "cache": {},
        "timings": timings,
    });
    if let Some(err) = row.error.as_deref().filter(|e| !e.is_empty()) {
        entry["comment"] = json!(err);
    }
    entry
}

/// Reassemble the absolute upstream URL and its query-string part.
///
/// The proxy stores the upstream target split into `upstream_path` and
/// `upstream_query`; the HAR `url` must be absolute, so we prefix the
/// scheme + authority.
fn split_url(row: &CaptureRow) -> (String, Vec<Value>) {
    let mut url = format!("{}://{}{}", row.upstream_scheme, row.upstream_authority, row.upstream_path);
    let mut query_string = Vec::new();
    if let Some(q) = &row.upstream_query {
        url.push('?');
        url.push_str(q);
        for pair in q.split('&') {
            let (name, value) = match pair.split_once('=') {
                Some((n, v)) => (n.to_string(), v.to_string()),
                None => (pair.to_string(), String::new()),
            };
            query_string.push(json!({ "name": name, "value": value }));
        }
    }
    (url, query_string)
}

fn build_request(row: &CaptureRow, url: &str, query_string: &[Value]) -> Value {
    let headers = parse_headers(&row.request_headers);
    let (headers_size, body_size) = body_sizes(&row.request_headers, row.request_body.as_deref());
    let mut request = json!({
        "method": row.request_method,
        "url": url,
        "httpVersion": format!("HTTP/{}", row.request_version),
        "cookies": [],
        "headers": headers,
        "queryString": query_string.to_vec(),
        "headersSize": headers_size,
        "bodySize": body_size,
    });
    if let Some(body) = &row.request_body
        && !body.is_empty()
    {
        request["postData"] = body_content(body, &row.request_headers);
    }
    request
}

fn build_response(row: &CaptureRow) -> Value {
    let headers = parse_headers(&row.response_headers);
    let (headers_size, _) = body_sizes(&row.response_headers, row.response_body.as_deref());
    // bodySize is the full on-wire length; the stored blob may be a truncated
    // prefix. (Requests are never truncated — oversized ones are rejected.)
    let body_size = match row.response_body.as_deref() {
        Some(b) if !b.is_empty() => row.response_body_len,
        _ => -1,
    };
    let status_text = match row.status_code {
        Some(code) => http::StatusCode::from_u16(code as u16)
            .ok()
            .and_then(|s| s.canonical_reason())
            .unwrap_or_default()
            .to_string(),
        None => row.error.clone().unwrap_or_default(),
    };
    let mut response = json!({
        "status": row.status_code.unwrap_or(0),
        "statusText": status_text,
        "httpVersion": row.response_version.as_deref().map(|v| format!("HTTP/{}", v)).unwrap_or_else(|| "HTTP/1.1".into()),
        "cookies": [],
        "headers": headers,
        "content": content_object(row.response_body.as_deref(), &row.response_headers, row.response_body_len, row.response_body_truncated),
        "redirectURL": "",
        "headersSize": headers_size,
        "bodySize": body_size,
    });
    if row.status_code.is_none() {
        // No response was ever received; the HAR spec has no "failed" flag,
        // so we mark it with status 0 and the error as statusText.
        response["_error"] = json!(row.error.clone().unwrap_or_else(|| "unknown error".into()));
    }
    response
}

/// HAR `content` object for a stored body.
///
/// Text bodies are inlined as UTF-8 `text`; binary bodies are base64-encoded
/// with `encoding: "base64"` so importers can decode them byte-for-byte.
fn content_object(body: Option<&[u8]>, headers_json: &str, len: i64, truncated: i64) -> Value {
    let bytes = body.unwrap_or_default();
    let mime = content_type(headers_json);
    let mut content = json!({
        "size": len,
        "mimeType": mime,
    });
    if bytes.is_empty() {
        return content;
    }
    if truncated == 1 {
        // The stored blob is a prefix of the original; say so rather than
        // emitting a body that decodes to the wrong bytes.
        content["comment"] = json!("body truncated to stored prefix");
    }
    if std::str::from_utf8(bytes).is_ok() {
        content["text"] = json!(String::from_utf8(bytes.to_vec()).unwrap());
    } else {
        content["text"] = json!(base64::engine::general_purpose::STANDARD.encode(bytes));
        content["encoding"] = json!("base64");
    }
    content
}

/// `postData` for the request body: `text` (or base64) plus `mimeType` from
/// the request `Content-Type` header.
fn body_content(body: &[u8], headers_json: &str) -> Value {
    let mime = content_type(headers_json);
    let mut post = json!({
        "mimeType": mime,
    });
    if std::str::from_utf8(body).is_ok() {
        post["text"] = json!(String::from_utf8(body.to_vec()).unwrap());
    } else {
        post["text"] = json!(base64::engine::general_purpose::STANDARD.encode(body));
        post["encoding"] = json!("base64");
    }
    post
}

/// Parse the stored `[{name,value}]` header array into a HAR `headers` array.
fn parse_headers(headers_json: &str) -> Vec<Value> {
    let arr: Vec<Value> = serde_json::from_str(headers_json).unwrap_or_default();
    arr.into_iter()
        .filter_map(|h| {
            let name = h.get("name")?.as_str()?.to_string();
            let value = h.get("value")?.as_str()?.to_string();
            Some(json!({ "name": name, "value": value }))
        })
        .collect()
}

/// Best-effort `headersSize`/`bodySize` for a side of the exchange.
///
/// We don't store wire-level header sizes, so we approximate: the sum of
/// `name: value` line lengths plus CRLFs. `-1` when unknown (no body).
fn body_sizes(headers_json: &str, body: Option<&[u8]>) -> (i64, i64) {
    let arr: Vec<Value> = serde_json::from_str(headers_json).unwrap_or_default();
    let mut size = 0i64;
    for h in &arr {
        let name = h.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let value = h.get("value").and_then(|v| v.as_str()).unwrap_or("");
        size += name.len() as i64 + value.len() as i64 + 3; // ": " + CRLF
    }
    let body_size = match body {
        Some(b) if !b.is_empty() => b.len() as i64,
        _ => -1,
    };
    (size, body_size)
}

/// Pull the `Content-Type` header value out of the stored header array.
fn content_type(headers_json: &str) -> String {
    let arr: Vec<Value> = serde_json::from_str(headers_json).unwrap_or_default();
    for h in &arr {
        let name = h.get("name").and_then(|v| v.as_str());
        if name.is_some_and(|n| n.eq_ignore_ascii_case("content-type")) {
            return h.get("value").and_then(|v| v.as_str()).unwrap_or_default().to_string();
        }
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(response_body: Option<Vec<u8>>, wire_len: i64, truncated: i64) -> CaptureRow {
        CaptureRow {
            id: 1,
            created_at: "2024-01-01T00:00:00Z".into(),
            total_ms: 5,
            client_addr: "10.0.0.1".into(),
            request_version: "1.1".into(),
            request_method: "GET".into(),
            request_target: "/app/a".into(),
            request_headers: json!([{"name": "host", "value": "example.org"}]).to_string(),
            request_body: None,
            request_body_len: 0,
            request_body_truncated: 0,
            request_body_sha256: None,
            upstream_scheme: "http".into(),
            upstream_authority: "example.org:80".into(),
            upstream_path: "/app/a".into(),
            upstream_query: None,
            response_version: Some("1.1".into()),
            status_code: Some(200),
            response_headers: json!([{"name": "content-type", "value": "application/json"}])
                .to_string(),
            response_body,
            response_body_len: wire_len,
            response_body_truncated: truncated,
            response_body_sha256: None,
            error: None,
        }
    }

    #[test]
    fn body_size_reports_wire_length_not_stored_blob() {
        // Stored blob is a truncated 5-byte prefix of a 20-byte body.
        let r = row(Some(b"01234".to_vec()), 20, 1);
        let doc = build_log(std::slice::from_ref(&r));
        let e = &doc["log"]["entries"][0];
        assert_eq!(e["response"]["bodySize"], 20);
        assert_eq!(e["response"]["content"]["size"], 20);
        assert_eq!(
            e["response"]["content"]["comment"],
            "body truncated to stored prefix"
        );
    }

    #[test]
    fn server_ip_address_omitted_without_resolution() {
        let r = row(None, 0, 0);
        let doc = build_log(std::slice::from_ref(&r));
        let e = &doc["log"]["entries"][0];
        assert!(e.get("serverIPAddress").is_none());
        // No error → no entry-level `comment` key (not "comment": null).
        assert!(e.get("comment").is_none());
    }

    #[test]
    fn unknown_status_text_is_empty() {
        let mut r = row(None, 0, 0);
        r.status_code = Some(599);
        let doc = build_log(std::slice::from_ref(&r));
        assert_eq!(doc["log"]["entries"][0]["response"]["statusText"], "");
    }
}
