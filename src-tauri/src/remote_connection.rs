//! Remote connection config model.
//!
//! Persists named connections (SSH or Direct) to `connections.json` in the
//! app config directory. Each connection has a UUID, a human-readable name,
//! a transport, auth info, and an enabled flag.

use std::path::Path;

use serde::{Deserialize, Serialize};

const CONNECTIONS_FILE: &str = "connections.json";

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// A saved remote connection.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub(crate) struct RemoteConnection {
    #[schemars(extend("x-setting" = {"sensitivity":"public","apply":"live","effect":"Updates id for the current owner.","operations":["get"],"confirmationClass":"ordinary"}))]
    pub(crate) id: String,
    #[schemars(extend("x-setting" = {"sensitivity":"public","apply":"live","effect":"Updates name for the current owner.","operations":["get","set"],"confirmationClass":"ordinary"}))]
    pub(crate) name: String,
    #[schemars(extend("x-setting" = {"sensitivity":"public","apply":"live","effect":"Updates transport for the current owner.","operations":["get","set"],"confirmationClass":"ordinary"}))]
    pub(crate) transport: RemoteTransport,
    #[schemars(extend("x-setting" = {"sensitivity":"private","apply":"live","effect":"Updates auth username for the current owner.","operations":["get","set"],"confirmationClass":"ordinary"}))]
    pub(crate) auth_username: String,
    #[schemars(extend("x-setting" = {"sensitivity":"public","apply":"live","effect":"Updates enabled for the current owner.","operations":["get","set"],"confirmationClass":"ordinary"}))]
    pub(crate) enabled: bool,
    #[serde(default)]
    #[schemars(extend("x-setting" = {"sensitivity":"public","apply":"live","effect":"Updates auto update for the current owner.","operations":["get","set"],"confirmationClass":"ordinary"}))]
    pub(crate) auto_update: bool,
    #[serde(default)]
    #[schemars(extend("x-setting" = {"sensitivity":"public","apply":"live","effect":"Updates deploy for the current owner.","operations":["get","set"],"confirmationClass":"ordinary"}))]
    pub(crate) deploy: DeployMode,
    #[serde(default = "default_survive_secs")]
    #[schemars(extend("x-setting" = {"sensitivity":"public","apply":"live","effect":"Updates survive secs for the current owner.","operations":["get","set"],"confirmationClass":"ordinary"}))]
    pub(crate) survive_secs: u64,
}

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DeployMode {
    #[default]
    Never,
    OnConnect,
    Installed,
}

const fn default_survive_secs() -> u64 {
    1_800
}

/// Transport layer for a remote connection.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "type")]
pub(crate) enum RemoteTransport {
    Ssh {
        #[schemars(extend("x-setting" = {"sensitivity":"private","apply":"live","effect":"Updates ssh host for the current owner.","operations":["get","set"],"confirmationClass":"ordinary"}))]
        ssh_host: String,
        #[schemars(extend("x-setting" = {"sensitivity":"public","apply":"live","effect":"Updates ssh port for the current owner.","operations":["get","set"],"confirmationClass":"ordinary"}))]
        ssh_port: u16,
        #[schemars(extend("x-setting" = {"sensitivity":"private","apply":"live","effect":"Updates ssh user for the current owner.","operations":["get","set"],"confirmationClass":"ordinary"}))]
        ssh_user: String,
        #[schemars(extend("x-setting" = {"sensitivity":"private","apply":"live","effect":"Updates identity file for the current owner.","operations":["get","set"],"confirmationClass":"ordinary"}))]
        identity_file: Option<String>,
        #[schemars(extend("x-setting" = {"sensitivity":"public","apply":"live","effect":"Updates remote daemon port for the current owner.","operations":["get","set"],"confirmationClass":"ordinary"}))]
        remote_daemon_port: u16,
    },
    Direct {
        #[schemars(extend("x-setting" = {"sensitivity":"private","apply":"live","effect":"Updates url for the current owner.","operations":["get","set"],"confirmationClass":"execution_authority"}))]
        url: String,
    },
    /// Reached through a relay. Everything that identifies the pairing lives in
    /// the vault bundle keyed by the connection id, not in this plain-text record.
    Relay,
}

impl RemoteConnection {
    /// Create a new SSH connection with default port (22) and daemon port (9877).
    pub(crate) fn new_ssh(
        name: impl Into<String>,
        host: impl Into<String>,
        user: impl Into<String>,
    ) -> Self {
        let ssh_user = user.into();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            transport: RemoteTransport::Ssh {
                ssh_host: host.into(),
                ssh_port: 22,
                ssh_user: ssh_user.clone(),
                identity_file: None,
                remote_daemon_port: 9877,
            },
            auth_username: ssh_user,
            enabled: true,
            auto_update: false,
            deploy: DeployMode::Never,
            survive_secs: default_survive_secs(),
        }
    }

    pub(crate) fn validate(&self) -> Result<(), String> {
        if uuid::Uuid::parse_str(&self.id).is_err() {
            return Err("id must be a valid UUID".to_string());
        }
        if self.name.trim().is_empty() {
            return Err("name must not be empty".to_string());
        }
        if self.auth_username.trim().is_empty() {
            return Err("auth_username must not be empty".to_string());
        }
        match &self.transport {
            RemoteTransport::Ssh {
                ssh_host,
                ssh_user,
                ssh_port,
                ..
            } => {
                if ssh_host.trim().is_empty() {
                    return Err("ssh_host must not be empty".to_string());
                }
                if ssh_user.trim().is_empty() {
                    return Err("ssh_user must not be empty".to_string());
                }
                if *ssh_port == 0 {
                    return Err("ssh_port must be in range 1-65535".to_string());
                }
            }
            RemoteTransport::Direct { url } => {
                if url.trim().is_empty() {
                    return Err("url must not be empty".to_string());
                }
            }
            RemoteTransport::Relay => {
                // Nothing can be deployed through a relay: the tunnel carries
                // only the daemon's own HTTP surface.
                if self.deploy != DeployMode::Never {
                    return Err("a relay connection cannot deploy the remote daemon".to_string());
                }
            }
        }
        Ok(())
    }

    /// Create a new relay connection. The pairing bundle is stored separately.
    pub(crate) fn new_relay(name: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            transport: RemoteTransport::Relay,
            auth_username: "relay".to_string(),
            enabled: true,
            auto_update: false,
            deploy: DeployMode::Never,
            survive_secs: default_survive_secs(),
        }
    }

    /// Create a new Direct connection.
    pub(crate) fn new_direct(
        name: impl Into<String>,
        url: impl Into<String>,
        auth_username: impl Into<String>,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            transport: RemoteTransport::Direct { url: url.into() },
            auth_username: auth_username.into(),
            enabled: true,
            auto_update: false,
            deploy: DeployMode::Never,
            survive_secs: default_survive_secs(),
        }
    }
}

// ---------------------------------------------------------------------------
// Persistence
// ---------------------------------------------------------------------------

pub(crate) struct RemoteConnectionStore;

#[derive(Deserialize)]
pub(crate) struct RemoteConnectionSaveRequest {
    pub(crate) base: Option<RemoteConnection>,
    pub(crate) connection: RemoteConnection,
}

impl RemoteConnectionStore {
    /// Mutate the latest array under the same process and file locks used by
    /// other config domains. Connection ids, rather than array positions, are
    /// the unit of change.
    pub(crate) fn update<R, F>(config_dir: &Path, mutate: F) -> Result<R, String>
    where
        F: FnOnce(&mut Vec<RemoteConnection>) -> Result<(R, bool), String>,
    {
        crate::config::ConfigFile::<Vec<RemoteConnection>>::at_path(
            config_dir.join(CONNECTIONS_FILE),
        )
        .update_with_strict(mutate)
    }

    /// Load connections from `<config_dir>/connections.json`.
    /// Returns an empty vec if the file does not exist.
    pub(crate) fn load(config_dir: &Path) -> anyhow::Result<Vec<RemoteConnection>> {
        let path = config_dir.join(CONNECTIONS_FILE);
        if !path.exists() {
            return Ok(Vec::new());
        }
        let content = std::fs::read_to_string(&path)
            .map_err(|e| anyhow::anyhow!("Failed to read {}: {e}", path.display()))?;
        let connections = serde_json::from_str(&content)
            .map_err(|e| anyhow::anyhow!("Failed to parse {}: {e}", path.display()))?;
        Ok(connections)
    }

    /// Save connections to `<config_dir>/connections.json` atomically.
    pub(crate) fn save(config_dir: &Path, connections: &[RemoteConnection]) -> anyhow::Result<()> {
        std::fs::create_dir_all(config_dir)
            .map_err(|e| anyhow::anyhow!("Failed to create config dir: {e}"))?;
        let json = serde_json::to_string_pretty(connections)
            .map_err(|e| anyhow::anyhow!("Failed to serialize connections: {e}"))?;
        let target = config_dir.join(CONNECTIONS_FILE);
        let temp = target.with_extension(format!("tmp.{}", std::process::id()));
        std::fs::write(&temp, json.as_bytes())
            .map_err(|e| anyhow::anyhow!("Failed to write temp file: {e}"))?;
        std::fs::rename(&temp, &target).map_err(|e| {
            let _ = std::fs::remove_file(&temp);
            anyhow::anyhow!("Failed to commit connections file: {e}")
        })?;
        Ok(())
    }
}

/// Validate then upsert `connection` into the store at `data_dir`.
///
/// Shared by the Tauri `save_remote_connection` command and the HTTP
/// `put_remote_connection` route so both enforce `validate()` identically
/// (IPC/HTTP parity) — the Tauri command used to skip validation. Callers hold
/// `state.connections_lock` around this to serialize concurrent writers.
pub(crate) fn upsert_remote_connection(
    data_dir: &Path,
    base: Option<RemoteConnection>,
    connection: RemoteConnection,
) -> Result<(), String> {
    connection.validate()?;
    if base.as_ref().is_some_and(|base| base.id != connection.id) {
        return Err("base and connection ids differ".to_string());
    }
    let delta = base
        .as_ref()
        .map(|base| {
            let base = serde_json::to_value(base).map_err(|e| e.to_string())?;
            let desired = serde_json::to_value(&connection).map_err(|e| e.to_string())?;
            Ok::<_, String>(crate::config::json_merge_delta(&base, &desired))
        })
        .transpose()?;
    RemoteConnectionStore::update(data_dir, move |connections| {
        let current = connections.iter_mut().find(|c| c.id == connection.id);
        match (base, current) {
            (None, None) => {
                connections.push(connection);
                Ok(((), true))
            }
            (None, Some(_)) => Err("connection already exists; load it before editing".to_string()),
            (Some(_), None) => Err("connection was deleted since it was loaded".to_string()),
            (Some(_), Some(current)) => {
                let Some(Some(delta)) = delta else {
                    return Ok(((), false));
                };
                crate::config::apply_typed_json_merge_delta(current, &delta)?;
                current.validate()?;
                Ok(((), true))
            }
        }
    })
}

pub(crate) fn remove_remote_connection(data_dir: &Path, id: &str) -> Result<bool, String> {
    RemoteConnectionStore::update(data_dir, |connections| {
        let before = connections.len();
        connections.retain(|connection| connection.id != id);
        let removed = connections.len() != before;
        Ok((removed, removed))
    })
}

/// How long the token exchange may take. The daemon answers from memory, so the
/// only thing this waits for is the network (and, on the SSH transport, a tunnel
/// that has already reported connected).
const TOKEN_FETCH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// Store (or, on an empty value, forget) the Basic Auth password for a
/// connection. The secret goes to the credential vault, never to
/// `connections.json` — that file is plain JSON on disk and is read by the
/// `/config/remote-connections` route.
pub(crate) fn set_connection_password(id: &str, password: &str) -> Result<(), String> {
    if password.is_empty() {
        return crate::credentials::delete(crate::credentials::Credential::RemoteConnection(id));
    }
    crate::credentials::set(
        crate::credentials::Credential::RemoteConnection(id),
        password,
    )
}

/// Legacy IPC and HTTP password writes share persistence and invalidation.
pub(crate) fn set_connection_password_with_refresh(
    state: &crate::AppState,
    id: &str,
    password: &str,
) -> Result<(), String> {
    set_connection_password(id, password)?;
    crate::settings::api::publish_change(state, "connections", true);
    Ok(())
}

/// Deployment creates a credential only when one is absent. Both deployment
/// paths invalidate the Settings credential-presence projection after persistence.
pub(crate) fn ensure_pairing_token(state: &crate::AppState, id: &str) -> Result<String, String> {
    match pairing_token(id)? {
        Some(token) => Ok(token),
        None => {
            let token = uuid::Uuid::new_v4().to_string();
            set_pairing_token(id, &token)?;
            crate::settings::api::publish_change(state, "connections", true);
            Ok(token)
        }
    }
}

pub(crate) fn connection_password_exists(id: &str) -> Result<bool, String> {
    crate::credentials::get(crate::credentials::Credential::RemoteConnection(id))
        .map(|v| v.is_some())
}

pub(crate) fn set_pairing_token(id: &str, token: &str) -> Result<(), String> {
    if token.is_empty() {
        return crate::credentials::delete(crate::credentials::Credential::RemotePairingToken(id));
    }
    crate::credentials::set(
        crate::credentials::Credential::RemotePairingToken(id),
        token,
    )
}

pub(crate) fn pairing_token(id: &str) -> Result<Option<String>, String> {
    crate::credentials::get(crate::credentials::Credential::RemotePairingToken(id))
}

pub(crate) fn delete_connection_credentials(id: &str) -> Result<(), String> {
    use crate::credentials::{Credential, delete};
    let failures: Vec<String> = [
        ("remote password", delete(Credential::RemoteConnection(id))),
        ("pairing token", delete(Credential::RemotePairingToken(id))),
        ("relay pairing", delete(Credential::RelayPairing(id))),
    ]
    .into_iter()
    .filter_map(|(what, result)| result.err().map(|e| format!("{what} ({e})")))
    .collect();
    match failures.as_slice() {
        [] => Ok(()),
        [only] => Err(only.clone()),
        _ => Err(format!("failed to delete {}", failures.join(" and "))),
    }
}

/// Validate a relay pairing bundle and keep it in the vault for this connection.
/// Nothing is stored when the text does not parse.
pub(crate) fn set_relay_bundle(id: &str, text: &str) -> Result<(), String> {
    let bundle = crate::relay_tunnel::bundle::PairingBundle::parse(text.trim())
        .map_err(|e| format!("relay pairing bundle: {e}"))?;
    crate::credentials::set(
        crate::credentials::Credential::RelayPairing(id),
        &bundle.to_text(),
    )
}

/// Store the bundle text, or remove the pairing when `text` is empty — the same
/// convention as [`set_pairing_token`].
pub(crate) fn set_relay_pairing(id: &str, text: &str) -> Result<(), String> {
    if text.is_empty() {
        return crate::credentials::delete(crate::credentials::Credential::RelayPairing(id));
    }
    set_relay_bundle(id, text)
}

/// Both the IPC command and the HTTP route import a bundle through this.
pub(crate) fn set_relay_pairing_with_refresh(
    state: &crate::AppState,
    id: &str,
    text: &str,
) -> Result<(), String> {
    set_relay_pairing(id, text)?;
    crate::settings::api::publish_change(state, "connections", true);
    Ok(())
}

pub(crate) fn relay_bundle(
    id: &str,
) -> Result<Option<crate::relay_tunnel::bundle::PairingBundle>, String> {
    let Some(text) = crate::credentials::get(crate::credentials::Credential::RelayPairing(id))?
    else {
        return Ok(None);
    };
    let text = zeroize::Zeroizing::new(text);
    crate::relay_tunnel::bundle::PairingBundle::parse(&text)
        .map(Some)
        .map_err(|e| format!("stored relay pairing: {e}"))
}

/// Trade the stored Basic Auth credentials for the daemon's session token.
///
/// A WebSocket upgrade cannot carry an `Authorization` header and the daemon
/// serves `Access-Control-Allow-Origin: *`, which forbids credentialed cookies,
/// so browser WebSocket upgrades carry `?token=` while native HTTP uses the
/// existing session cookie header. The exchange runs here rather than in the WebView so the password never leaves
/// the backend.
///
/// The token lives in the daemon's memory and changes on every restart, so the
/// caller re-runs this on every connect and never persists the result.
pub(crate) async fn fetch_connection_token(
    id: &str,
    base_url: &str,
    username: &str,
) -> Result<String, String> {
    let password = crate::credentials::get(crate::credentials::Credential::RemoteConnection(id))?
        .ok_or_else(|| "No password stored for this connection".to_string())?;
    request_session_token(base_url, username, &password).await
}

/// The HTTP half of [`fetch_connection_token`], split out so the failure modes
/// that matter — a rejected password, an older daemon with no such route — are
/// testable without an interactive keyring.
async fn request_session_token(
    base_url: &str,
    username: &str,
    password: &str,
) -> Result<String, String> {
    let url = format!("{}/api/auth/session-token", base_url.trim_end_matches('/'));
    let response = reqwest::Client::new()
        .get(&url)
        .basic_auth(username, Some(password))
        .timeout(TOKEN_FETCH_TIMEOUT)
        .send()
        .await
        .map_err(|e| format!("Token request failed: {}", e.without_url()))?;
    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err("Authentication rejected by the remote daemon".to_string());
    }
    if !status.is_success() {
        return Err(format!(
            "Remote daemon answered {status} for the session-token request"
        ));
    }
    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Malformed token response: {}", e.without_url()))?;
    let token = body
        .get("token")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    if token.is_empty() {
        // An older daemon has no such route and the request fell through to an
        // SPA shell or an empty body; say so instead of handing back "".
        return Err("Remote daemon returned no session token — is it running a build with /api/auth/session-token?".to_string());
    }
    Ok(token.to_string())
}

/// State-bearing owner path shared by IPC and HTTP. Callers hold connections_lock.
pub(crate) fn save_connection_record(
    state: &crate::AppState,
    base: Option<RemoteConnection>,
    connection: RemoteConnection,
) -> Result<(), String> {
    upsert_remote_connection(&state.data_dir, base, connection)?;
    crate::settings::api::publish_change(state, "connections", true);
    Ok(())
}

/// Publish persisted metadata even if credential cleanup subsequently fails.
pub(crate) fn delete_connection_record(
    state: &crate::AppState,
    id: &str,
    cleanup: impl FnOnce(&str) -> Result<(), String>,
) -> Result<bool, String> {
    let removed = remove_remote_connection(&state.data_dir, id)?;
    if removed {
        let cleanup_result = cleanup(id);
        crate::settings::api::publish_change(state, "connections", cleanup_result.is_ok());
        cleanup_result?;
    }
    Ok(removed)
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn list_remote_connections(
    state: tauri::State<'_, std::sync::Arc<crate::AppState>>,
) -> Result<Vec<RemoteConnection>, String> {
    RemoteConnectionStore::load(&state.data_dir).map_err(|e| e.to_string())
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn save_remote_connection(
    state: tauri::State<'_, std::sync::Arc<crate::AppState>>,
    base: Option<RemoteConnection>,
    connection: RemoteConnection,
) -> Result<(), String> {
    let _guard = state.connections_lock.lock().await;
    save_connection_record(&state, base, connection)
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn delete_remote_connection(
    state: tauri::State<'_, std::sync::Arc<crate::AppState>>,
    id: String,
) -> Result<(), String> {
    // Before the store, not after: everything the runtime is holding for this
    // connection — the poll, the mirror, its rows, the tunnel, the session token
    // — is keyed by an id that is about to name nothing. Deleting the record
    // first leaves all of it running with no way left to address it.
    crate::remote_runtime::teardown_deleted(state.inner(), &id);
    let _guard = state.connections_lock.lock().await;
    if !delete_connection_record(&state, &id, delete_connection_credentials)? {
        // Preserve IPC's idempotent credential cleanup even when metadata is already absent.
        delete_connection_credentials(&id)?;
    }
    Ok(())
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn set_remote_connection_password(
    state: tauri::State<'_, std::sync::Arc<crate::AppState>>,
    id: String,
    password: String,
) -> Result<(), String> {
    set_connection_password_with_refresh(&state, &id, &password)
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn set_remote_connection_relay_pairing(
    state: tauri::State<'_, std::sync::Arc<crate::AppState>>,
    id: String,
    bundle: String,
) -> Result<(), String> {
    set_relay_pairing_with_refresh(&state, &id, &bundle)
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn remote_connection_password_exists(id: String) -> Result<bool, String> {
    connection_password_exists(&id)
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn fetch_remote_connection_token(
    id: String,
    base_url: String,
    username: String,
) -> Result<String, String> {
    fetch_connection_token(&id, &base_url, &username).await
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ssh_json_round_trip() {
        let conn = RemoteConnection::new_ssh("my-server", "example.com", "alice");
        let json = serde_json::to_string(&conn).unwrap();
        let decoded: RemoteConnection = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.name, conn.name);
        assert_eq!(decoded.auth_username, conn.auth_username);
        assert!(decoded.enabled);
        match decoded.transport {
            RemoteTransport::Ssh {
                ssh_host,
                ssh_port,
                ssh_user,
                identity_file,
                remote_daemon_port,
            } => {
                assert_eq!(ssh_host, "example.com");
                assert_eq!(ssh_port, 22);
                assert_eq!(ssh_user, "alice");
                assert!(identity_file.is_none());
                assert_eq!(remote_daemon_port, 9877);
            }
            other => panic!("expected Ssh, got {other:?}"),
        }
    }

    #[test]
    fn direct_json_round_trip() {
        let conn = RemoteConnection::new_direct("office", "http://office:9877", "bob");
        let json = serde_json::to_string(&conn).unwrap();
        let decoded: RemoteConnection = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.name, "office");
        assert_eq!(decoded.auth_username, "bob");
        assert!(decoded.enabled);
        match decoded.transport {
            RemoteTransport::Direct { url } => assert_eq!(url, "http://office:9877"),
            other => panic!("expected Direct, got {other:?}"),
        }
    }

    #[test]
    fn serde_tag_produces_correct_type_field() {
        let ssh = RemoteConnection::new_ssh("s", "h", "u");
        let ssh_json = serde_json::to_string(&ssh).unwrap();
        let ssh_val: serde_json::Value = serde_json::from_str(&ssh_json).unwrap();
        assert_eq!(ssh_val["transport"]["type"], "Ssh");

        let direct = RemoteConnection::new_direct("d", "http://x", "u");
        let direct_json = serde_json::to_string(&direct).unwrap();
        let direct_val: serde_json::Value = serde_json::from_str(&direct_json).unwrap();
        assert_eq!(direct_val["transport"]["type"], "Direct");
    }

    #[test]
    fn store_save_then_load_returns_same_data() {
        let dir = tempfile::tempdir().unwrap();
        let conn1 = RemoteConnection::new_ssh("server1", "host1.example.com", "alice");
        let conn2 = RemoteConnection::new_direct("direct1", "http://10.0.0.1:9877", "bob");
        let connections = vec![conn1, conn2];

        RemoteConnectionStore::save(dir.path(), &connections).unwrap();
        let loaded = RemoteConnectionStore::load(dir.path()).unwrap();

        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].name, "server1");
        assert_eq!(loaded[1].name, "direct1");
        assert_eq!(loaded[0].id, connections[0].id);
        assert_eq!(loaded[1].id, connections[1].id);
    }

    #[test]
    fn store_load_nonexistent_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let loaded = RemoteConnectionStore::load(dir.path()).unwrap();
        assert!(loaded.is_empty());
    }

    #[test]
    fn new_ssh_defaults() {
        let conn = RemoteConnection::new_ssh("test", "myhost", "myuser");
        assert!(conn.enabled);
        match conn.transport {
            RemoteTransport::Ssh {
                ssh_port,
                remote_daemon_port,
                ssh_user,
                ..
            } => {
                assert_eq!(ssh_port, 22);
                assert_eq!(remote_daemon_port, 9877);
                assert_eq!(ssh_user, "myuser");
            }
            other => panic!("expected Ssh, got {other:?}"),
        }
        assert_eq!(conn.auth_username, "myuser");
    }

    #[test]
    fn new_direct_enabled() {
        let conn = RemoteConnection::new_direct("d", "http://x", "u");
        assert!(conn.enabled);
    }

    // --- Story 781-9652: the password lives in the vault, never on disk -------

    /// `connections.json` is plain JSON that `GET /config/remote-connections`
    /// serves to any authenticated caller. A password field added to
    /// `RemoteConnection` would be published by both, silently — this is the
    /// guard that fails first.
    #[test]
    fn serialized_connection_carries_no_secret() {
        let conn = RemoteConnection::new_direct("office", "http://office:9877", "bob");
        let value: serde_json::Value = serde_json::to_value(&conn).unwrap();
        let keys: Vec<&str> = value
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            keys,
            [
                "id",
                "name",
                "transport",
                "auth_username",
                "enabled",
                "auto_update",
                "deploy",
                "survive_secs"
            ],
            "connections.json gained a field — if it holds a secret, it must go to the vault instead"
        );
        let serialized = serde_json::to_string(&conn).unwrap();
        assert!(!serialized.contains("password"));
        assert!(!serialized.contains("token"));
    }

    #[test]
    fn legacy_connection_defaults_to_no_deploy_and_thirty_minutes() {
        let mut value = serde_json::to_value(RemoteConnection::new_direct(
            "office",
            "http://office:9877",
            "bob",
        ))
        .unwrap();
        value.as_object_mut().unwrap().remove("deploy");
        value.as_object_mut().unwrap().remove("survive_secs");
        value.as_object_mut().unwrap().remove("auto_update");

        let decoded: RemoteConnection = serde_json::from_value(value).unwrap();

        assert_eq!(decoded.deploy, DeployMode::Never);
        assert_eq!(decoded.survive_secs, 1_800);
        assert!(!decoded.auto_update);
    }

    // Catches: explicitly echoed base URLs or reqwest URLs leaking URL userinfo in token-request errors.
    #[tokio::test]
    async fn token_request_error_redacts_url_credentials() {
        let socket = crate::test_support::resetting_tcp_peer();
        let address = socket.local_addr().expect("address");
        let base = format!("http://username:URL_SECRET_1457@{address}");
        let error = request_session_token(&base, "user", "password")
            .await
            .expect_err("resetting peer");
        assert!(error.contains("failed"), "{error}");
        assert!(
            !error.contains("URL_SECRET_1457"),
            "URL credential in error: {error}"
        );
        assert!(
            !error.contains("password"),
            "Basic credential in error: {error}"
        );
    }

    #[tokio::test]
    async fn session_token_request_reads_the_token_field() {
        let mut server = mockito::Server::new_async().await;
        let route = server
            .mock("GET", "/api/auth/session-token")
            // Basic dm9tOnMzY3JldA== is "vom:s3cret" — proof the credential is
            // sent as a header on this one call, before anything moves to ?token=.
            .match_header("authorization", "Basic dm9tOnMzY3JldA==")
            .with_status(200)
            .with_body(r#"{"token":"tok-abc"}"#)
            .create_async()
            .await;

        let token = request_session_token(&server.url(), "vom", "s3cret")
            .await
            .unwrap();

        assert_eq!(token, "tok-abc");
        route.assert_async().await;
    }

    /// A wrong password and an unreachable daemon need different fixes, so 401
    /// must not be reported as a generic upstream failure.
    #[tokio::test]
    async fn session_token_request_names_a_rejected_password() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", "/api/auth/session-token")
            .with_status(401)
            .with_body("Scan the QR code or authenticate with Basic Auth")
            .create_async()
            .await;

        let err = request_session_token(&server.url(), "vom", "wrong")
            .await
            .unwrap_err();

        assert!(
            err.contains("Authentication rejected"),
            "expected a rejection, got: {err}"
        );
    }

    /// A daemon older than this route has no handler for it: the request falls
    /// through to something without a `token` field. Handing back "" would make
    /// every later call 401 with no explanation.
    #[tokio::test]
    async fn session_token_request_rejects_a_response_without_a_token() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", "/api/auth/session-token")
            .with_status(200)
            .with_body(r#"{"ok":true}"#)
            .create_async()
            .await;

        let err = request_session_token(&server.url(), "vom", "s3cret")
            .await
            .unwrap_err();

        assert!(
            err.contains("no session token"),
            "expected a missing-token error, got: {err}"
        );
    }

    /// The base URL comes from a user-typed field; a trailing slash there would
    /// otherwise produce `//api/auth/session-token`, which axum does not match.
    #[tokio::test]
    async fn session_token_request_tolerates_a_trailing_slash() {
        let mut server = mockito::Server::new_async().await;
        let route = server
            .mock("GET", "/api/auth/session-token")
            .with_status(200)
            .with_body(r#"{"token":"tok-abc"}"#)
            .create_async()
            .await;

        let token = request_session_token(&format!("{}/", server.url()), "vom", "s3cret")
            .await
            .unwrap();

        assert_eq!(token, "tok-abc");
        route.assert_async().await;
    }

    #[test]
    fn validate_valid_ssh_connection() {
        let conn = RemoteConnection::new_ssh("server", "host.example.com", "alice");
        assert!(conn.validate().is_ok());
    }

    #[test]
    fn validate_invalid_uuid_rejected() {
        let mut conn = RemoteConnection::new_ssh("server", "host", "alice");
        conn.id = "../../malicious".to_string();
        let err = conn.validate().unwrap_err();
        assert!(
            err.contains("valid UUID"),
            "expected UUID error, got: {err}"
        );
    }

    #[test]
    fn validate_whitespace_name_rejected() {
        let conn = RemoteConnection::new_ssh("  ", "host", "alice");
        assert!(conn.validate().is_err());
    }

    #[test]
    fn validate_empty_ssh_host_rejected() {
        let conn = RemoteConnection::new_ssh("s", "", "alice");
        assert!(conn.validate().is_err());
    }

    #[test]
    fn validate_empty_url_rejected() {
        let conn = RemoteConnection::new_direct("d", "  ", "u");
        assert!(conn.validate().is_err());
    }

    // --- IPC/HTTP parity: shared save path enforces validate() (story 127-89ec) -
    // `upsert_remote_connection` is the single persist path behind both the Tauri
    // `save_remote_connection` command (which previously skipped validation) and
    // the HTTP `put_remote_connection` route. This proves invalid input is
    // rejected before it can be persisted, on either transport.

    #[test]
    fn upsert_rejects_invalid_before_persisting() {
        let dir = tempfile::tempdir().unwrap();
        let mut bad = RemoteConnection::new_ssh("server", "host", "alice");
        bad.id = "../../escape".to_string(); // invalid UUID
        let err = upsert_remote_connection(dir.path(), None, bad).unwrap_err();
        assert!(
            err.contains("valid UUID"),
            "expected UUID error, got: {err}"
        );
        assert!(
            RemoteConnectionStore::load(dir.path()).unwrap().is_empty(),
            "invalid input must not be written to the store"
        );
    }

    #[test]
    fn upsert_persists_valid_and_updates_existing() {
        let dir = tempfile::tempdir().unwrap();
        let conn = RemoteConnection::new_ssh("server", "host.example.com", "alice");
        let id = conn.id.clone();
        upsert_remote_connection(dir.path(), None, conn).unwrap();
        assert_eq!(RemoteConnectionStore::load(dir.path()).unwrap().len(), 1);

        // Same id updates in place rather than appending a duplicate.
        let mut updated = RemoteConnection::new_ssh("renamed", "host.example.com", "alice");
        updated.id = id.clone();
        let base = RemoteConnectionStore::load(dir.path()).unwrap().remove(0);
        upsert_remote_connection(dir.path(), Some(base), updated).unwrap();
        let loaded = RemoteConnectionStore::load(dir.path()).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, id);
        assert_eq!(loaded[0].name, "renamed");
    }

    #[test]
    fn stale_remote_connection_saves_preserve_independent_fields() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let original = RemoteConnection::new_direct("first", "https://host.example", "alice");
        upsert_remote_connection(dir.path(), None, original.clone()).unwrap();

        let mut first = original.clone();
        first.auto_update = true;
        let mut second = original.clone();
        second.name = "renamed".to_string();
        upsert_remote_connection(dir.path(), Some(original.clone()), first).unwrap();
        upsert_remote_connection(dir.path(), Some(original), second).unwrap();

        let saved = RemoteConnectionStore::load(dir.path()).unwrap();
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0].name, "renamed");
        assert!(
            saved[0].auto_update,
            "an unchanged stale field must not revert another writer"
        );
    }
}

#[cfg(test)]
mod refresh_review_tests {
    use super::*;

    // Catches: desktop owner writes persisting records without waking peer settings readers.
    #[test]
    fn connection_owner_save_and_delete_wake_settings_readers() {
        let state = crate::state::tests_support::make_test_app_state();
        let mut events = state.event_bus.subscribe();
        let connection = RemoteConnection::new_ssh("fixture", "not-contacted.example", "fixture");
        save_connection_record(&state, None, connection.clone()).unwrap();
        let notice = events.try_recv().expect("connection save did not publish");
        assert!(
            matches!(notice, crate::state::AppEvent::SettingsChanged(ref change) if change.section == "connections" && change.persisted && change.applied)
        );
        delete_connection_record(&state, &connection.id, |_| Ok(())).unwrap();
        let notice = events
            .try_recv()
            .expect("connection delete did not publish");
        assert!(
            matches!(notice, crate::state::AppEvent::SettingsChanged(ref change) if change.section == "connections" && change.persisted && change.applied)
        );
        assert!(
            RemoteConnectionStore::load(&state.data_dir)
                .unwrap()
                .is_empty()
        );
    }
    // Catches: a failed save or missing delete advertising a durable connection change.
    #[test]
    fn failed_connection_writes_do_not_wake_settings_readers() {
        let state = crate::state::tests_support::make_test_app_state();
        let mut events = state.event_bus.subscribe();
        let mut connection =
            RemoteConnection::new_ssh("fixture", "not-contacted.example", "fixture");
        connection.name.clear();
        assert!(save_connection_record(&state, None, connection).is_err());
        assert!(
            !delete_connection_record(&state, "missing", |_| panic!(
                "missing record has no credentials to delete"
            ))
            .unwrap()
        );
        assert!(events.try_recv().is_err());
    }

    // Catches: credential cleanup failure hiding an already persisted metadata deletion.
    #[test]
    fn connection_cleanup_failure_still_wakes_readers_as_unapplied() {
        let state = crate::state::tests_support::make_test_app_state();
        let connection = RemoteConnection::new_ssh("fixture", "not-contacted.example", "fixture");
        save_connection_record(&state, None, connection.clone()).unwrap();
        let mut events = state.event_bus.subscribe();
        assert!(
            delete_connection_record(
                &state,
                &connection.id,
                |_| Err("cleanup unavailable".into())
            )
            .is_err()
        );
        assert!(
            RemoteConnectionStore::load(&state.data_dir)
                .unwrap()
                .is_empty()
        );
        let event = events
            .try_recv()
            .expect("persisted deletion needs an invalidation");
        assert!(
            matches!(event, crate::state::AppEvent::SettingsChanged(ref change) if change.section == "connections" && change.persisted && !change.applied)
        );
        assert!(events.try_recv().is_err());
    }

    // Catches: a relay connection that still honours a deploy mode (nothing can
    // be deployed through the tunnel), a garbage bundle stored as a pairing, and
    // a deleted connection leaving its relay pairing secret in the vault.
    #[test]
    fn relay_connection_rules_and_pairing_lifecycle() {
        let mut connection = RemoteConnection::new_relay("phone");
        connection.validate().unwrap();
        connection.deploy = DeployMode::OnConnect;
        assert!(connection.validate().is_err());

        let id = connection.id.as_str();
        assert!(set_relay_bundle(id, "not a bundle").is_err());
        assert!(relay_bundle(id).unwrap().is_none());

        let bearer = crate::relay_tunnel::bundle::RelayBearer::new("R".into()).unwrap();
        let bundle =
            crate::relay_tunnel::bundle::PairingBundle::generate("ws://127.0.0.1:1", bearer)
                .unwrap();
        set_relay_bundle(id, &format!("  {}\n", bundle.to_text().as_str())).unwrap();
        assert_eq!(relay_bundle(id).unwrap().unwrap().room, bundle.room);
        delete_connection_credentials(id).unwrap();
        assert!(relay_bundle(id).unwrap().is_none());
    }
}
