# ghost

**IMPORTANT NOTICE:** This program has been developed as an experiment in using
AI assisted development using AI agents and open-source LLMs (Qwen3.6-35B, 
Qwen3.8-27B). It comes with no guarantees regarding correctness or completeness
of functionality, security or suitability for any particular purpose.

A transparent HTTP capture reverse-proxy. Point any client at it (e.g.
`http://127.0.0.1:8080/endpoint/path`); the request is remapped onto a fixed
upstream (`http://infra/app/endpoint/path`), the response is relayed back,
and **every** exchange is stored in SQLite with full header records, SHA-256
hashes, and bodies (sensitive fields masked before storage).

HTTP/1.x plaintext only. `CONNECT` is rejected with `405 Method Not Allowed`.

Intended to be used with a traditional reverse proxy (e.g. Nginx, Caddy, 
Traefik etc.) in front of it for HTTPS support, security, load-balancing, 
caching etc.

The application consists of 3 parts:

- The `ghost` program, which is the actual proxy that captures and records 
  requests in an SQLite database.
- The `ghost-api` program, which is a web server exposing an HTTP API to access 
  the captured data stored in the SQLite database. It can also server a web UI.
- A React SPA web UI that utilizes the HTTP API mentioned above.

## Usage

```
ghost --listen 127.0.0.1:8080 --upstream http://127.0.0.1:9000/app --db captures.db
```

| Flag | Default | Meaning |
| --- | --- | --- |
| `--listen` | `127.0.0.1:8080` | Address the proxy accepts clients on. |
| `--upstream` | *(required)* | Fixed upstream base: `http://host[:port][/prefix]`. |
| `--db` | `captures.db` | SQLite file (created if missing). |
| `--max-body-bytes` | `104857600` (100 MiB) | Cap on body bytes retained for storage. |
| `--sensitive-headers <name>` | *(built-in list)* | Header names masked before storage, replacing the built-in sensitive list. Repeatable, and each occurrence may carry comma-separated names. |
| `--sensitive-keys <name>` | *(built-in list)* | Query-parameter names masked before storage, replacing the built-in sensitive list **for query parameters only**. Repeatable, comma-separated. Form-body and JSON-body key masking keep the built-in list and are not affected by this flag. |
| `--sensitive-jsonp <path>` | *(none)* | JSON paths whose values are masked before storage, e.g. `body[].redact.this.key` — `name[]` = any array element, `name[N]` = the Nth element (0-based). A deliberate subset of RFC 9535: the standard spelling (`$.body[*].redact`) is accepted as an alias; filters, unions, recursive descent and quoted names are unsupported and fail fast. When set, JSON masking switches from key-name matching to these paths. Non-JSON bodies and unmatched paths are silent no-ops; malformed syntax fails fast at startup. Repeatable, comma-separated. |
| `--retention-days` | *(unset)* | Prune captures older than this many days (`0` = disabled). Runs once at startup and daily at 00:00:01 local. |
| `--include-path` | *(unset)* | Record only requests whose client-side path starts with one of these plain strings (repeatable; unset = record everything). |
| `--exclude-path` | *(unset)* | Never record requests whose path starts with one of these plain strings; exclude wins over include (repeatable). |

Behavior:

- The client target `/endpoint/path?q` is resolved against the upstream
  prefix: `/app/endpoint/path?q`. Only the request target changes; method,
  version, and everything else pass through.
- A request body over `--max-body-bytes` is rejected with `413` and still
  recorded (with `error` set).
- Upstream connection/send failures become `502 Bad Gateway` to the client
  and are recorded with `error` set.
- Path filters (`--include-path`/`--exclude-path`) gate what is *recorded*,
  never what is relayed. Matching is a plain byte-faithful prefix match on
  the client-side path (no boundary enforcement, no percent-decoding, no
  case folding); exclude wins; CONNECT rows are exempt. Empty filter values
  are rejected at startup.
- Every completed exchange is one row in the `captures` table, inserted
  after the client has been fully served. A DB error is logged and never
  breaks proxying.
- With `--retention-days N` (N > 0), captures older than N days are deleted
  once at startup and then daily at 00:00:01 local time; a
  `wal_checkpoint(TRUNCATE)` reclaims disk space after a non-empty delete.
  Prune failures are logged as warnings and never break proxying.

## Transparency contract

What the proxy guarantees on the wire:

- **Header values** are forwarded byte-for-byte, in their original order,
  with duplicate header lines preserved.
- **Header names** are emitted in their canonical lowercase form, both on
  the wire and in the database. Field names are case-insensitive per RFC
  9110, and hyper's `HeaderName` is always the canonical lowercase form;
  the stored record therefore matches exactly what the upstream and the
  client receive.
- **Framing headers** (`Content-Length`, `Transfer-Encoding`) are normalised
  to a single plain `Content-Length` sized to the exact body bytes; this is
  the only header the proxy rewrites.
- The **request target** is remapped onto the upstream prefix (see above).
- `Host` is set **only when the client omitted it**, to the upstream
  authority (well-formedness).
- **No body bytes are ever altered on the wire** — no re-encoding, no
  compression, no chunking changes. What the client and the upstream
  exchange is byte-for-byte.

What the proxy does *not* guarantee about the stored copies:

- **Sensitive values are masked before storage.** Header values for
  sensitive names (`authorization`, `cookie`, `x-api-key`, ...), sensitive
  query parameters, and sensitive JSON/form body fields are replaced with a
  fixed mask (first 2–3 chars + `***` + last 1–2 chars; shorter values
  become `***`), so the SQLite file — and the read-only API + SPA that
  serve it — never holds a raw secret. Everything else is stored exactly as
  received.
- `*_body_sha256`, `*_body_len`, and `*_body_truncated` describe the
  **original on-wire** body, so they no longer verify the stored blob once
  masking or truncation changed it.
- **Reflected secrets are not scrubbed** (known limitation): a body that
  merely *echoes* a secret in a non-sensitive field is stored as-is —
  key-based masking can only see named fields.

## Storage schema

`captures` (one row per exchange):

| Column | Notes |
| --- | --- |
| `id` | PK, autoincrement. |
| `created_at` | UTC RFC 3339. |
| `total_ms` | Exchange duration in ms. |
| `client_addr` | Peer address of the client. |
| `request_version`, `request_method`, `request_target` | As received; sensitive query params masked. |
| `request_headers` | JSON array of `{"name","value"}` in original order, names lowercase; values masked for sensitive names. |
| `request_body` | Body bytes BLOB (truncated to the cap); sensitive fields masked. |
| `request_body_len`, `request_body_truncated` | Full length; 1 if the stored copy is truncated. |
| `request_body_sha256` | SHA-256 hex of the full in-memory (original on-wire) body (NULL if none); does not verify the stored blob once masked. |
| `upstream_scheme`, `upstream_authority`, `upstream_path`, `upstream_query` | Resolved upstream URL parts; sensitive query params masked. |
| `response_version`, `status_code` | NULL when no response arrived (CONNECT, 502). |
| `response_headers` | JSON array, same format as `request_headers` (values masked for sensitive names). |
| `response_body` | Body bytes BLOB (truncated to the cap); sensitive fields masked. |
| `response_body_len`, `response_body_truncated` | As for the request. |
| `response_body_sha256` | SHA-256 hex of the full in-memory (original on-wire) body (NULL if none); does not verify the stored blob once masked. |
| `error` | NULL on success; reason for a failed exchange otherwise. |

Indexes on `created_at`, `request_method`, `status_code`, `upstream_path` —
the common filter axes for the viewer below.

The database runs in **WAL mode** with `busy_timeout`; the proxy is the sole
writer, so a separate read-only process can open the same file concurrently
without blocking the proxy.

## Viewer (SPA + API)

A read-only API binary and a dark-mode React/TypeScript SPA browse captures.

```
ghost-api --db captures.db --listen 127.0.0.1:8090 --static-dir web/dist
```

| Flag | Default | Meaning |
| --- | --- | --- |
| `--db` | *(required)* | Path to the captures SQLite file (opened read-only). |
| `--listen` | `127.0.0.1:8090` | Address the API serves on. |
| `--static-dir` | `web/dist` | Directory containing the built SPA. |

### API endpoints

- `GET /api/captures?method=&status=&path=<sub>&client_addr=&since=&until=&error=&limit=&offset=`
  — list. `method`, `path`, `client_addr`, and `error` are case-insensitive
  substring matches against their columns; `status` takes a status-code prefix
  (e.g. `status=4` → all 4xx) or `err`/`error` → rows with no HTTP status
  (failed exchanges); `since`/`until` compare the `created_at` strings;
  `limit` defaults to 50 (max 500), `offset` paginates. Returns
  `{count, captures:[...]}` with `count` = total matches.
- `GET /api/captures/{id}` — full record: all fields, headers as `[{name,value}]` arrays,
  body views (`text`/`binary`/`hex_preview` with SHA-256).
- `GET /api/captures/{id}/body?part=request|response` — raw stored body bytes as an
  `application/octet-stream` download (`Content-Disposition: attachment`, plus
  `x-body-truncated` / `x-body-original-length` headers). No such body → `404`.
  The raw endpoint always serves the **stored (compressed) bytes** — decoding
  never applies here, so downloads and byte-faithful replays stay exact.
- **Decode-on-read (FR-019):** in the capture detail, a body with a single
  `Content-Encoding: gzip`/`deflate` coding is transparently inflated for the
  *view only* — stored bytes stay compressed, `sha256` still hashes the on-wire
  body, and the UI shows a `decoded from gzip/deflate` badge. Decoding is
  bounded (8 MiB); corrupt input, multi-coding lists (`gzip, br`) and anything
  else fall back to the raw view silently. Copy cURL / Copy Hurl always use the
  raw wire bytes (decoded bodies take the binary paths), so replays stay
  byte-faithful.
  **Security note:** masking is applied at write time and cannot see inside
  compressed bodies — fields that would be masked in an uncompressed body are
  visible in the decoded view. The raw bytes were always retrievable via the
  download endpoint; decoding makes them readable. Treat compressed bodies as
  unmasked content.
- `GET /api/facets` — `{methods, statuses, errors}` for filter dropdowns; `errors`
  is the count of rows with an error set, not a list of kinds.
- `GET /api/export/captures` — the current filter set as a **HAR 1.2** log
  (same filters as the list endpoint, `limit` defaults to 50 / max 500),
  returned as an `application/json` attachment (`captures.har`).
- `GET /api/export/captures/{id}` — a single capture as a HAR 1.2 log
  (`capture-{id}.har` attachment). Unknown id → `404`.

  HAR is the lingua franca of HTTP tooling: the `.har` file opens directly in
  Chrome DevTools (Network → Import HAR) and Charles. Text bodies are inlined
  as UTF-8 `content.text`; binary bodies are base64-encoded with
  `encoding: "base64"` so they survive the JSON round-trip byte-for-byte.
  Failed exchanges (no HTTP status) are emitted with `status: 0` and the
  error as `statusText`. Note that bodies are exported exactly as stored —
  wire-faithful — so gzip-encoded responses are base64 of the compressed
  bytes (the viewer's decode-on-read is a separate view).

### Dev flow

```
# Vite dev server (proxies /api → 8090) and API on 8090:
cd web && npm run dev
cd ..
cargo run --bin ghost-api -- --db captures.db --listen 127.0.0.1:8090

# Or build the ui and serve both the API and web UI:
cd web && npm run build
cd ..
cargo run --bin ghost-api -- --db captures.db --listen 127.0.0.1:8090 --static-dir web/dist
```

### Tests

- `cargo test` — Rust unit + e2e + API integration tests.
- `cd web && npm test` — vitest + jsdom + @testing-library/react (28 tests).

## Development

- `cargo test` — unit tests (`url_join`) plus an end-to-end test that spawns a
  real origin and the proxy on ephemeral ports with a temp DB and asserts
  byte-exact header order, body round-trip, hashes, the `405` on CONNECT, and
  the `502` on upstream failure.
- `cargo run --example origin` — a tiny upstream that echoes what it received
  (method, target, headers in order, raw body) as JSON, so transparency can be
  eyeballed: run it, point `ghost --upstream http://127.0.0.1:9000/app` at it,
  then `curl -v -H 'X-Test: abc' -d hello-body http://127.0.0.1:8080/endpoint/path`.

## License

Released under the MIT License - see the LICENSE in the root of this repository.
