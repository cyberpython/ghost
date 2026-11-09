#!/usr/bin/env python3
"""Build a sample ghost captures DB for manual testing.

Produces ``sample.db`` (path overridable via the first CLI arg) populated with
realistic capture rows spread over the last ~48 hours so the requests/hour
graph (FR-018) and the method x status-class KPI panel (FR-016) have data to
show in the default 24 h window.

The schema mirrors ``src/db.rs`` exactly. ``created_at`` is RFC3339 with
seconds precision (``2026-10-25T21:00:00Z``), matching the proxy writer.
Header columns are JSON arrays of ``{"name","value"}`` objects, matching
``proxy::headers_to_json``.
"""

import hashlib
import json
import os
import random
import sqlite3
import sys
from datetime import datetime, timedelta, timezone

SCHEMA = """
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
"""

# (method, path, query, weight) — weighted so GET dominates like real traffic.
ROUTES = [
    ("GET",    "/api/v1/users",        None, 5),
    ("GET",    "/api/v1/users/42",    None, 3),
    ("GET",    "/api/v1/orders",      "page=1", 3),
    ("GET",    "/api/v1/orders/1001", None, 2),
    ("GET",    "/api/v1/products",    "category=books", 2),
    ("GET",    "/static/app.js",      None, 4),
    ("GET",    "/static/logo.png",    None, 2),
    ("POST",   "/api/v1/orders",      None, 3),
    ("POST",   "/api/v1/login",       None, 2),
    ("POST",   "/api/v1/search",      "q=laptop", 2),
    ("PUT",    "/api/v1/users/42",    None, 2),
    ("PATCH",  "/api/v1/orders/1001", None, 1),
    ("DELETE", "/api/v1/orders/1001", None, 1),
    ("DELETE", "/api/v1/sessions/9",  None, 1),
    ("HEAD",   "/api/v1/health",      None, 1),
    ("OPTIONS","/api/v1/orders",      None, 1),
]

# Status outcomes weighted by (status, weight). NULL status => error row.
OUTCOMES = [
    (200, 6), (201, 2), (204, 1),
    (301, 1), (304, 1),
    (400, 2), (401, 1), (403, 1), (404, 3),
    (500, 2), (502, 1), (503, 1),
    (None, 1),  # error row: no status, error set
]

CLIENTS = ["10.0.0.1", "10.0.0.2", "192.168.1.10", "172.16.5.20", "203.0.113.7"]
AUTHORITIES = ["upstream-a:9000", "upstream-b:9000", "api.internal:8443"]


def headers_json(pairs):
    return json.dumps([{"name": n, "value": v} for n, v in pairs])


def sha(b):
    return hashlib.sha256(b).hexdigest() if b is not None else None


def make_body(method):
    """Return (blob, stored_len, truncated) for a request body."""
    if method in ("GET", "HEAD", "OPTIONS"):
        return None, 0, 0
    if method in ("POST", "PUT", "PATCH"):
        if random.random() < 0.25:
            # Binary body (e.g. an uploaded file).
            blob = bytes(random.getrandbits(8) for _ in range(random.randint(200, 4096)))
            return blob, len(blob), 0
        payload = json.dumps({
            "id": random.randint(1, 999),
            "name": f"item-{random.randint(1, 99)}",
            "qty": random.randint(1, 9),
            "tags": ["a", "b", "c"][:random.randint(1, 3)],
        }).encode()
        return payload, len(payload), 0
    return None, 0, 0


def make_response(status, method):
    """Return (blob, stored_len, truncated) for a response body."""
    if status is None or status in (204, 304) or method == "HEAD":
        return None, 0, 0
    if status >= 500:
        blob = json.dumps({"error": "internal", "detail": "upstream failure"}).encode()
        return blob, len(blob), 0
    if status == 404:
        blob = json.dumps({"error": "not_found"}).encode()
        return blob, len(blob), 0
    # 2xx/3xx/4xx: a JSON document.
    doc = {
        "id": random.randint(1, 9999),
        "ok": status < 400,
        "items": [
            {"sku": f"SKU-{random.randint(100, 999)}", "price": round(random.uniform(1, 500), 2)}
            for _ in range(random.randint(1, 5))
        ],
    }
    blob = json.dumps(doc).encode()
    # Occasionally simulate a truncated (large) body.
    truncated = 0
    if random.random() < 0.1:
        truncated = 1
    return blob, len(blob), truncated


def pick(pairs):
    """Weighted pick: each entry is a tuple whose last element is its weight."""
    total = sum(entry[-1] for entry in pairs)
    r = random.uniform(0, total)
    acc = 0
    for entry in pairs:
        acc += entry[-1]
        if r <= acc:
            return entry
    return pairs[-1]


def build(now, count=320, seed=20261025):
    rng = random.Random(seed)
    rows = []
    for _ in range(count):
        method, path, query, _ = pick(ROUTES)
        status, _ = pick(OUTCOMES)
        is_error = status is None

        # Spread over the last ~48 h, biased toward recent hours.
        hours_ago = int(rng.expovariate(1 / 18))  # mean ~18 h ago
        hours_ago = min(hours_ago, 47)
        ts = now - timedelta(hours=hours_ago, seconds=rng.randint(0, 3599))
        created = ts.strftime("%Y-%m-%dT%H:%M:%SZ")

        client = rng.choice(CLIENTS)
        authority = rng.choice(AUTHORITIES)
        total_ms = rng.randint(2, 2500)

        req_headers = headers_json([
            ("Host", authority.split(":")[0]),
            ("User-Agent", "ghost-sample/1.0"),
            ("Accept", "application/json"),
        ])
        if method in ("POST", "PUT", "PATCH"):
            req_headers = headers_json([
                ("Host", authority.split(":")[0]),
                ("Content-Type", "application/json"),
                ("User-Agent", "ghost-sample/1.0"),
            ])

        req_body, req_len, req_trunc = make_body(method)

        if is_error:
            resp_headers = "[]"
            resp_body, resp_len, resp_trunc = None, 0, 0
            resp_version = None
            err = rng.choice([
                "connection reset by peer",
                "upstream timeout after 30s",
                "TLS handshake failed",
                "no route to host",
            ])
        else:
            resp_headers = headers_json([
                ("Server", "ghost-upstream"),
                ("Content-Type", "application/json"),
                ("Date", created),
            ])
            resp_body, resp_len, resp_trunc = make_response(status, method)
            resp_version = "HTTP/1.1"
            err = None

        target = path if query is None else f"{path}?{query}"
        rows.append((
            created, total_ms, client,
            "HTTP/1.1", method, target, req_headers,
            req_body, req_len, req_trunc, sha(req_body),
            "http", authority, path, query,
            resp_version, status, resp_headers,
            resp_body, resp_len, resp_trunc, sha(resp_body),
            err,
        ))
    rows.sort(key=lambda r: r[0])
    return rows


def main():
    out = sys.argv[1] if len(sys.argv) > 1 else "sample.db"
    if os.path.exists(out):
        os.remove(out)
    now = datetime.now(timezone.utc).replace(microsecond=0)
    rows = build(now)
    con = sqlite3.connect(out)
    con.executescript(SCHEMA)
    con.executemany(
        """INSERT INTO captures (
            created_at, total_ms, client_addr,
            request_version, request_method, request_target, request_headers,
            request_body, request_body_len, request_body_truncated, request_body_sha256,
            upstream_scheme, upstream_authority, upstream_path, upstream_query,
            response_version, status_code, response_headers,
            response_body, response_body_len, response_body_truncated, response_body_sha256,
            error
        ) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)""",
        rows,
    )
    con.commit()
    con.close()
    print(f"Wrote {len(rows)} rows to {out}")


if __name__ == "__main__":
    main()
