use tuic_relay::relay;
use tuic_relay::routes;

use std::net::SocketAddr;

use clap::Parser;
use tracing::info;
use tracing_subscriber::EnvFilter;

/// Blind E2E-encrypted WebSocket relay for TUICommander mobile access.
#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    /// Address to bind the relay server.
    #[arg(long, env = "RELAY_BIND", default_value = "0.0.0.0:8080")]
    bind: SocketAddr,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("tuic_relay=info".parse()?))
        .init();

    let args = Args::parse();
    let state = relay::AppState::new();
    let router = routes::build_router(state);

    info!(addr = %args.bind, "relay server starting");

    let listener = tokio::net::TcpListener::bind(args.bind).await?;
    axum::serve(listener, router).await?;

    Ok(())
}
