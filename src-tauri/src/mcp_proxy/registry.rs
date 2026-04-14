//! Upstream MCP registry — central hub that manages upstream connections,
//! aggregates their tools with namespace prefixes, and routes tool calls.
//!
//! # Architecture
//!
//! Each upstream server is tracked as an `UpstreamEntry` inside a `DashMap`.
//! The registry is stored in `AppState` as `Arc<UpstreamRegistry>` so it can be
//! shared across the HTTP handler layer and background tasks.
//!
//! # Tool namespace
//!
//! Tools are exposed with the prefix `{upstream_name}__{tool_name}`. Clients
//! call `aggregated_tools()` to get the full merged list, and `proxy_tool_call()`
//! to route a prefixed call to the correct upstream.
//!
//! # Circuit breaker (per upstream)
//!
//! - 3 consecutive failures → circuit opens (backoff starts at 1s, capped at 60s)
//! - After max 10 retries in Failed state, the entry is marked `Failed` permanently
//!   until the user re-enables/re-connects it.
//! - Background task runs `health_check` every 60s on every Ready upstream.

use crate::mcp_proxy::http_client::{HttpMcpClient, UpstreamError, UpstreamToolDef};
use crate::mcp_proxy::stdio_client::StdioMcpClient;
use crate::mcp_upstream_config::{FilterMode, UpstreamMcpServer, UpstreamTransport};
use dashmap::DashMap;
use serde_json::Value;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
// tokio::sync::RwLock used inline for UpstreamClient::Http

// ---------------------------------------------------------------------------
// Circuit breaker constants
// ---------------------------------------------------------------------------

/// Consecutive failures before the circuit opens.
const CB_THRESHOLD: u32 = 3;
/// Initial backoff when the circuit opens (1 second).
const CB_BASE_MS: f64 = 1_000.0;
/// Maximum backoff cap (60 seconds).
const CB_MAX_MS: f64 = 60_000.0;
/// After this many total retries from CircuitOpen, mark the entry Failed.
const CB_MAX_RETRIES: u32 = 10;

/// Background health check interval.
const HEALTH_CHECK_INTERVAL: Duration = Duration::from_secs(60);

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Lifecycle status of a single upstream connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum UpstreamStatus {
    /// Initial state while `initialize()` is in progress.
    Connecting,
    /// Handshake complete — tools are available.
    Ready,
    /// Circuit breaker has opened due to failures; will retry after backoff.
    CircuitOpen,
    /// Disabled by the user in config (`enabled: false`).
    Disabled,
    /// Permanently failed after `CB_MAX_RETRIES` consecutive circuit re-opens.
    Failed,
    /// OAuth flow in progress — tool calls are rejected with -32001.
    Authenticating,
}

/// Internal state protected by a single mutex to avoid race conditions
/// between failure_count, retry_count, and open_until writes.
struct CircuitBreakerState {
    failure_count: u32,
    retry_count: u32,
    open_until: Option<Instant>,
}

/// Per-upstream circuit breaker.
struct CircuitBreaker {
    state: parking_lot::Mutex<CircuitBreakerState>,
}

impl CircuitBreaker {
    fn new() -> Self {
        Self {
            state: parking_lot::Mutex::new(CircuitBreakerState {
                failure_count: 0,
                retry_count: 0,
                open_until: None,
            }),
        }
    }

    /// Returns `true` if the circuit is currently open (backoff active).
    fn is_open(&self) -> bool {
        self.state
            .lock()
            .open_until
            .map(|until| Instant::now() < until)
            .unwrap_or(false)
    }

    /// Record success — resets failure count and closes the circuit.
    fn record_success(&self) {
        let mut s = self.state.lock();
        s.failure_count = 0;
        s.retry_count = 0;
        s.open_until = None;
    }

    /// Record failure. Returns `true` if we have hit `CB_MAX_RETRIES`.
    fn record_failure(&self) -> bool {
        let mut s = self.state.lock();
        s.failure_count += 1;
        if s.failure_count >= CB_THRESHOLD {
            s.retry_count += 1;
            if s.retry_count > CB_MAX_RETRIES {
                return true; // caller should transition to Failed
            }
            // Backoff based on retry_count (circuit re-opens), not cumulative failures.
            // Reset failure_count so the next half-open window gets CB_THRESHOLD
            // fresh attempts before re-opening.
            let delay_ms = (CB_BASE_MS * 2_f64.powf(s.retry_count.saturating_sub(1) as f64)).min(CB_MAX_MS);
            s.open_until = Some(Instant::now() + Duration::from_millis(delay_ms as u64));
            s.failure_count = 0;
        }
        false
    }
}

/// Per-upstream call metrics (lock-free counters).
pub(crate) struct UpstreamMetrics {
    pub(crate) call_count: AtomicU32,
    pub(crate) error_count: AtomicU32,
    /// Last observed round-trip latency in milliseconds (0 = never called).
    pub(crate) last_latency_ms: AtomicU32,
}

impl UpstreamMetrics {
    fn new() -> Self {
        Self {
            call_count: AtomicU32::new(0),
            error_count: AtomicU32::new(0),
            last_latency_ms: AtomicU32::new(0),
        }
    }

    /// Snapshot metrics for serialization.
    pub(crate) fn snapshot(&self) -> serde_json::Value {
        serde_json::json!({
            "call_count": self.call_count.load(Ordering::Relaxed),
            "error_count": self.error_count.load(Ordering::Relaxed),
            "last_latency_ms": self.last_latency_ms.load(Ordering::Relaxed),
        })
    }
}

/// Client variant — wraps either transport behind a Mutex for interior mutability.
///
/// The stdio variant uses `Arc<std::sync::Mutex<…>>` so that the Arc can be
/// cloned into `spawn_blocking` closures without any unsafe lifetime extension.
pub(crate) enum UpstreamClient {
    Http(tokio::sync::RwLock<HttpMcpClient>),
    Stdio(Arc<std::sync::Mutex<StdioMcpClient>>),
}

/// One upstream server tracked in the registry.
pub(crate) struct UpstreamEntry {
    pub(crate) config: UpstreamMcpServer,
    pub(crate) status: parking_lot::RwLock<UpstreamStatus>,
    /// Cached tool list (set on successful initialize / health-check).
    pub(crate) tools: parking_lot::RwLock<Vec<UpstreamToolDef>>,
    pub(crate) metrics: UpstreamMetrics,
    client: UpstreamClient,
    cb: CircuitBreaker,
}

impl UpstreamEntry {
    fn new(config: UpstreamMcpServer, client: UpstreamClient) -> Self {
        let status = if config.enabled {
            UpstreamStatus::Connecting
        } else {
            UpstreamStatus::Disabled
        };
        Self {
            config,
            status: parking_lot::RwLock::new(status),
            tools: parking_lot::RwLock::new(Vec::new()),
            metrics: UpstreamMetrics::new(),
            client,
            cb: CircuitBreaker::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

/// Central registry for all upstream MCP connections.
pub(crate) struct UpstreamRegistry {
    /// `upstream_name` → entry
    entries: DashMap<String, Arc<UpstreamEntry>>,
    /// Broadcast bus for emitting upstream status change events (optional).
    event_bus: parking_lot::RwLock<Option<tokio::sync::broadcast::Sender<crate::state::AppEvent>>>,
    /// MCP tools_changed signal — fired when upstream tool availability changes.
    mcp_tools_tx: parking_lot::RwLock<Option<tokio::sync::broadcast::Sender<()>>>,
    /// Serializes concurrent OAuth flows so only one browser auth runs at a time.
    /// Shared with `OAuthFlowManager` — both hold the same `Arc`.
    pub(crate) auth_semaphore: Arc<tokio::sync::Semaphore>,
    /// OAuth flow orchestrator. Stored as `Weak` to avoid an `Arc` cycle since
    /// `AppState` holds both the registry and the flow manager as `Arc`s.
    oauth_flow: parking_lot::RwLock<
        Option<std::sync::Weak<crate::mcp_oauth::flow::OAuthFlowManager>>,
    >,
}

impl UpstreamRegistry {
    pub(crate) fn new() -> Self {
        Self {
            entries: DashMap::new(),
            event_bus: parking_lot::RwLock::new(None),
            mcp_tools_tx: parking_lot::RwLock::new(None),
            auth_semaphore: Arc::new(tokio::sync::Semaphore::new(1)),
            oauth_flow: parking_lot::RwLock::new(None),
        }
    }

    /// Wire the event bus so status changes emit SSE events.
    pub(crate) fn set_event_bus(&self, bus: tokio::sync::broadcast::Sender<crate::state::AppEvent>) {
        *self.event_bus.write() = Some(bus);
    }

    /// Wire the OAuth flow orchestrator. Called from `lib.rs` after `AppState`
    /// is assembled; stores a `Weak` to avoid an `Arc` cycle.
    pub(crate) fn set_oauth_flow_manager(
        &self,
        mgr: Arc<crate::mcp_oauth::flow::OAuthFlowManager>,
    ) {
        *self.oauth_flow.write() = Some(Arc::downgrade(&mgr));
    }

    fn oauth_flow(&self) -> Option<Arc<crate::mcp_oauth::flow::OAuthFlowManager>> {
        self.oauth_flow.read().as_ref().and_then(|w| w.upgrade())
    }

    /// Wire the MCP tools_changed signal so upstream changes notify MCP clients.
    pub(crate) fn set_mcp_tools_tx(&self, tx: tokio::sync::broadcast::Sender<()>) {
        *self.mcp_tools_tx.write() = Some(tx);
    }

    /// Emit an upstream status change event (fire-and-forget).
    pub(crate) fn emit_status_change(&self, name: &str, status: &str) {
        if let Some(bus) = self.event_bus.read().as_ref() {
            let _ = bus.send(crate::state::AppEvent::UpstreamStatusChanged {
                name: name.to_string(),
                status: status.to_string(),
            });
        }
        // Status changes to/from Ready affect the merged tool list.
        // Authenticating is transient — tools haven't changed, skip notification.
        let status_lower = status.to_ascii_lowercase();
        if matches!(status_lower.as_str(), "ready" | "error" | "disconnected" | "failed" | "circuit_open")
            && let Some(tx) = self.mcp_tools_tx.read().as_ref()
        {
            let _ = tx.send(());
        }
    }

    // -----------------------------------------------------------------------
    // Aggregation
    // -----------------------------------------------------------------------

    /// Returns all tools from `Ready` upstreams, prefixed with `{upstream}__`.
    ///
    /// Tool filter (allow / deny patterns) is applied per upstream config.
    pub(crate) fn aggregated_tools(&self) -> Vec<Value> {
        let mut result = Vec::new();
        for entry_ref in self.entries.iter() {
            let entry = entry_ref.value();
            if *entry.status.read() != UpstreamStatus::Ready {
                continue;
            }
            let tools = entry.tools.read();
            for tool in tools.iter() {
                if !apply_filter(&tool.original_name, &entry.config) {
                    continue;
                }
                let prefixed_name = format!("{}__{}", entry.config.name, tool.original_name);
                let mut def = tool.definition.clone();
                if let Some(obj) = def.as_object_mut() {
                    obj.insert("name".to_string(), Value::String(prefixed_name));
                    // Annotate description with upstream origin
                    let desc = obj
                        .get("description")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let annotated = format!("[via {}] {}", entry.config.name, desc);
                    obj.insert("description".to_string(), Value::String(annotated));
                }
                result.push(def);
            }
        }
        result
    }

    // -----------------------------------------------------------------------
    // Routing
    // -----------------------------------------------------------------------

    /// Route a `{upstream_name}__{tool_name}` call to the correct upstream.
    ///
    /// Returns the upstream's response or an error string.
    pub(crate) async fn proxy_tool_call(
        &self,
        prefixed_name: &str,
        args: Value,
    ) -> Result<Value, String> {
        let (upstream_name, tool_name) = split_prefixed_name(prefixed_name)?;

        let entry = self
            .entries
            .get(upstream_name)
            .ok_or_else(|| format!("Unknown upstream '{upstream_name}'"))?;

        let entry = Arc::clone(entry.value());

        // Re-apply the allow/deny filter at call time. Listing already excludes
        // filtered tools from discovery, but with the collapsed meta-tool API
        // an agent can call any tool by exact name — so the filter must also
        // gate dispatch, not just enumeration.
        if !apply_filter(tool_name, &entry.config) {
            return Err(format!(
                "Tool '{tool_name}' on upstream '{upstream_name}' is blocked by its allow/deny filter"
            ));
        }

        // Block calls while backoff is active, upstream permanently failed, or authenticating
        if entry.cb.is_open() {
            return Err(format!("Circuit open for upstream '{upstream_name}' (backoff active, will retry)"));
        }
        {
            let status = entry.status.read().clone();
            if status == UpstreamStatus::Failed {
                return Err(format!("Upstream '{upstream_name}' has failed — restart the server or reconnect"));
            }
            if status == UpstreamStatus::Authenticating {
                return Err(format!(
                    "{{\"code\":-32001,\"message\":\"Upstream '{upstream_name}' is awaiting OAuth authentication\"}}"
                ));
            }
        }

        entry.metrics.call_count.fetch_add(1, Ordering::Relaxed);
        let t0 = Instant::now();
        let result = dispatch_tool_call(&entry.client, tool_name, args).await;
        let elapsed_ms = t0.elapsed().as_millis().min(u32::MAX as u128) as u32;
        entry.metrics.last_latency_ms.store(elapsed_ms, Ordering::Relaxed);

        match result {
            Ok(v) => {
                entry.cb.record_success();
                // Recover from CircuitOpen → Ready on success
                let prev = entry.status.read().clone();
                if prev == UpstreamStatus::CircuitOpen {
                    *entry.status.write() = UpstreamStatus::Ready;
                    self.emit_status_change(upstream_name, "ready");
                }
                Ok(v)
            }
            Err(UpstreamError::NeedsOAuth { .. }) => {
                // Don't trip the circuit breaker — this is an auth state, not a failure.
                self.trigger_oauth_flow(&entry, upstream_name);
                Err(format!(
                    "{{\"code\":-32001,\"message\":\"Upstream '{upstream_name}' is awaiting OAuth authentication\"}}"
                ))
            }
            Err(e) => {
                entry.metrics.error_count.fetch_add(1, Ordering::Relaxed);
                let exhausted = entry.cb.record_failure();
                if exhausted {
                    *entry.status.write() = UpstreamStatus::Failed;
                    self.emit_status_change(upstream_name, "failed");
                } else if entry.cb.is_open() {
                    let prev = entry.status.read().clone();
                    if prev != UpstreamStatus::CircuitOpen {
                        *entry.status.write() = UpstreamStatus::CircuitOpen;
                        self.emit_status_change(upstream_name, "circuit_open");
                    }
                }
                Err(e.to_string())
            }
        }
    }

    /// Transition an upstream into `Authenticating` and spawn
    /// [`OAuthFlowManager::start_flow`] in the background. Emits a
    /// `McpOAuthStart` event so the frontend can open the browser. No-op when
    /// already authenticating or when the flow manager is not wired.
    fn trigger_oauth_flow(&self, entry: &Arc<UpstreamEntry>, name: &str) {
        let bus_snapshot = self.event_bus.read().clone();
        let flow_mgr = self.oauth_flow();
        start_oauth_flow(entry, name, bus_snapshot, flow_mgr);
    }

    /// Called by the OAuth command layer once tokens have been successfully
    /// exchanged and persisted. Transitions the upstream back to `Connecting`
    /// and spawns [`initialize_entry`] to resume the handshake.
    #[allow(dead_code)] // invoked from Tauri command layer (#1198)
    pub(crate) async fn on_oauth_complete(&self, upstream_name: &str) -> Result<(), String> {
        let entry = self
            .entries
            .get(upstream_name)
            .ok_or_else(|| format!("Unknown upstream '{upstream_name}'"))?;
        let entry = Arc::clone(entry.value());

        *entry.status.write() = UpstreamStatus::Connecting;
        self.emit_status_change(upstream_name, "connecting");

        let name = upstream_name.to_string();
        let bus_snapshot = self.event_bus.read().clone();
        let flow_snapshot = self.oauth_flow();
        tokio::spawn(async move {
            initialize_entry_with_oauth(&entry, &name, bus_snapshot.as_ref(), flow_snapshot).await;
        });
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Lifecycle
    // -----------------------------------------------------------------------

    /// Register and connect an upstream server.
    ///
    /// Returns an error if `name` collides with an existing entry or if the
    /// config is self-referential (proxy pointing to itself).
    pub(crate) async fn connect_upstream(
        &self,
        config: UpstreamMcpServer,
        self_port: Option<u16>,
    ) -> Result<(), String> {
        let name = config.name.clone();

        if self.entries.contains_key(&name) {
            return Err(format!("Upstream '{name}' already registered"));
        }

        // Circular proxy guard
        if let UpstreamTransport::Http { ref url } = config.transport
            && let Some(port) = self_port
            && crate::mcp_upstream_config::is_self_referential(url, port)
        {
            return Err(format!(
                "Upstream '{name}' points to TUIC itself — circular proxy"
            ));
        }

        let client = build_client(&name, &config)?;
        let entry = Arc::new(UpstreamEntry::new(config.clone(), client));
        self.entries.insert(name.clone(), Arc::clone(&entry));

        if !config.enabled {
            return Ok(());
        }

        // Run initialize in background (don't block the caller).
        // Snapshot the event bus sender and flow manager (if any) so the
        // spawned task can emit events and trigger OAuth without a reference
        // back to the registry.
        let entry_clone = Arc::clone(&entry);
        let name_clone = name.clone();
        let bus_snapshot = self.event_bus.read().clone();
        let flow_snapshot = self.oauth_flow();
        tokio::spawn(async move {
            initialize_entry_with_oauth(
                &entry_clone,
                &name_clone,
                bus_snapshot.as_ref(),
                flow_snapshot,
            )
            .await;
        });

        Ok(())
    }

    /// Remove an upstream and shut it down.
    pub(crate) fn disconnect_upstream(&self, name: &str) -> Result<(), String> {
        let (_, entry) = self
            .entries
            .remove(name)
            .ok_or_else(|| format!("Upstream '{name}' not found"))?;

        // Fire-and-forget shutdown for stdio (sync)
        if let UpstreamClient::Stdio(ref mutex) = entry.client
            && let Ok(mut client) = mutex.lock()
        {
            client.shutdown();
        }
        // HTTP clients don't hold persistent connections — nothing to clean up.

        Ok(())
    }

    /// Names of all registered upstreams.
    #[allow(dead_code)]
    pub(crate) fn upstream_names(&self) -> Vec<String> {
        self.entries.iter().map(|e| e.key().clone()).collect()
    }

    /// Status of a specific upstream.
    #[allow(dead_code)]
    pub(crate) fn status(&self, name: &str) -> Option<UpstreamStatus> {
        self.entries.get(name).map(|e| e.status.read().clone())
    }

    /// Returns a JSON snapshot of all upstream statuses and metrics.
    pub(crate) fn status_snapshot(&self) -> serde_json::Value {
        let upstreams: Vec<serde_json::Value> = self.entries.iter().map(|entry_ref| {
            let e = entry_ref.value();
            let status_str = match *e.status.read() {
                UpstreamStatus::Connecting => "connecting",
                UpstreamStatus::Ready => "ready",
                UpstreamStatus::CircuitOpen => "circuit_open",
                UpstreamStatus::Disabled => "disabled",
                UpstreamStatus::Failed => "failed",
                UpstreamStatus::Authenticating => "authenticating",
            };
            let transport_info = match &e.config.transport {
                UpstreamTransport::Http { url } => serde_json::json!({
                    "type": "http",
                    "url": url,
                }),
                UpstreamTransport::Stdio { command, args, cwd, .. } => serde_json::json!({
                    "type": "stdio",
                    "command": command,
                    "args": args,
                    "cwd": cwd,
                }),
            };
            let tools_read = e.tools.read();
            let tool_names: Vec<&str> = tools_read.iter()
                .map(|t| t.original_name.as_str())
                .collect();
            serde_json::json!({
                "name": entry_ref.key(),
                "status": status_str,
                "transport": transport_info,
                "tool_count": tools_read.len(),
                "tools": tool_names,
                "metrics": e.metrics.snapshot(),
            })
        }).collect();
        serde_json::json!({ "upstreams": upstreams })
    }

    // -----------------------------------------------------------------------
    // Background health check
    // -----------------------------------------------------------------------

    // -----------------------------------------------------------------------
    // Config diff (hot-reload)
    // -----------------------------------------------------------------------

    /// Apply a config diff to the live registry without restarting the server.
    ///
    /// - Removed servers → disconnect
    /// - Added servers → connect
    /// - Changed servers (same id, different config) → disconnect + reconnect
    pub(crate) async fn apply_config_diff(
        &self,
        old: &crate::mcp_upstream_config::UpstreamMcpConfig,
        new: &crate::mcp_upstream_config::UpstreamMcpConfig,
        self_port: u16,
    ) {
        use std::collections::HashMap;

        let old_by_id: HashMap<&str, &crate::mcp_upstream_config::UpstreamMcpServer> =
            old.servers.iter().map(|s| (s.id.as_str(), s)).collect();
        let new_by_id: HashMap<&str, &crate::mcp_upstream_config::UpstreamMcpServer> =
            new.servers.iter().map(|s| (s.id.as_str(), s)).collect();

        // Disconnect removed or changed servers
        for (id, old_server) in &old_by_id {
            let should_disconnect = match new_by_id.get(id) {
                None => true, // removed
                Some(new_server) => {
                    old_server.name != new_server.name
                        || old_server.transport != new_server.transport
                        || old_server.enabled != new_server.enabled
                        || old_server.timeout_secs != new_server.timeout_secs
                        || old_server.tool_filter != new_server.tool_filter
                        || old_server.auth != new_server.auth
                }
            };
            if should_disconnect {
                let _ = self.disconnect_upstream(&old_server.name);
            }
        }

        // Connect new or changed servers
        for new_server in &new.servers {
            let old = old_by_id.get(new_server.id.as_str());
            let should_connect = match old {
                None => true, // new
                Some(old_server) => {
                    old_server.name != new_server.name
                        || old_server.transport != new_server.transport
                        || old_server.enabled != new_server.enabled
                        || old_server.timeout_secs != new_server.timeout_secs
                        || old_server.tool_filter != new_server.tool_filter
                        || old_server.auth != new_server.auth
                }
            };
            if should_connect
                && let Err(e) = self.connect_upstream(new_server.clone(), Some(self_port)).await
            {
                tracing::error!(source = "mcp_registry", name = %new_server.name, "Failed to connect upstream: {e}");
            }
        }
    }

    // -----------------------------------------------------------------------
    // Background health check
    // -----------------------------------------------------------------------

    /// Spawn the background health-check task.
    ///
    /// The task runs every `HEALTH_CHECK_INTERVAL` and calls `health_check()`
    /// on every `Ready` upstream. Failures update the circuit breaker.
    pub(crate) fn spawn_health_checker(registry: Arc<UpstreamRegistry>) {
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(HEALTH_CHECK_INTERVAL).await;
                run_health_checks(&registry).await;
            }
        });
    }
}

// ---------------------------------------------------------------------------
// Helpers (module-private)
// ---------------------------------------------------------------------------

/// Build the appropriate client variant from the upstream config.
fn build_client(name: &str, config: &UpstreamMcpServer) -> Result<UpstreamClient, String> {
    match &config.transport {
        UpstreamTransport::Http { .. } => {
            let client = HttpMcpClient::from_config(
                name.to_string(),
                &config.transport,
                config.timeout_secs,
            )
            .ok_or_else(|| format!("Failed to build HTTP client for '{name}'"))?;
            Ok(UpstreamClient::Http(tokio::sync::RwLock::new(client)))
        }
        UpstreamTransport::Stdio { .. } => {
            let client = StdioMcpClient::from_upstream_config(name.to_string(), &config.transport)
                .ok_or_else(|| format!("Failed to build stdio client for '{name}'"))?;
            Ok(UpstreamClient::Stdio(Arc::new(std::sync::Mutex::new(client))))
        }
    }
}

/// Parse `{upstream}__{tool}` — returns `(upstream_name, tool_name)`.
fn split_prefixed_name(prefixed: &str) -> Result<(&str, &str), String> {
    prefixed
        .split_once("__")
        .ok_or_else(|| format!("Tool name '{prefixed}' lacks upstream prefix (expected '{{upstream}}__{{tool}}')"))
}

/// Check whether a tool passes the upstream's allow/deny filter.
fn apply_filter(tool_name: &str, config: &UpstreamMcpServer) -> bool {
    let Some(ref filter) = config.tool_filter else {
        return true; // no filter → allow all
    };
    let matches = filter.patterns.iter().any(|p| {
        // Simple glob: trailing `*` prefix match, else exact match
        if let Some(prefix) = p.strip_suffix('*') {
            tool_name.starts_with(prefix)
        } else {
            tool_name == p
        }
    });
    match filter.mode {
        FilterMode::Allow => matches,
        FilterMode::Deny => !matches,
    }
}

/// Dispatch a tool call to the concrete client type.
async fn dispatch_tool_call(
    client: &UpstreamClient,
    tool_name: &str,
    args: Value,
) -> Result<Value, UpstreamError> {
    match client {
        UpstreamClient::Http(rwlock) => {
            // Try with read lock first — allows concurrent tool calls
            let result = {
                let guard = rwlock.read().await;
                guard.call_tool(tool_name, args.clone()).await
            };
            match result {
                Ok(val) => Ok(val),
                Err(UpstreamError::NeedsOAuth { www_authenticate }) => {
                    Err(UpstreamError::NeedsOAuth { www_authenticate })
                }
                Err(UpstreamError::AuthFailed) => Err(UpstreamError::AuthFailed),
                Err(UpstreamError::Other(e_str)) => {
                    if e_str.contains("400")
                        || e_str.contains("connection")
                        || e_str.contains("session")
                    {
                        // Reconnectable error — take exclusive lock for initialize + retry
                        let mut guard = rwlock.write().await;
                        if let Err(ie) = guard.initialize().await {
                            // Surface the original error class too, but preserve NeedsOAuth if it surfaced on reconnect.
                            return match ie {
                                UpstreamError::NeedsOAuth { www_authenticate } => {
                                    Err(UpstreamError::NeedsOAuth { www_authenticate })
                                }
                                other => Err(UpstreamError::Other(format!(
                                    "Upstream reconnect failed: {other} (original: {e_str})"
                                ))),
                            };
                        }
                        guard.call_tool(tool_name, args).await
                    } else {
                        Err(UpstreamError::Other(e_str))
                    }
                }
            }
        }
        UpstreamClient::Stdio(mutex) => {
            // StdioMcpClient is sync — clone the Arc and run in a blocking thread.
            let arc = Arc::clone(mutex);
            let tool_name = tool_name.to_string();
            tokio::task::spawn_blocking(move || {
                let mut guard = arc
                    .lock()
                    .map_err(|e| format!("Stdio client mutex poisoned: {e}"))?;
                guard.call_tool(&tool_name, args)
            })
            .await
            .map_err(|e| UpstreamError::Other(format!("spawn_blocking panicked: {e}")))?
            .map_err(UpstreamError::Other)
        }
    }
}

/// Fire the OAuth flow for an upstream in the background (shared helper).
///
/// Transitions status to `Authenticating`, emits `UpstreamStatusChanged`
/// immediately via the supplied bus, and spawns the manager's `start_flow`.
/// On success, emits `McpOAuthStart` with the browser URL.
fn start_oauth_flow(
    entry: &Arc<UpstreamEntry>,
    name: &str,
    bus: Option<tokio::sync::broadcast::Sender<crate::state::AppEvent>>,
    flow_mgr: Option<Arc<crate::mcp_oauth::flow::OAuthFlowManager>>,
) {
    {
        let mut status = entry.status.write();
        if *status == UpstreamStatus::Authenticating {
            return;
        }
        *status = UpstreamStatus::Authenticating;
    }
    if let Some(ref sender) = bus {
        let _ = sender.send(crate::state::AppEvent::UpstreamStatusChanged {
            name: name.to_string(),
            status: "authenticating".to_string(),
        });
    }

    let Some(flow_mgr) = flow_mgr else {
        tracing::warn!(
            source = "mcp_registry",
            %name,
            "OAuth flow requested but OAuthFlowManager is not wired"
        );
        return;
    };

    let Some(ref auth) = entry.config.auth else {
        tracing::warn!(
            source = "mcp_registry",
            %name,
            "OAuth challenge received but no auth config is set — cannot start flow"
        );
        return;
    };
    let auth = auth.clone();

    let server_url = match &entry.config.transport {
        UpstreamTransport::Http { url } => url.clone(),
        UpstreamTransport::Stdio { .. } => {
            tracing::warn!(
                source = "mcp_registry",
                %name,
                "OAuth challenge on stdio transport — unsupported"
            );
            return;
        }
    };

    // Dev-mode uses the localhost callback server; production uses the
    // `tuic://` deep-link handler (resolved in the Tauri command layer, #1198).
    let redirect_uri = "tuic://oauth/callback".to_string();
    let name_owned = name.to_string();

    tokio::spawn(async move {
        match flow_mgr
            .start_flow(&name_owned, &server_url, &auth, &redirect_uri)
            .await
        {
            Ok(outcome) => {
                if let Some(ref sender) = bus {
                    let _ = sender.send(crate::state::AppEvent::McpOAuthStart {
                        name: name_owned.clone(),
                        authorization_url: outcome.authorization_url,
                    });
                }
                tracing::info!(
                    source = "mcp_registry",
                    name = %name_owned,
                    "OAuth flow started"
                );
            }
            Err(e) => {
                tracing::error!(
                    source = "mcp_registry",
                    name = %name_owned,
                    "OAuth flow failed to start: {e}"
                );
            }
        }
    });
}

/// Run the MCP initialize sequence for an entry and update its status.
/// Triggers the OAuth flow on a `NeedsOAuth` result when `flow_mgr` is wired;
/// otherwise NeedsOAuth is logged and the entry is left in `Authenticating`.
async fn initialize_entry_with_oauth(
    entry: &Arc<UpstreamEntry>,
    name: &str,
    bus: Option<&tokio::sync::broadcast::Sender<crate::state::AppEvent>>,
    flow_mgr: Option<Arc<crate::mcp_oauth::flow::OAuthFlowManager>>,
) {
    let result: Result<Vec<UpstreamToolDef>, UpstreamError> = match &entry.client {
        UpstreamClient::Http(rwlock) => {
            let mut guard = rwlock.write().await;
            guard.initialize().await
        }
        UpstreamClient::Stdio(mutex) => {
            let arc = Arc::clone(mutex);
            tokio::task::spawn_blocking(move || {
                let mut guard = arc.lock().map_err(|e| e.to_string())?;
                guard.spawn_and_initialize()
            })
            .await
            .unwrap_or_else(|e| Err(format!("spawn_blocking error: {e}")))
            .map_err(UpstreamError::Other)
        }
    };

    let status_str = match result {
        Ok(tools) => {
            *entry.tools.write() = tools;
            *entry.status.write() = UpstreamStatus::Ready;
            tracing::info!(source = "mcp_registry", %name, "Initialized (Ready)");
            "ready"
        }
        Err(UpstreamError::NeedsOAuth { .. }) => {
            tracing::info!(
                source = "mcp_registry",
                %name,
                "Upstream requires OAuth authentication — starting flow"
            );
            start_oauth_flow(entry, name, bus.cloned(), flow_mgr);
            // Status already set to Authenticating inside start_oauth_flow.
            return;
        }
        Err(e) => {
            let e_str = e.to_string();
            let exhausted = entry.cb.record_failure();
            if exhausted {
                tracing::error!(source = "mcp_registry", %name, "Failed permanently: {e_str}");
                *entry.status.write() = UpstreamStatus::Failed;
                "failed"
            } else if entry.cb.is_open() {
                tracing::warn!(source = "mcp_registry", %name, "Initialization failed (circuit open): {e_str}");
                *entry.status.write() = UpstreamStatus::CircuitOpen;
                "circuit_open"
            } else {
                tracing::warn!(source = "mcp_registry", %name, "Initialization failed: {e_str}");
                "connecting"
            }
        }
    };

    if let Some(sender) = bus {
        let _ = sender.send(crate::state::AppEvent::UpstreamStatusChanged {
            name: name.to_string(),
            status: status_str.to_string(),
        });
    }
}

/// Run health checks on Ready and recoverable CircuitOpen upstreams.
async fn run_health_checks(registry: &UpstreamRegistry) {
    // Snapshot the bus once so each spawned task gets a clone without holding the lock.
    let bus_snapshot = registry.event_bus.read().clone();
    let flow_snapshot = registry.oauth_flow();

    for entry_ref in registry.entries.iter() {
        let status = entry_ref.status.read().clone();
        // Probe Ready upstreams, CircuitOpen with expired backoff, stuck Connecting,
        // and Failed entries (allows recovery after sleep/wake or transient outages)
        let should_probe = match status {
            UpstreamStatus::Ready => true,
            UpstreamStatus::CircuitOpen => !entry_ref.cb.is_open(),
            UpstreamStatus::Connecting => true,
            UpstreamStatus::Failed => true,
            UpstreamStatus::Disabled => false,
            UpstreamStatus::Authenticating => false,
        };
        if !should_probe {
            continue;
        }
        let entry = Arc::clone(entry_ref.value());
        let name = entry_ref.key().clone();
        let bus = bus_snapshot.clone();
        let flow = flow_snapshot.clone();
        let needs_recovery = status != UpstreamStatus::Ready;
        tokio::spawn(async move {
            // For stuck Connecting entries, re-run full initialization
            if status == UpstreamStatus::Connecting {
                initialize_entry_with_oauth(&entry, &name, bus.as_ref(), flow).await;
                return;
            }

            match health_check_entry(&entry).await {
                Ok(tools) => {
                    // Always refresh the tool list (fixes "0 tools" after recovery)
                    let old_count = entry.tools.read().len();
                    let new_count = tools.len();
                    *entry.tools.write() = tools;

                    entry.cb.record_success();
                    if needs_recovery {
                        *entry.status.write() = UpstreamStatus::Ready;
                        tracing::info!(source = "mcp_registry", %name, "Recovered (Ready) with {new_count} tools");
                        if let Some(ref sender) = bus {
                            let _ = sender.send(crate::state::AppEvent::UpstreamStatusChanged {
                                name: name.clone(),
                                status: "ready".to_string(),
                            });
                        }
                    } else if old_count != new_count {
                        tracing::info!(source = "mcp_registry", %name, "Tool list refreshed: {old_count} → {new_count}");
                    }
                }
                Err(UpstreamError::NeedsOAuth { .. }) => {
                    tracing::info!(
                        source = "mcp_registry",
                        %name,
                        "Health check received OAuth challenge — starting flow"
                    );
                    start_oauth_flow(&entry, &name, bus.clone(), flow);
                }
                Err(_) => {
                    let exhausted = entry.cb.record_failure();
                    let new_status = if exhausted {
                        // Only log on first transition to Failed, not on repeated probe failures
                        if status != UpstreamStatus::Failed {
                            tracing::error!(source = "mcp_registry", %name, "Health check failed permanently");
                        }
                        UpstreamStatus::Failed
                    } else {
                        tracing::warn!(source = "mcp_registry", %name, "Health check failed — circuit opening");
                        UpstreamStatus::CircuitOpen
                    };
                    *entry.status.write() = new_status.clone();
                    if let Some(ref sender) = bus {
                        let status_str = match new_status {
                            UpstreamStatus::Failed => "failed",
                            _ => "circuit_open",
                        };
                        let _ = sender.send(crate::state::AppEvent::UpstreamStatusChanged {
                            name: name.clone(),
                            status: status_str.to_string(),
                        });
                    }
                }
            }
        });
    }
}

/// Perform a health check on a single entry.
/// Returns the refreshed tool list on success, or a typed error on failure.
async fn health_check_entry(entry: &UpstreamEntry) -> Result<Vec<UpstreamToolDef>, UpstreamError> {
    match &entry.client {
        UpstreamClient::Http(rwlock) => {
            let guard = rwlock.read().await;
            guard.health_check().await
        }
        UpstreamClient::Stdio(mutex) => {
            let arc = Arc::clone(mutex);
            tokio::task::spawn_blocking(move || {
                arc.lock()
                    .map_err(|e| e.to_string())?
                    .health_check()
            })
            .await
            .unwrap_or_else(|e| Err(format!("spawn_blocking error: {e}")))
            .map_err(UpstreamError::Other)
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp_upstream_config::{FilterMode, ToolFilter, UpstreamMcpServer, UpstreamTransport};

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn http_server_config(name: &str, url: &str) -> UpstreamMcpServer {
        UpstreamMcpServer {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.to_string(),
            transport: UpstreamTransport::Http { url: url.to_string() },
            enabled: true,
            timeout_secs: 10,
            tool_filter: None,
            auth: None,
        }
    }

    fn http_server_config_disabled(name: &str, url: &str) -> UpstreamMcpServer {
        let mut cfg = http_server_config(name, url);
        cfg.enabled = false;
        cfg
    }

    // -----------------------------------------------------------------------
    // split_prefixed_name
    // -----------------------------------------------------------------------

    #[test]
    fn split_valid_prefix() {
        let (up, tool) = split_prefixed_name("myserver__do_thing").unwrap();
        assert_eq!(up, "myserver");
        assert_eq!(tool, "do_thing");
    }

    #[test]
    fn split_invalid_prefix_returns_err() {
        assert!(split_prefixed_name("nodoubleunderscore").is_err());
    }

    #[test]
    fn split_takes_first_double_underscore() {
        // Tool name itself contains __ — only split on first occurrence
        let (up, tool) = split_prefixed_name("upstream__tool__with__underscores").unwrap();
        assert_eq!(up, "upstream");
        assert_eq!(tool, "tool__with__underscores");
    }

    // -----------------------------------------------------------------------
    // apply_filter
    // -----------------------------------------------------------------------

    fn config_with_filter(mode: FilterMode, patterns: &[&str]) -> UpstreamMcpServer {
        UpstreamMcpServer {
            id: "test".to_string(),
            name: "test".to_string(),
            transport: UpstreamTransport::Http { url: "http://localhost/mcp".to_string() },
            enabled: true,
            timeout_secs: 10,
            tool_filter: Some(ToolFilter {
                mode,
                patterns: patterns.iter().map(|s| s.to_string()).collect(),
            }),
            auth: None,
        }
    }

    #[test]
    fn filter_no_filter_allows_all() {
        let cfg = http_server_config("s", "http://x");
        assert!(apply_filter("anything", &cfg));
    }

    #[test]
    fn filter_allow_exact_match() {
        let cfg = config_with_filter(FilterMode::Allow, &["read_file"]);
        assert!(apply_filter("read_file", &cfg));
        assert!(!apply_filter("write_file", &cfg));
    }

    #[test]
    fn filter_allow_glob_prefix() {
        let cfg = config_with_filter(FilterMode::Allow, &["read_*"]);
        assert!(apply_filter("read_file", &cfg));
        assert!(apply_filter("read_dir", &cfg));
        assert!(!apply_filter("write_file", &cfg));
    }

    #[test]
    fn filter_deny_blocks_match() {
        let cfg = config_with_filter(FilterMode::Deny, &["dangerous_*"]);
        assert!(!apply_filter("dangerous_rm", &cfg));
        assert!(apply_filter("safe_read", &cfg));
    }

    // -----------------------------------------------------------------------
    // aggregated_tools
    // -----------------------------------------------------------------------

    fn make_tool_def(name: &str) -> UpstreamToolDef {
        UpstreamToolDef {
            original_name: name.to_string(),
            definition: serde_json::json!({
                "name": name,
                "description": "test tool",
                "inputSchema": { "type": "object", "properties": {} }
            }),
        }
    }

    fn ready_entry(name: &str, tools: Vec<UpstreamToolDef>) -> (String, Arc<UpstreamEntry>) {
        let config = http_server_config(name, "http://example.com/mcp");
        // Build a dummy HTTP client (won't be called in aggregation tests)
        let client = HttpMcpClient::new(name.to_string(), "http://example.com/mcp".to_string(), 10);
        let entry = Arc::new(UpstreamEntry::new(config, UpstreamClient::Http(tokio::sync::RwLock::new(client))));
        *entry.tools.write() = tools;
        *entry.status.write() = UpstreamStatus::Ready;
        (name.to_string(), entry)
    }

    #[test]
    fn aggregated_tools_empty_when_no_upstreams() {
        let registry = UpstreamRegistry::new();
        assert!(registry.aggregated_tools().is_empty());
    }

    #[test]
    fn aggregated_tools_prefixes_names() {
        let registry = UpstreamRegistry::new();
        let (name, entry) = ready_entry("myserver", vec![make_tool_def("do_thing")]);
        registry.entries.insert(name, entry);

        let tools = registry.aggregated_tools();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["name"], "myserver__do_thing");
    }

    #[test]
    fn aggregated_tools_annotates_description() {
        let registry = UpstreamRegistry::new();
        let (name, entry) = ready_entry("myserver", vec![make_tool_def("do_thing")]);
        registry.entries.insert(name, entry);

        let tools = registry.aggregated_tools();
        assert_eq!(tools.len(), 1);
        let desc = tools[0]["description"].as_str().unwrap();
        assert!(desc.starts_with("[via myserver]"), "got: {desc}");
        assert!(desc.contains("test tool"), "got: {desc}");
    }

    #[test]
    fn aggregated_tools_merges_multiple_upstreams() {
        let registry = UpstreamRegistry::new();
        let (n1, e1) = ready_entry("alpha", vec![make_tool_def("tool_a")]);
        let (n2, e2) = ready_entry("beta", vec![make_tool_def("tool_b")]);
        registry.entries.insert(n1, e1);
        registry.entries.insert(n2, e2);

        let tools = registry.aggregated_tools();
        assert_eq!(tools.len(), 2);
        let names: Vec<String> = tools.iter().map(|t| t["name"].as_str().unwrap().to_string()).collect();
        assert!(names.contains(&"alpha__tool_a".to_string()));
        assert!(names.contains(&"beta__tool_b".to_string()));
    }

    #[test]
    fn aggregated_tools_skips_non_ready() {
        let registry = UpstreamRegistry::new();
        let (name, entry) = ready_entry("offline", vec![make_tool_def("tool")]);
        *entry.status.write() = UpstreamStatus::CircuitOpen;
        registry.entries.insert(name, entry);

        assert!(registry.aggregated_tools().is_empty());
    }

    #[test]
    fn aggregated_tools_applies_deny_filter() {
        let registry = UpstreamRegistry::new();
        let config = config_with_filter(FilterMode::Deny, &["secret_*"]);
        let name = config.name.clone(); // "test"
        let client = HttpMcpClient::new(name.clone(), "http://x/mcp".to_string(), 10);
        let entry = Arc::new(UpstreamEntry::new(config, UpstreamClient::Http(tokio::sync::RwLock::new(client))));
        *entry.tools.write() = vec![make_tool_def("secret_key"), make_tool_def("safe_read")];
        *entry.status.write() = UpstreamStatus::Ready;
        registry.entries.insert(name.clone(), entry);

        let tools = registry.aggregated_tools();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["name"], format!("{name}__safe_read"));
    }

    // -----------------------------------------------------------------------
    // proxy_tool_call filter enforcement
    // -----------------------------------------------------------------------

    /// With the Speakeasy `call_tool` meta-tool, agents can invoke any upstream
    /// tool by exact name — discovery no longer gates dispatch. The filter
    /// must therefore be re-checked on the call path, otherwise a known denied
    /// tool name bypasses the allow/deny list.
    #[tokio::test]
    async fn proxy_tool_call_rejects_denied_tool_by_name() {
        let registry = UpstreamRegistry::new();
        let config = config_with_filter(FilterMode::Deny, &["secret_*"]);
        let name = config.name.clone();
        let client = HttpMcpClient::new(name.clone(), "http://x/mcp".to_string(), 10);
        let entry = Arc::new(UpstreamEntry::new(
            config,
            UpstreamClient::Http(tokio::sync::RwLock::new(client)),
        ));
        *entry.tools.write() = vec![make_tool_def("secret_key"), make_tool_def("safe_read")];
        *entry.status.write() = UpstreamStatus::Ready;
        registry.entries.insert(name.clone(), entry);

        let err = registry
            .proxy_tool_call(&format!("{name}__secret_key"), serde_json::json!({}))
            .await
            .expect_err("denied tool must be blocked at call time");
        assert!(
            err.contains("blocked by its allow/deny filter"),
            "unexpected error: {err}"
        );
    }

    // -----------------------------------------------------------------------
    // CircuitBreaker
    // -----------------------------------------------------------------------

    #[test]
    fn cb_starts_closed() {
        let cb = CircuitBreaker::new();
        assert!(!cb.is_open());
    }

    #[test]
    fn cb_opens_after_threshold() {
        let cb = CircuitBreaker::new();
        for _ in 0..CB_THRESHOLD {
            cb.record_failure();
        }
        assert!(cb.is_open());
    }

    #[test]
    fn cb_resets_on_success() {
        let cb = CircuitBreaker::new();
        for _ in 0..CB_THRESHOLD {
            cb.record_failure();
        }
        cb.record_success();
        assert!(!cb.is_open());
        assert_eq!(cb.state.lock().failure_count, 0);
    }

    #[test]
    fn cb_exhausts_after_max_retries() {
        let cb = CircuitBreaker::new();
        // Each "cycle" = CB_THRESHOLD failures to open the circuit (retry_count += 1).
        // After opening, failure_count resets to 0, so the next cycle needs
        // CB_THRESHOLD fresh failures. Exhaustion fires when retry_count > CB_MAX_RETRIES.
        for cycle in 1..=(CB_MAX_RETRIES + 1) {
            // Clear backoff so we can record new failures
            cb.state.lock().open_until = None;
            for j in 0..CB_THRESHOLD {
                let exhausted = cb.record_failure();
                let should_exhaust = cycle > CB_MAX_RETRIES && j == CB_THRESHOLD - 1;
                assert_eq!(exhausted, should_exhaust, "cycle {cycle}, failure {j}");
            }
        }
    }

    #[test]
    fn cb_resets_failure_count_on_open() {
        let cb = CircuitBreaker::new();
        // Accumulate CB_THRESHOLD failures to open the circuit
        for _ in 0..CB_THRESHOLD {
            cb.record_failure();
        }
        assert!(cb.is_open());
        // failure_count should be reset so the next half-open window
        // gets CB_THRESHOLD fresh attempts before re-opening
        assert_eq!(cb.state.lock().failure_count, 0);
    }

    // -----------------------------------------------------------------------
    // connect / disconnect (sync parts only — async parts use tokio)
    // -----------------------------------------------------------------------

    #[test]
    fn disconnect_returns_err_for_unknown() {
        let registry = UpstreamRegistry::new();
        assert!(registry.disconnect_upstream("unknown").is_err());
    }

    #[tokio::test]
    async fn connect_disabled_upstream_stays_disabled() {
        let registry = UpstreamRegistry::new();
        // Use a non-existent URL — disabled so initialize is never called
        let cfg = http_server_config_disabled("disabled", "http://127.0.0.1:1/mcp");
        registry.connect_upstream(cfg, None).await.unwrap();

        assert_eq!(
            registry.status("disabled"),
            Some(UpstreamStatus::Disabled)
        );
    }

    #[tokio::test]
    async fn connect_duplicate_returns_err() {
        let registry = UpstreamRegistry::new();
        let cfg1 = http_server_config_disabled("dup", "http://127.0.0.1:1/mcp");
        let cfg2 = http_server_config_disabled("dup", "http://127.0.0.1:2/mcp");
        registry.connect_upstream(cfg1, None).await.unwrap();
        assert!(registry.connect_upstream(cfg2, None).await.is_err());
    }

    #[tokio::test]
    async fn connect_self_referential_rejected() {
        let registry = UpstreamRegistry::new();
        let cfg = http_server_config("self", "http://127.0.0.1:9999/mcp");
        let err = registry.connect_upstream(cfg, Some(9999)).await.unwrap_err();
        assert!(err.contains("circular"), "got: {err}");
    }

    #[tokio::test]
    async fn disconnect_after_connect() {
        let registry = UpstreamRegistry::new();
        let cfg = http_server_config_disabled("toremove", "http://127.0.0.1:1/mcp");
        registry.connect_upstream(cfg, None).await.unwrap();
        assert!(registry.status("toremove").is_some());
        registry.disconnect_upstream("toremove").unwrap();
        assert!(registry.status("toremove").is_none());
    }

    // -----------------------------------------------------------------------
    // Authenticating state
    // -----------------------------------------------------------------------

    #[test]
    fn status_snapshot_includes_authenticating() {
        let registry = UpstreamRegistry::new();
        let (name, entry) = ready_entry("auth-test", vec![]);
        *entry.status.write() = UpstreamStatus::Authenticating;
        registry.entries.insert(name, entry);

        let snap = registry.status_snapshot();
        let upstreams = snap["upstreams"].as_array().unwrap();
        assert_eq!(upstreams.len(), 1);
        assert_eq!(upstreams[0]["status"], "authenticating");
    }

    #[test]
    fn aggregated_tools_skips_authenticating() {
        let registry = UpstreamRegistry::new();
        let (name, entry) = ready_entry("auth-skip", vec![make_tool_def("tool")]);
        *entry.status.write() = UpstreamStatus::Authenticating;
        registry.entries.insert(name, entry);

        assert!(registry.aggregated_tools().is_empty());
    }

    #[tokio::test]
    async fn proxy_tool_call_rejects_authenticating_with_32001() {
        let registry = UpstreamRegistry::new();
        let config = http_server_config_disabled("auth-block", "http://127.0.0.1:1/mcp");
        registry.connect_upstream(config, None).await.unwrap();
        // Transition to Authenticating
        if let Some(entry) = registry.entries.get("auth-block") {
            *entry.status.write() = UpstreamStatus::Authenticating;
        }

        let err = registry
            .proxy_tool_call("auth-block__some_tool", serde_json::json!({}))
            .await
            .expect_err("should reject during auth");
        assert!(err.contains("-32001"), "expected -32001 error code, got: {err}");
        assert!(err.contains("OAuth authentication"), "expected auth message, got: {err}");
    }

    #[test]
    fn auth_semaphore_has_one_permit() {
        let registry = UpstreamRegistry::new();
        assert_eq!(registry.auth_semaphore.available_permits(), 1);
    }

    // -----------------------------------------------------------------------
    // OAuth integration (#1197)
    // -----------------------------------------------------------------------

    #[test]
    fn set_oauth_flow_manager_stores_weak_ref() {
        let registry = UpstreamRegistry::new();
        assert!(registry.oauth_flow().is_none());

        let flow = Arc::new(crate::mcp_oauth::flow::OAuthFlowManager::new(
            registry.auth_semaphore.clone(),
        ));
        registry.set_oauth_flow_manager(flow.clone());
        assert!(registry.oauth_flow().is_some());

        // Dropping the only strong ref makes the weak upgrade return None.
        drop(flow);
        assert!(registry.oauth_flow().is_none());
    }

    #[tokio::test]
    async fn start_oauth_flow_transitions_entry_to_authenticating() {
        let registry = UpstreamRegistry::new();
        let cfg = http_server_config_disabled("auth-entry", "http://127.0.0.1:1/mcp");
        registry.connect_upstream(cfg, None).await.unwrap();
        let entry = Arc::clone(registry.entries.get("auth-entry").unwrap().value());

        // No flow manager wired — should still transition status & emit event.
        let (tx, mut rx) = tokio::sync::broadcast::channel(4);
        super::start_oauth_flow(&entry, "auth-entry", Some(tx), None);

        assert_eq!(*entry.status.read(), UpstreamStatus::Authenticating);
        let evt = rx.recv().await.expect("status event");
        match evt {
            crate::state::AppEvent::UpstreamStatusChanged { name, status } => {
                assert_eq!(name, "auth-entry");
                assert_eq!(status, "authenticating");
            }
            other => panic!("expected UpstreamStatusChanged, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn start_oauth_flow_is_idempotent_when_already_authenticating() {
        let registry = UpstreamRegistry::new();
        let cfg = http_server_config_disabled("auth-idem", "http://127.0.0.1:1/mcp");
        registry.connect_upstream(cfg, None).await.unwrap();
        let entry = Arc::clone(registry.entries.get("auth-idem").unwrap().value());
        *entry.status.write() = UpstreamStatus::Authenticating;

        let (tx, mut rx) = tokio::sync::broadcast::channel(4);
        super::start_oauth_flow(&entry, "auth-idem", Some(tx), None);

        // No event should be emitted — second call is a no-op.
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn on_oauth_complete_transitions_to_connecting() {
        let registry = UpstreamRegistry::new();
        let cfg = http_server_config_disabled("auth-done", "http://127.0.0.1:1/mcp");
        registry.connect_upstream(cfg, None).await.unwrap();
        {
            let entry = registry.entries.get("auth-done").unwrap();
            *entry.status.write() = UpstreamStatus::Authenticating;
        }

        registry.on_oauth_complete("auth-done").await.unwrap();
        assert_eq!(
            registry.status("auth-done"),
            Some(UpstreamStatus::Connecting)
        );
    }

    #[tokio::test]
    async fn on_oauth_complete_returns_err_for_unknown_upstream() {
        let registry = UpstreamRegistry::new();
        assert!(registry.on_oauth_complete("nope").await.is_err());
    }

    #[tokio::test]
    async fn upstream_names_lists_all() {
        let registry = UpstreamRegistry::new();
        let c1 = http_server_config_disabled("a", "http://127.0.0.1:1/mcp");
        let c2 = http_server_config_disabled("b", "http://127.0.0.1:2/mcp");
        registry.connect_upstream(c1, None).await.unwrap();
        registry.connect_upstream(c2, None).await.unwrap();
        let mut names = registry.upstream_names();
        names.sort();
        assert_eq!(names, vec!["a", "b"]);
    }
}
