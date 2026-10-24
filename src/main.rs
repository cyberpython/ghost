//! `ghost` command-line entrypoint: parses config, opens the SQLite pool, and
//! runs the transparent capture proxy until Ctrl-C.

use clap::Parser;
use std::net::SocketAddr;

use ghost::{db, proxy, redact, url_join};

/// Return the next 00:00:01 local time after `now`.
///
/// DST-safe: in zones where the transition sits at midnight, local 00:00:01
/// may not exist (spring-forward) or may occur twice (fall-back). We pick
/// the first occurrence when ambiguous, and fall back to the UTC instant
/// when the local time doesn't exist at all.
fn next_prune_time(now: chrono::DateTime<chrono::Local>) -> chrono::DateTime<chrono::Local> {
    let naive = (now.date_naive() + chrono::Days::new(1)).and_hms_opt(0, 0, 1).unwrap();
    match naive.and_local_timezone(chrono::Local) {
        chrono::MappedLocalTime::Single(dt) => dt,
        chrono::MappedLocalTime::Ambiguous(dt, _) => dt,
        chrono::MappedLocalTime::None => naive
            .and_local_timezone(chrono::Utc)
            .unwrap()
            .with_timezone(&chrono::Local),
    }
}

#[derive(Parser)]
#[command(version, about, long_about)]
struct Args {
    /// Address the proxy listens on.
    #[arg(long, default_value = "127.0.0.1:8080")]
    listen: SocketAddr,

    /// Fixed upstream base: `http://host[:port][/prefix]`.
    #[arg(long)]
    upstream: String,

    /// SQLite database file (created if missing).
    #[arg(long, default_value = "captures.db")]
    db: String,

    /// Cap on body bytes retained for storage (relaying is always in full).
    #[arg(long, default_value_t = 100 * 1024 * 1024)]
    max_body_bytes: usize,

    /// Header names to mask before storage, replacing the built-in sensitive
    /// list (repeatable, comma-separated inside an occurrence).
    #[arg(long)]
    sensitive_headers: Vec<String>,

    /// Query-parameter names to mask before storage, replacing the built-in
    /// sensitive list for query parameters only — form fields and JSON body
    /// keys keep the built-in list (repeatable, comma-separated).
    #[arg(long)]
    sensitive_keys: Vec<String>,

    /// JSON paths (e.g. `body[].redact.this.key`) whose values are masked
    /// before storage — replaces key-name JSON masking when set. Non-JSON
    /// bodies and unmatched paths are silent no-ops (repeatable,
    /// comma-separated).
    #[arg(long)]
    sensitive_jsonp: Vec<String>,
    /// Prune captures older than this many days (0 = disabled).
    #[arg(long, value_parser = clap::value_parser!(u64).range(0..=36500))]
    retention_days: Option<u64>,

    /// Record only requests whose client-side path starts with one of these
    /// plain strings (repeatable; absent = record everything).
    #[arg(long, value_parser = non_empty_path)]
    include_path: Vec<String>,

    /// Never record requests whose client-side path starts with one of these
    /// plain strings, even when an include path matches (repeatable).
    #[arg(long, value_parser = non_empty_path)]
    exclude_path: Vec<String>,
}


/// Flag-list syntax shared by all sensitive-list overrides: occurrences
/// accumulate and may carry comma-separated values; empty entries are
/// dropped after trimming.
fn split_list(vals: &[String]) -> Vec<String> {
    vals.iter()
        .flat_map(|s| s.split(','))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}


/// FR-027 filter values must be non-empty: an empty string is a prefix of
/// every path, so an empty `--exclude-path` would silently disable all
/// capture, and an empty `--include-path` would be a no-op. Values are never
/// trimmed — matching is byte-faithful by design.
fn non_empty_path(s: &str) -> Result<String, String> {
    if s.is_empty() {
        Err("path filter must not be empty (an empty exclude would match every path)".to_string())
    } else {
        Ok(s.to_string())
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let args = Args::parse();
    let upstream = url_join::UpstreamBase::parse(&args.upstream)?;
    // Sensitive-list overrides replace the compiled-in defaults (never merge).
    let sensitive_headers = if args.sensitive_headers.is_empty() {
        redact::default_sensitive_headers()
    } else {
        split_list(&args.sensitive_headers)
    };
    let sensitive_query_parameters = if args.sensitive_keys.is_empty() {
        redact::default_sensitive_query_parameters()
    } else {
        split_list(&args.sensitive_keys)
    };
    // Malformed JSON-path syntax is the only hard failure: at startup, before
    // the listener binds (FR-011c forgivingness contract).
    let sensitive_json_paths = if args.sensitive_jsonp.is_empty() {
        Vec::new()
    } else {
        redact::parse_json_paths(&split_list(&args.sensitive_jsonp))
            .map_err(anyhow::Error::msg)?
    };
    let pool = db::init(&args.db).await?;
    let listener = tokio::net::TcpListener::bind(args.listen).await?;
    let state = proxy::State {
        db: pool.clone(),
        upstream,
        max_body_bytes: args.max_body_bytes,
        sensitive_headers,
        sensitive_query_parameters,
        sensitive_json_paths,
        include_paths: args.include_path,
        exclude_paths: args.exclude_path,
    };

    // Run initial prune if retention is configured.
    if let Some(days) = args.retention_days
        && days > 0
    {
        match db::prune(&pool, days).await {
            Ok(n) => tracing::info!(pruned = n, "initial retention prune done"),
            Err(err) => tracing::warn!(error = %err, "initial retention prune failed (proxy continues)"),
        }
    }

    // Spawn scheduled pruning task: runs daily at 00:00:01 local time.
    if let Some(days) = args.retention_days
        && days > 0
    {
        let pool_for_schedule = pool.clone();
        tokio::spawn(async move {
            loop {
                let now = chrono::Local::now();
                let next = next_prune_time(now);
                let delay = next - now;
                let delay = std::time::Duration::from_secs(
                    delay.num_seconds().max(0) as u64,
                );
                tokio::time::sleep(delay).await;

                match db::prune(&pool_for_schedule, days).await {
                    Ok(n) => tracing::info!(pruned = n, "scheduled retention prune done"),
                    Err(err) => tracing::warn!(error = %err, "scheduled retention prune failed (proxy continues)"),
                }
            }
        });
    }

    tracing::info!(
        listen = %args.listen,
        upstream = %args.upstream,
        db = %args.db,
        "ghost is listening"
    );

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("shutdown signal received");
        }
        res = proxy::serve(listener, state) => {
            res?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_path_filters_are_rejected() {
        assert!(Args::try_parse_from(["ghost", "--upstream", "http://u", "--include-path", ""]).is_err());
        assert!(Args::try_parse_from(["ghost", "--upstream", "http://u", "--exclude-path", ""]).is_err());
        assert!(Args::try_parse_from(["ghost", "--upstream", "http://u", "--include-path", "/a", "--exclude-path", "/b"]).is_ok());
    }
}
