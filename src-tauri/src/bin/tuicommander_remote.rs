#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let port: u16 = std::env::var("TUIC_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(9877);

    tuicommander_lib::run_headless(port).await
}
