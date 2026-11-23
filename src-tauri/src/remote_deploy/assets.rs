use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tuic_gateway::model::Provider;

use crate::github::gateway::Origin;
use crate::provider_gateway::direct::{self, Direct};

const TARGETS: &[(&str, &str)] = &[
    ("Linux x86_64", "x86_64-unknown-linux-gnu"),
    ("Linux aarch64", "aarch64-unknown-linux-gnu"),
    ("Darwin arm64", "aarch64-apple-darwin"),
];
/// Builds the sibling tuic-remote the update fallback serves. Run from the
/// repository root; `make dev` runs the same command so the sibling lands in
/// the target directory of the desktop dev binary.
const BUILD_SIBLING_COMMAND: &str =
    "cd src-tauri && cargo build --bin tuic-remote --no-default-features";
const MAX_ASSET_BYTES: u64 = 512 * 1024 * 1024;
const DOWNLOAD_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);
/// The gateway send's own ceiling; `DOWNLOAD_TIMEOUT` (or the caller's) bounds
/// the transfer from outside, so this only has to exceed it.
const TRANSFER_CEILING: std::time::Duration = std::time::Duration::from_secs(3600);
/// The budget a release download spends: it belongs to no account.
const RELEASE_ACCOUNT: &str = "release";

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct BuildIdentity {
    pub(crate) version: String,
    pub(crate) target: String,
    pub(crate) sha256: String,
}

/// Keep the digest of the executable that started this process. Reading the
/// path again after an update would describe the replacement, not this daemon.
pub(crate) fn running_build_identity() -> Result<&'static BuildIdentity, String> {
    static RUNNING: OnceLock<Result<BuildIdentity, String>> = OnceLock::new();
    RUNNING
        .get_or_init(|| {
            let path = std::env::current_exe()
                .map_err(|error| format!("could not locate running executable: {error}"))?;
            let mut file = std::fs::File::open(&path)
                .map_err(|error| format!("could not open running executable: {error}"))?;
            let mut hasher = Sha256::new();
            let mut buffer = [0u8; 64 * 1024];
            loop {
                let read = file
                    .read(&mut buffer)
                    .map_err(|error| format!("could not hash running executable: {error}"))?;
                if read == 0 {
                    break;
                }
                hasher.update(&buffer[..read]);
            }
            Ok(BuildIdentity {
                version: env!("CARGO_PKG_VERSION").to_string(),
                target: env!("TUIC_TARGET_TRIPLE").to_string(),
                sha256: hex_digest(&hasher.finalize()),
            })
        })
        .as_ref()
        .map_err(Clone::clone)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LocalAsset {
    pub(crate) path: PathBuf,
    pub(crate) sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UpdateAsset {
    pub(crate) binary: LocalAsset,
    pub(crate) source: &'static str,
    pub(crate) target: String,
    pub(crate) version: String,
}

/// Prefer the desktop release asset. Development builds can use a sibling
/// tuic-remote binary, but only when the daemon target equals this build target.
pub(crate) async fn resolve_update_asset(target: &str) -> Result<UpdateAsset, String> {
    let executable =
        std::env::current_exe().map_err(|e| format!("could not locate desktop binary: {e}"))?;
    let sibling = executable
        .parent()
        .ok_or_else(|| "desktop executable has no directory".to_string())?
        .join(if cfg!(windows) {
            "tuic-remote.exe"
        } else {
            "tuic-remote"
        });
    resolve_update_asset_from_url(
        target,
        &asset_url(env!("CARGO_PKG_VERSION"), target),
        &sibling,
        None,
    )
    .await
}

async fn resolve_update_asset_from_url(
    target: &str,
    url: &str,
    local_path: &Path,
    known_local_target: Option<&str>,
) -> Result<UpdateAsset, String> {
    match ensure_local_from_url(env!("CARGO_PKG_VERSION"), target, url, DOWNLOAD_TIMEOUT).await {
        Ok(binary) => Ok(UpdateAsset {
            binary,
            source: "release",
            target: target.to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }),
        Err(error) if error.starts_with("no tuic-remote release asset") => {
            // Target first: a remote of another target has no sibling fallback
            // whatever the sibling's state, so that is the cause to report.
            let local_target = known_local_target.unwrap_or(env!("TUIC_TARGET_TRIPLE"));
            if target != local_target {
                return Err(format!(
                    "release asset unavailable for remote target {target}; local tuic-remote target is {local_target}"
                ));
            }
            if !local_path.is_file() {
                return Err(format!(
                    "release asset unavailable; locally built tuic-remote for {local_target} not found at {}; build it with: {BUILD_SIBLING_COMMAND}",
                    local_path.display()
                ));
            }
            let local_identity = if known_local_target.is_none() {
                Some(probe_local_binary(local_path).await?)
            } else {
                None
            };
            let binary = local_asset(local_path.to_path_buf()).await?;
            if local_identity
                .as_ref()
                .is_some_and(|identity| identity.sha256 != binary.sha256)
            {
                return Err("local daemon binary changed after build identity probe".to_string());
            }
            Ok(UpdateAsset {
                binary,
                source: "local",
                target: target.to_string(),
                version: local_identity.map_or_else(
                    || env!("CARGO_PKG_VERSION").to_string(),
                    |identity| identity.version,
                ),
            })
        }
        Err(error) => Err(error),
    }
}

async fn probe_local_binary(path: &Path) -> Result<BuildIdentity, String> {
    let mut command = tokio::process::Command::new(path);
    command.arg("--build-info").kill_on_drop(true);
    let output = tokio::time::timeout(std::time::Duration::from_secs(300), command.output())
        .await
        .map_err(|_| {
            format!(
                "local tuic-remote build probe timed out at {}",
                path.display()
            )
        })?
        .map_err(|error| {
            format!(
                "could not run local tuic-remote at {}: {error}",
                path.display()
            )
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("requires --no-default-features") {
            return Err(format!(
                "local tuic-remote at {} requires --no-default-features; build it with: {BUILD_SIBLING_COMMAND}",
                path.display()
            ));
        }
        return Err(format!(
            "local tuic-remote did not report build identity at {}",
            path.display()
        ));
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("invalid local tuic-remote build identity: {error}"))
}

pub(crate) fn target_for(uname: &str) -> Option<&'static str> {
    TARGETS
        .iter()
        .find_map(|(candidate, target)| (*candidate == uname.trim()).then_some(*target))
}

pub(crate) fn require_target(uname: &str) -> Result<&'static str, String> {
    target_for(uname).ok_or_else(|| format!("unsupported remote host: {uname}"))
}

/// A nightly desktop (`1.8.0-nightly.<date>.t<time>`) has no release of its own:
/// nightly.yml publishes its daemons to the rolling `nightly` release.
pub(crate) fn asset_url(version: &str, target: &str) -> String {
    let tag = if version.contains("-nightly") {
        "nightly".to_string()
    } else {
        format!("v{version}")
    };
    format!("https://github.com/sstraus/tuicommander/releases/download/{tag}/tuic-remote-{target}")
}

pub(super) async fn ensure_local_from_url(
    version: &str,
    target: &str,
    url: &str,
    timeout: std::time::Duration,
) -> Result<LocalAsset, String> {
    validate_component("version", version)?;
    validate_component("target", target)?;

    let root = crate::config::config_dir().join("remote-bin");
    super::asset_cache::prune_other_versions(&root, version).await;
    let directory = root.join(version);
    let destination = directory.join(format!("tuic-remote-{target}"));
    if destination.is_file() {
        return local_asset(destination).await;
    }

    tokio::fs::create_dir_all(&directory)
        .await
        .map_err(|error| format!("could not create {}: {error}", directory.display()))?;
    let staging = directory.join(format!(
        ".tuic-remote-{target}.tmp-{}",
        uuid::Uuid::new_v4()
    ));

    let result = tokio::time::timeout(
        timeout,
        download_and_promote(version, target, url, &staging, &destination),
    )
    .await
    .unwrap_or_else(|_| {
        Err(format!(
            "tuic-remote release download timed out for {target}"
        ))
    });
    if result.is_err() {
        let _ = tokio::fs::remove_file(&staging).await;
    }
    result
}

async fn download_and_promote(
    version: &str,
    target: &str,
    url: &str,
    staging: &Path,
    destination: &Path,
) -> Result<LocalAsset, String> {
    let response = Direct::new(
        Origin::interactive("remote_deploy.download_and_promote"),
        Provider::Generic,
        RELEASE_ACCOUNT,
    )
    .timeout(TRANSFER_CEILING)
    .stream(direct::builder(reqwest::Method::GET, url))
    .await
    .map_err(|error| format!("could not download {url}: {error}"))?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(format!(
            "no tuic-remote release asset for {target} at v{version}"
        ));
    }
    if !response.status().is_success() {
        return Err(format!(
            "could not download {url}: HTTP {}",
            response.status()
        ));
    }
    if direct::declared_length(&response).is_some_and(|length| length > MAX_ASSET_BYTES) {
        return Err(format!("tuic-remote asset exceeds {MAX_ASSET_BYTES} bytes"));
    }

    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(staging)
        .await
        .map_err(|error| format!("could not create {}: {error}", staging.display()))?;
    let mut hasher = Sha256::new();
    let mut written = 0u64;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| format!("could not download {url}: {error}"))?;
        written = written
            .checked_add(chunk.len() as u64)
            .ok_or_else(|| "tuic-remote asset size overflow".to_string())?;
        if written > MAX_ASSET_BYTES {
            return Err(format!("tuic-remote asset exceeds {MAX_ASSET_BYTES} bytes"));
        }
        hasher.update(&chunk);
        file.write_all(&chunk)
            .await
            .map_err(|error| format!("could not write {}: {error}", staging.display()))?;
    }
    file.flush()
        .await
        .map_err(|error| format!("could not flush {}: {error}", staging.display()))?;
    file.sync_all()
        .await
        .map_err(|error| format!("could not sync {}: {error}", staging.display()))?;
    drop(file);

    let sha256 = hex_digest(&hasher.finalize());
    match tokio::fs::rename(staging, destination).await {
        Ok(()) => Ok(LocalAsset {
            path: destination.to_path_buf(),
            sha256,
        }),
        Err(_) if destination.is_file() => {
            let _ = tokio::fs::remove_file(staging).await;
            local_asset(destination.to_path_buf()).await
        }
        Err(error) => Err(format!(
            "could not promote {} to {}: {error}",
            staging.display(),
            destination.display()
        )),
    }
}

async fn local_asset(path: PathBuf) -> Result<LocalAsset, String> {
    let mut file = tokio::fs::File::open(&path)
        .await
        .map_err(|error| format!("could not open cached {}: {error}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .await
            .map_err(|error| format!("could not read cached {}: {error}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(LocalAsset {
        path,
        sha256: hex_digest(&hasher.finalize()),
    })
}

fn validate_component(label: &str, value: &str) -> Result<(), String> {
    if value.is_empty()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
    {
        return Err(format!("invalid release {label}: {value}"));
    }
    Ok(())
}

fn hex_digest(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::new(), |mut output, byte| {
        let _ = write!(output, "{byte:02x}");
        output
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_supported_uname_values_to_release_targets() {
        assert_eq!(target_for("Linux x86_64"), Some("x86_64-unknown-linux-gnu"));
        assert_eq!(
            target_for("Linux aarch64"),
            Some("aarch64-unknown-linux-gnu")
        );
        assert_eq!(target_for("Darwin arm64"), Some("aarch64-apple-darwin"));
    }

    #[test]
    fn unknown_uname_is_rejected_by_name() {
        assert_eq!(target_for("Plan 9 mips"), None);
        let error = require_target("Plan 9 mips").expect_err("unsupported host");
        assert!(error.contains("Plan 9 mips"));
    }

    #[test]
    fn release_asset_url_uses_the_package_version() {
        assert_eq!(
            asset_url(env!("CARGO_PKG_VERSION"), "aarch64-apple-darwin"),
            format!(
                "https://github.com/sstraus/tuicommander/releases/download/v{}/tuic-remote-aarch64-apple-darwin",
                env!("CARGO_PKG_VERSION")
            )
        );
    }

    #[test]
    fn nightly_version_resolves_to_the_rolling_nightly_release() {
        assert_eq!(
            asset_url("1.8.0-nightly.20261008.t1200", "x86_64-unknown-linux-gnu"),
            "https://github.com/sstraus/tuicommander/releases/download/nightly/tuic-remote-x86_64-unknown-linux-gnu"
        );
    }

    #[tokio::test]
    async fn download_is_hashed_promoted_and_then_served_from_cache() {
        let config = tempfile::tempdir().expect("temporary config directory");
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let mut server = mockito::Server::new_async().await;
        let route = server
            .mock("GET", "/tuic-remote-test-target")
            .with_status(200)
            .with_body(b"remote binary".as_slice())
            .create_async()
            .await;
        let url = format!("{}/tuic-remote-test-target", server.url());

        let first = ensure_local_from_url("1.2.3", "test-target", &url, DOWNLOAD_TIMEOUT)
            .await
            .expect("download succeeds");
        let second = ensure_local_from_url(
            "1.2.3",
            "test-target",
            "http://127.0.0.1:1",
            DOWNLOAD_TIMEOUT,
        )
        .await
        .expect("cached file needs no network");

        let expected_hash = "7dee7cc2fcb3d9ee8394182fe8d23a1a3d7e5c80b869b281269df9215a5abf2f";
        assert_eq!(first.sha256, expected_hash);
        assert_eq!(second.sha256, expected_hash);
        assert_eq!(first.path, second.path);
        assert_eq!(std::fs::read(&first.path).unwrap(), b"remote binary");
        assert_eq!(
            first.path,
            config
                .path()
                .join("remote-bin/1.2.3/tuic-remote-test-target")
        );
        assert!(!config.path().join("remote-bin/1.2.3/.staging").exists());
        route.assert_async().await;
    }

    #[tokio::test]
    async fn stalled_release_download_times_out_without_caching_an_asset() {
        let config = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let (entered_tx, mut entered_rx) = tokio::sync::mpsc::unbounded_channel();
        let router = axum::Router::new().route(
            "/asset",
            axum::routing::get({
                let entered_tx = entered_tx.clone();
                move || {
                    let entered_tx = entered_tx.clone();
                    async move {
                        entered_tx.send(()).expect("record the stalled request");
                        std::future::pending::<axum::http::StatusCode>().await
                    }
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/asset", listener.local_addr().unwrap());
        let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });

        let download = tokio::spawn(async move {
            ensure_local_from_url("1.2.3", "test-target", &url, DOWNLOAD_TIMEOUT).await
        });
        tokio::time::timeout(std::time::Duration::from_secs(60), entered_rx.recv())
            .await
            .expect("local request must reach the stalled endpoint")
            .expect("server must report the stalled request");

        tokio::time::pause();
        tokio::time::advance(DOWNLOAD_TIMEOUT + std::time::Duration::from_secs(1)).await;
        tokio::time::resume();
        let error = tokio::time::timeout(std::time::Duration::from_secs(15), download)
            .await
            .expect("release download must have its own timeout")
            .expect("download task must finish")
            .expect_err("stalled response must not be cached");
        assert!(error.contains("timed out"), "{error}");
        assert!(
            !config
                .path()
                .join("remote-bin/1.2.3/tuic-remote-test-target")
                .exists()
        );
        server.abort();
    }

    #[tokio::test]
    async fn not_found_names_the_target_and_version() {
        let config = tempfile::tempdir().expect("temporary config directory");
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", "/missing")
            .with_status(404)
            .create_async()
            .await;

        let error = ensure_local_from_url(
            "9.8.7",
            "x86_64-unknown-linux-gnu",
            &format!("{}/missing", server.url()),
            DOWNLOAD_TIMEOUT,
        )
        .await
        .expect_err("404 is a named error");

        assert_eq!(
            error,
            "no tuic-remote release asset for x86_64-unknown-linux-gnu at v9.8.7"
        );
    }

    #[tokio::test]
    async fn update_source_falls_back_only_for_a_matching_local_target() {
        let config = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let local = config.path().join("tuic-remote");
        std::fs::write(&local, b"replacement executable").unwrap();
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", "/missing")
            .with_status(404)
            .expect(2)
            .create_async()
            .await;
        let url = format!("{}/missing", server.url());

        let selected = resolve_update_asset_from_url(
            "aarch64-apple-darwin",
            &url,
            &local,
            Some("aarch64-apple-darwin"),
        )
        .await
        .expect("matching local binary");
        assert_eq!(selected.source, "local");
        assert_eq!(
            selected.binary.sha256,
            "74faa3811f5e551111ed370650ae6d6acf14f8f7141bc5c4f653eb52bf57bf16"
        );

        let error = resolve_update_asset_from_url(
            "x86_64-unknown-linux-gnu",
            &url,
            &local,
            Some("aarch64-apple-darwin"),
        )
        .await
        .expect_err("cross-target binary must be refused");
        assert!(error.contains("remote target x86_64-unknown-linux-gnu"));
        assert!(error.contains("local tuic-remote target is aarch64-apple-darwin"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn desktop_feature_stub_names_the_headless_build_command() {
        use std::os::unix::fs::PermissionsExt;
        let config = tempfile::tempdir().unwrap();
        let stub = config.path().join("tuic-remote");
        std::fs::write(
            &stub,
            "#!/bin/sh\necho 'tuic-remote requires --no-default-features' >&2\nexit 1\n",
        )
        .unwrap();
        std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap();
        let error = probe_local_binary(&stub).await.unwrap_err();
        assert!(error.contains("requires --no-default-features"), "{error}");
        assert!(
            error.contains("cargo build --bin tuic-remote --no-default-features"),
            "{error}"
        );
    }

    #[tokio::test]
    async fn update_source_prefers_the_matching_release_asset() {
        let config = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let local = config.path().join("tuic-remote");
        std::fs::write(&local, b"replacement executable").unwrap();
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", "/release")
            .with_status(200)
            .with_body(b"remote binary".as_slice())
            .create_async()
            .await;
        let selected = resolve_update_asset_from_url(
            "aarch64-apple-darwin",
            &format!("{}/release", server.url()),
            &local,
            Some("aarch64-apple-darwin"),
        )
        .await
        .unwrap();
        assert_eq!(selected.source, "release");
        assert_eq!(
            selected.binary.sha256,
            "7dee7cc2fcb3d9ee8394182fe8d23a1a3d7e5c80b869b281269df9215a5abf2f"
        );
    }

    /// A remote whose target differs from this build has no sibling fallback
    /// whatever the sibling's state, so the missing release asset must be the
    /// reported cause, not the missing local file.
    #[tokio::test]
    async fn other_target_without_asset_reports_the_target_not_the_missing_sibling() {
        let config = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let missing = config.path().join("tuic-remote");
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", "/missing")
            .with_status(404)
            .create_async()
            .await;

        let error = resolve_update_asset_from_url(
            "no-such-target-triple",
            &format!("{}/missing", server.url()),
            &missing,
            None,
        )
        .await
        .expect_err("no asset and no matching local build");

        assert!(error.contains("no-such-target-triple"), "{error}");
        assert!(error.contains("release asset unavailable"), "{error}");
        assert!(!error.contains("not found at"), "{error}");
    }

    #[tokio::test]
    async fn same_target_without_sibling_names_the_build_command() {
        let config = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let missing = config.path().join("tuic-remote");
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", "/missing")
            .with_status(404)
            .create_async()
            .await;

        let error = resolve_update_asset_from_url(
            env!("TUIC_TARGET_TRIPLE"),
            &format!("{}/missing", server.url()),
            &missing,
            None,
        )
        .await
        .expect_err("sibling is absent");

        assert!(
            error.contains("cargo build --bin tuic-remote --no-default-features"),
            "{error}"
        );
        assert!(error.contains("src-tauri"), "{error}");
        assert!(error.contains(&missing.display().to_string()), "{error}");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn same_target_sibling_is_probed_and_served_when_no_asset_exists() {
        use std::os::unix::fs::PermissionsExt;
        let config = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let sibling = config.path().join("tuic-remote");
        // The stub reports the digest of its own file, as the real daemon does.
        let script = format!(
            "#!/bin/sh\necho \"{{\\\"version\\\":\\\"0.0.1\\\",\\\"target\\\":\\\"{}\\\",\\\"sha256\\\":\\\"$(shasum -a 256 \"$0\" | cut -d' ' -f1)\\\"}}\"\n",
            env!("TUIC_TARGET_TRIPLE")
        );
        std::fs::write(&sibling, script).unwrap();
        std::fs::set_permissions(&sibling, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", "/missing")
            .with_status(404)
            .create_async()
            .await;

        let selected = resolve_update_asset_from_url(
            env!("TUIC_TARGET_TRIPLE"),
            &format!("{}/missing", server.url()),
            &sibling,
            None,
        )
        .await
        .expect("matching sibling serves the update");

        assert_eq!(selected.source, "local");
        assert_eq!(selected.version, "0.0.1");
        assert_eq!(selected.binary.path, sibling);
    }

    /// A directory at the sibling path must not pass as a built binary.
    #[tokio::test]
    async fn same_target_sibling_that_is_a_directory_names_the_build_command() {
        let config = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let sibling = config.path().join("tuic-remote");
        std::fs::create_dir(&sibling).unwrap();
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", "/missing")
            .with_status(404)
            .create_async()
            .await;

        let error = resolve_update_asset_from_url(
            env!("TUIC_TARGET_TRIPLE"),
            &format!("{}/missing", server.url()),
            &sibling,
            None,
        )
        .await
        .expect_err("a directory is not a sibling binary");

        assert!(
            error.contains("cargo build --bin tuic-remote --no-default-features"),
            "{error}"
        );
    }

    /// A sibling built with default features refuses to report its identity;
    /// the error must say how to rebuild it, not only that the probe failed.
    #[cfg(unix)]
    #[tokio::test]
    async fn same_target_sibling_needing_no_default_features_names_the_build_command() {
        use std::os::unix::fs::PermissionsExt;
        let config = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let sibling = config.path().join("tuic-remote");
        std::fs::write(
            &sibling,
            "#!/bin/sh\necho 'requires --no-default-features' >&2\nexit 2\n",
        )
        .unwrap();
        std::fs::set_permissions(&sibling, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", "/missing")
            .with_status(404)
            .create_async()
            .await;

        let error = resolve_update_asset_from_url(
            env!("TUIC_TARGET_TRIPLE"),
            &format!("{}/missing", server.url()),
            &sibling,
            None,
        )
        .await
        .expect_err("default-feature sibling cannot serve the update");

        assert!(
            error.contains("cargo build --bin tuic-remote --no-default-features"),
            "{error}"
        );
    }
}
