use tuic_relay::{db, relay, routes};

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

    /// Path to SQLite database file.
    #[arg(long, env = "RELAY_DB_PATH", default_value = "./relay.db")]
    db_path: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("tuic_relay=info".parse()?))
        .init();

    let args = Args::parse();

    let conn = db::init(&args.db_path).await?;
    let state = relay::AppState::with_db(conn);
    let router = routes::build_router(state);

    info!(addr = %args.bind, db = %args.db_path, "relay server starting");

    let listener = tokio::net::TcpListener::bind(args.bind).await?;
    axum::serve(listener, router).await?;

    Ok(())
}
