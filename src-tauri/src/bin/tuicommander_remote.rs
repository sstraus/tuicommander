#[cfg(not(feature = "desktop"))]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let port: u16 = match std::env::var("TUIC_PORT") {
        Ok(val) => val.parse().unwrap_or_else(|e| {
            eprintln!("warning: TUIC_PORT={val:?} is not a valid port ({e}), using default 9877");
            9877
        }),
        Err(_) => 9877,
    };

    tuicommander_lib::run_headless(port).await
}

#[cfg(feature = "desktop")]
fn main() {
    eprintln!("tuicommander-remote requires --no-default-features (desktop feature must be disabled)");
    std::process::exit(1);
}
