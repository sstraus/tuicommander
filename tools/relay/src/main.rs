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

    /// Session idle timeout in seconds. Sessions with no activity are cleaned up.
    #[arg(long, env = "RELAY_SESSION_TIMEOUT_SECS", default_value = "3600")]
    session_timeout_secs: u64,

    /// Max token registrations per IP per hour (0 = unlimited).
    #[arg(long, env = "RELAY_RATE_LIMIT_PER_HOUR", default_value = "10")]
    rate_limit_per_hour: u32,

    /// Max concurrent sessions per token (0 = unlimited).
    #[arg(long, env = "RELAY_MAX_SESSIONS_PER_TOKEN", default_value = "5")]
    max_sessions_per_token: u32,
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
        rate_limits: dashmap::DashMap::new(),
        rate_limit_per_hour: args.rate_limit_per_hour,
        max_sessions_per_token: args.max_sessions_per_token,
    });
    // Spawn session reaper
    relay::spawn_session_reaper(
        state.clone(),
        std::time::Duration::from_secs(args.session_timeout_secs),
        std::time::Duration::from_secs(30),
    );

    let router = routes::build_router(state);

    info!(addr = %args.bind, db = %args.db_path, timeout_secs = args.session_timeout_secs, "relay server starting");

    let listener = tokio::net::TcpListener::bind(args.bind).await?;
    axum::serve(listener, router.into_make_service_with_connect_info::<std::net::SocketAddr>()).await?;

    Ok(())
}
