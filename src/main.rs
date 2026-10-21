//! `ghost` command-line entrypoint: parses config, opens the SQLite pool, and
//! runs the transparent capture proxy until Ctrl-C.

use clap::Parser;
use std::net::SocketAddr;

use ghost::{db, proxy, redact, url_join};

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
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let args = Args::parse();
    let upstream = url_join::UpstreamBase::parse(&args.upstream)?;
    // Sensitive-list overrides replace the compiled-in defaults (never merge);
    // occurrences accumulate and may carry comma-separated values.
    let sensitive_headers = if args.sensitive_headers.is_empty() {
        redact::default_sensitive_headers()
    } else {
        args.sensitive_headers
            .iter()
            .flat_map(|s| s.split(','))
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    };
    let pool = db::init(&args.db).await?;
    let listener = tokio::net::TcpListener::bind(args.listen).await?;
    let state = proxy::State {
        db: pool,
        upstream,
        max_body_bytes: args.max_body_bytes,
        sensitive_headers,
    };

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
