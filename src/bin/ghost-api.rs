//! Read-only API + static SPA server.
use ghost::api;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let args = match <ghost::api::Args as clap::Parser>::try_parse() {
        Ok(args) => args,
        // Handled clap's own exit behavior for --help/--version/usage errors
        // (proper exit codes: 0 for help, 2 for errors) instead of turning
        // them into an anyhow error that would always exit 1.
        Err(e) => e.exit(),
    };
    api::run(args).await
}
