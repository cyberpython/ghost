//! `ghost` — a transparent HTTP capture proxy.
//!
//! Clients send plain HTTP/1.x to the proxy; the proxy remaps the request
//! target onto a fixed upstream base, relays the response unchanged, and
//! stores the whole exchange in SQLite. See the crate docs in [`proxy`].

pub mod api;
pub mod db;
pub mod proxy;
pub mod redact;
pub mod url_join;
