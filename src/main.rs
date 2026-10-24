//! `ghost` command-line entrypoint: parses config, opens the SQLite pool, and
//! runs the transparent capture proxy until Ctrl-C.

use clap::Parser;
use std::net::SocketAddr;

use ghost::{db, proxy, url_join};

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

    /// Prune captures older than this many days (0 = disabled).
    #[arg(long, value_parser = clap::value_parser!(u64).range(0..=36500))]
    retention_days: Option<u64>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let args = Args::parse();
    let upstream = url_join::UpstreamBase::parse(&args.upstream)?;
    let pool = db::init(&args.db).await?;
    let listener = tokio::net::TcpListener::bind(args.listen).await?;
    let state = proxy::State {
        db: pool.clone(),
        upstream,
        max_body_bytes: args.max_body_bytes,
    };

    // Run initial prune if retention is configured.
    if let Some(days) = args.retention_days {
        if days > 0 {
            match db::prune(&pool, days).await {
                Ok(n) => tracing::info!(pruned = n, "initial retention prune done"),
                Err(err) => tracing::warn!(error = %err, "initial retention prune failed (proxy continues)"),
            }
        }
    }

    // Spawn scheduled pruning task: runs daily at 00:00:01 local time.
    if let Some(days) = args.retention_days {
        if days > 0 {
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
