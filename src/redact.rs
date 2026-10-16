//! Redaction of *stored* capture copies.
//!
//! Sensitive values (API keys, passwords, tokens, cookies, …) are masked
//! before anything is written to SQLite. Only the stored copies change; the
//! bytes relayed to client and upstream stay byte-for-byte.
//!
//! Mask format: first 2–3 chars + exactly `***` + last 1–2 chars, so the
//! hidden middle's length cannot be inferred. Values of five chars or fewer
//! are replaced entirely by `***` (too short to keep a prefix/suffix safely).
//!
//! Known limitation: masking is key-based — a secret *reflected* into a
//! non-sensitive field (e.g. an echoed string) is not scrubbed.

use http::header::{HeaderMap, HeaderValue};

const SENSITIVE_HEADERS: &[&str] = &[
    "authorization",
    "proxy-authorization",
    "cookie",
    "set-cookie",
    "x-api-key",
    "x-auth-token",
    "x-access-token",
    "x-amz-security-token",
    "x-goog-auth-token",
    "x-hub-authorization",
    "x-secret",
    "x-secret-key",
    "x-csrf-token",
    "x-csrftoken",
    "x-xsrf-token",
];

const SENSITIVE_KEYS: &[&str] = &[
    "api_key",
    "apikey",
    "key",
    "access_key",
    "access_key_secret",
    "secret",
    "secret_key",
    "secretkey",
    "client_secret",
    "private_key",
    "privatekey",
    "signing_key",
    "master_key",
    "encryption_key",
    "password",
    "passwd",
    "pwd",
    "pass",
    "pword",
    "token",
    "access_token",
    "refresh_token",
    "id_token",
    "auth_token",
    "api_token",
    "session_token",
    "session",
    "session_id",
    "sessionid",
    "sid",
    "jsessionid",
    "auth",
    "credentials",
    "signature",
    "sign",
    "hmac",
];

/// Mask a scalar value: first 2–3 chars + `***` + last 1–2 chars (see module
/// docs). Empty in → empty out; ≤5 chars → `***`.
pub fn mask_value(value: &str) -> String {
    let chars: Vec<char> = value.chars().collect();
    let n = chars.len();
    if n == 0 {
        return String::new();
    }
    if n <= 5 {
        return "***".to_string();
    }
    let (p, s) = if n <= 10 { (2, 1) } else { (3, 2) };
    let mut out = String::with_capacity(n.min(p + 3 + s));
    for &c in &chars[..p] {
        out.push(c);
    }
    out.push_str("***");
    for &c in &chars[n - s..] {
        out.push(c);
    }
    out
}

/// Does this header *name* mark its value as sensitive? (Case-insensitive.)
pub fn is_sensitive_header(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    SENSITIVE_HEADERS.iter().any(|h| *h == n)
}

/// Does this query-param / JSON object key name mark the value as sensitive?
/// (Case-insensitive, exact match — `api_keys`/`tokens` do not match.)
pub fn is_sensitive_key(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    SENSITIVE_KEYS.iter().any(|k| *k == n)
}

/// Return (canonical-lowercase-name, value) pairs in original order; the
/// value is masked iff the name is sensitive.
pub fn mask_headers(h: &HeaderMap<HeaderValue>) -> Vec<(String, String)> {
    h.iter()
        .map(|(name, value)| {
            let value = value.to_str().unwrap_or("");
            let value = if is_sensitive_header(name.as_str()) {
                mask_value(value)
            } else {
                value.to_string()
            };
            (name.as_str().to_string(), value)
        })
        .collect()
}

/// Mask sensitive params in a query string (or `a=b&c=d` form body).
/// Non-sensitive pairs are kept byte-exact; bare keys and empty values are
/// untouched. Keys are matched after percent-decoding, so a percent-encoded
/// sensitive key is still recognised.
pub fn mask_query(q: &str) -> String {
    q.split('&')
        .map(|pair| match pair.split_once('=') {
            Some((k, v)) if is_sensitive_key(&pct_decode(k)) => {
                format!("{k}={}", mask_value(v))
            }
            _ => pair.to_string(),
        })
        .collect::<Vec<_>>()
        .join("&")
}

/// Percent-decode (`%XX` and `+`→space) for *key* matching only.
fn pct_decode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '+' => out.push(' '),
            '%' => {
                let (h1, h2) = (it.next(), it.next());
                match (h1, h2) {
                    (Some(h1), Some(h2)) if h1.is_ascii_hexdigit() && h2.is_ascii_hexdigit() => {
                        let cp = (h1.to_digit(16).unwrap() << 4) | h2.to_digit(16).unwrap();
                        out.push(char::from_u32(cp).expect("byte decode is valid ASCII"));
                    }
                    _ => {
                        out.push('%');
                        if let Some(h1) = h1 {
                            out.push(h1);
                        }
                        if let Some(h2) = h2 {
                            out.push(h2);
                        }
                    }
                }
            }
            c => out.push(c),
        }
    }
    out
}

/// Mask sensitive fields in a body. Returns `Some(masked)` **only if
/// something changed**, else `None`.
///
/// - `x-www-form-urlencoded` (any casing) → `k=v&…` pairs masked like query
///   strings. `multipart/form-data` is out of scope.
/// - Content-type contains `json`, **or** the body parses as JSON → JSON
///   strings under sensitive object keys are masked in place (key order,
///   whitespace and every other byte preserved).
/// - Anything else (binary, unknown text) → untouched.
pub fn mask_body(body: &[u8], ct: Option<&str>) -> Option<Vec<u8>> {
    let ct = ct.unwrap_or_default().trim().to_ascii_lowercase();
    if ct.contains("x-www-form-urlencoded") {
        let s = match std::str::from_utf8(body) {
            Ok(s) => s,
            Err(_) => return None,
        };
        let masked = mask_query(s);
        return (masked != s).then(|| masked.into_bytes());
    }
    let is_json = ct.contains("json") || serde_json::from_slice::<serde_json::Value>(body).is_ok();
    if !is_json {
        return None;
    }
    let s = match std::str::from_utf8(body) {
        Ok(s) => s,
        Err(_) => return None,
    };
    let spans = json_value_spans(s.as_bytes());
    if spans.is_empty() {
        return None;
    }
    let out = splice(s.as_bytes(), &spans);
    (out.as_slice() != body).then_some(out)
}

// ── Raw-JSON span scanner ────────────────────────────────────────────────────
//
// Records the byte spans of JSON string *values* whose sensitivity context is
// true, for a later in-place splice. The walk is iterative (explicit frame
// stack): a 100 MiB cap-deep `[[[…` nest must not overflow a recursive call
// stack on a network-facing proxy. Tolerant by design: it masks complete
// values and bails (no panic) on malformed/unterminated input.

#[derive(Clone, Copy, PartialEq)]
enum Step {
    Key,
    Colon,
    Value,
    Punct,
}

#[derive(Clone, Copy)]
struct Frame {
    /// Object container (true) or array (false).
    object: bool,
    /// This container's value-sensitivity, inherited by its contents.
    ambient: bool,
    step: Step,
    /// Object only: sensitivity of the value being read.
    value_ctx: bool,
}

#[derive(Clone, Copy, PartialEq)]
enum Status {
    Completed,
    Pushed,
    Bailed,
}

struct Sc<'a> {
    b: &'a [u8],
    i: usize,
    spans: Vec<(usize, usize)>,
    stack: Vec<Frame>,
    bailed: bool,
}

fn json_value_spans(b: &[u8]) -> Vec<(usize, usize)> {
    let mut sc = Sc {
        b,
        i: 0,
        spans: Vec::new(),
        stack: Vec::new(),
        bailed: false,
    };
    if read_value(&mut sc, false) == Status::Bailed {
        return sc.spans;
    }
    while !sc.stack.is_empty() {
        step_once(&mut sc);
        if sc.bailed {
            break;
        }
    }
    sc.spans
}

fn step_once(sc: &mut Sc) {
    let last = sc.stack.len() - 1;
    let step = sc.stack[last].step;
    match step {
        Step::Key => {
            sc.skip_ws();
            if sc.peek() == Some(b'}') {
                sc.i += 1;
                pop(sc);
                return;
            }
            let Some((s, e)) = scan_string(sc) else {
                sc.bailed = true;
                return;
            };
            sc.stack[last].value_ctx =
                is_sensitive_key_bytes(sc.b[s..e].as_ref()) || sc.stack[last].ambient;
            sc.stack[last].step = Step::Colon;
        }
        Step::Colon => {
            sc.skip_ws();
            if sc.peek() != Some(b':') {
                sc.bailed = true;
                return;
            }
            sc.i += 1;
            sc.stack[last].step = Step::Value;
        }
        Step::Value => {
            let ctx = if sc.stack[last].object {
                sc.stack[last].value_ctx
            } else {
                sc.stack[last].ambient
            };
            match read_value(sc, ctx) {
                Status::Completed => {
                    sc.stack[last].step = Step::Punct;
                }
                Status::Pushed => {}
                Status::Bailed => {
                    sc.bailed = true;
                }
            }
        }
        Step::Punct => {
            sc.skip_ws();
            let closing = if sc.stack[last].object { b'}' } else { b']' };
            match sc.peek() {
                Some(b',') => {
                    sc.i += 1;
                    sc.stack[last].step = if sc.stack[last].object {
                        Step::Key
                    } else {
                        Step::Value
                    };
                }
                Some(c) if c == closing => {
                    sc.i += 1;
                    pop(sc);
                }
                _ => {
                    sc.bailed = true;
                }
            }
        }
    }
}

/// Pop a completed container; its parent's value has just finished.
fn pop(sc: &mut Sc) {
    sc.stack.pop();
    if let Some(top) = sc.stack.last_mut()
        && top.step == Step::Value
    {
        top.step = Step::Punct;
    }
}

/// Read one value at `sc.i` (whitespace skipped first). Primitive values
/// complete inline; containers push a frame.
fn read_value(sc: &mut Sc, value_ctx: bool) -> Status {
    sc.skip_ws();
    match sc.peek() {
        Some(b'"') => match scan_string(sc) {
            Some((s, e)) => {
                if value_ctx {
                    sc.spans.push((s, e));
                }
                Status::Completed
            }
            None => Status::Bailed,
        },
        Some(b'{') => {
            sc.i += 1;
            sc.stack.push(Frame {
                object: true,
                ambient: value_ctx,
                step: Step::Key,
                value_ctx: false,
            });
            Status::Pushed
        }
        Some(b'[') => {
            sc.i += 1;
            sc.stack.push(Frame {
                object: false,
                ambient: value_ctx,
                step: Step::Value,
                value_ctx: false,
            });
            Status::Pushed
        }
        Some(c) if c.is_ascii_digit() || c == b'-' => {
            while let Some(c) = sc.peek() {
                if c.is_ascii_digit() || matches!(c, b'.' | b'e' | b'E' | b'+' | b'-') {
                    sc.i += 1;
                } else {
                    break;
                }
            }
            Status::Completed
        }
        Some(b't') if sc.b[sc.i..].starts_with(b"true") => {
            sc.i += 4;
            Status::Completed
        }
        Some(b'f') if sc.b[sc.i..].starts_with(b"false") => {
            sc.i += 5;
            Status::Completed
        }
        Some(b'n') if sc.b[sc.i..].starts_with(b"null") => {
            sc.i += 4;
            Status::Completed
        }
        _ => Status::Bailed,
    }
}

/// From the opening quote at `sc.i`, find the closing quote.
/// Returns the content span (start, end) — excluding both quotes — or
/// `None` when the string is unterminated.
fn scan_string(sc: &mut Sc) -> Option<(usize, usize)> {
    let start = sc.i + 1;
    let mut i = start;
    let b = sc.b;
    while i < b.len() {
        match b[i] {
            b'\\' => i += 2,
            b'"' => {
                // Leave `sc.i` just past the closing quote: every caller then
                // reads the next token (the `:` / `,` / `}`) directly.
                sc.i = i + 1;
                return Some((start, i));
            }
            _ => i += 1,
        }
    }
    None
}

fn is_sensitive_key_bytes(b: &[u8]) -> bool {
    match std::str::from_utf8(b) {
        Ok(s) => is_sensitive_key(s),
        Err(_) => false,
    }
}

fn skip_ws_bytes(s: &mut Sc) {
    while s.i < s.b.len() {
        match s.b[s.i] {
            b' ' | b'\t' | b'\n' | b'\r' => s.i += 1,
            _ => break,
        }
    }
}

impl<'a> Sc<'a> {
    fn skip_ws(&mut self) {
        skip_ws_bytes(self);
    }
    fn peek(&self) -> Option<u8> {
        self.b.get(self.i).copied()
    }
}

/// Splice masked values into the original body: every byte outside a recorded
/// span is copied verbatim; each span is replaced by decode → `mask_value` →
/// re-escape, so the result stays valid JSON.
fn splice(body: &[u8], spans: &[(usize, usize)]) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::with_capacity(body.len());
    let mut cur = 0;
    for &(s, e) in spans {
        out.extend_from_slice(&body[cur..s]);
        let decoded = json_unescape(&body[s..e]);
        let masked = mask_value(&decoded);
        out.extend_from_slice(json_escape(&masked).as_bytes());
        cur = e;
    }
    out.extend_from_slice(&body[cur..]);
    out
}

/// Decode a JSON string's content (raw bytes between the quotes).
/// Valid-JSON input round-trips exactly; malformed escapes degrade safely
/// (the input was not valid JSON to begin with).
fn json_unescape(s: &[u8]) -> String {
    let mut out: Vec<u8> = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        let c = s[i];
        if c != b'\\' {
            out.push(c);
            i += 1;
            continue;
        }
        let Some(&n) = s.get(i + 1) else {
            break;
        };
        i += 2;
        match n {
            b'n' => out.push(b'\n'),
            b't' => out.push(b'\t'),
            b'r' => out.push(b'\r'),
            b'b' => out.push(0x08),
            b'f' => out.push(0x0C),
            b'"' | b'\\' | b'/' => out.push(n),
            b'u' => {
                if s.len() >= i + 4 && s[i..i + 4].iter().all(|c| c.is_ascii_hexdigit()) {
                    let hex = std::str::from_utf8(&s[i..i + 4]).unwrap();
                    let cp = u16::from_str_radix(hex, 16).unwrap();
                    i += 4;
                    match char::from_u32(cp as u32) {
                        Some(ch) => {
                            let mut tmp = [0u8; 4];
                            out.extend_from_slice(ch.encode_utf8(&mut tmp).as_bytes());
                        }
                        None => out.extend_from_slice("\u{FFFD}".as_bytes()),
                    }
                } else {
                    let take = (i + 4).min(s.len());
                    out.extend_from_slice(b"\\u");
                    out.extend_from_slice(&s[i..take]);
                    i = take;
                }
            }
            // Unknown escape: keep the backslash and the char verbatim.
            _ => {
                out.push(b'\\');
                out.push(n);
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Re-escape a string for JSON. Output is always valid JSON.
fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0C}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_value_basics() {
        assert_eq!(mask_value(""), "");
        assert_eq!(mask_value("a"), "***");
        assert_eq!(mask_value("abcde"), "***");
        assert_eq!(mask_value("123456"), "12***6");
        assert_eq!(mask_value("0123456789"), "01***9");
        assert_eq!(mask_value("01234567890"), "012***90");
        assert_eq!(mask_value(&"x".repeat(100)), "xxx***xx");
    }

    #[test]
    fn mask_value_unicode() {
        assert_eq!(mask_value("héllo"), "***"); // 5 chars
        assert_eq!(mask_value("héllo123"), "hé***3"); // 8 chars
    }

    #[test]
    fn sensitive_lists() {
        assert!(is_sensitive_header("Authorization"));
        assert!(is_sensitive_header("x-api-key"));
        assert!(is_sensitive_header("SET-COOKIE"));
        assert!(!is_sensitive_header("accept"));
        assert!(is_sensitive_key("API_KEY"));
        assert!(is_sensitive_key("HMAC"));
        assert!(is_sensitive_key("Session_Id"));
        assert!(!is_sensitive_key("api_keys"));
        assert!(!is_sensitive_key("tokens"));
        assert!(!is_sensitive_key("id"));
    }

    #[test]
    fn mask_headers_test() {
        let mut h = HeaderMap::new();
        h.append(
            "Authorization",
            HeaderValue::from_static("Bearer supersecretkey123"),
        );
        h.append("X-Test", HeaderValue::from_static("abc"));
        let out = mask_headers(&h);
        assert_eq!(
            out,
            vec![
                ("authorization".to_string(), "Bea***23".to_string()),
                ("x-test".to_string(), "abc".to_string()),
            ]
        );
    }

    #[test]
    fn mask_query_test() {
        assert_eq!(mask_query("q=1"), "q=1");
        assert_eq!(mask_query("api_key=hunter2&x=1"), "api_key=hu***2&x=1");
        assert_eq!(mask_query("password"), "password");
        assert_eq!(mask_query("password="), "password=");
        assert_eq!(mask_query(""), "");
        assert_eq!(mask_query("API_KEY=hunter2"), "API_KEY=hu***2");
        assert_eq!(mask_query("token=a=b"), "token=***");
        // Percent-encoded sensitive key is still recognised; the key token
        // itself is preserved byte-exact.
        assert_eq!(mask_query("ap%69_key=hunter2"), "ap%69_key=hu***2");
    }

    const JSON_CT: Option<&str> = Some("application/json");

    #[test]
    fn mask_body_json_basics() {
        let out = mask_body(b"{\"password\":\"hunter2\"}", JSON_CT).unwrap();
        assert_eq!(out, br#"{"password":"hu***2"}"#.to_vec());
        // JSON without a Content-Type is still scanned.
        let out = mask_body(b"{\"password\":\"hunter2\"}", None).unwrap();
        assert_eq!(out, br#"{"password":"hu***2"}"#.to_vec());
        // Non-sensitive → unchanged.
        assert_eq!(mask_body(b"{\"id\":1}", JSON_CT), None);
        // Not JSON → untouched, even when the Content-Type says JSON.
        assert_eq!(mask_body(b"not json", JSON_CT), None);
        assert_eq!(mask_body(b"not json", None), None);
    }

    #[test]
    fn mask_body_json_structures() {
        // Nested object.
        let out = mask_body(b"{\"user\":{\"password\":\"hunter2\"}}", JSON_CT).unwrap();
        assert_eq!(out, br#"{"user":{"password":"hu***2"}}"#.to_vec());
        // Duplicate keys: each value masked.
        let out = mask_body(
            b"{\"password\":\"a123456789\",\"password\":\"b123456789\"}",
            JSON_CT,
        )
        .unwrap();
        assert_eq!(
            out,
            br#"{"password":"a1***9","password":"b1***9"}"#.to_vec()
        );
        // Array under a sensitive key: elements inherit sensitivity.
        let out = mask_body(b"{\"access_token\":[\"hunter2\",\"x\"]}", JSON_CT).unwrap();
        assert_eq!(out, br#"{"access_token":["hu***2","***"]}"#.to_vec());
        // A bare sensitive word as an *array element* is not a key: untouched.
        assert_eq!(mask_body(b"[\"password\"]", JSON_CT), None);
        // Non-string values under sensitive keys: untouched.
        assert_eq!(mask_body(b"{\"password\":123}", JSON_CT), None);
        assert_eq!(mask_body(b"{\"password\":{}}", JSON_CT), None);
        // Everything inside a sensitive key's value subtree is over-masked
        // (safe direction).
        let out = mask_body(b"{\"password\":{\"id\":\"hunter2\"}}", JSON_CT).unwrap();
        assert_eq!(out, br#"{"password":{"id":"hu***2"}}"#.to_vec());
        // Whitespace and key order preserved byte-for-byte.
        let body = b"{\n  \"password\" : \"hunter2\"\n}";
        assert_eq!(
            mask_body(body, JSON_CT).unwrap(),
            b"{\n  \"password\" : \"hu***2\"\n}"
        );
    }

    #[test]
    fn mask_body_json_escapes() {
        // Escaped quotes: the value decodes to `say "hi" now12345` (17 chars).
        let out = mask_body(b"{\"password\":\"say \\\"hi\\\" now12345\"}", JSON_CT).unwrap();
        assert_eq!(out, br#"{"password":"say***45"}"#.to_vec());
        // A value whose raw form starts with a \u escape: output stays valid
        // JSON (decode → mask → re-escape).
        let out = mask_body(b"{\"password\":\"\\u0041bcdef12345\"}", JSON_CT).unwrap();
        assert_eq!(out, br#"{"password":"Abc***45"}"#.to_vec());
        let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(v["password"].as_str().unwrap(), "Abc***45");
    }

    #[test]
    fn mask_body_json_robustness() {
        // Empty value → nothing changed.
        assert_eq!(mask_body(b"{\"password\":\"\"}", JSON_CT), None);
        // Unterminated string → tolerant bail, no panic.
        assert_eq!(mask_body(b"{\"password\":\"hunte", JSON_CT), None);
        // Non-UTF8 "JSON" body → untouched.
        let body = [
            b'{', b'"', b'p', b'a', b's', b's', b'w', b'o', b'r', b'd', b'"', b':', b'"', 0xff,
            b'"', b'}',
        ];
        assert_eq!(mask_body(&body, JSON_CT), None);
        // Trailing garbage after the root value: copied verbatim.
        let out = mask_body(b"{\"password\":\"hunter2\"}xx", JSON_CT).unwrap();
        assert_eq!(out, br#"{"password":"hu***2"}xx"#.to_vec());
        // Deep nest (100 MiB cap of `[[[` would be 50M frames): completes
        // without overflowing a recursive call stack.
        let s = format!("{}{}", "[".repeat(50_000), "]".repeat(50_000));
        assert_eq!(mask_body(s.as_bytes(), JSON_CT), None);
    }

    #[test]
    fn mask_body_form() {
        let ct = Some("application/x-www-form-urlencoded");
        let out = mask_body(b"password=hunter2&user=x", ct).unwrap();
        assert_eq!(out, b"password=hu***2&user=x");
        // Form masking needs the Content-Type.
        assert_eq!(mask_body(b"password=hunter2&user=x", None), None);
    }

    #[test]
    fn mask_full_then_truncate_straddle() {
        // The sensitive value straddles a 30-byte cap: masking the FULL body
        // first means the stored slice can never expose a raw prefix.
        let body = format!("{{\"password\":\"{}\"}}", "A".repeat(40));
        let masked = mask_body(body.as_bytes(), JSON_CT).unwrap();
        let m = std::str::from_utf8(&masked).unwrap();
        assert!(!m.contains("AAAA"), "raw run leaked: {m}");
        assert!(m.contains("AAA***"), "slice not masked: {m}");
    }
}
