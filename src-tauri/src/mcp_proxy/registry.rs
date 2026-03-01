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

use crate::mcp_proxy::http_client::{HttpMcpClient, UpstreamToolDef};
use crate::mcp_proxy::stdio_client::StdioMcpClient;
use crate::mcp_upstream_config::{FilterMode, UpstreamMcpServer, UpstreamTransport};
use dashmap::DashMap;
use serde_json::Value;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

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
            let excess = s.failure_count.saturating_sub(CB_THRESHOLD) as f64;
            let delay_ms = (CB_BASE_MS * 2_f64.powf(excess)).min(CB_MAX_MS);
            s.open_until = Some(Instant::now() + Duration::from_millis(delay_ms as u64));
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
    Http(Mutex<HttpMcpClient>),
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
}

impl UpstreamRegistry {
    pub(crate) fn new() -> Self {
        Self {
            entries: DashMap::new(),
            event_bus: parking_lot::RwLock::new(None),
        }
    }

    /// Wire the event bus so status changes emit SSE events.
    pub(crate) fn set_event_bus(&self, bus: tokio::sync::broadcast::Sender<crate::state::AppEvent>) {
        *self.event_bus.write() = Some(bus);
    }

    /// Emit an upstream status change event (fire-and-forget).
    pub(crate) fn emit_status_change(&self, name: &str, status: &str) {
        if let Some(bus) = self.event_bus.read().as_ref() {
            let _ = bus.send(crate::state::AppEvent::UpstreamStatusChanged {
                name: name.to_string(),
                status: status.to_string(),
            });
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

        if entry.cb.is_open() {
            return Err(format!("Circuit open for upstream '{upstream_name}'"));
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
                Err(e)
            }
        }
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
        // Snapshot the event bus sender (if any) so the spawned task can emit events
        // without needing a reference back to the registry.
        let entry_clone = Arc::clone(&entry);
        let name_clone = name.clone();
        let bus_snapshot = self.event_bus.read().clone();
        tokio::spawn(async move {
            initialize_entry(&entry_clone, &name_clone, bus_snapshot.as_ref()).await;
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
            };
            let transport_info = match &e.config.transport {
                UpstreamTransport::Http { url } => serde_json::json!({
                    "type": "http",
                    "url": url,
                }),
                UpstreamTransport::Stdio { command, args, .. } => serde_json::json!({
                    "type": "stdio",
                    "command": command,
                    "args": args,
                }),
            };
            serde_json::json!({
                "name": entry_ref.key(),
                "status": status_str,
                "transport": transport_info,
                "tool_count": e.tools.read().len(),
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
                }
            };
            if should_connect
                && let Err(e) = self.connect_upstream(new_server.clone(), Some(self_port)).await
            {
                eprintln!(
                    "[mcp-registry] Failed to connect upstream '{}': {e}",
                    new_server.name
                );
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
            Ok(UpstreamClient::Http(Mutex::new(client)))
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
) -> Result<Value, String> {
    match client {
        UpstreamClient::Http(mutex) => {
            let mut guard = mutex.lock().await;
            guard.call_tool_with_reconnect(tool_name, args).await
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
            .map_err(|e| format!("spawn_blocking panicked: {e}"))?
        }
    }
}

/// Run the MCP initialize sequence for an entry and update its status.
async fn initialize_entry(
    entry: &Arc<UpstreamEntry>,
    name: &str,
    bus: Option<&tokio::sync::broadcast::Sender<crate::state::AppEvent>>,
) {
    let result = match &entry.client {
        UpstreamClient::Http(mutex) => {
            let mut guard = mutex.lock().await;
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
        }
    };

    let status_str = match result {
        Ok(tools) => {
            *entry.tools.write() = tools;
            *entry.status.write() = UpstreamStatus::Ready;
            eprintln!("[mcp-registry] '{name}' initialized (Ready)");
            "ready"
        }
        Err(e) => {
            let exhausted = entry.cb.record_failure();
            if exhausted {
                eprintln!("[mcp-registry] '{name}' failed permanently: {e}");
                *entry.status.write() = UpstreamStatus::Failed;
                "failed"
            } else if entry.cb.is_open() {
                eprintln!("[mcp-registry] '{name}' initialization failed (circuit open): {e}");
                *entry.status.write() = UpstreamStatus::CircuitOpen;
                "circuit_open"
            } else {
                // Below threshold — stay in Connecting, CB not open yet
                eprintln!("[mcp-registry] '{name}' initialization failed: {e}");
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

    for entry_ref in registry.entries.iter() {
        let status = entry_ref.status.read().clone();
        // Probe Ready upstreams and CircuitOpen with expired backoff
        let should_probe = match status {
            UpstreamStatus::Ready => true,
            UpstreamStatus::CircuitOpen => !entry_ref.cb.is_open(),
            _ => false,
        };
        if !should_probe {
            continue;
        }
        let entry = Arc::clone(entry_ref.value());
        let name = entry_ref.key().clone();
        let bus = bus_snapshot.clone();
        let was_circuit_open = status == UpstreamStatus::CircuitOpen;
        tokio::spawn(async move {
            let ok = health_check_entry(&entry).await;
            if ok {
                entry.cb.record_success();
                // Recovery: CircuitOpen → Ready
                if was_circuit_open {
                    *entry.status.write() = UpstreamStatus::Ready;
                    eprintln!("[mcp-registry] '{name}' recovered (Ready)");
                    if let Some(ref sender) = bus {
                        let _ = sender.send(crate::state::AppEvent::UpstreamStatusChanged {
                            name: name.clone(),
                            status: "ready".to_string(),
                        });
                    }
                }
            } else {
                let exhausted = entry.cb.record_failure();
                let new_status = if exhausted {
                    eprintln!("[mcp-registry] '{name}' health check failed permanently");
                    UpstreamStatus::Failed
                } else {
                    eprintln!("[mcp-registry] '{name}' health check failed — circuit opening");
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
        });
    }
}

/// Perform a health check on a single entry. Returns true on success.
async fn health_check_entry(entry: &UpstreamEntry) -> bool {
    match &entry.client {
        UpstreamClient::Http(mutex) => {
            let guard = mutex.lock().await;
            guard.health_check().await.is_ok()
        }
        UpstreamClient::Stdio(mutex) => {
            let arc = Arc::clone(mutex);
            tokio::task::spawn_blocking(move || {
                arc.lock()
                    .map(|mut g| g.is_alive())
                    .unwrap_or(false)
            })
            .await
            .unwrap_or(false)
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
        let entry = Arc::new(UpstreamEntry::new(config, UpstreamClient::Http(Mutex::new(client))));
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
        let entry = Arc::new(UpstreamEntry::new(config, UpstreamClient::Http(Mutex::new(client))));
        *entry.tools.write() = vec![make_tool_def("secret_key"), make_tool_def("safe_read")];
        *entry.status.write() = UpstreamStatus::Ready;
        registry.entries.insert(name.clone(), entry);

        let tools = registry.aggregated_tools();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["name"], format!("{name}__safe_read"));
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
        // First CB_THRESHOLD failures open the circuit; retry_count starts at 1.
        for _ in 0..CB_THRESHOLD {
            let exhausted = cb.record_failure();
            assert!(!exhausted);
        }
        // Each additional failure increments retry_count. Exhaustion fires when
        // retry_count exceeds CB_MAX_RETRIES (i.e., on the (CB_MAX_RETRIES)th extra failure).
        for i in 1..=CB_MAX_RETRIES {
            let exhausted = cb.record_failure();
            // retry_count after this call = i (started at 1 from the threshold batch above,
            // but the threshold batch already pushed it to 1, 2, 3 — see record_failure logic).
            // Exhaustion fires when retry_count > CB_MAX_RETRIES.
            let should_exhaust = i >= CB_MAX_RETRIES;
            assert_eq!(exhausted, should_exhaust, "extra failure #{i} (total #{})", CB_THRESHOLD + i);
        }
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
