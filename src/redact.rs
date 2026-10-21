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
use std::rc::Rc;

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

/// Query-parameter names whose values are masked before storage
/// (case-insensitive exact match, matched after percent-decoding).
/// `--sensitive-keys` replaces this list for **query-parameter masking
/// only** (FR-011b). Form-body and JSON-body key masking keep the built-in
/// list regardless of overrides.
const SENSITIVE_QUERY_PARAMETERS: &[&str] = &[
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

/// The compiled-in sensitive-header list as owned strings — the default when
/// `--sensitive-headers` is not given (FR-011a overrides replace this list
/// wholesale; they never merge with it).
pub fn default_sensitive_headers() -> Vec<String> {
    SENSITIVE_HEADERS.iter().map(|s| s.to_string()).collect()
}

/// Case-insensitive exact match against the configured header list.
pub fn is_sensitive_header_in(name: &str, list: &[String]) -> bool {
    list.iter().any(|h| name.eq_ignore_ascii_case(h))
}

/// Does this query-param / JSON object key name mark the value as sensitive?
/// (Case-insensitive, exact match — `api_keys`/`tokens` do not match.)
pub fn is_sensitive_key(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    SENSITIVE_QUERY_PARAMETERS.iter().any(|k| *k == n)
}

/// The compiled-in sensitive query-parameter list as owned strings — the
/// default when `--sensitive-keys` is not given (FR-011b overrides replace
/// this list for query-parameter masking only).
pub fn default_sensitive_query_parameters() -> Vec<String> {
    SENSITIVE_QUERY_PARAMETERS.iter().map(|s| s.to_string()).collect()
}

/// Case-insensitive exact match against the configured query-param list.
pub fn is_sensitive_key_in(name: &str, list: &[String]) -> bool {
    list.iter().any(|k| name.eq_ignore_ascii_case(k))
}

/// Return (canonical-lowercase-name, value) pairs in original order; the
/// value is masked iff the name is in the configured sensitive list.
pub fn mask_headers_with(h: &HeaderMap<HeaderValue>, list: &[String]) -> Vec<(String, String)> {
    h.iter()
        .map(|(name, value)| {
            let value = value.to_str().unwrap_or("");
            let value = if is_sensitive_header_in(name.as_str(), list) {
                mask_value(value)
            } else {
                value.to_string()
            };
            (name.as_str().to_string(), value)
        })
        .collect()
}

/// Same as [`mask_headers_with`] against the compiled-in default list.
pub fn mask_headers(h: &HeaderMap<HeaderValue>) -> Vec<(String, String)> {
    mask_headers_with(h, &default_sensitive_headers())
}

/// Mask sensitive params in a query string (or `a=b&c=d` form body) with the
/// built-in list. Form-body masking rides this default path deliberately:
/// query overrides (`--sensitive-keys`) never reach form fields.
pub fn mask_query(q: &str) -> String {
    mask_query_with(q, &default_sensitive_query_parameters())
}

/// [`mask_query`] against an explicit list (the FR-011b override path for
/// query parameters). Same pair semantics: non-sensitive pairs kept
/// byte-exact, bare keys and empty values untouched, keys matched after
/// percent-decoding.
pub fn mask_query_with(q: &str, list: &[String]) -> String {
    q.split('&')
        .map(|pair| match pair.split_once('=') {
            Some((k, v)) if is_sensitive_key_in(&pct_decode(k), list) => {
                format!("{k}={}", mask_value(v))
            }
            _ => pair.to_string(),
        })
        .collect::<Vec<_>>()
        .join("&")
}

// ── JSON-path configuration (FR-011c) ───────────────────────────────────

/// One step of a JSON path: an object key (matched case-insensitively, like
/// the built-in key masking) or an array selector.
#[derive(Clone, Debug, PartialEq)]
pub enum JsonSeg {
    Key(String),
    AnyIndex,
    Index(usize),
}

/// A configured JSON path pattern, e.g. `body[].redact.this.key`.
pub type JsonPath = Vec<JsonSeg>;

/// Parse a dotted JSON path such as `body[].redact.this.key`, `items[0].name`
/// or `[].id` (root array). Within a segment a bare `key` is an object-key
/// step; trailing `[...]` groups are array steps (`[]` = any element,
/// `[N]` = the Nth element, 0-based). Keys containing literal `[`/`]` are
/// unaddressable — deliberately dumb matching, documented as such. Malformed
/// input → `Err` (startup fails fast; request time never sees it).
pub fn parse_json_path(s: &str) -> Result<JsonPath, String> {
    // RFC 9535 alias: `$.body[*].redact` ≡ `body[].redact` — the standard's
    // root marker and `*` wildcard are accepted. Filters (`[?(…)]`), unions,
    // recursive descent (`..`) and quoted-name selectors are outside this
    // subset and fail fast as syntax errors. Consequence of the `$` rule:
    // JSON keys starting with `$` are not addressable in shorthand form.
    let s = s
        .strip_prefix('$')
        .map(|r| r.strip_prefix('.').unwrap_or(r))
        .unwrap_or(s);
    let mut path = Vec::new();
    for part in s.split('.') {
        let (name, rest) = match part.find('[') {
            Some(p) => (&part[..p], &part[p..]),
            None => (part, ""),
        };
        if name.is_empty() && rest.is_empty() {
            return Err(format!("empty segment in JSON path {s:?}"));
        }
        if !name.is_empty() {
            path.push(JsonSeg::Key(name.to_string()));
        }
        let mut rest = rest;
        while let Some(sel) = rest.strip_prefix('[') {
            let Some(close) = sel.find(']') else {
                return Err(format!("unterminated array selector in JSON path {s:?}"));
            };
            let inner = &sel[..close];
            if inner.is_empty() || inner == "*" {
                path.push(JsonSeg::AnyIndex);
            } else if inner.chars().all(|c| c.is_ascii_digit()) {
                path.push(JsonSeg::Index(
                    inner
                        .parse::<usize>()
                        .map_err(|_| format!("array index too large in JSON path {s:?}"))?,
                ));
            } else {
                return Err(format!("invalid array selector [{inner}] in JSON path {s:?}"));
            }
            rest = &sel[close + 1..];
        }
        if !rest.is_empty() {
            return Err(format!("trailing junk after array selector in JSON path {s:?}"));
        }
    }
    Ok(path)
}

/// Parse every configured value (already comma-split by the shared flag-list
/// syntax). Errors surface at startup only (FR-011c forgivingness contract).
pub fn parse_json_paths(vals: &[String]) -> Result<Vec<JsonPath>, String> {
    vals.iter().map(|s| parse_json_path(s)).collect()
}

/// A walked path is a persistent `Rc` chain, child-first (the tail is the
/// deepest step, `parent` links toward the root). One O(1)-sized node per
/// container/key step with shared prefixes — deep nesting stays linear in
/// memory, which a per-frame `Vec` clone is not: it is quadratic, and the
/// 50 000-deep robustness test measured that at ~30 GB.
struct PathCell {
    parent: Option<Rc<PathCell>>,
    step: JsonStep,
}

/// Steps that appear on the *walked* side; patterns additionally carry
/// `AnyIndex` (see `JsonSeg`).
enum JsonStep {
    Key(String),
    Idx(usize),
}

/// Does the walked path (chain tail + depth) match `pat` exactly? The chain
/// is walked tail→root while the pattern (root-first) is consumed
/// back-to-front — allocation-free, O(pattern length).
fn chain_matches(tail: &Rc<PathCell>, depth: usize, pat: &[JsonSeg]) -> bool {
    if depth != pat.len() {
        return false;
    }
    let mut i = pat.len();
    let mut node = Some(tail);
    while let Some(c) = node {
        i -= 1;
        let ok = match &c.step {
            JsonStep::Key(k) => match &pat[i] {
                JsonSeg::Key(p) => k.eq_ignore_ascii_case(p),
                _ => false,
            },
            JsonStep::Idx(idx) => match &pat[i] {
                JsonSeg::Index(n) => idx == n,
                JsonSeg::AnyIndex => true,
                _ => false,
            },
        };
        if !ok {
            return false;
        }
        node = c.parent.as_ref();
    }
    i == 0
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
///
/// Same as [`mask_body_with`] with no JSON-path override (key-name masking).
pub fn mask_body(body: &[u8], ct: Option<&str>) -> Option<Vec<u8>> {
    mask_body_with(body, ct, &[])
}

/// [`mask_body`] with configured JSON paths (FR-011c): when any path is
/// configured, JSON masking switches from key-name matching to these paths
/// (exact match, case-insensitive keys, `[]`/`[N]` array selectors).
/// Non-JSON bodies and unmatched paths are silent no-ops — the body is
/// stored unchanged and nothing fails. Paths never affect form bodies.
pub fn mask_body_with(body: &[u8], ct: Option<&str>, pats: &[JsonPath]) -> Option<Vec<u8>> {
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
    let spans = json_value_spans_with(s.as_bytes(), pats);
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

#[derive(Clone)]
struct Frame {
    /// Object container (true) or array (false).
    object: bool,
    /// This container's value-sensitivity, inherited by its contents.
    ambient: bool,
    step: Step,
    /// Object only: sensitivity of the value being read.
    value_ctx: bool,
    /// FR-011c: persistent path chain addressing this container
    /// (`None` = root depth 0) plus its length.
    path: Option<Rc<PathCell>>,
    depth: usize,
    /// Object only: path chain of the value being read + its length.
    value_path: Option<Rc<PathCell>>,
    value_depth: usize,
    /// Array only: index of the element being read next.
    arr_idx: usize,
}

#[derive(Clone, Copy, PartialEq)]
enum Status {
    Completed,
    Pushed,
    Bailed,
}

struct Sc<'a> {
    b: &'a [u8],
    /// FR-011c: configured JSON-path patterns (empty = key-name masking).
    pats: &'a [JsonPath],
    i: usize,
    spans: Vec<(usize, usize)>,
    stack: Vec<Frame>,
    bailed: bool,
}

fn json_value_spans_with(b: &[u8], pats: &[JsonPath]) -> Vec<(usize, usize)> {
    let mut sc = Sc {
        b,
        pats,
        i: 0,
        spans: Vec::new(),
        stack: Vec::new(),
        bailed: false,
    };
    if read_value(&mut sc, false, None, 0) == Status::Bailed {
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
            // FR-011c: the value path is this container's path + the raw key
            // token. Key-name masking applies only when no path override is
            // configured — a jsonp config switches JSON masking to paths.
            let vp = Rc::new(PathCell {
                parent: sc.stack[last].path.clone(),
                step: JsonStep::Key(String::from_utf8_lossy(&sc.b[s..e]).into_owned()),
            });
            let vdepth = sc.stack[last].depth + 1;
            let key_match = sc.pats.is_empty() && is_sensitive_key_bytes(sc.b[s..e].as_ref());
            let path_match = sc.pats.iter().any(|p| chain_matches(&vp, vdepth, p));
            sc.stack[last].value_path = Some(vp);
            sc.stack[last].value_depth = vdepth;
            sc.stack[last].value_ctx =
                key_match || sc.stack[last].ambient || path_match;
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
            sc.skip_ws();
            // Empty array: close immediately (an unhandled `]` used to bail
            // the walk and silently skip every sibling after it).
            if !sc.stack[last].object && sc.peek() == Some(b']') {
                sc.i += 1;
                pop(sc);
                return;
            }
            let is_arr = !sc.stack[last].object;
            let (ctx, vp, vdepth) = if sc.stack[last].object {
                let vp = sc.stack[last].value_path.take();
                (sc.stack[last].value_ctx, vp, sc.stack[last].value_depth)
            } else {
                let vp = Rc::new(PathCell {
                    parent: sc.stack[last].path.clone(),
                    step: JsonStep::Idx(sc.stack[last].arr_idx),
                });
                let vdepth = sc.stack[last].depth + 1;
                let ctx = sc.stack[last].ambient
                    || sc.pats.iter().any(|p| chain_matches(&vp, vdepth, p));
                (ctx, Some(vp), vdepth)
            };
            match read_value(sc, ctx, vp, vdepth) {
                Status::Completed => {
                    sc.stack[last].step = Step::Punct;
                    if is_arr {
                        sc.stack[last].arr_idx += 1;
                    }
                }
                Status::Pushed => {
                    if is_arr {
                        sc.stack[last].arr_idx += 1;
                    }
                }
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
/// complete inline; containers push a frame. `value_path`/`value_depth` are
/// the FR-011c path addressing the value being read (the pushed container's
/// own path).
fn read_value(
    sc: &mut Sc,
    value_ctx: bool,
    value_path: Option<Rc<PathCell>>,
    value_depth: usize,
) -> Status {
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
                path: value_path,
                depth: value_depth,
                value_path: None,
                value_depth: 0,
                arr_idx: 0,
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
                path: value_path,
                depth: value_depth,
                value_path: None,
                value_depth: 0,
                arr_idx: 0,
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

    #[test]
    fn is_sensitive_header_in_replaces_defaults() {
        // FR-011a: an override list replaces the defaults wholesale — built-ins
        // no longer match, and the compiled-in fn is unchanged.
        let list = vec!["x-custom-secret".to_string()];
        assert!(is_sensitive_header_in("X-CUSTOM-SECRET", &list));
        assert!(is_sensitive_header_in("x-custom-secret", &list));
        assert!(!is_sensitive_header_in("authorization", &list));
        assert!(is_sensitive_header("AUTHORIZATION"));
        assert!(!is_sensitive_header("x-custom-secret"));
    }

    #[test]
    fn mask_headers_with_override() {
        let list = vec!["x-token".to_string()];
        let mut h = HeaderMap::new();
        h.append("Authorization", HeaderValue::from_static("Bearer rawsecret123"));
        h.append("X-Token", HeaderValue::from_static("verysecrettoken1"));
        let out = mask_headers_with(&h, &list);
        let get = |n: &str| {
            out.iter()
                .find(|(k, _)| k == n)
                .map(|(_, v)| v.clone())
        };
        assert_eq!(get("x-token").as_deref(), Some("ver***n1"));
        assert_eq!(get("authorization").as_deref(), Some("Bearer rawsecret123"));
    }

    #[test]
    fn empty_array_does_not_bail_the_walk() {
        // Regression: an empty array used to bail the walk and silently skip
        // every sibling after it (their sensitive keys were never masked).
        let out = mask_body(b"[[],{\"password\":\"hunter2\"}]", JSON_CT).unwrap();
        assert_eq!(out, br#"[[],{"password":"hu***2"}]"#.to_vec());
    }

    #[test]
    fn parse_json_path_ok() {
        use JsonSeg::*;
        assert_eq!(
            parse_json_path("body[].redact.this.key").unwrap(),
            vec![
                Key("body".into()),
                AnyIndex,
                Key("redact".into()),
                Key("this".into()),
                Key("key".into()),
            ]
        );
        assert_eq!(
            parse_json_path("items[0].name").unwrap(),
            vec![Key("items".into()), Index(0), Key("name".into())]
        );
        assert_eq!(parse_json_path("[].id").unwrap(), vec![AnyIndex, Key("id".into())]);
        assert_eq!(parse_json_path("a.b").unwrap(), vec![Key("a".into()), Key("b".into())]);
    }

    #[test]
    fn parse_json_path_rfc9535_alias() {
        use JsonSeg::*;
        // The RFC-canonical spelling parses identically to the shorthand.
        assert_eq!(
            parse_json_path("$.body[*].redact").unwrap(),
            parse_json_path("body[].redact").unwrap()
        );
        assert_eq!(parse_json_path("$.a").unwrap(), vec![Key("a".into())]);
        assert_eq!(parse_json_path("$[0].x").unwrap(), vec![Index(0), Key("x".into())]);
        // Outside the accepted subset → fail fast.
        assert!(parse_json_path("$.a[?(@.x==1)]").is_err());
        assert!(parse_json_path("$.a..b").is_err());
        assert!(parse_json_path("$['quoted']").is_err());
    }

    #[test]
    fn parse_json_path_errs() {
        // Malformed syntax is the only hard failure (startup-time).
        for bad in ["", "a..b", ".a", "a.", "a[0]b", "a[x]", "a[0"] {
            assert!(parse_json_path(bad).is_err(), "expected err: {bad:?}");
        }
    }

    #[test]
    fn mask_body_jsonp_paths() {
        // Canonical example shape: body[] → redact (and → another).
        let pats =
            parse_json_paths(&["body[].redact".to_string(), "body[].another".to_string()])
                .unwrap();
        let out = mask_body_with(
            br#"{"body":[{"redact":"sk-aaaaaaaaaaaaa"},{"another":"sk-bbbbbbbbbbbbb"},{"keep":"sk-ccccccccccccc"}]}"#,
            JSON_CT,
            &pats,
        )
        .unwrap();
        assert_eq!(
            String::from_utf8(out).unwrap(),
            r#"{"body":[{"redact":"sk-***aa"},{"another":"sk-***bb"},{"keep":"sk-ccccccccccccc"}]}"#
        );
    }

    #[test]
    fn mask_body_jsonp_index_and_container() {
        // `[N]` matches exactly that index.
        let pats = parse_json_paths(&["items[1]".to_string()]).unwrap();
        let out = mask_body_with(
            br#"{"items":["a123456789012","b123456789012"]}"#,
            JSON_CT,
            &pats,
        )
        .unwrap();
        assert_eq!(
            String::from_utf8(out).unwrap(),
            r#"{"items":["a123456789012","b12***12"]}"#
        );
        // A path landing on a container masks everything inside it.
        let pats = parse_json_paths(&["body[]".to_string()]).unwrap();
        let out = mask_body_with(
            br#"{"body":[{"any":"q12345678901"},{"deep":{"x":"r12345678901"}}]}"#,
            JSON_CT,
            &pats,
        )
        .unwrap();
        assert_eq!(
            String::from_utf8(out).unwrap(),
            r#"{"body":[{"any":"q12***01"},{"deep":{"x":"r12***01"}}]}"#
        );
    }

    #[test]
    fn mask_body_jsonp_replaces_key_matching_and_noops() {
        // With a path override configured, key-name masking is switched off:
        // only the configured paths mask.
        let pats = parse_json_paths(&["inner".to_string()]).unwrap();
        let out = mask_body_with(
            br#"{"password":"hunter2","inner":"z12345678901"}"#,
            JSON_CT,
            &pats,
        )
        .unwrap();
        assert_eq!(
            String::from_utf8(out).unwrap(),
            r#"{"password":"hunter2","inner":"z12***01"}"#
        );
        // Non-JSON body under jsonp-only config: silent no-op, never dies.
        assert_eq!(
            mask_body_with(b"plain text", JSON_CT, &parse_json_paths(&["body[]".to_string()]).unwrap()),
            None
        );
        // Unmatched path: silent no-op.
        assert_eq!(
            mask_body_with(
                br#"{"a":"v12345678901"}"#,
                JSON_CT,
                &parse_json_paths(&["nope[]".to_string()]).unwrap()
            ),
            None
        );
        // Form body under jsonp config: form masking unaffected.
        let out = mask_body_with(
            b"password=hunter2",
            Some("application/x-www-form-urlencoded"),
            &parse_json_paths(&["body[]".to_string()]).unwrap(),
        )
        .unwrap();
        assert_eq!(out, b"password=hu***2");
    }

    #[test]
    fn is_sensitive_key_in_replaces_defaults() {
        // FR-011b: an override list replaces the built-in query-param list
        // wholesale; the built-in fn (JSON keys + form path) is unchanged.
        let list = vec!["user_id".to_string()];
        assert!(is_sensitive_key_in("USER_ID", &list));
        assert!(is_sensitive_key_in("user_id", &list));
        assert!(!is_sensitive_key_in("api_key", &list));
        assert!(is_sensitive_key("API_KEY"));
    }

    #[test]
    fn mask_query_with_override() {
        let list = vec!["user_id".to_string()];
        // Override matches; built-ins no longer do in the override path.
        assert_eq!(
            mask_query_with("user_id=12345678&api_key=hunter2", &list),
            "user_id=12***8&api_key=hunter2"
        );
        // Same matching rule as the built-in: key matched after
        // percent-decoding, key token preserved byte-exact.
        assert_eq!(mask_query_with("user%5Fid=12345678", &list), "user%5Fid=12***8");
        // The built-in default still masks api_key.
        assert_eq!(
            mask_query_with("api_key=hunter2", &default_sensitive_query_parameters()),
            "api_key=hu***2"
        );
    }
}
