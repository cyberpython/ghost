//! Read-only API + static SPA server.
use ghost::api;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let args = <ghost::api::Args as clap::Parser>::try_parse()
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    api::run(args).await
}
