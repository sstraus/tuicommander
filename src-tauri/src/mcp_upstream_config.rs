//! Configuration schema and persistence for upstream MCP servers.
//!
//! Each upstream server is identified by a unique name and can use either
//! HTTP (Streamable HTTP) or stdio (spawned process) transport. Configuration
//! lives in `mcp-upstreams.json`, separate from the main `AppConfig`.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use crate::config::{ConfigFile, load_json_config};

pub(crate) const UPSTREAMS_FILE: &str = "mcp-upstreams.json";

/// Orders each in-process upstream commit through its live-registry update.
///
/// The blocking config and advisory file locks are acquired only inside
/// `persist_upstream_delta` and released before `apply_config_diff` awaits. This
/// async mutex remains held across both stages so a later commit cannot apply its
/// runtime diff before an earlier commit and then be overwritten by that stale diff.
static UPSTREAM_SAVE_SEQUENCE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Top-level wrapper for the upstream config file.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub(crate) struct UpstreamMcpConfig {
    #[serde(default)]
    pub(crate) servers: Vec<UpstreamMcpServer>,
}

/// Transport-neutral save request. `base` is the configuration the caller loaded;
/// `config` is its desired result. The backend derives the semantic delta and applies
/// it to the latest locked file rather than replacing that file with `config`.
#[derive(Clone, Debug, Deserialize)]
pub(crate) struct UpstreamMcpSaveRequest {
    pub(crate) base: UpstreamMcpConfig,
    pub(crate) config: UpstreamMcpConfig,
}

/// A single upstream MCP server configuration.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub(crate) struct UpstreamMcpServer {
    /// Unique identifier (UUID).
    pub(crate) id: String,
    /// Human-readable name, also used as the tool namespace prefix.
    /// Must be unique, non-empty, and contain only `[a-z0-9_-]`.
    pub(crate) name: String,
    /// Transport configuration.
    pub(crate) transport: UpstreamTransport,
    /// Whether this upstream is active.
    #[serde(default = "default_true")]
    pub(crate) enabled: bool,
    /// Timeout in seconds for tool calls (0 = no timeout).
    #[serde(default = "default_timeout")]
    pub(crate) timeout_secs: u32,
    /// Optional tool filter (allow/deny list).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) tool_filter: Option<ToolFilter>,
    /// Optional authentication configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) auth: Option<UpstreamAuth>,
    /// Secret header metadata only; values live in the upstream credential vault.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) headers: Vec<UpstreamHeader>,
}

/// A secret header and its opaque, upstream-scoped credential reference.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct UpstreamHeader {
    pub(crate) name: String,
    pub(crate) credential_ref: String,
}

impl UpstreamHeader {
    pub(crate) fn validate(&self) -> Result<(), String> {
        let name = reqwest::header::HeaderName::from_bytes(self.name.as_bytes())
            .map_err(|_| "Invalid custom header name".to_string())?;
        // These names control routing, framing or the MCP transport itself.
        if matches!(
            name.as_str(),
            "host"
                | "content-length"
                | "transfer-encoding"
                | "connection"
                | "content-type"
                | "accept"
                | "mcp-session-id"
                | "mcp-protocol-version"
                | "proxy-authorization"
                | "cookie"
        ) {
            return Err("Custom header name is reserved".into());
        }
        uuid::Uuid::parse_str(&self.credential_ref)
            .map_err(|_| "Invalid header credential reference".to_string())?;
        Ok(())
    }

    pub(crate) fn credential_key(&self, upstream: &str) -> String {
        format!("{upstream}/header/{}", self.credential_ref)
    }
}

/// Transport type for connecting to an upstream MCP server.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub(crate) enum UpstreamTransport {
    Http {
        url: String,
    },
    Stdio {
        command: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        env: HashMap<String, String>,
        /// Working directory for the spawned process.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cwd: Option<String>,
    },
}

/// Tool filter: allow or deny specific tools by glob pattern.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub(crate) struct ToolFilter {
    pub(crate) mode: FilterMode,
    pub(crate) patterns: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum FilterMode {
    Allow,
    Deny,
}

/// Authentication method for an upstream MCP server.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum UpstreamAuth {
    /// Static bearer token.
    Bearer { token: String },
    /// OAuth 2.1 with PKCE (RFC 9449 / RFC 8707).
    ///
    /// Wire tag is `oauth2` (matches the frontend `UpstreamAuth` type and the
    /// Authorize-button gate). The `o_auth2` alias keeps reading config files
    /// written before this rename — they migrate to `oauth2` on the next save.
    /// Without the explicit rename, `rename_all = "snake_case"` would derive
    /// `o_auth2`, which never matched the frontend's `"oauth2"` comparison and
    /// left OAuth upstreams (e.g. publishwith-ai) with no Authorize action.
    #[serde(rename = "oauth2", alias = "o_auth2")]
    OAuth2 {
        client_id: String,
        /// Confidential clients (e.g. from DCR) have a secret; public clients omit it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        client_secret: Option<String>,
        #[serde(default)]
        scopes: Vec<String>,
        /// Override discovered authorization endpoint.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        authorization_endpoint: Option<String>,
        /// Override discovered token endpoint.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        token_endpoint: Option<String>,
    },
}

fn default_true() -> bool {
    true
}

fn default_timeout() -> u32 {
    30
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/// Validation errors for upstream config.
#[derive(Debug, PartialEq)]
pub(crate) enum UpstreamConfigError {
    EmptyName(String),
    InvalidName(String),
    DuplicateName(String),
    InvalidUrlScheme(String),
    SelfReferentialUrl(String),
    EmptyUrl(String),
    EmptyCommand(String),
    EmptyOAuthClientId(String),
    InvalidHeaders,
}

impl std::fmt::Display for UpstreamConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidHeaders => f.write_str("Invalid or duplicate custom headers"),
            Self::EmptyName(id) => write!(f, "Server '{id}' has an empty name"),
            Self::InvalidName(name) => write!(
                f,
                "Name '{name}' is invalid: must contain only lowercase letters, digits, hyphens, and underscores"
            ),
            Self::DuplicateName(name) => write!(f, "Duplicate server name: '{name}'"),
            Self::InvalidUrlScheme(url) => {
                write!(f, "URL '{url}' must use http:// or https:// scheme")
            }
            Self::SelfReferentialUrl(url) => {
                write!(
                    f,
                    "URL '{url}' points to this TUIC instance (circular proxy)"
                )
            }
            Self::EmptyUrl(id) => write!(f, "Server '{id}' has an empty HTTP URL"),
            Self::EmptyCommand(id) => write!(f, "Server '{id}' has an empty stdio command"),
            Self::EmptyOAuthClientId(id) => {
                write!(f, "Server '{id}' has an OAuth2 auth with empty client_id")
            }
        }
    }
}

/// Validate the upstream config. Returns all errors found (not just the first).
pub(crate) fn validate_upstream_config(
    config: &UpstreamMcpConfig,
    self_port: u16,
) -> Vec<UpstreamConfigError> {
    let mut errors = Vec::new();
    let mut seen_names = std::collections::HashSet::new();
    let name_re = regex::Regex::new(r"^[a-z0-9_-]+$").unwrap();

    for server in &config.servers {
        // Empty name
        if server.name.is_empty() {
            errors.push(UpstreamConfigError::EmptyName(server.id.clone()));
            continue;
        }

        // Invalid name characters
        if !name_re.is_match(&server.name) {
            errors.push(UpstreamConfigError::InvalidName(server.name.clone()));
        }

        // Duplicate name
        if !seen_names.insert(&server.name) {
            errors.push(UpstreamConfigError::DuplicateName(server.name.clone()));
        }

        // Transport-specific validation
        match &server.transport {
            UpstreamTransport::Http { url } => {
                if url.is_empty() {
                    errors.push(UpstreamConfigError::EmptyUrl(server.id.clone()));
                } else if !url.starts_with("http://") && !url.starts_with("https://") {
                    errors.push(UpstreamConfigError::InvalidUrlScheme(url.clone()));
                } else if is_self_referential(url, self_port) {
                    errors.push(UpstreamConfigError::SelfReferentialUrl(url.clone()));
                }
            }
            UpstreamTransport::Stdio { command, .. } => {
                if command.is_empty() {
                    errors.push(UpstreamConfigError::EmptyCommand(server.id.clone()));
                }
            }
        }

        let mut header_names = HashSet::new();
        let mut header_refs = HashSet::new();
        for header in &server.headers {
            if header.validate().is_err()
                || !matches!(server.transport, UpstreamTransport::Http { .. })
                || !header_names.insert(header.name.to_ascii_lowercase())
                || !header_refs.insert(&header.credential_ref)
                || (header.name.eq_ignore_ascii_case("authorization") && server.auth.is_some())
            {
                errors.push(UpstreamConfigError::InvalidHeaders);
            }
        }

        // Auth-specific validation
        if let Some(UpstreamAuth::OAuth2 { client_id, .. }) = &server.auth
            && client_id.is_empty()
        {
            errors.push(UpstreamConfigError::EmptyOAuthClientId(server.id.clone()));
        }
    }

    errors
}

/// Check if a URL points to this TUIC instance (circular proxy).
pub(crate) fn is_self_referential(url: &str, self_port: u16) -> bool {
    let Ok(parsed) = url::Url::parse(url) else {
        return false;
    };
    let host = parsed.host_str().unwrap_or("");
    let port = parsed
        .port()
        .unwrap_or(if parsed.scheme() == "https" { 443 } else { 80 });
    let is_localhost = matches!(
        host,
        "localhost" | "127.0.0.1" | "::1" | "[::1]" | "0.0.0.0"
    );
    is_localhost && port == self_port
}

// ---------------------------------------------------------------------------
// Boot-time auto-connect
// ---------------------------------------------------------------------------

/// Connect all saved upstream MCP servers on app startup.
/// Skips validation errors (logs them) and connects each enabled server.
pub(crate) async fn auto_connect_saved_upstreams(state: &crate::state::AppState) {
    let config: UpstreamMcpConfig = load_json_config(UPSTREAMS_FILE);
    if config.servers.is_empty() {
        // No upstreams to wait for — let tools/list serve immediately.
        state.mcp.upstream_registry.mark_initial_connect_complete();
        return;
    }

    let self_port = state.config.read().services.server.port;
    let errors = validate_upstream_config(&config, self_port);
    if !errors.is_empty() {
        for e in &errors {
            tracing::warn!(source = "mcp_upstream", "Boot-time config error: {e}");
        }
    }

    let registry = &state.mcp.upstream_registry;
    tracing::info!(
        source = "mcp_upstream",
        count = config.servers.len(),
        names = ?config.servers.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(),
        "Boot-time auto-connect starting"
    );
    for server in config.servers {
        tracing::info!(source = "mcp_upstream", name = %server.name, "Connecting upstream...");
        if let Err(e) = registry
            .connect_upstream(server.clone(), Some(self_port))
            .await
        {
            tracing::warn!(source = "mcp_upstream", name = %server.name, "Boot-time connect failed: {e}");
        }
    }
    // All upstreams registered (async initialize may still be in flight). This
    // unblocks `await_initial_settle`, which then waits only for the in-flight
    // initializations to settle before serving the first tools/list.
    registry.mark_initial_connect_complete();

    tracing::info!(
        source = "mcp_upstream",
        "Boot-time upstream auto-connect complete"
    );
}

// ---------------------------------------------------------------------------
// Persistence (Tauri commands)
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct UpstreamMcpDelta {
    removed_ids: HashSet<String>,
    added: Vec<UpstreamMcpServer>,
    server_patches: HashMap<String, serde_json::Value>,
    desired_order: Option<Vec<String>>,
}

fn index_servers<'a>(
    config: &'a UpstreamMcpConfig,
    label: &str,
) -> Result<HashMap<&'a str, &'a UpstreamMcpServer>, String> {
    let mut by_id = HashMap::new();
    for server in &config.servers {
        if by_id.insert(server.id.as_str(), server).is_some() {
            return Err(format!(
                "Duplicate upstream id '{}' in {label} configuration",
                server.id
            ));
        }
    }
    Ok(by_id)
}

fn derive_upstream_delta(
    base: &UpstreamMcpConfig,
    desired: &UpstreamMcpConfig,
) -> Result<UpstreamMcpDelta, String> {
    let base_by_id = index_servers(base, "base")?;
    let desired_by_id = index_servers(desired, "requested")?;

    let removed_ids = base_by_id
        .keys()
        .filter(|id| !desired_by_id.contains_key::<str>(**id))
        .map(|id| (*id).to_string())
        .collect();
    let added = desired
        .servers
        .iter()
        .filter(|server| !base_by_id.contains_key(server.id.as_str()))
        .cloned()
        .collect::<Vec<_>>();

    let mut server_patches = HashMap::new();
    for server in &desired.servers {
        let Some(base_server) = base_by_id.get(server.id.as_str()) else {
            continue;
        };
        let base_json = serde_json::to_value(*base_server)
            .map_err(|e| format!("Could not serialize base upstream '{}': {e}", server.id))?;
        let desired_json = serde_json::to_value(server).map_err(|e| {
            format!(
                "Could not serialize requested upstream '{}': {e}",
                server.id
            )
        })?;
        if let Some(delta) = crate::config::json_merge_delta(&base_json, &desired_json) {
            server_patches.insert(server.id.clone(), delta);
        }
    }

    let desired_ids = desired
        .servers
        .iter()
        .map(|server| server.id.clone())
        .collect::<Vec<_>>();
    let retained_base_order = base
        .servers
        .iter()
        .filter(|server| desired_by_id.contains_key(server.id.as_str()))
        .map(|server| server.id.as_str())
        .collect::<Vec<_>>();
    let retained_desired_order = desired
        .servers
        .iter()
        .filter(|server| base_by_id.contains_key(server.id.as_str()))
        .map(|server| server.id.as_str())
        .collect::<Vec<_>>();
    let desired_order =
        (!added.is_empty() || retained_base_order != retained_desired_order).then_some(desired_ids);

    Ok(UpstreamMcpDelta {
        removed_ids,
        added,
        server_patches,
        desired_order,
    })
}

impl UpstreamMcpDelta {
    fn apply(self, latest: &mut UpstreamMcpConfig) -> Result<(), String> {
        latest
            .servers
            .retain(|server| !self.removed_ids.contains(&server.id));

        for server in &mut latest.servers {
            let Some(patch) = self.server_patches.get(&server.id) else {
                continue;
            };
            let mut json = serde_json::to_value(&*server)
                .map_err(|e| format!("Could not serialize upstream '{}': {e}", server.id))?;
            crate::config::merge_json_value(&mut json, patch.clone());
            *server = serde_json::from_value(json)
                .map_err(|e| format!("Invalid upstream delta for '{}': {e}", server.id))?;
        }

        for server in self.added {
            if latest.servers.iter().any(|current| current.id == server.id) {
                return Err(format!(
                    "Upstream id '{}' was added concurrently; reload before adding it again",
                    server.id
                ));
            }
            latest.servers.push(server);
        }

        if let Some(desired_order) = self.desired_order {
            let current_order = latest
                .servers
                .iter()
                .map(|server| server.id.clone())
                .collect::<Vec<_>>();
            let mut by_id = latest
                .servers
                .drain(..)
                .map(|server| (server.id.clone(), server))
                .collect::<HashMap<_, _>>();
            for id in desired_order.into_iter().chain(current_order) {
                if let Some(server) = by_id.remove(&id) {
                    latest.servers.push(server);
                }
            }
            // Defensive only: duplicate IDs are rejected before deriving a delta, but
            // retain any unforeseen entry rather than silently deleting user config.
            latest.servers.extend(by_id.into_values());
        }

        Ok(())
    }
}

fn persist_upstream_delta(
    base: &UpstreamMcpConfig,
    desired: &UpstreamMcpConfig,
    self_port: u16,
) -> Result<(UpstreamMcpConfig, UpstreamMcpConfig), String> {
    let delta = derive_upstream_delta(base, desired)?;
    ConfigFile::<UpstreamMcpConfig>::new(UPSTREAMS_FILE).update_with_strict(move |latest| {
        index_servers(latest, "current")?;
        let old = latest.clone();
        delta.apply(latest)?;
        let errors = validate_upstream_config(latest, self_port);
        if !errors.is_empty() {
            return Err(format!(
                "Invalid upstream config: {}",
                errors
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("; ")
            ));
        }
        let new = latest.clone();
        let changed = old != new;
        Ok(((old, new), changed))
    })
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn load_mcp_upstreams() -> UpstreamMcpConfig {
    load_json_config(UPSTREAMS_FILE)
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn save_mcp_upstreams(
    base: UpstreamMcpConfig,
    config: UpstreamMcpConfig,
    state: tauri::State<'_, std::sync::Arc<crate::state::AppState>>,
) -> Result<(), String> {
    save_mcp_upstreams_inner(base, config, state.inner()).await
}

pub(crate) async fn save_mcp_upstreams_inner(
    base: UpstreamMcpConfig,
    config: UpstreamMcpConfig,
    state: &crate::state::AppState,
) -> Result<(), String> {
    let self_port = state.config.read().services.server.port;
    persist_and_apply_upstream_delta(
        base,
        config,
        self_port,
        &state.mcp.upstream_registry,
        || std::future::ready(()),
    )
    .await
}

async fn persist_and_apply_upstream_delta<F, Fut>(
    base: UpstreamMcpConfig,
    config: UpstreamMcpConfig,
    self_port: u16,
    registry: &crate::mcp_proxy::registry::UpstreamRegistry,
    before_apply: F,
) -> Result<(), String>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    let _sequence = UPSTREAM_SAVE_SEQUENCE.lock().await;
    let (old_config, config) =
        tokio::task::spawn_blocking(move || persist_upstream_delta(&base, &config, self_port))
            .await
            .map_err(|e| format!("Upstream config save task failed: {e}"))??;

    // The test hook deterministically pressures the persist/apply boundary. In
    // production it is an immediately-ready future.
    before_apply().await;

    // Apply the exact locked pre/post diff to the live registry (no server restart
    // needed). The blocking file lock is released, while the async sequence lock
    // keeps this diff ordered with later commits from this process.
    registry
        .apply_config_diff(&old_config, &config, self_port)
        .await;

    Ok(())
}

/// Persist a DCR-obtained (or manually set) auth config for a single upstream.
/// Loads the config file, patches the matching entry, and writes it back
/// atomically. Other upstreams are left untouched.
pub(crate) fn update_upstream_auth(name: &str, auth: UpstreamAuth) -> Result<(), String> {
    set_upstream_auth(name, Some(auth))
}

/// Clear persisted auth for a single upstream (e.g. when transport URL changes
/// and a DCR-obtained client_id is stale).
pub(crate) fn clear_upstream_auth(name: &str) -> Result<(), String> {
    set_upstream_auth(name, None)
}

fn set_upstream_auth(name: &str, auth: Option<UpstreamAuth>) -> Result<(), String> {
    let found =
        ConfigFile::<UpstreamMcpConfig>::new(UPSTREAMS_FILE).update_with_strict(|config| {
            match config.servers.iter_mut().find(|s| s.name == name) {
                Some(entry) => {
                    entry.auth = auth;
                    Ok((true, true))
                }
                None => Ok((false, false)),
            }
        })?;
    if !found {
        return Err(format!("Upstream '{name}' not found in {UPSTREAMS_FILE}"));
    }
    Ok(())
}

/// Set per-project upstream MCP allowlist. `None` clears the override
/// (inherits all globally-enabled servers). Persists to repo-settings.json
/// and emits a tool-list refresh so connected MCP clients see the change.
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn set_project_mcp_upstreams(
    repo_path: String,
    upstream_names: Option<Vec<String>>,
    state: tauri::State<'_, std::sync::Arc<crate::state::AppState>>,
) -> Result<(), String> {
    set_project_mcp_upstreams_inner(&state, &repo_path, upstream_names)
}

/// Testable inner function (no Tauri state wrapper).
pub(crate) fn set_project_mcp_upstreams_inner(
    state: &crate::state::AppState,
    repo_path: &str,
    upstream_names: Option<Vec<String>>,
) -> Result<(), String> {
    let mut settings = crate::config::load_repo_settings();
    let base = settings.clone();
    let entry = settings
        .repos
        .entry(repo_path.to_string())
        .or_insert_with(|| crate::config::RepoSettingsEntry {
            path: repo_path.to_string(),
            ..Default::default()
        });
    entry.mcp_upstreams = upstream_names;
    crate::config::save_repo_settings(base, settings)?;
    // Notify connected MCP clients that the tool list may have changed
    let _ = state.mcp.tools_changed.send(());
    Ok(())
}

/// Reconnect a single upstream by name (disconnect + connect).
///
/// Useful when credentials change or the upstream is temporarily unreachable.
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn reconnect_mcp_upstream(
    name: String,
    state: tauri::State<'_, std::sync::Arc<crate::state::AppState>>,
) -> Result<(), String> {
    let config: UpstreamMcpConfig = load_json_config(UPSTREAMS_FILE);
    let self_port = state.config.read().services.server.port;

    // Validate before connecting
    let errors = validate_upstream_config(&config, self_port);
    if !errors.is_empty() {
        let msgs: Vec<String> = errors.iter().map(|e| e.to_string()).collect();
        return Err(msgs.join("; "));
    }

    let server = config
        .servers
        .into_iter()
        .find(|s| s.name == name)
        .ok_or_else(|| format!("Upstream '{name}' not found in config"))?;

    let registry = &state.mcp.upstream_registry;

    // Emit reconnecting event so the UI can show feedback
    registry.emit_status_change(&name, "connecting");

    // Disconnect if currently registered (ignore error if not present)
    let _ = registry.disconnect_upstream(&name);

    // Reconnect
    registry.connect_upstream(server, Some(self_port)).await
}

/// Returns a JSON snapshot of all upstream statuses, tool lists, and metrics.
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn get_mcp_upstream_status(
    state: tauri::State<'_, std::sync::Arc<crate::state::AppState>>,
) -> serde_json::Value {
    state.mcp.upstream_registry.status_snapshot()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_headers_reject_injection_duplicates_and_bearer_collision() {
        for names in [
            vec![""],
            vec!["x key"],
            vec!["x\r\nkey"],
            vec!["x-key", "X-Key"],
            vec!["Authorization"],
            vec!["Host"],
            vec!["Mcp-Session-Id"],
        ] {
            let mut server = http_server("headers", "https://example.com/mcp");
            server.auth = Some(UpstreamAuth::Bearer {
                token: String::new(),
            });
            server.headers = names
                .into_iter()
                .map(|name| UpstreamHeader {
                    name: name.into(),
                    credential_ref: uuid::Uuid::new_v4().to_string(),
                })
                .collect();
            assert!(
                !validate_upstream_config(
                    &UpstreamMcpConfig {
                        servers: vec![server]
                    },
                    9876
                )
                .is_empty()
            );
        }
    }

    #[test]
    fn legacy_bearer_and_oauth_configs_need_no_header_migration() {
        for auth in [
            serde_json::json!({"type":"bearer","token":""}),
            serde_json::json!({"type":"oauth2","client_id":"public"}),
        ] {
            let server: UpstreamMcpServer = serde_json::from_value(serde_json::json!({"id":"legacy", "name":"legacy", "transport":{"type":"http","url":"https://example.com/mcp"}, "auth":auth})).unwrap();
            assert!(server.headers.is_empty());
            assert!(
                validate_upstream_config(
                    &UpstreamMcpConfig {
                        servers: vec![server]
                    },
                    9876
                )
                .is_empty()
            );
        }
    }

    #[test]
    #[serial_test::serial]
    fn secret_header_values_never_enter_persisted_config_or_validation_errors() {
        let tmp = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _guard = crate::config::set_config_dir_override(tmp.path().to_path_buf());
        let header = UpstreamHeader {
            name: "x-api-key".into(),
            credential_ref: uuid::Uuid::new_v4().to_string(),
        };
        let sentinel = "DUMMY_PERSISTENCE_SENTINEL";
        crate::mcp_upstream_credentials::save_mcp_upstream_credential(
            "header-persistence".into(),
            sentinel.into(),
            Some(header.clone()),
        )
        .unwrap();
        let mut server = http_server("header-persistence", "https://example.com/mcp");
        server.enabled = false;
        server.headers = vec![header.clone()];
        persist_upstream_delta(
            &UpstreamMcpConfig::default(),
            &UpstreamMcpConfig {
                servers: vec![server],
            },
            9876,
        )
        .unwrap();
        let disk = std::fs::read_to_string(tmp.path().join(UPSTREAMS_FILE)).unwrap();
        assert!(!disk.contains(sentinel));
        assert!(disk.contains("x-api-key"));
        assert!(disk.contains(&header.credential_ref));
        assert_eq!(
            crate::mcp_upstream_credentials::read_upstream_credential(
                &header.credential_key("header-persistence")
            )
            .unwrap()
            .as_deref(),
            Some(sentinel)
        );
        let error = crate::mcp_upstream_credentials::save_mcp_upstream_credential(
            "header-persistence".into(),
            format!("{sentinel}\r\ninjected"),
            Some(header.clone()),
        )
        .unwrap_err();
        assert!(!error.contains(sentinel));
        crate::mcp_upstream_credentials::delete_mcp_upstream_credential(
            "header-persistence".into(),
            Some(header.clone()),
        )
        .unwrap();
        assert!(
            crate::mcp_upstream_credentials::read_upstream_credential(
                &header.credential_key("header-persistence")
            )
            .unwrap()
            .is_none()
        );
    }

    fn http_server(name: &str, url: &str) -> UpstreamMcpServer {
        UpstreamMcpServer {
            id: format!("id-{name}"),
            name: name.to_string(),
            transport: UpstreamTransport::Http {
                url: url.to_string(),
            },
            enabled: true,
            timeout_secs: 30,
            headers: vec![],
            tool_filter: None,
            auth: None,
        }
    }

    fn stdio_server(name: &str, command: &str) -> UpstreamMcpServer {
        UpstreamMcpServer {
            id: format!("id-{name}"),
            name: name.to_string(),
            transport: UpstreamTransport::Stdio {
                command: command.to_string(),
                args: vec![
                    "-y".to_string(),
                    "@modelcontextprotocol/server-filesystem".to_string(),
                ],
                env: HashMap::new(),
                cwd: None,
            },
            enabled: true,
            timeout_secs: 30,
            headers: vec![],
            tool_filter: None,
            auth: None,
        }
    }

    // -- Serialization round-trip --

    #[test]
    fn http_server_round_trip() {
        let server = http_server("github", "http://localhost:8080/mcp");
        let json = serde_json::to_string_pretty(&server).unwrap();
        let parsed: UpstreamMcpServer = serde_json::from_str(&json).unwrap();
        assert_eq!(server, parsed);
    }

    #[test]
    fn stdio_server_round_trip() {
        let server = stdio_server("filesystem", "npx");
        let json = serde_json::to_string_pretty(&server).unwrap();
        let parsed: UpstreamMcpServer = serde_json::from_str(&json).unwrap();
        assert_eq!(server, parsed);
    }

    #[test]
    fn config_round_trip() {
        let config = UpstreamMcpConfig {
            servers: vec![
                http_server("github", "http://localhost:8080/mcp"),
                stdio_server("filesystem", "npx"),
            ],
        };
        let json = serde_json::to_string_pretty(&config).unwrap();
        let parsed: UpstreamMcpConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config, parsed);
    }

    #[test]
    fn empty_config_round_trip() {
        let config = UpstreamMcpConfig::default();
        let json = serde_json::to_string_pretty(&config).unwrap();
        let parsed: UpstreamMcpConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config, parsed);
        assert!(parsed.servers.is_empty());
    }

    #[test]
    fn defaults_applied_on_deserialize() {
        // Minimal JSON without optional fields
        let json = r#"{
            "id": "abc",
            "name": "test",
            "transport": { "type": "http", "url": "http://example.com/mcp" }
        }"#;
        let server: UpstreamMcpServer = serde_json::from_str(json).unwrap();
        assert!(server.enabled); // default_true
        assert_eq!(server.timeout_secs, 30); // default_timeout
        assert!(server.tool_filter.is_none());
        assert!(server.auth.is_none());
    }

    #[test]
    fn tool_filter_round_trip() {
        let server = UpstreamMcpServer {
            id: "id-filtered".to_string(),
            name: "filtered".to_string(),
            transport: UpstreamTransport::Http {
                url: "http://localhost:9000/mcp".to_string(),
            },
            enabled: true,
            timeout_secs: 60,
            headers: vec![],
            tool_filter: Some(ToolFilter {
                mode: FilterMode::Deny,
                patterns: vec!["dangerous_*".to_string(), "admin_*".to_string()],
            }),
            auth: None,
        };
        let json = serde_json::to_string_pretty(&server).unwrap();
        let parsed: UpstreamMcpServer = serde_json::from_str(&json).unwrap();
        assert_eq!(server, parsed);
        let filter = parsed.tool_filter.unwrap();
        assert_eq!(filter.mode, FilterMode::Deny);
        assert_eq!(filter.patterns.len(), 2);
    }

    // -- Auth serialization --

    #[test]
    fn bearer_auth_round_trip() {
        let mut server = http_server("secure", "https://remote:443/mcp");
        server.auth = Some(UpstreamAuth::Bearer {
            token: "sk-test-123".to_string(),
        });
        let json = serde_json::to_string_pretty(&server).unwrap();
        let parsed: UpstreamMcpServer = serde_json::from_str(&json).unwrap();
        assert_eq!(server, parsed);
        assert!(matches!(parsed.auth, Some(UpstreamAuth::Bearer { .. })));
    }

    #[test]
    fn oauth2_auth_round_trip() {
        let mut server = http_server("oauth", "https://remote:443/mcp");
        server.auth = Some(UpstreamAuth::OAuth2 {
            client_id: "my-app".to_string(),
            client_secret: None,
            scopes: vec!["read".to_string(), "write".to_string()],
            authorization_endpoint: Some("https://auth.example.com/authorize".to_string()),
            token_endpoint: Some("https://auth.example.com/token".to_string()),
        });
        let json = serde_json::to_string_pretty(&server).unwrap();
        let parsed: UpstreamMcpServer = serde_json::from_str(&json).unwrap();
        assert_eq!(server, parsed);
        if let Some(UpstreamAuth::OAuth2 {
            client_id, scopes, ..
        }) = &parsed.auth
        {
            assert_eq!(client_id, "my-app");
            assert_eq!(scopes.len(), 2);
        } else {
            panic!("Expected OAuth2 auth variant");
        }
    }

    #[test]
    fn oauth2_serializes_with_oauth2_tag() {
        // The wire tag MUST be `oauth2` — the frontend's UpstreamAuth type and the
        // Authorize-button gate compare against this exact string. Regressing to the
        // snake_case-derived `o_auth2` silently hides the Authorize action.
        let mut server = http_server("oauth", "https://remote:443/mcp");
        server.auth = Some(UpstreamAuth::OAuth2 {
            client_id: "my-app".to_string(),
            client_secret: None,
            scopes: vec![],
            authorization_endpoint: None,
            token_endpoint: None,
        });
        let json = serde_json::to_string(&server).unwrap();
        assert!(
            json.contains(r#""type":"oauth2""#),
            "expected oauth2 tag, got: {json}"
        );
        assert!(
            !json.contains("o_auth2"),
            "must not emit legacy o_auth2 tag: {json}"
        );
    }

    #[test]
    fn oauth2_reads_legacy_o_auth2_tag() {
        // Config files written before the rename use `o_auth2`; the serde alias keeps
        // them loadable (they migrate to `oauth2` on the next save).
        let json = r#"{
            "id": "abc",
            "name": "legacy",
            "transport": { "type": "http", "url": "https://remote:443/mcp" },
            "auth": { "type": "o_auth2", "client_id": "my-app" }
        }"#;
        let server: UpstreamMcpServer = serde_json::from_str(json).unwrap();
        assert!(matches!(
            server.auth,
            Some(UpstreamAuth::OAuth2 { ref client_id, .. }) if client_id == "my-app"
        ));
    }

    #[test]
    fn oauth2_minimal_round_trip() {
        let mut server = http_server("oauth-min", "https://remote:443/mcp");
        server.auth = Some(UpstreamAuth::OAuth2 {
            client_id: "my-app".to_string(),
            client_secret: None,
            scopes: vec![],
            authorization_endpoint: None,
            token_endpoint: None,
        });
        let json = serde_json::to_string_pretty(&server).unwrap();
        // Optional fields should be omitted
        assert!(!json.contains("authorization_endpoint"));
        assert!(!json.contains("token_endpoint"));
        let parsed: UpstreamMcpServer = serde_json::from_str(&json).unwrap();
        assert_eq!(server, parsed);
    }

    #[test]
    fn config_without_auth_deserializes_to_none() {
        let json = r#"{
            "id": "abc",
            "name": "test",
            "transport": { "type": "http", "url": "http://example.com/mcp" }
        }"#;
        let server: UpstreamMcpServer = serde_json::from_str(json).unwrap();
        assert!(server.auth.is_none());
    }

    #[test]
    fn auth_not_serialized_when_none() {
        let server = http_server("plain", "http://remote:8080/mcp");
        let json = serde_json::to_string_pretty(&server).unwrap();
        assert!(!json.contains("\"auth\""));
    }

    // -- Validation: auth --

    #[test]
    fn oauth2_empty_client_id_rejected() {
        let mut server = http_server("bad-oauth", "https://remote:443/mcp");
        server.auth = Some(UpstreamAuth::OAuth2 {
            client_id: String::new(),
            client_secret: None,
            scopes: vec![],
            authorization_endpoint: None,
            token_endpoint: None,
        });
        let config = UpstreamMcpConfig {
            servers: vec![server],
        };
        let errors = validate_upstream_config(&config, 3845);
        assert_eq!(errors.len(), 1);
        assert!(matches!(
            errors[0],
            UpstreamConfigError::EmptyOAuthClientId(_)
        ));
    }

    #[test]
    fn oauth2_valid_client_id_passes() {
        let mut server = http_server("good-oauth", "https://remote:443/mcp");
        server.auth = Some(UpstreamAuth::OAuth2 {
            client_id: "my-app".to_string(),
            client_secret: None,
            scopes: vec!["read".to_string()],
            authorization_endpoint: None,
            token_endpoint: None,
        });
        let config = UpstreamMcpConfig {
            servers: vec![server],
        };
        assert!(validate_upstream_config(&config, 3845).is_empty());
    }

    #[test]
    fn bearer_auth_passes_validation() {
        let mut server = http_server("bearer", "https://remote:443/mcp");
        server.auth = Some(UpstreamAuth::Bearer {
            token: "sk-123".to_string(),
        });
        let config = UpstreamMcpConfig {
            servers: vec![server],
        };
        assert!(validate_upstream_config(&config, 3845).is_empty());
    }

    // -- Validation: valid configs --

    #[test]
    fn valid_config_passes() {
        let config = UpstreamMcpConfig {
            servers: vec![
                http_server("github", "http://remote-host:8080/mcp"),
                stdio_server("filesystem", "npx"),
            ],
        };
        let errors = validate_upstream_config(&config, 3845);
        assert!(errors.is_empty(), "Expected no errors, got: {errors:?}");
    }

    #[test]
    fn valid_single_char_name() {
        let config = UpstreamMcpConfig {
            servers: vec![http_server("a", "http://remote:8080/mcp")],
        };
        assert!(validate_upstream_config(&config, 3845).is_empty());
    }

    #[test]
    fn valid_name_with_hyphens_underscores_digits() {
        let config = UpstreamMcpConfig {
            servers: vec![http_server("my-server_2", "http://remote:8080/mcp")],
        };
        assert!(validate_upstream_config(&config, 3845).is_empty());
    }

    // -- Validation: empty name --

    #[test]
    fn empty_name_rejected() {
        let config = UpstreamMcpConfig {
            servers: vec![http_server("", "http://remote:8080/mcp")],
        };
        let errors = validate_upstream_config(&config, 3845);
        assert_eq!(errors.len(), 1);
        assert!(matches!(errors[0], UpstreamConfigError::EmptyName(_)));
    }

    // -- Validation: invalid name characters --

    #[test]
    fn uppercase_name_rejected() {
        let config = UpstreamMcpConfig {
            servers: vec![http_server("GitHub", "http://remote:8080/mcp")],
        };
        let errors = validate_upstream_config(&config, 3845);
        assert_eq!(errors.len(), 1);
        assert!(matches!(errors[0], UpstreamConfigError::InvalidName(_)));
    }

    #[test]
    fn name_with_spaces_rejected() {
        let config = UpstreamMcpConfig {
            servers: vec![http_server("my server", "http://remote:8080/mcp")],
        };
        let errors = validate_upstream_config(&config, 3845);
        assert_eq!(errors.len(), 1);
        assert!(matches!(errors[0], UpstreamConfigError::InvalidName(_)));
    }

    #[test]
    fn name_with_dots_rejected() {
        let config = UpstreamMcpConfig {
            servers: vec![http_server("my.server", "http://remote:8080/mcp")],
        };
        let errors = validate_upstream_config(&config, 3845);
        assert_eq!(errors.len(), 1);
        assert!(matches!(errors[0], UpstreamConfigError::InvalidName(_)));
    }

    // -- Validation: duplicate names --

    #[test]
    fn duplicate_names_rejected() {
        let config = UpstreamMcpConfig {
            servers: vec![
                http_server("github", "http://host-a:8080/mcp"),
                http_server("github", "http://host-b:9090/mcp"),
            ],
        };
        let errors = validate_upstream_config(&config, 3845);
        assert_eq!(errors.len(), 1);
        assert!(matches!(errors[0], UpstreamConfigError::DuplicateName(_)));
    }

    // -- Validation: self-referential URL --

    #[test]
    fn self_referential_localhost_rejected() {
        let config = UpstreamMcpConfig {
            servers: vec![http_server("bad", "http://localhost:3845/mcp")],
        };
        let errors = validate_upstream_config(&config, 3845);
        assert_eq!(errors.len(), 1);
        assert!(matches!(
            errors[0],
            UpstreamConfigError::SelfReferentialUrl(_)
        ));
    }

    #[test]
    fn self_referential_127_rejected() {
        let config = UpstreamMcpConfig {
            servers: vec![http_server("bad", "http://127.0.0.1:3845/mcp")],
        };
        let errors = validate_upstream_config(&config, 3845);
        assert_eq!(errors.len(), 1);
        assert!(matches!(
            errors[0],
            UpstreamConfigError::SelfReferentialUrl(_)
        ));
    }

    #[test]
    fn self_referential_ipv6_rejected() {
        let config = UpstreamMcpConfig {
            servers: vec![http_server("bad", "http://[::1]:3845/mcp")],
        };
        let errors = validate_upstream_config(&config, 3845);
        assert_eq!(errors.len(), 1);
        assert!(matches!(
            errors[0],
            UpstreamConfigError::SelfReferentialUrl(_)
        ));
    }

    #[test]
    fn different_port_not_self_referential() {
        let config = UpstreamMcpConfig {
            servers: vec![http_server("ok", "http://localhost:9999/mcp")],
        };
        assert!(validate_upstream_config(&config, 3845).is_empty());
    }

    #[test]
    fn remote_host_not_self_referential() {
        let config = UpstreamMcpConfig {
            servers: vec![http_server("ok", "http://remote-host:3845/mcp")],
        };
        assert!(validate_upstream_config(&config, 3845).is_empty());
    }

    // -- Validation: empty URL / command --

    #[test]
    fn empty_url_rejected() {
        let config = UpstreamMcpConfig {
            servers: vec![http_server("bad", "")],
        };
        let errors = validate_upstream_config(&config, 3845);
        assert_eq!(errors.len(), 1);
        assert!(matches!(errors[0], UpstreamConfigError::EmptyUrl(_)));
    }

    #[test]
    fn empty_command_rejected() {
        let mut server = stdio_server("bad", "npx");
        if let UpstreamTransport::Stdio {
            ref mut command, ..
        } = server.transport
        {
            *command = String::new();
        }
        let config = UpstreamMcpConfig {
            servers: vec![server],
        };
        let errors = validate_upstream_config(&config, 3845);
        assert_eq!(errors.len(), 1);
        assert!(matches!(errors[0], UpstreamConfigError::EmptyCommand(_)));
    }

    // -- Validation: multiple errors --

    #[test]
    fn multiple_errors_collected() {
        let config = UpstreamMcpConfig {
            servers: vec![
                http_server("", "http://remote:8080/mcp"), // empty name
                http_server("BAD", ""),                    // invalid name + empty url
                http_server("ok", "http://localhost:3845/mcp"), // self-ref
            ],
        };
        let errors = validate_upstream_config(&config, 3845);
        assert!(
            errors.len() >= 3,
            "Expected at least 3 errors, got: {errors:?}"
        );
    }

    // -- Validation: invalid URL scheme --

    #[test]
    fn ftp_scheme_rejected() {
        let config = UpstreamMcpConfig {
            servers: vec![http_server("bad", "ftp://remote:8080/mcp")],
        };
        let errors = validate_upstream_config(&config, 3845);
        assert_eq!(errors.len(), 1);
        assert!(matches!(
            errors[0],
            UpstreamConfigError::InvalidUrlScheme(_)
        ));
    }

    #[test]
    fn file_scheme_rejected() {
        let config = UpstreamMcpConfig {
            servers: vec![http_server("bad", "file:///etc/passwd")],
        };
        let errors = validate_upstream_config(&config, 3845);
        assert_eq!(errors.len(), 1);
        assert!(matches!(
            errors[0],
            UpstreamConfigError::InvalidUrlScheme(_)
        ));
    }

    #[test]
    fn javascript_scheme_rejected() {
        let config = UpstreamMcpConfig {
            servers: vec![http_server("bad", "javascript:alert(1)")],
        };
        let errors = validate_upstream_config(&config, 3845);
        assert_eq!(errors.len(), 1);
        assert!(matches!(
            errors[0],
            UpstreamConfigError::InvalidUrlScheme(_)
        ));
    }

    #[test]
    fn http_scheme_accepted() {
        let config = UpstreamMcpConfig {
            servers: vec![http_server("ok", "http://remote:8080/mcp")],
        };
        assert!(validate_upstream_config(&config, 3845).is_empty());
    }

    #[test]
    fn https_scheme_accepted() {
        let config = UpstreamMcpConfig {
            servers: vec![http_server("ok", "https://remote:443/mcp")],
        };
        assert!(validate_upstream_config(&config, 3845).is_empty());
    }

    // -- is_self_referential edge cases --

    #[test]
    fn invalid_url_not_self_referential() {
        assert!(!is_self_referential("not-a-url", 3845));
    }

    #[test]
    fn https_localhost_with_matching_port() {
        assert!(is_self_referential("https://localhost:3845/mcp", 3845));
    }

    #[test]
    fn zero_zero_zero_zero_is_localhost() {
        assert!(is_self_referential("http://0.0.0.0:3845/mcp", 3845));
    }

    // -- Persistence (file I/O) --

    #[test]
    fn load_nonexistent_returns_default() {
        // load_json_config returns Default for missing files
        let config: UpstreamMcpConfig = load_json_config("nonexistent-mcp-upstreams-test.json");
        assert!(config.servers.is_empty());
    }

    // -- apply_config_diff (via UpstreamRegistry) --

    use crate::mcp_proxy::registry::UpstreamRegistry;

    fn disabled_http_server(name: &str, url: &str) -> UpstreamMcpServer {
        let mut s = http_server(name, url);
        s.enabled = false;
        s
    }

    #[tokio::test]
    async fn diff_connects_new_server() {
        let registry = UpstreamRegistry::new();
        let old = UpstreamMcpConfig { servers: vec![] };
        let new_server = disabled_http_server("alpha", "http://127.0.0.1:1/mcp");
        let new = UpstreamMcpConfig {
            servers: vec![new_server],
        };

        registry.apply_config_diff(&old, &new, 9999).await;

        assert_eq!(registry.upstream_names(), vec!["alpha"]);
    }

    #[tokio::test]
    async fn diff_disconnects_removed_server() {
        let registry = UpstreamRegistry::new();
        let server = disabled_http_server("beta", "http://127.0.0.1:1/mcp");
        registry
            .connect_upstream(server.clone(), None)
            .await
            .unwrap();

        let old = UpstreamMcpConfig {
            servers: vec![server],
        };
        let new = UpstreamMcpConfig { servers: vec![] };

        registry.apply_config_diff(&old, &new, 9999).await;

        assert!(registry.upstream_names().is_empty());
    }

    #[tokio::test]
    async fn diff_reconnects_changed_server() {
        let registry = UpstreamRegistry::new();
        let mut old_server = disabled_http_server("gamma", "http://127.0.0.1:1/mcp");
        registry
            .connect_upstream(old_server.clone(), None)
            .await
            .unwrap();

        let old = UpstreamMcpConfig {
            servers: vec![old_server.clone()],
        };

        // Change the URL → same id, different transport
        old_server.transport = UpstreamTransport::Http {
            url: "http://127.0.0.1:2/mcp".to_string(),
        };
        let new = UpstreamMcpConfig {
            servers: vec![old_server],
        };

        registry.apply_config_diff(&old, &new, 9999).await;

        // gamma should still be registered (disconnected + reconnected)
        assert!(registry.upstream_names().contains(&"gamma".to_string()));
    }

    #[tokio::test]
    async fn diff_unchanged_server_stays_connected() {
        let registry = UpstreamRegistry::new();
        let server = disabled_http_server("delta", "http://127.0.0.1:1/mcp");
        registry
            .connect_upstream(server.clone(), None)
            .await
            .unwrap();

        let old = UpstreamMcpConfig {
            servers: vec![server.clone()],
        };
        let new = UpstreamMcpConfig {
            servers: vec![server],
        };

        registry.apply_config_diff(&old, &new, 9999).await;

        // Unchanged → still registered once (no double-connect)
        assert_eq!(registry.upstream_names(), vec!["delta"]);
    }

    #[tokio::test]
    async fn diff_reconnects_on_auth_change() {
        let registry = UpstreamRegistry::new();
        let server = disabled_http_server("epsilon", "http://127.0.0.1:1/mcp");
        registry
            .connect_upstream(server.clone(), None)
            .await
            .unwrap();

        let old = UpstreamMcpConfig {
            servers: vec![server.clone()],
        };

        // Add bearer auth → should trigger reconnect
        let mut new_server = server;
        new_server.auth = Some(UpstreamAuth::Bearer {
            token: "tok".to_string(),
        });
        let new = UpstreamMcpConfig {
            servers: vec![new_server],
        };

        registry.apply_config_diff(&old, &new, 9999).await;

        // epsilon should still be registered (disconnected + reconnected)
        assert!(registry.upstream_names().contains(&"epsilon".to_string()));
    }

    #[test]
    fn save_and_load_round_trip_via_file() {
        use tempfile::TempDir;

        // We can't easily override config_dir() in tests, so we test the
        // serialization format is compatible with load_json_config's expectations.
        let config = UpstreamMcpConfig {
            servers: vec![
                http_server("github", "http://remote:8080/mcp"),
                stdio_server("filesystem", "npx"),
            ],
        };
        let json = serde_json::to_string_pretty(&config).unwrap();
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("mcp-upstreams.json");
        std::fs::write(&path, &json).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        let loaded: UpstreamMcpConfig = serde_json::from_str(&content).unwrap();
        assert_eq!(config, loaded);
    }

    #[test]
    #[serial_test::serial]
    fn stale_ui_delta_preserves_a_concurrent_oauth_auth_update() {
        let tmp = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(tmp.path().to_path_buf());
        let server = http_server("alpha", "https://a.example.com/mcp");
        let base = UpstreamMcpConfig {
            servers: vec![server.clone()],
        };
        ConfigFile::<UpstreamMcpConfig>::new(UPSTREAMS_FILE)
            .save(&base)
            .unwrap();

        // The popup changes only enabled from the stale base while OAuth/DCR writes
        // auth after that base was loaded but before the popup save reaches disk.
        let mut desired_server = server;
        desired_server.enabled = false;
        let desired = UpstreamMcpConfig {
            servers: vec![desired_server],
        };
        let concurrent_auth = UpstreamAuth::OAuth2 {
            client_id: "fresh-dcr-id".into(),
            client_secret: None,
            scopes: vec!["read".into()],
            authorization_endpoint: None,
            token_endpoint: None,
        };
        update_upstream_auth("alpha", concurrent_auth.clone()).unwrap();

        persist_upstream_delta(&base, &desired, 3845).expect("apply stale UI delta");

        let saved: UpstreamMcpConfig = load_json_config(UPSTREAMS_FILE);
        assert!(!saved.servers[0].enabled, "requested toggle was not saved");
        assert_eq!(
            saved.servers[0].auth,
            Some(concurrent_auth),
            "stale whole-document UI save erased the concurrent OAuth/DCR update"
        );
    }

    #[test]
    #[serial_test::serial]
    fn upstream_delta_applies_intentional_removal_without_losing_a_concurrent_addition() {
        let tmp = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(tmp.path().to_path_buf());
        let alpha = http_server("alpha", "https://a.example.com/mcp");
        let beta = http_server("beta", "https://b.example.com/mcp");
        let gamma = http_server("gamma", "https://g.example.com/mcp");
        let base = UpstreamMcpConfig {
            servers: vec![alpha.clone(), beta.clone()],
        };
        let desired = UpstreamMcpConfig {
            servers: vec![alpha],
        };
        let latest = UpstreamMcpConfig {
            servers: vec![base.servers[0].clone(), beta, gamma],
        };
        ConfigFile::<UpstreamMcpConfig>::new(UPSTREAMS_FILE)
            .save(&latest)
            .unwrap();

        persist_upstream_delta(&base, &desired, 3845).expect("apply removal delta");

        let saved: UpstreamMcpConfig = load_json_config(UPSTREAMS_FILE);
        let names = saved
            .servers
            .iter()
            .map(|server| server.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["alpha", "gamma"]);
    }

    #[test]
    #[serial_test::serial]
    fn upstream_delta_treats_removed_auth_key_as_an_intentional_clear() {
        let tmp = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(tmp.path().to_path_buf());
        let mut server = http_server("alpha", "https://a.example.com/mcp");
        server.auth = Some(UpstreamAuth::OAuth2 {
            client_id: "old-id".into(),
            client_secret: None,
            scopes: vec![],
            authorization_endpoint: None,
            token_endpoint: None,
        });
        let base = UpstreamMcpConfig {
            servers: vec![server.clone()],
        };
        let mut desired_server = server;
        desired_server.auth = None;
        let desired = UpstreamMcpConfig {
            servers: vec![desired_server],
        };
        ConfigFile::<UpstreamMcpConfig>::new(UPSTREAMS_FILE)
            .save(&base)
            .unwrap();

        persist_upstream_delta(&base, &desired, 3845).expect("apply auth clear");

        let saved: UpstreamMcpConfig = load_json_config(UPSTREAMS_FILE);
        assert!(saved.servers[0].auth.is_none());
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn concurrent_saves_keep_live_registry_in_disk_order() {
        let tmp = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(tmp.path().to_path_buf());
        let registry = std::sync::Arc::new(UpstreamRegistry::new());
        let initial = UpstreamMcpConfig::default();
        ConfigFile::<UpstreamMcpConfig>::new(UPSTREAMS_FILE)
            .save(&initial)
            .unwrap();

        let alpha = disabled_http_server("alpha", "http://127.0.0.1:1/mcp");
        let after_first = UpstreamMcpConfig {
            servers: vec![alpha],
        };
        let beta = disabled_http_server("beta", "http://127.0.0.1:2/mcp");
        let after_second = UpstreamMcpConfig {
            servers: vec![beta],
        };

        let first_persisted = std::sync::Arc::new(tokio::sync::Notify::new());
        let allow_first_apply = std::sync::Arc::new(tokio::sync::Notify::new());
        let first = tokio::spawn({
            let registry = std::sync::Arc::clone(&registry);
            let first_persisted = std::sync::Arc::clone(&first_persisted);
            let allow_first_apply = std::sync::Arc::clone(&allow_first_apply);
            let initial = initial.clone();
            let after_first = after_first.clone();
            async move {
                persist_and_apply_upstream_delta(
                    initial,
                    after_first,
                    3845,
                    &registry,
                    move || async move {
                        first_persisted.notify_one();
                        allow_first_apply.notified().await;
                    },
                )
                .await
            }
        });

        first_persisted.notified().await;
        assert_eq!(load_mcp_upstreams(), after_first);
        assert!(
            UPSTREAM_SAVE_SEQUENCE.try_lock().is_err(),
            "the first save released ordering before applying its live diff"
        );

        // Poll the second save while the first is paused after persistence. The
        // sequence mutex queues it before it can persist B -> C, preventing the
        // historical apply order C then stale B.
        let second = persist_and_apply_upstream_delta(
            after_first.clone(),
            after_second.clone(),
            3845,
            &registry,
            || std::future::ready(()),
        );
        tokio::pin!(second);
        tokio::select! {
            biased;
            result = &mut second => panic!("second save bypassed the first: {result:?}"),
            _ = tokio::task::yield_now() => {}
        }
        assert_eq!(
            load_mcp_upstreams(),
            after_first,
            "second save persisted before the first live diff was allowed"
        );

        allow_first_apply.notify_one();
        first.await.unwrap().unwrap();
        second.await.unwrap();

        let saved = load_mcp_upstreams();
        assert_eq!(saved, after_second);
        let mut live_names = registry.upstream_names();
        live_names.sort();
        let mut saved_names = saved
            .servers
            .iter()
            .map(|server| server.name.clone())
            .collect::<Vec<_>>();
        saved_names.sort();
        assert_eq!(
            live_names, saved_names,
            "live upstream registry diverged from the final on-disk config"
        );
    }

    // -- update_upstream_auth --

    #[test]
    #[serial_test::serial]
    fn update_upstream_auth_writes_auth_without_affecting_other_upstreams() {
        let tmp = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(tmp.path().to_path_buf());

        // Seed two upstreams — one with existing auth, one without
        let mut server_a = http_server("alpha", "https://a.example.com/mcp");
        server_a.auth = Some(UpstreamAuth::Bearer {
            token: "old-tok".into(),
        });
        let server_b = http_server("beta", "https://b.example.com/mcp");
        let config = UpstreamMcpConfig {
            servers: vec![server_a, server_b.clone()],
        };
        ConfigFile::<UpstreamMcpConfig>::new(UPSTREAMS_FILE)
            .save(&config)
            .unwrap();

        // Write OAuth auth to beta
        let new_auth = UpstreamAuth::OAuth2 {
            client_id: "dcr-obtained-id".into(),
            client_secret: None,
            scopes: vec![],
            authorization_endpoint: None,
            token_endpoint: None,
        };
        update_upstream_auth("beta", new_auth.clone()).unwrap();

        // Reload and verify
        let reloaded: UpstreamMcpConfig = load_json_config(UPSTREAMS_FILE);
        let alpha = reloaded.servers.iter().find(|s| s.name == "alpha").unwrap();
        let beta = reloaded.servers.iter().find(|s| s.name == "beta").unwrap();

        // alpha's auth must be untouched
        assert!(matches!(alpha.auth, Some(UpstreamAuth::Bearer { .. })));
        // beta must have the new OAuth2 auth
        assert_eq!(beta.auth, Some(new_auth));
    }

    #[test]
    #[serial_test::serial]
    fn update_upstream_auth_returns_err_for_unknown_name() {
        let tmp = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(tmp.path().to_path_buf());

        let config = UpstreamMcpConfig {
            servers: vec![http_server("alpha", "https://a.example.com/mcp")],
        };
        ConfigFile::<UpstreamMcpConfig>::new(UPSTREAMS_FILE)
            .save(&config)
            .unwrap();

        let auth = UpstreamAuth::OAuth2 {
            client_id: "x".into(),
            client_secret: None,
            scopes: vec![],
            authorization_endpoint: None,
            token_endpoint: None,
        };
        let result = update_upstream_auth("nonexistent", auth);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[test]
    #[serial_test::serial]
    fn clear_upstream_auth_removes_auth() {
        let tmp = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(tmp.path().to_path_buf());

        let mut server = http_server("delta", "https://d.example.com/mcp");
        server.auth = Some(UpstreamAuth::OAuth2 {
            client_id: "stale-id".into(),
            client_secret: None,
            scopes: vec![],
            authorization_endpoint: None,
            token_endpoint: None,
        });
        let config = UpstreamMcpConfig {
            servers: vec![server],
        };
        ConfigFile::<UpstreamMcpConfig>::new(UPSTREAMS_FILE)
            .save(&config)
            .unwrap();

        clear_upstream_auth("delta").unwrap();

        let reloaded: UpstreamMcpConfig = load_json_config(UPSTREAMS_FILE);
        let delta = reloaded.servers.iter().find(|s| s.name == "delta").unwrap();
        assert!(delta.auth.is_none());
    }

    #[test]
    #[serial_test::serial]
    fn update_upstream_auth_overwrites_existing_auth() {
        let tmp = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(tmp.path().to_path_buf());

        let mut server = http_server("gamma", "https://g.example.com/mcp");
        server.auth = Some(UpstreamAuth::OAuth2 {
            client_id: "old-id".into(),
            client_secret: None,
            scopes: vec!["read".into()],
            authorization_endpoint: None,
            token_endpoint: None,
        });
        let config = UpstreamMcpConfig {
            servers: vec![server],
        };
        ConfigFile::<UpstreamMcpConfig>::new(UPSTREAMS_FILE)
            .save(&config)
            .unwrap();

        let new_auth = UpstreamAuth::OAuth2 {
            client_id: "new-id".into(),
            client_secret: None,
            scopes: vec![],
            authorization_endpoint: None,
            token_endpoint: None,
        };
        update_upstream_auth("gamma", new_auth.clone()).unwrap();

        let reloaded: UpstreamMcpConfig = load_json_config(UPSTREAMS_FILE);
        let gamma = reloaded.servers.iter().find(|s| s.name == "gamma").unwrap();
        assert_eq!(gamma.auth, Some(new_auth));
    }

    #[test]
    #[serial_test::serial]
    fn set_project_mcp_upstreams_persists_and_emits_signal() {
        let tmp = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(tmp.path().to_path_buf());

        let state = crate::state::tests_support::make_test_app_state();
        let mut rx = state.mcp.tools_changed.subscribe();

        // Set allowlist
        set_project_mcp_upstreams_inner(
            &state,
            "/test/repo",
            Some(vec!["server-a".to_string(), "server-b".to_string()]),
        )
        .unwrap();

        // Verify persisted
        let settings = crate::config::load_repo_settings();
        let entry = settings
            .repos
            .get("/test/repo")
            .expect("entry should exist");
        assert_eq!(
            entry.mcp_upstreams,
            Some(vec!["server-a".to_string(), "server-b".to_string()])
        );

        // Verify signal emitted
        assert!(
            rx.try_recv().is_ok(),
            "mcp_tools_changed should have been emitted"
        );
    }

    #[test]
    #[serial_test::serial]
    fn set_project_mcp_upstreams_none_clears_override() {
        let tmp = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(tmp.path().to_path_buf());

        let state = crate::state::tests_support::make_test_app_state();

        // Set then clear
        set_project_mcp_upstreams_inner(&state, "/test/repo", Some(vec!["server-a".to_string()]))
            .unwrap();
        set_project_mcp_upstreams_inner(&state, "/test/repo", None).unwrap();

        let settings = crate::config::load_repo_settings();
        let entry = settings
            .repos
            .get("/test/repo")
            .expect("entry should exist");
        assert_eq!(entry.mcp_upstreams, None, "None should clear the override");
    }
}
