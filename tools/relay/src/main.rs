use tuic_relay::{db, push, relay, routes};

use std::net::SocketAddr;
use std::sync::Arc;

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

    /// Base64url-encoded ES256 private key for VAPID Web Push.
    /// Generate with: openssl ecparam -name prime256v1 -genkey -noout | openssl ec -outform DER | base64 -w0
    #[arg(long, env = "RELAY_VAPID_PRIVATE_KEY")]
    vapid_private_key: Option<String>,

    /// Contact URI for VAPID subject claim (mailto: or https:).
    #[arg(long, env = "RELAY_VAPID_SUBJECT", default_value = "mailto:admin@tuicommander.com")]
    vapid_subject: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("tuic_relay=info".parse()?))
        .init();

    let args = Args::parse();

    let vapid = match &args.vapid_private_key {
        Some(key) => {
            let config = push::VapidConfig::new(key, &args.vapid_subject)?;
            info!(subject = %args.vapid_subject, "Web Push (VAPID) enabled");
            Some(config)
        }
        None => {
            info!("Web Push disabled (no VAPID key configured)");
            None
        }
    };

    let conn = db::init(&args.db_path).await?;
    let state = Arc::new(relay::AppState {
        sessions: dashmap::DashMap::new(),
        db: Some(conn),
        token_cache: dashmap::DashMap::new(),
        vapid,
        http_client: reqwest::Client::new(),
    });
    let router = routes::build_router(state);

    info!(addr = %args.bind, db = %args.db_path, "relay server starting");

    let listener = tokio::net::TcpListener::bind(args.bind).await?;
    axum::serve(listener, router).await?;

    Ok(())
}
