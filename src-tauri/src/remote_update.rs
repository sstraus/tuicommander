use std::path::PathBuf;
use std::sync::Arc;

use crate::remote_connection::{DeployMode, RemoteTransport};
use crate::remote_deploy::assets::{BuildIdentity, resolve_update_asset};
use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use futures_util::StreamExt;
use sha2::{Digest, Sha256};

#[cfg(not(test))]
const UPLOAD_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);
#[cfg(test)]
const UPLOAD_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(1);

#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct UpdatePreview {
    pub(crate) remote_build: Option<BuildIdentity>,
    pub(crate) desktop_build: BuildIdentity,
    pub(crate) source: &'static str,
    pub(crate) session_count: usize,
    pub(crate) out_of_date: bool,
}

pub(crate) fn build_is_out_of_date(remote: &BuildIdentity, selected: &BuildIdentity) -> bool {
    remote.sha256 != selected.sha256
}

pub(crate) async fn prepare(
    state: &Arc<crate::AppState>,
    id: &str,
) -> Result<UpdatePreview, String> {
    let connection = crate::remote_runtime::load_connection(state, id)?;
    let base_url = state
        .remote
        .base_url(id)
        .ok_or("Remote connection is not connected")?;
    let client = state.remote.http_client();
    let health = crate::remote_runtime::read_health(&client, &base_url).await?;
    let remote_build = health.build;
    let target = if let Some(build) = &remote_build {
        build.target.clone()
    } else if let Some(profile) = crate::remote_runtime::ssh_profile(&connection) {
        let uname = crate::tunnels::exec::ssh_exec(
            &profile,
            "uname -sm",
            None,
            std::time::Duration::from_secs(10),
        )
        .await
        .map_err(|e| format!("SSH target probe failed: {e:?}"))?;
        crate::remote_deploy::assets::require_target(uname.stdout.trim())?.to_string()
    } else {
        return Err("Direct daemon does not report a build target; install a daemon with /health.build once before in-process updates".to_string());
    };
    let asset = resolve_update_asset(&target).await?;
    let session_count = health
        .session_count
        .ok_or("Remote daemon does not report live session count")?;
    let desktop_build = BuildIdentity {
        version: asset.version,
        target: asset.target,
        sha256: asset.binary.sha256,
    };
    Ok(UpdatePreview {
        out_of_date: remote_build
            .as_ref()
            .is_none_or(|build| build_is_out_of_date(build, &desktop_build)),
        remote_build,
        desktop_build,
        source: asset.source,
        session_count,
    })
}

pub(crate) async fn update_and_restart(
    state: &Arc<crate::AppState>,
    id: &str,
    confirmed_sessions: usize,
    expected_sha256: &str,
) -> Result<UpdatePreview, String> {
    let _claim = crate::remote_runtime::claim_update(state, id)?;
    perform_update_and_restart(state, id, confirmed_sessions, expected_sha256).await
}

/// POST the binary to the daemon. The token travels only in the `tui-session`
/// cookie, never in the URL; the error is stripped of the URL regardless.
async fn send_binary_upload(
    client: &reqwest::Client,
    base_url: &str,
    token: &str,
    build: &BuildIdentity,
    confirmed_sessions: usize,
    body: reqwest::Body,
) -> Result<reqwest::Response, String> {
    tokio::time::timeout(
        UPLOAD_TIMEOUT,
        client
            .post(format!("{}/remote/update", base_url.trim_end_matches('/')))
            .header(
                reqwest::header::COOKIE,
                format!("{}={token}", crate::mcp_http::auth::SESSION_COOKIE),
            )
            .header("x-tuic-target", &build.target)
            .header("x-tuic-sha256", &build.sha256)
            .header("x-tuic-confirmed-sessions", confirmed_sessions)
            .body(body)
            .send(),
    )
    .await
    .map_err(|_| "Remote binary upload timed out".to_string())?
    .map_err(|e| format!("Remote binary upload failed: {}", e.without_url()))
}

pub(crate) async fn perform_update_and_restart(
    state: &Arc<crate::AppState>,
    id: &str,
    confirmed_sessions: usize,
    expected_sha256: &str,
) -> Result<UpdatePreview, String> {
    let connection = crate::remote_runtime::load_connection(state, id)?;
    let base_url = state
        .remote
        .base_url(id)
        .ok_or("Remote connection is not connected")?;
    let client = state.remote.http_client();
    let token = state
        .remote
        .token(id)
        .ok_or("Remote connection has no session token")?;
    let preview = prepare(state, id).await?;
    if confirmed_sessions != preview.session_count {
        return Err(format!(
            "Live session count changed from {confirmed_sessions} to {}",
            preview.session_count
        ));
    }
    if expected_sha256 != preview.desktop_build.sha256 {
        return Err("Selected remote binary changed after confirmation".to_string());
    }
    let asset = resolve_update_asset(&preview.desktop_build.target).await?;
    if asset.binary.sha256 != preview.desktop_build.sha256 {
        return Err("Selected remote binary changed while preparing transfer".to_string());
    }
    match &connection.transport {
        // The restart that finishes an update severs the tunnel it travelled on.
        RemoteTransport::Relay => {
            return Err("A relay connection cannot update the remote daemon".to_string());
        }
        RemoteTransport::Direct { .. } => {
            let file = tokio::fs::File::open(&asset.binary.path)
                .await
                .map_err(|e| format!("Could not open remote binary: {e}"))?;
            let stream = futures_util::stream::try_unfold(file, |mut file| async move {
                use tokio::io::AsyncReadExt;
                let mut buffer = [0u8; 64 * 1024];
                let count = file.read(&mut buffer).await?;
                if count == 0 {
                    Ok::<_, std::io::Error>(None)
                } else {
                    Ok(Some((
                        axum::body::Bytes::copy_from_slice(&buffer[..count]),
                        file,
                    )))
                }
            });
            let response = send_binary_upload(
                &client,
                &base_url,
                &token,
                &preview.desktop_build,
                confirmed_sessions,
                reqwest::Body::wrap_stream(stream),
            )
            .await?;
            if !response.status().is_success() {
                let status = response.status();
                let detail = tokio::time::timeout(UPLOAD_TIMEOUT, response.text())
                    .await
                    .map_err(|_| "Remote binary upload response timed out".to_string())?
                    .unwrap_or_default();
                return Err(format!(
                    "Remote binary upload rejected: {}: {}",
                    status, detail
                ));
            }
        }
        RemoteTransport::Ssh {
            remote_daemon_port, ..
        } => {
            let profile = crate::remote_runtime::ssh_profile(&connection)
                .ok_or("Could not resolve SSH profile")?;
            let uname = crate::tunnels::exec::ssh_exec(
                &profile,
                "uname -sm",
                None,
                std::time::Duration::from_secs(10),
            )
            .await
            .map_err(|e| format!("SSH target probe failed: {e:?}"))?;
            let ssh_target = crate::remote_deploy::assets::require_target(uname.stdout.trim())?;
            if ssh_target != preview.desktop_build.target {
                return Err(format!(
                    "SSH target {ssh_target} differs from daemon target {}",
                    preview.desktop_build.target
                ));
            }
            let latest = crate::remote_runtime::read_health(&client, &base_url).await?;
            if latest.session_count != Some(confirmed_sessions) {
                return Err("Live session count changed before SSH deployment".to_string());
            }
            if connection.deploy == DeployMode::Installed {
                crate::remote_deploy::service::update_installed(
                    &profile,
                    *remote_daemon_port,
                    &token,
                )
                .await
                .map_err(|e| format!("Installed SSH update failed: {e}"))?;
            } else {
                crate::remote_deploy::deploy_ephemeral(
                    &profile,
                    *remote_daemon_port,
                    &token,
                    connection.survive_secs,
                )
                .await
                .map_err(|e| format!("SSH update failed: {e}"))?;
            }
        }
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        if let Ok(health) = crate::remote_runtime::read_health(&client, &base_url).await
            && health
                .build
                .as_ref()
                .is_some_and(|build| build.sha256 == preview.desktop_build.sha256)
        {
            crate::remote_runtime::record_updated_build(state, id, preview.desktop_build.clone());
            return Ok(preview);
        }
        if std::time::Instant::now() >= deadline {
            return Err(format!(
                "Remote restart did not report build {} within 30s",
                preview.desktop_build.sha256
            ));
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn prepare_remote_update(
    state: tauri::State<'_, Arc<crate::AppState>>,
    id: String,
) -> Result<UpdatePreview, String> {
    prepare(state.inner(), &id).await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn update_and_restart_remote(
    state: tauri::State<'_, Arc<crate::AppState>>,
    id: String,
    confirmed_sessions: usize,
    expected_sha256: String,
) -> Result<UpdatePreview, String> {
    update_and_restart(state.inner(), &id, confirmed_sessions, &expected_sha256).await
}

const MAX_UPDATE_BYTES: u64 = 512 * 1024 * 1024;

/// An update may touch only the executable selected when the daemon started.
pub(crate) struct RemoteUpdateState {
    pub(crate) executable: PathBuf,
    pub(crate) restart: Arc<tokio::sync::Notify>,
    pub(crate) in_progress: tokio::sync::Mutex<()>,
    pub(crate) installed: std::sync::atomic::AtomicBool,
}

pub(crate) async fn upload(
    State(state): State<Arc<crate::AppState>>,
    headers: HeaderMap,
    body: Body,
) -> (StatusCode, String) {
    if !crate::mcp_http::auth::has_valid_session_token(&headers, &state.session_token.read()) {
        return (
            StatusCode::UNAUTHORIZED,
            "remote session token required".to_string(),
        );
    }
    match upload_inner(&state, &headers, body).await {
        Ok(()) => (
            StatusCode::ACCEPTED,
            "remote update staged; restarting".to_string(),
        ),
        Err((status, message)) => (status, message),
    }
}

async fn upload_inner(
    state: &crate::AppState,
    headers: &HeaderMap,
    body: Body,
) -> Result<(), (StatusCode, String)> {
    let error = |status, message: &str| (status, message.to_string());
    if cfg!(windows) {
        return Err(error(
            StatusCode::NOT_IMPLEMENTED,
            "in-process updates of Windows executables are not supported",
        ));
    }
    let target = header(headers, "x-tuic-target")?;
    if target != env!("TUIC_TARGET_TRIPLE") {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "remote target does not match binary",
        ));
    }
    let digest = header(headers, "x-tuic-sha256")?;
    if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(error(StatusCode::BAD_REQUEST, "invalid sha256 digest"));
    }
    let confirmed: usize = header(headers, "x-tuic-confirmed-sessions")?
        .parse()
        .map_err(|_| error(StatusCode::BAD_REQUEST, "invalid confirmed session count"))?;
    if confirmed != state.session_maps.sessions.len() {
        return Err(error(StatusCode::CONFLICT, "live session count changed"));
    }
    if headers
        .get(axum::http::header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .is_some_and(|length| length > MAX_UPDATE_BYTES)
    {
        return Err(error(
            StatusCode::PAYLOAD_TOO_LARGE,
            "remote binary too large",
        ));
    }
    let Some(runtime) = state.remote_update.as_ref() else {
        return Err(error(
            StatusCode::SERVICE_UNAVAILABLE,
            "not a remote daemon",
        ));
    };
    let _update_guard = runtime
        .in_progress
        .try_lock()
        .map_err(|_| error(StatusCode::CONFLICT, "another remote update is in progress"))?;
    if runtime.installed.load(std::sync::atomic::Ordering::Acquire) {
        return Err(error(
            StatusCode::CONFLICT,
            "remote restart is already pending",
        ));
    }

    let executable = std::fs::canonicalize(&runtime.executable).map_err(|_| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "could not locate daemon executable",
        )
    })?;
    let metadata = std::fs::metadata(&executable).map_err(|_| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "could not inspect daemon executable",
        )
    })?;
    if !metadata.is_file() {
        return Err(error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "daemon executable is not a file",
        ));
    }
    let directory = executable.parent().ok_or_else(|| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "daemon executable has no directory",
        )
    })?;
    let staging = directory.join(format!(".tuic-remote-update-{}", uuid::Uuid::new_v4()));
    let result = async {
        let mut file = tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&staging)
            .await
            .map_err(|_| {
                error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "could not stage remote binary",
                )
            })?;
        let mut stream = body.into_data_stream();
        let mut hasher = Sha256::new();
        let mut written = 0u64;
        while let Some(chunk) = stream.next().await {
            let chunk =
                chunk.map_err(|_| error(StatusCode::BAD_REQUEST, "upload stream failed"))?;
            written = written
                .checked_add(chunk.len() as u64)
                .ok_or_else(|| error(StatusCode::PAYLOAD_TOO_LARGE, "remote binary too large"))?;
            if written > MAX_UPDATE_BYTES {
                return Err(error(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    "remote binary too large",
                ));
            }
            hasher.update(&chunk);
            use tokio::io::AsyncWriteExt;
            file.write_all(&chunk).await.map_err(|_| {
                error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "could not write remote binary",
                )
            })?;
        }
        if written == 0 {
            return Err(error(StatusCode::BAD_REQUEST, "empty remote binary"));
        }
        use tokio::io::AsyncWriteExt;
        file.flush().await.map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "could not flush remote binary",
            )
        })?;
        file.sync_all().await.map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "could not sync remote binary",
            )
        })?;
        drop(file);
        use std::fmt::Write as _;
        let mut actual = String::with_capacity(64);
        for byte in hasher.finalize() {
            write!(actual, "{byte:02x}").expect("writing to a String cannot fail");
        }
        if !actual.eq_ignore_ascii_case(digest) {
            return Err(error(
                StatusCode::BAD_REQUEST,
                "remote binary sha256 mismatch",
            ));
        }
        if confirmed != state.session_maps.sessions.len() {
            return Err(error(
                StatusCode::CONFLICT,
                "live session count changed during upload",
            ));
        }
        tokio::fs::set_permissions(&staging, metadata.permissions())
            .await
            .map_err(|_| {
                error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "could not set executable permissions",
                )
            })?;
        tokio::fs::rename(&staging, &executable)
            .await
            .map_err(|_| {
                error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "could not replace daemon executable",
                )
            })?;
        Ok(())
    }
    .await;
    if result.is_err() {
        let _ = tokio::fs::remove_file(&staging).await;
    } else {
        runtime
            .installed
            .store(true, std::sync::atomic::Ordering::Release);
        tracing::info!(
            source = "remote",
            target,
            sha256 = digest,
            "Remote binary updated"
        );
        let restart = runtime.restart.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            restart.notify_one();
        });
    }
    result
}

fn header<'a>(headers: &'a HeaderMap, name: &str) -> Result<&'a str, (StatusCode, String)> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                format!("missing or malformed {name}"),
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn daemon_upload_request(path: &str, cookie: Option<&str>) -> axum::http::Request<Body> {
        let mut request = axum::http::Request::post(path)
            .header(axum::http::header::HOST, "127.0.0.1:9876")
            .header("x-tuic-target", env!("TUIC_TARGET_TRIPLE"))
            .header(
                "x-tuic-sha256",
                "74faa3811f5e551111ed370650ae6d6acf14f8f7141bc5c4f653eb52bf57bf16",
            )
            .header("x-tuic-confirmed-sessions", "0");
        if let Some(cookie) = cookie {
            request = request.header(axum::http::header::COOKIE, cookie);
        }
        let mut request = request.body(Body::from("replacement executable")).unwrap();
        request
            .extensions_mut()
            .insert(axum::extract::ConnectInfo(std::net::SocketAddr::from((
                [203, 0, 113, 5],
                5555,
            ))));
        request
    }

    // Catches: the upload handler rejects a cookie already admitted by middleware,
    // or a stray query token overrides the cookie decision.
    #[cfg(not(windows))]
    #[tokio::test]
    async fn daemon_upload_credentials_cookie_installs_the_binary() {
        use tower::ServiceExt;
        for (path, cookie) in [
            (
                "/remote/update",
                Some("other=value; tui-session=update-secret; last=value"),
            ),
            (
                "/remote/update?token=wrong",
                Some("tui-session=update-secret"),
            ),
        ] {
            let directory = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
            let executable = directory.path().join("tuic-remote");
            std::fs::write(&executable, b"old executable").unwrap();
            let mut state = crate::state::tests_support::make_test_app_state();
            *state.session_token.write() = "update-secret".to_string();
            state.remote_update = Some(RemoteUpdateState {
                executable: executable.clone(),
                restart: Arc::new(tokio::sync::Notify::new()),
                in_progress: tokio::sync::Mutex::new(()),
                installed: std::sync::atomic::AtomicBool::new(false),
            });
            let response = crate::mcp_http::build_remote_router(Arc::new(state))
                .oneshot(daemon_upload_request(path, cookie))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::ACCEPTED);
            assert_eq!(
                std::fs::read(&executable).unwrap(),
                b"replacement executable"
            );
            assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
        }
    }

    // Catches: the shared middleware still admits a query-only token on
    // /fs/upload-copy (the release N compatibility path), or rejects the cookie.
    #[tokio::test]
    async fn daemon_upload_credentials_copy_reaches_validation_only_with_a_valid_cookie() {
        use tower::ServiceExt;
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        *state.session_token.write() = "update-secret".to_string();
        let app = crate::mcp_http::build_remote_router(state);
        for (path, cookie, expected) in [
            (
                "/fs/upload-copy",
                Some("tui-session=update-secret"),
                StatusCode::BAD_REQUEST,
            ),
            (
                "/fs/upload-copy?token=wrong",
                Some("tui-session=update-secret"),
                StatusCode::BAD_REQUEST,
            ),
            (
                "/fs/upload-copy?token=update-secret",
                None,
                StatusCode::UNAUTHORIZED,
            ),
            (
                "/fs/upload-copy?token=update-secret",
                Some("tui-session=wrong"),
                StatusCode::UNAUTHORIZED,
            ),
            ("/fs/upload-copy", None, StatusCode::UNAUTHORIZED),
            (
                "/fs/upload-copy?token=wrong",
                Some("tui-session=wrong"),
                StatusCode::UNAUTHORIZED,
            ),
        ] {
            // Missing destDir/name must reach the real query extractor only after auth.
            let response = app
                .clone()
                .oneshot(daemon_upload_request(path, cookie))
                .await
                .unwrap();
            assert_eq!(response.status(), expected, "{path} {cookie:?}");
        }
    }

    // Catches: Basic auth or a valid query token accidentally replaces the cookie
    // requirement, or an empty/retired token authorizes an executable replacement.
    #[tokio::test]
    async fn daemon_upload_credentials_missing_wrong_empty_and_rotated_tokens_are_401() {
        use base64::Engine;
        use tower::ServiceExt;
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        *state.session_token.write() = "update-secret".to_string();
        {
            let mut config = state.config.write();
            config.services.auth.username = "boss".to_string();
            config.services.auth.password_hash = bcrypt::hash("known-password", 4).unwrap();
        }
        let credentials = base64::engine::general_purpose::STANDARD.encode("boss:known-password");
        let app = crate::mcp_http::build_remote_router(Arc::clone(&state));
        for (path, cookie) in [
            ("/remote/update", None),
            ("/remote/update?token=wrong", None),
            ("/remote/update?token=update-secret", None),
            (
                "/remote/update?token=update-secret",
                Some("tui-session=wrong"),
            ),
            ("/remote/update", Some("tui-session=wrong")),
            ("/remote/update?token=wrong", Some("tui-session=wrong")),
            ("/remote/update?token=", Some("tui-session=")),
            (
                "/remote/update",
                Some("other=update-secret; tui-session=update-secret-extra"),
            ),
        ] {
            for basic in [false, true] {
                let mut request = daemon_upload_request(path, cookie);
                if basic {
                    request.headers_mut().insert(
                        axum::http::header::AUTHORIZATION,
                        format!("Basic {credentials}").parse().unwrap(),
                    );
                }
                assert_eq!(
                    app.clone().oneshot(request).await.unwrap().status(),
                    StatusCode::UNAUTHORIZED
                );
            }
        }
        *state.session_token.write() = "rotated-secret".to_string();
        let request = daemon_upload_request("/remote/update", Some("tui-session=update-secret"));
        assert_eq!(
            app.clone().oneshot(request).await.unwrap().status(),
            StatusCode::UNAUTHORIZED
        );
        state.session_token.write().clear();
        let request = daemon_upload_request("/remote/update?token=", Some("tui-session="));
        assert_eq!(
            app.oneshot(request).await.unwrap().status(),
            StatusCode::UNAUTHORIZED
        );
    }

    // Catches: the upload connect error carrying the request URL, and with it the
    // session token, into the message the UI shows.
    #[tokio::test]
    async fn upload_connect_failure_never_reports_the_session_token() {
        // Reserve the port without listening, so connect fails and no parallel
        // server can claim the address before the upload attempts it.
        let socket = tokio::net::TcpSocket::new_v4().unwrap();
        socket.bind("127.0.0.1:0".parse().unwrap()).unwrap();
        let address = socket.local_addr().unwrap();
        let build = BuildIdentity {
            version: "1.7.7".to_string(),
            target: "t".to_string(),
            sha256: "a".repeat(64),
        };
        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_millis(100))
            .no_proxy()
            .build()
            .unwrap();
        let error = send_binary_upload(
            &client,
            &format!("http://{address}"),
            "super-secret-token",
            &build,
            0,
            reqwest::Body::from("x"),
        )
        .await
        .unwrap_err();
        assert!(error.starts_with("Remote binary upload failed"), "{error}");
        assert!(!error.contains("super-secret-token"), "{error}");
    }

    #[tokio::test]
    async fn direct_update_waits_until_the_new_build_answers_health() {
        use std::sync::atomic::{AtomicBool, Ordering};
        let cache = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(cache.path().to_path_buf());
        let target = env!("TUIC_TARGET_TRIPLE");
        let directory = cache
            .path()
            .join("remote-bin")
            .join(env!("CARGO_PKG_VERSION"));
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            directory.join(format!("tuic-remote-{target}")),
            b"remote binary",
        )
        .unwrap();
        let installed = Arc::new(AtomicBool::new(false));
        let advertised = Arc::new(AtomicBool::new(false));
        let upload_seen = Arc::new(tokio::sync::Notify::new());
        let router = axum::Router::new()
            .route("/health", axum::routing::get({
                let advertised = advertised.clone();
                move || {
                    let advertised = advertised.clone();
                    async move {
                        let sha256 = if advertised.load(Ordering::SeqCst) {
                            "7dee7cc2fcb3d9ee8394182fe8d23a1a3d7e5c80b869b281269df9215a5abf2f".to_string()
                        } else {
                            "a".repeat(64)
                        };
                        axum::Json(serde_json::json!({
                            "session_count": 0,
                            "build": { "version": "1.7.6", "target": env!("TUIC_TARGET_TRIPLE"), "sha256": sha256 }
                        }))
                    }
                }
            }))
            .route("/remote/update", axum::routing::post({
                let installed = installed.clone();
                let upload_seen = upload_seen.clone();
                move |uri: axum::http::Uri, headers: axum::http::HeaderMap, body: axum::body::Bytes| {
                    let installed = installed.clone();
                    let upload_seen = upload_seen.clone();
                    async move {
                        assert_eq!(uri.query(), None);
                        assert_eq!(headers["cookie"], "tui-session=test-token");
                        assert_eq!(headers["x-tuic-confirmed-sessions"], "0");
                        assert_eq!(headers["x-tuic-target"], env!("TUIC_TARGET_TRIPLE"));
                        assert_eq!(body.as_ref(), b"remote binary");
                        installed.store(true, Ordering::SeqCst);
                        upload_seen.notify_one();
                        StatusCode::ACCEPTED
                    }
                }
            }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });

        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let connection = crate::remote_connection::RemoteConnection::new_direct(
            "test remote",
            url.clone(),
            "boss",
        );
        let id = connection.id.clone();
        crate::remote_connection::RemoteConnectionStore::save(&state.data_dir, &[connection])
            .unwrap();
        state
            .remote
            .force_connected_for_test(&id, &url, Some("test-token"));

        let task = tokio::spawn(async move {
            update_and_restart(
                &state,
                &id,
                0,
                "7dee7cc2fcb3d9ee8394182fe8d23a1a3d7e5c80b869b281269df9215a5abf2f",
            )
            .await
        });
        upload_seen.notified().await;
        assert!(installed.load(Ordering::SeqCst));
        assert!(
            !task.is_finished(),
            "upload alone must not count as a verified restart"
        );
        advertised.store(true, Ordering::SeqCst);
        let preview = task.await.unwrap().unwrap();
        assert_eq!(
            preview.desktop_build.sha256,
            "7dee7cc2fcb3d9ee8394182fe8d23a1a3d7e5c80b869b281269df9215a5abf2f"
        );
        server.abort();
    }

    #[tokio::test]
    async fn prepare_reports_selected_release_and_live_sessions_for_direct() {
        let cache = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(cache.path().to_path_buf());
        let target = env!("TUIC_TARGET_TRIPLE");
        let directory = cache
            .path()
            .join("remote-bin")
            .join(env!("CARGO_PKG_VERSION"));
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            directory.join(format!("tuic-remote-{target}")),
            b"remote binary",
        )
        .unwrap();
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", "/health")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                serde_json::json!({
                    "session_count": 2,
                    "build": { "version": "1.7.7", "target": target, "sha256": "a".repeat(64) }
                })
                .to_string(),
            )
            .create_async()
            .await;
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let connection = crate::remote_connection::RemoteConnection::new_direct(
            "test remote",
            server.url(),
            "boss",
        );
        let id = connection.id.clone();
        crate::remote_connection::RemoteConnectionStore::save(&state.data_dir, &[connection])
            .unwrap();
        state
            .remote
            .force_connected_for_test(&id, &server.url(), Some("token"));

        let preview = prepare(&state, &id).await.unwrap();
        assert_eq!(preview.session_count, 2);
        assert_eq!(preview.source, "release");
        assert_eq!(preview.desktop_build.target, target);
        assert_eq!(
            preview.desktop_build.sha256,
            "7dee7cc2fcb3d9ee8394182fe8d23a1a3d7e5c80b869b281269df9215a5abf2f"
        );
        assert!(preview.out_of_date);
    }

    #[tokio::test]
    async fn older_direct_daemon_without_build_target_names_the_bootstrap_limit() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", "/health")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"session_count":1,"protocol_version":4}"#)
            .create_async()
            .await;
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let connection = crate::remote_connection::RemoteConnection::new_direct(
            "older daemon",
            server.url(),
            "boss",
        );
        let id = connection.id.clone();
        crate::remote_connection::RemoteConnectionStore::save(&state.data_dir, &[connection])
            .unwrap();
        state
            .remote
            .force_connected_for_test(&id, &server.url(), Some("token"));
        let error = prepare(&state, &id)
            .await
            .expect_err("old Direct daemon needs one manual bootstrap");
        assert!(error.contains("Direct daemon does not report a build target"));
    }

    #[cfg(not(windows))]
    fn upload_headers(content_length: Option<u64>) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert("x-tuic-target", env!("TUIC_TARGET_TRIPLE").parse().unwrap());
        headers.insert("x-tuic-sha256", "0".repeat(64).parse().unwrap());
        headers.insert("x-tuic-confirmed-sessions", "0".parse().unwrap());
        if let Some(length) = content_length {
            headers.insert(
                axum::http::header::CONTENT_LENGTH,
                length.to_string().parse().unwrap(),
            );
        }
        headers
    }

    /// Streams `sizes` bytes per chunk into `upload_inner` against a scratch daemon
    /// executable. The digest never matches, so an upload that passes the size
    /// limit ends in the sha256 check instead of replacing anything.
    #[cfg(not(windows))]
    async fn upload_chunks(sizes: &[usize]) -> (StatusCode, String) {
        let directory = tempfile::tempdir().unwrap();
        let executable = directory.path().join("tuic-remote");
        std::fs::write(&executable, b"daemon").unwrap();
        let mut state = crate::state::tests_support::make_test_app_state();
        state.remote_update = Some(RemoteUpdateState {
            executable: executable.clone(),
            restart: Arc::new(tokio::sync::Notify::new()),
            in_progress: tokio::sync::Mutex::new(()),
            installed: std::sync::atomic::AtomicBool::new(false),
        });
        let largest = *sizes.iter().max().unwrap();
        let shared = axum::body::Bytes::from(vec![0u8; largest]);
        let chunks: Vec<Result<axum::body::Bytes, std::io::Error>> =
            sizes.iter().map(|size| Ok(shared.slice(..*size))).collect();
        let body = Body::from_stream(futures_util::stream::iter(chunks));
        let error = upload_inner(&state, &upload_headers(None), body)
            .await
            .expect_err("a zero-filled upload never matches the digest");
        assert_eq!(std::fs::read(&executable).unwrap(), b"daemon");
        assert_eq!(
            std::fs::read_dir(directory.path()).unwrap().count(),
            1,
            "a rejected upload must not leave its staging file behind"
        );
        error
    }

    #[cfg(not(windows))]
    // Catches `>` -> `>=` in the declared Content-Length check: an upload of exactly
    // the limit would be refused with 413 before the daemon state is even read.
    #[tokio::test]
    async fn declared_length_equal_to_limit_is_not_rejected_but_one_more_is() {
        let state = crate::state::tests_support::make_test_app_state();
        let (status, _) = upload_inner(
            &state,
            &upload_headers(Some(MAX_UPDATE_BYTES)),
            Body::empty(),
        )
        .await
        .unwrap_err();
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);

        let (status, _) = upload_inner(
            &state,
            &upload_headers(Some(MAX_UPDATE_BYTES + 1)),
            Body::empty(),
        )
        .await
        .unwrap_err();
        assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[cfg(not(windows))]
    // Catches `>` -> `>=` and `>` -> `==` on the streamed byte count: exactly the
    // limit reaches the sha256 check (`>=` and `==` reject it as too large).
    #[tokio::test]
    async fn streamed_body_of_exactly_the_limit_reaches_the_digest_check() {
        const MIB: usize = 1024 * 1024;
        let (status, message) = upload_chunks(&[MIB; 512]).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(message, "remote binary sha256 mismatch");
    }

    #[cfg(not(windows))]
    // Catches `>` -> `==` on the streamed byte count: the running total steps over
    // the limit without ever landing on it, so only a `>` comparison refuses it.
    #[tokio::test]
    async fn streamed_body_that_steps_past_the_limit_is_rejected() {
        const MIB: usize = 1024 * 1024;
        let mut sizes = vec![MIB; 511];
        sizes.push(MIB + 1);
        let (status, message) = upload_chunks(&sizes).await;
        assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
        assert_eq!(message, "remote binary too large");
    }

    #[test]
    fn identity_detects_an_older_binary_even_with_the_same_version() {
        let remote = BuildIdentity {
            version: "1.7.7".into(),
            target: "aarch64-apple-darwin".into(),
            sha256: "a".repeat(64),
        };
        let selected = BuildIdentity {
            version: "1.7.7".into(),
            target: "aarch64-apple-darwin".into(),
            sha256: "b".repeat(64),
        };
        assert!(build_is_out_of_date(&remote, &selected));
        assert!(!build_is_out_of_date(&selected, &selected));
    }
}

#[cfg(test)]
mod cookie_critic_tests {
    use super::*;
    use axum::extract::ConnectInfo;
    use axum::http::{Request, header};
    use tower::ServiceExt;

    fn request(uri: &str, cookie: &str, body: Body) -> Request<Body> {
        Request::post(uri)
            .header(header::HOST, "127.0.0.1:9876")
            .header(header::COOKIE, cookie)
            .extension(ConnectInfo(std::net::SocketAddr::from((
                [203, 0, 113, 5],
                5555,
            ))))
            .body(body)
            .unwrap()
    }

    /// Catches ambient cookie auth letting a foreign browser origin replace the daemon.
    #[tokio::test]
    async fn foreign_origin_cookie_update_cannot_replace_the_executable() {
        let scratch = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let executable = scratch.path().join("tuic-remote");
        std::fs::write(&executable, b"original executable").unwrap();
        let mut state = crate::state::tests_support::make_test_app_state();
        *state.session_token.write() = "current-secret".into();
        state.remote_update = Some(RemoteUpdateState {
            executable: executable.clone(),
            restart: Arc::new(tokio::sync::Notify::new()),
            in_progress: tokio::sync::Mutex::new(()),
            installed: std::sync::atomic::AtomicBool::new(false),
        });
        let app = crate::mcp_http::build_remote_router(Arc::new(state));
        let binary = b"unwanted replacement";
        let digest = "f602ffe15523d9d24bf76461d09e2836933cf4efd30e1a6e09eb3df13129e38a";
        for (origin, site) in [
            ("https://foreign.example", "cross-site"),
            ("http://127.0.0.1:9999", "same-site"),
        ] {
            let mut req = request(
                "/remote/update",
                "tui-session=current-secret",
                Body::from(binary.to_vec()),
            );
            for (name, value) in [
                ("origin", origin),
                ("sec-fetch-site", site),
                ("x-tuic-target", env!("TUIC_TARGET_TRIPLE")),
                ("x-tuic-sha256", digest),
                ("x-tuic-confirmed-sessions", "0"),
            ] {
                req.headers_mut().insert(
                    axum::http::HeaderName::from_static(name),
                    value.parse().unwrap(),
                );
            }
            let response = app.clone().oneshot(req).await.unwrap();
            assert_eq!(response.status(), StatusCode::FORBIDDEN, "{origin}");
            assert_eq!(std::fs::read(&executable).unwrap(), b"original executable");
            assert_eq!(std::fs::read_dir(scratch.path()).unwrap().count(), 1);
        }
    }
}
