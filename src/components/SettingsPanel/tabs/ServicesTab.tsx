import { Component, For, Show, createSignal, createResource, createMemo, createEffect, onMount, onCleanup } from "solid-js";
import { rpc, type UpstreamMcpConfig, type UpstreamMcpServer, type UpstreamTransport } from "../../../transport";
import { appLogger } from "../../../stores/appLogger";
import QRCode from "qrcode";
import { t } from "../../../i18n";
import { cx } from "../../../utils";
import s from "../Settings.module.css";

interface McpStatus {
  enabled: boolean;
  running: boolean;
  remote_port: number | null;
  active_sessions: number;
  /** Connected MCP protocol clients (reaped after 1h idle) */
  mcp_clients: number;
  max_sessions: number;
  /** UUID token for QR-code auth — only present when server is running */
  session_token?: string;
  /** null = remote disabled, true = TCP reachable, false = likely firewalled */
  reachable?: boolean | null;
}

interface AppConfig {
  shell: string | null;
  font_family: string;
  font_size: number;
  theme: string;
  mcp_server_enabled: boolean;
  remote_access_enabled: boolean;
  remote_access_port: number;
  remote_access_username: string;
  remote_access_password_hash: string;
  session_token_duration_secs: number;
  ipv6_enabled: boolean;
  lan_auth_bypass: boolean;
  disabled_native_tools: string[];
}

interface LocalIpEntry { ip: string; label: string; }

interface UpstreamStatusEntry {
  name: string;
  status: "connecting" | "ready" | "circuit_open" | "disabled" | "failed";
  transport: { type: string; url?: string; command?: string; args?: string[] };
  tool_count: number;
  tools: string[];
  metrics: { call_count: number; error_count: number; last_latency_ms: number };
}

/** Static definition of native TUIC tools exposed via MCP */
const NATIVE_TOOLS: { name: string; description: string; actions: string }[] = [
  { name: "session", description: "Manage PTY terminal sessions", actions: "list, create, input, output, resize, close, pause, resume" },
  { name: "git", description: "Query git repository state", actions: "info, diff, files, branches, github, prs" },
  { name: "agent", description: "Detect and spawn AI agents", actions: "detect, spawn, stats, metrics" },
  { name: "config", description: "Read and write app config", actions: "get, save" },
  { name: "workspace", description: "Query repos, groups, worktrees", actions: "list, active" },
  { name: "notify", description: "Show notifications to the user", actions: "toast, confirm" },
  { name: "plugin_dev_guide", description: "Plugin authoring reference", actions: "Returns full plugin authoring guide" },
];

export const ServicesTab: Component = () => {
  const [status, setStatus] = createSignal<McpStatus | null>(null);
  const [localIps] = createResource(() => rpc<LocalIpEntry[]>("get_local_ips"));
  const [selectedIp, setSelectedIp] = createSignal<string>("");
  const [saving, setSaving] = createSignal(false);

  // Remote access form state
  const [raEnabled, setRaEnabled] = createSignal(false);
  const [raPort, setRaPort] = createSignal(9876);
  const [raUsername, setRaUsername] = createSignal("");
  const [raPassword, setRaPassword] = createSignal("");
  const [raHasPassword, setRaHasPassword] = createSignal(false);
  const [raShowPassword, setRaShowPassword] = createSignal(false);
  const [qrDataUrl, setQrDataUrl] = createSignal<string | null>(null);
  const [tokenDuration, setTokenDuration] = createSignal(86400);
  const [ipv6Enabled, setIpv6Enabled] = createSignal(false);
  const [lanAuthBypass, setLanAuthBypass] = createSignal(false);
  const [urlCopied, setUrlCopied] = createSignal(false);
  const [regenerating, setRegenerating] = createSignal(false);
  const [disabledNativeTools, setDisabledNativeTools] = createSignal<string[]>([]);
  const [upstreamStatus, setUpstreamStatus] = createSignal<UpstreamStatusEntry[]>([]);

  // Auto-select best IP when list loads (prefer Tailscale, then LAN/Wi-Fi)
  createEffect(() => {
    const ips = localIps();
    if (!ips?.length || selectedIp()) return;
    const preferred = ips.find(e => e.label.includes("Tailscale"))
      ?? ips.find(e => e.label.includes("Wi-Fi") || e.label.includes("LAN"))
      ?? ips[0];
    setSelectedIp(preferred.ip);
  });

  const activeIp = () => selectedIp() || localIps()?.[0]?.ip;

  /** URL to embed in QR: token-based auth, never user:pass in URL. */
  const qrContent = createMemo(() => {
    const ip = activeIp();
    const token = status()?.session_token;
    if (!ip || !token) return null;
    // Bracket-wrap IPv6 literals for valid URL syntax
    const host = ip.includes(":") ? `[${ip}]` : ip;
    return `http://${host}:${raPort()}/?token=${token}`;
  });

  createEffect(() => {
    const content = qrContent();
    if (!content) { setQrDataUrl(null); return; }
    QRCode.toDataURL(content, { width: 160, margin: 2, color: { dark: "#ffffff", light: "#1e1e1e" } })
      .then(setQrDataUrl)
      .catch(() => setQrDataUrl(null));
  });

  const refreshStatus = async () => {
    try {
      const s = await rpc<McpStatus>("get_mcp_status");
      setStatus(s);
    } catch (e) {
      // Transient poll failures are normal during app startup
      appLogger.debug("config", "MCP status refresh failed", e);
    }
    try {
      const snap = await rpc<{ upstreams: UpstreamStatusEntry[] }>("get_mcp_upstream_status");
      setUpstreamStatus(snap.upstreams ?? []);
    } catch {
      // Upstream status not available (e.g. server not running)
    }
  };

  const loadRemoteConfig = async () => {
    try {
      const config = await rpc<AppConfig>("load_config");
      setRaEnabled(config.remote_access_enabled);
      setRaPort(config.remote_access_port);
      setRaUsername(config.remote_access_username);
      setRaHasPassword(config.remote_access_password_hash.length > 0);
      setTokenDuration(config.session_token_duration_secs ?? 86400);
      setIpv6Enabled(config.ipv6_enabled ?? false);
      setLanAuthBypass(config.lan_auth_bypass ?? false);
      setDisabledNativeTools(config.disabled_native_tools ?? []);
    } catch (e) {
      appLogger.warn("config", "Failed to load remote access config, using defaults", e);
    }
  };

  onMount(() => {
    refreshStatus();
    loadRemoteConfig();
    const interval = setInterval(refreshStatus, 3000);
    onCleanup(() => clearInterval(interval));
  });

  const toggleMcp = async (enabled: boolean) => {
    setSaving(true);
    try {
      const config = await rpc<AppConfig>("load_config");
      config.mcp_server_enabled = enabled;
      await rpc("save_config", { config });
      await refreshStatus();
    } catch (e) {
      appLogger.error("config", "Failed to save MCP config", e);
    } finally {
      setSaving(false);
    }
  };

  /** Save a single config field (load-modify-save pattern matching other tabs) */
  const saveConfigField = async (updater: (config: AppConfig) => void) => {
    try {
      const config = await rpc<AppConfig>("load_config");
      updater(config);
      await rpc("save_config", { config });
    } catch (e) {
      appLogger.error("config", "Failed to save config", e);
    }
  };

  /** Hash and save a new password */
  const savePassword = async (password: string) => {
    if (!password) return;
    try {
      const hash = await rpc<string>("hash_password", { password });
      await saveConfigField((c) => { c.remote_access_password_hash = hash; });
      setRaPassword("");
      setRaHasPassword(true);
    } catch (e) {
      appLogger.error("config", "Failed to hash password", e);
    }
  };

  const copyUrl = async () => {
    const url = qrContent();
    if (!url) return;
    try {
      await navigator.clipboard.writeText(url);
      setUrlCopied(true);
      setTimeout(() => setUrlCopied(false), 2000);
    } catch {
      // Fallback: select text for manual copy
    }
  };

  const regenerateToken = async () => {
    setRegenerating(true);
    try {
      await rpc("regenerate_session_token");
      await refreshStatus();
    } catch (e) {
      appLogger.error("config", "Failed to regenerate token", e);
    } finally {
      setRegenerating(false);
    }
  };

  /** Token duration options */
  const TOKEN_DURATIONS = [
    { value: 3600, label: t("services.tokenDuration.1h", "1 hour") },
    { value: 86400, label: t("services.tokenDuration.24h", "24 hours") },
    { value: 604800, label: t("services.tokenDuration.7d", "7 days") },
    { value: 31536000, label: t("services.tokenDuration.never", "Never") },
  ];

  return (
    <div class={s.section}>
      <h3>{t("services.heading.httpApiServer", "HTTP API Server")}</h3>

      <div class={s.group}>
        <div class={s.toggle}>
          <input
            type="checkbox"
            checked={status()?.enabled ?? false}
            disabled={saving()}
            onChange={(e) => toggleMcp(e.currentTarget.checked)}
          />
          <span>{t("services.toggle.enableHttpServer", "Enable HTTP API server")}</span>
        </div>
        <p class={s.hint}>
          {t("services.hint.httpDescription", "Serves the REST API and MCP protocol for AI agents and automation tools")}
        </p>
      </div>

      <div class={s.group}>
        <p class={s.hint}>
          {t("services.hint.socketInfo", "Local MCP connections use a Unix socket. No port configuration needed.")}
        </p>
      </div>

      <Show when={status()}>
        {(st) => (
          <>
            <div class={s.group}>
              <label>{t("services.label.serverStatus", "Server Status")}</label>
              <div class={s.mcpStatusRow}>
                <span class={cx(s.mcpStatusDot, st().running && s.running)} />
                <span class={s.mcpStatusText}>
                  {st().running ? t("services.status.running", "Running") : st().enabled ? t("services.status.pendingRestart", "Pending restart") : t("services.status.stopped", "Stopped")}
                </span>
                <Show when={st().running}>
                  <span class={s.mcpStatusPort}>{t("services.label.socket", "Socket")}</span>
                </Show>
              </div>
            </div>

            <Show when={st().running}>
              <div class={s.group}>
                <label>{t("services.label.mcpConnection", "MCP Connection")}</label>
                <p class={s.hint}>
                  {t("services.hint.mcpConnection", "AI agents connect via the tuic-bridge sidecar. MCP configs are auto-installed in supported agents (Claude Code, Cursor, etc.).")}
                </p>
              </div>
            </Show>
          </>
        )}
      </Show>

      <h3 style={{ "margin-top": "24px" }}>{t("services.heading.remoteAccess", "Remote Access")}</h3>

      <div class={s.group}>
        <div class={s.toggle}>
          <input
            type="checkbox"
            checked={raEnabled()}
            onChange={(e) => {
              const val = e.currentTarget.checked;
              setRaEnabled(val);
              saveConfigField((c) => { c.remote_access_enabled = val; });
            }}
          />
          <span>{t("services.toggle.enableRemoteAccess", "Enable remote access")}</span>
        </div>
        <p class={s.hint} style={{ color: "var(--warning, #e5c07b)" }}>
          {t("services.hint.remoteAccessWarning", "Warning: exposes a web interface on your local network. Secure with a strong password.")}
        </p>
      </div>

      <Show when={raEnabled()}>
        <div class={s.raBody}>
          <div class={s.raFields}>
            <div class={s.group}>
              <label>{t("services.label.port", "Port")}</label>
              <input
                type="number"
                class={s.input}
                value={raPort()}
                min={1024}
                max={65535}
                onInput={(e) => setRaPort(parseInt(e.currentTarget.value) || 9876)}
                onChange={() => saveConfigField((c) => { c.remote_access_port = raPort(); })}
              />
              <p class={s.hint}>
                {t("services.hint.port", "TCP port for the remote access web server")}
              </p>
            </div>

            {/* Username + Password side by side */}
            <div class={s.credentialsRow}>
              <div class={s.group} style={{ flex: "1", "min-width": 0 }}>
                <label>{t("services.label.username", "Username")}</label>
                <input
                  type="text"
                  class={s.input}
                  value={raUsername()}
                  placeholder={t("services.placeholder.username", "admin")}
                  onInput={(e) => setRaUsername(e.currentTarget.value)}
                  onChange={() => saveConfigField((c) => { c.remote_access_username = raUsername(); })}
                />
              </div>
              <div class={s.group} style={{ flex: "1", "min-width": 0 }}>
                <label>{t("services.label.password", "Password")}</label>
                <div class={s.passwordRow}>
                  <input
                    type={raShowPassword() ? "text" : "password"}
                    class={s.input}
                    value={raPassword()}
                    placeholder={raHasPassword() ? t("services.placeholder.passwordSet", "Password set — enter to change") : t("services.placeholder.passwordEnter", "Enter password")}
                    onInput={(e) => setRaPassword(e.currentTarget.value)}
                    onChange={() => savePassword(raPassword())}
                  />
                  <button
                    class={s.toggleBtn}
                    onClick={() => setRaShowPassword(!raShowPassword())}
                    title={raShowPassword() ? t("services.btn.hidePassword", "Hide password") : t("services.btn.showPassword", "Show password")}
                  >
                    {raShowPassword()
                      ? /* eye-slash */ <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor"><path d="M13.359 11.238C15.06 9.72 16 8 16 8s-3-5.5-8-5.5a7.028 7.028 0 0 0-2.79.588l.77.771A5.944 5.944 0 0 1 8 3.5c2.12 0 3.879 1.168 5.168 2.457A13.134 13.134 0 0 1 14.828 8c-.058.087-.122.183-.195.288-.335.48-.83 1.12-1.465 1.755-.165.165-.337.328-.517.486l.708.709z"/><path d="M11.297 9.176a3.5 3.5 0 0 0-4.474-4.474l.823.823a2.5 2.5 0 0 1 2.829 2.829l.822.822zm-2.943 1.299.822.822a3.5 3.5 0 0 1-4.474-4.474l.823.823a2.5 2.5 0 0 0 2.829 2.829z"/><path d="M3.35 5.47c-.18.16-.353.322-.518.487A13.134 13.134 0 0 0 1.172 8l.195.288c.335.48.83 1.12 1.465 1.755C4.121 11.332 5.881 12.5 8 12.5c.716 0 1.39-.133 2.02-.36l.77.772A7.029 7.029 0 0 1 8 13.5C3 13.5 0 8 0 8s.939-1.721 2.641-3.238l.708.709zm10.296 8.884-12-12 .708-.708 12 12-.708.708z"/></svg>
                      : /* eye */      <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor"><path d="M16 8s-3-5.5-8-5.5S0 8 0 8s3 5.5 8 5.5S16 8 16 8zM1.173 8a13.133 13.133 0 0 1 1.66-2.043C4.12 4.668 5.88 3.5 8 3.5c2.12 0 3.879 1.168 5.168 2.457A13.133 13.133 0 0 1 14.828 8c-.058.087-.122.183-.195.288-.335.48-.83 1.12-1.465 1.755C11.879 11.332 10.119 12.5 8 12.5c-2.12 0-3.879-1.168-5.168-2.457A13.134 13.134 0 0 1 1.172 8z"/><path d="M8 5.5a2.5 2.5 0 1 0 0 5 2.5 2.5 0 0 0 0-5zM4.5 8a3.5 3.5 0 1 1 7 0 3.5 3.5 0 0 1-7 0z"/></svg>
                    }
                  </button>
                </div>
                <p class={s.hint}>{t("services.hint.passwordLeaveBlank", "Leave blank to keep the current password")}</p>
              </div>
            </div>

            <Show when={status()?.running && status()?.remote_port}>
              <div class={s.group}>
                <label>{t("services.label.networkInterface", "Network Interface")}</label>
                <Show when={(localIps()?.length ?? 0) > 1} fallback={
                  <code class={s.url}>{activeIp() ?? "…"}</code>
                }>
                  <select
                    class={s.input}
                    value={selectedIp()}
                    onChange={(e) => setSelectedIp(e.currentTarget.value)}
                  >
                    <For each={localIps()}>
                      {(entry) => <option value={entry.ip}>{entry.label} — {entry.ip}</option>}
                    </For>
                  </select>
                </Show>
                <p class={s.hint} style={{ "margin-top": "4px" }}>
                  {t("services.hint.qrScan", "Scan the QR code to connect from another device")}
                </p>
                <Show when={status()?.reachable === false}>
                  <p class={s.hint} style={{ color: "var(--warning, #e5c07b)", "margin-top": "4px" }}>
                    {t("services.hint.firewallWarning", "Port may be blocked by a firewall")}
                  </p>
                </Show>
                <Show when={status()?.reachable === true}>
                  <p class={s.hint} style={{ color: "var(--green, #98c379)", "margin-top": "4px" }}>
                    {t("services.hint.serverReachable", "Server is reachable from the network")}
                  </p>
                </Show>
              </div>
            </Show>

            <div class={s.group}>
              <label>{t("services.label.tokenDuration", "Session Token Duration")}</label>
              <select
                class={s.input}
                value={tokenDuration()}
                onChange={(e) => {
                  const val = parseInt(e.currentTarget.value);
                  setTokenDuration(val);
                  saveConfigField((c) => { c.session_token_duration_secs = val; });
                }}
              >
                <For each={TOKEN_DURATIONS}>
                  {(opt) => <option value={opt.value}>{opt.label}</option>}
                </For>
              </select>
              <p class={s.hint}>
                {t("services.hint.tokenDuration", "How long remote sessions stay authenticated. Token always resets on app restart.")}
              </p>
            </div>

            <div class={s.group}>
              <button
                class={s.testBtn}
                disabled={regenerating()}
                onClick={regenerateToken}
              >
                {regenerating()
                  ? t("services.btn.regenerating", "Regenerating...")
                  : t("services.btn.regenerateToken", "Regenerate Token")}
              </button>
              <p class={s.hint}>
                {t("services.hint.regenerateToken", "Generates a new token, disconnecting all active remote sessions")}
              </p>
            </div>
          </div>

          <Show when={status()?.running && qrContent()}>
            <div class={s.qr}>
              <Show when={qrDataUrl()}>
                {(url) => <img src={url()} width={120} height={120} alt={t("services.alt.qrCode", "QR code")} title={t("services.title.qrCode", "Scan to connect")} />}
              </Show>
              <span class={s.qrLabel}>{t("services.label.scanToConnect", "Scan to connect")}</span>
              {/* Connection URL right under QR code */}
              <div class={s.urlCopyRow} style={{ "margin-top": "8px", "max-width": "200px" }}>
                <code class={s.urlFull} style={{ "font-size": "10px", "word-break": "break-all" }}>{qrContent()}</code>
                <button
                  class={s.copyBtn}
                  onClick={copyUrl}
                  title={t("services.btn.copyUrl", "Copy URL to clipboard")}
                >
                  {urlCopied()
                    ? <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor"><path d="M13.78 4.22a.75.75 0 0 1 0 1.06l-7.25 7.25a.75.75 0 0 1-1.06 0L2.22 9.28a.75.75 0 0 1 1.06-1.06L6 10.94l6.72-6.72a.75.75 0 0 1 1.06 0z"/></svg>
                    : <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor"><path d="M0 6.75C0 5.784.784 5 1.75 5h1.5a.75.75 0 0 1 0 1.5h-1.5a.25.25 0 0 0-.25.25v7.5c0 .138.112.25.25.25h7.5a.25.25 0 0 0 .25-.25v-1.5a.75.75 0 0 1 1.5 0v1.5A1.75 1.75 0 0 1 9.25 16h-7.5A1.75 1.75 0 0 1 0 14.25Z"/><path d="M5 1.75C5 .784 5.784 0 6.75 0h7.5C15.216 0 16 .784 16 1.75v7.5A1.75 1.75 0 0 1 14.25 11h-7.5A1.75 1.75 0 0 1 5 9.25Zm1.75-.25a.25.25 0 0 0-.25.25v7.5c0 .138.112.25.25.25h7.5a.25.25 0 0 0 .25-.25v-7.5a.25.25 0 0 0-.25-.25Z"/></svg>
                  }
                </button>
              </div>
            </div>
          </Show>
        </div>

        <div class={s.group} style={{ "margin-top": "16px" }}>
          <div class={s.toggle}>
            <input
              type="checkbox"
              checked={ipv6Enabled()}
              onChange={(e) => {
                const val = e.currentTarget.checked;
                setIpv6Enabled(val);
                saveConfigField((c) => { c.ipv6_enabled = val; });
              }}
            />
            <span>{t("services.toggle.enableIpv6", "Enable IPv6 (dual-stack)")}</span>
          </div>
          <p class={s.hint}>
            {t("services.hint.ipv6Description", "Binds the server to both IPv4 and IPv6 addresses. Requires save + server restart.")}
          </p>
        </div>

        <div class={s.group}>
          <div class={s.toggle}>
            <input
              type="checkbox"
              checked={lanAuthBypass()}
              onChange={(e) => {
                const val = e.currentTarget.checked;
                setLanAuthBypass(val);
                saveConfigField((c) => { c.lan_auth_bypass = val; });
              }}
            />
            <span>{t("services.toggle.lanAuthBypass", "Allow LAN access without authentication")}</span>
          </div>
          <p class={s.hint} style={{ color: lanAuthBypass() ? "var(--warning, #e5c07b)" : undefined }}>
            {lanAuthBypass()
              ? t("services.hint.lanAuthBypassWarning", "Devices on your local network can access without a password. Only use on trusted networks.")
              : t("services.hint.lanAuthBypassDescription", "Skips authentication for private/LAN IP addresses (RFC1918, Tailscale, IPv6 ULA)")}
          </p>
        </div>
      </Show>

      {/* ── TUIC Tools ── */}
      <h3 style={{ "margin-top": "24px" }}>TUIC Tools</h3>
      <div class={s.group}>
        <p class={s.hint}>
          Native tools exposed via MCP. Disable tools to restrict what AI agents can access.
        </p>
      </div>
      <For each={NATIVE_TOOLS}>
        {(tool) => {
          const disabled = () => disabledNativeTools().includes(tool.name);
          return (
            <div class={s.group} style={{ display: "flex", "align-items": "center", gap: "8px", padding: "4px 0" }}>
              <div class={s.toggle} style={{ "margin-right": "4px" }}>
                <input
                  type="checkbox"
                  checked={!disabled()}
                  onChange={(e) => {
                    const enabled = e.currentTarget.checked;
                    const updated = enabled
                      ? disabledNativeTools().filter(n => n !== tool.name)
                      : [...disabledNativeTools(), tool.name];
                    setDisabledNativeTools(updated);
                    saveConfigField((c) => { (c as AppConfig & { disabled_native_tools: string[] }).disabled_native_tools = updated; });
                  }}
                />
              </div>
              <div style={{ display: "flex", "align-items": "center", gap: "6px" }}>
                <span style={{ "font-weight": 500, "font-size": "13px", "font-family": "monospace" }}>{tool.name}</span>
                <span class={s.hint} style={{ margin: 0 }}>{tool.description}</span>
                <span
                  title={tool.actions}
                  style={{
                    display: "inline-flex", "align-items": "center", "justify-content": "center",
                    width: "16px", height: "16px", "border-radius": "50%", "flex-shrink": 0,
                    background: "rgba(255,255,255,0.08)", color: "var(--fg-muted, #888)",
                    "font-size": "11px", "font-weight": 600, cursor: "help",
                  }}
                >?</span>
              </div>
            </div>
          );
        }}
      </For>

      <UpstreamMcpPanel upstreamStatus={upstreamStatus()} />

      <p class={s.hint} style={{ "margin-top": "16px", color: "var(--text-dimmed)" }}>
        {t("services.hint.autoSave", "Settings are saved automatically when changed")}
      </p>
    </div>
  );
};

// ---------------------------------------------------------------------------
// Upstream MCP Servers panel (Tauri-only — uses OS keyring)
// ---------------------------------------------------------------------------

/** Blank form state for adding a new upstream */
function emptyForm() {
  return {
    name: "",
    transportType: "http" as "http" | "stdio",
    url: "",
    command: "",
    args: "",
    credential: "",
    timeout: 30,
  };
}

const UpstreamMcpPanel: Component<{ upstreamStatus: UpstreamStatusEntry[] }> = (props) => {
  const [upstreams, setUpstreams] = createSignal<UpstreamMcpServer[]>([]);
  const [showAdd, setShowAdd] = createSignal(false);
  const [form, setForm] = createSignal(emptyForm());
  const [saving, setSaving] = createSignal(false);
  const [error, setError] = createSignal("");
  const [editingId, setEditingId] = createSignal<string | null>(null);
  const [editForm, setEditForm] = createSignal(emptyForm());

  // Load upstream config on mount (Tauri-only)
  onMount(async () => {
    try {
      const cfg = await rpc<UpstreamMcpConfig>("load_mcp_upstreams");
      setUpstreams(cfg.servers ?? []);
    } catch {
      // Not in Tauri — silently skip
    }
  });

  async function saveUpstreams(servers: UpstreamMcpServer[]) {
    setSaving(true);
    setError("");
    try {
      await rpc("save_mcp_upstreams", { config: { servers } });
      setUpstreams(servers);
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  }

  async function addUpstream() {
    const f = form();
    if (!f.name.trim()) { setError("Name is required"); return; }

    const transport: UpstreamTransport = f.transportType === "http"
      ? { type: "http", url: f.url.trim() }
      : {
          type: "stdio",
          command: f.command.trim(),
          args: f.args.trim() ? f.args.trim().split(/\s+/) : [],
        };

    const server: UpstreamMcpServer = {
      id: crypto.randomUUID(),
      name: f.name.trim(),
      transport,
      enabled: true,
      timeout_secs: f.timeout,
    };

    // Save credential before persisting config (ignored if empty)
    if (f.credential) {
      try {
        await rpc("save_mcp_upstream_credential", { name: server.name, token: f.credential });
      } catch {
        // Non-fatal — credential might not be needed
      }
    }

    await saveUpstreams([...upstreams(), server]);
    setForm(emptyForm());
    setShowAdd(false);
  }

  /** Get live status entry for an upstream by name */
  function getStatus(name: string): UpstreamStatusEntry | undefined {
    return props.upstreamStatus.find(u => u.name === name);
  }

  /** Status dot color based on upstream connection state */
  function statusColor(st: string | undefined): string {
    switch (st) {
      case "ready": return "var(--green, #98c379)";
      case "connecting": return "var(--warning, #e5c07b)";
      case "circuit_open":
      case "failed": return "var(--error, #e06c75)";
      default: return "var(--text-dimmed)";
    }
  }

  function startEdit(server: UpstreamMcpServer) {
    setEditingId(server.id);
    setEditForm({
      name: server.name,
      transportType: server.transport.type,
      url: server.transport.type === "http" ? server.transport.url : "",
      command: server.transport.type === "stdio" ? server.transport.command : "",
      args: server.transport.type === "stdio" ? (server.transport.args?.join(" ") ?? "") : "",
      credential: "",
      timeout: server.timeout_secs,
    });
  }

  async function saveEdit(server: UpstreamMcpServer) {
    const f = editForm();
    const transport: UpstreamTransport = f.transportType === "http"
      ? { type: "http", url: f.url.trim() }
      : { type: "stdio", command: f.command.trim(), args: f.args.trim() ? f.args.trim().split(/\s+/) : [] };

    const updated: UpstreamMcpServer = {
      ...server,
      transport,
      timeout_secs: f.timeout,
    };

    if (f.credential) {
      try {
        await rpc("save_mcp_upstream_credential", { name: server.name, token: f.credential });
      } catch { /* non-fatal */ }
    }

    await saveUpstreams(upstreams().map(s => s.id === server.id ? updated : s));
    setEditingId(null);
  }

  async function toggleUpstream(id: string, enabled: boolean) {
    const updated = upstreams().map(s => s.id === id ? { ...s, enabled } : s);
    await saveUpstreams(updated);
  }

  async function removeUpstream(id: string, name: string) {
    if (!confirm(`Remove upstream "${name}"?`)) return;
    await rpc("delete_mcp_upstream_credential", { name }).catch((e) => appLogger.error("settings", "Failed to delete MCP upstream credential", { error: String(e) }));
    await saveUpstreams(upstreams().filter(s => s.id !== id));
  }

  return (
    <div style={{ "margin-top": "24px", "border-top": "1px solid var(--border)", "padding-top": "16px" }}>
      <div class={s.group}>
        <label style={{ display: "flex", "align-items": "center", gap: "8px", "justify-content": "space-between" }}>
          <span>Upstream MCP Servers</span>
          <button
            class={s.copyBtn}
            onClick={() => { setShowAdd(v => !v); setError(""); }}
            title="Add upstream server"
            style={{ "font-size": "18px", "line-height": 1 }}
          >
            {showAdd() ? "−" : "+"}
          </button>
        </label>
        <p class={s.hint}>
          Proxy external MCP servers through TUIC. Their tools appear prefixed as <code>{"{name}__{tool}"}</code>.
        </p>
      </div>

      {/* Add upstream form */}
      <Show when={showAdd()}>
        <div class={s.group} style={{ background: "var(--bg-secondary, rgba(255,255,255,0.03))", padding: "12px", "border-radius": "6px" }}>
          <div style={{ display: "grid", gap: "8px" }}>
            <input
              type="text"
              class={s.input}
              placeholder="Name (e.g. context7, github)"
              value={form().name}
              onInput={e => setForm(f => ({ ...f, name: e.currentTarget.value }))}
            />
            <select
              class={s.input}
              value={form().transportType}
              onChange={e => setForm(f => ({ ...f, transportType: e.currentTarget.value as "http" | "stdio" }))}
            >
              <option value="http">HTTP (Streamable MCP)</option>
              <option value="stdio">stdio (process)</option>
            </select>
            <Show when={form().transportType === "http"}>
              <input
                type="text"
                class={s.input}
                placeholder="URL (e.g. http://localhost:8080/mcp)"
                value={form().url}
                onInput={e => setForm(f => ({ ...f, url: e.currentTarget.value }))}
              />
              <input
                type="password"
                class={s.input}
                placeholder="API key for remote MCP servers (optional, stored in OS keychain)"
                value={form().credential}
                onInput={e => setForm(f => ({ ...f, credential: e.currentTarget.value }))}
              />
            </Show>
            <Show when={form().transportType === "stdio"}>
              <input
                type="text"
                class={s.input}
                placeholder="Command (e.g. npx)"
                value={form().command}
                onInput={e => setForm(f => ({ ...f, command: e.currentTarget.value }))}
              />
              <input
                type="text"
                class={s.input}
                placeholder="Args (space-separated, e.g. -y @modelcontextprotocol/server-filesystem /path)"
                value={form().args}
                onInput={e => setForm(f => ({ ...f, args: e.currentTarget.value }))}
              />
            </Show>
            <div style={{ display: "flex", gap: "8px", "align-items": "center" }}>
              <label style={{ "font-size": "12px", color: "var(--text-dimmed)" }}>Timeout (s):</label>
              <input
                type="number"
                class={s.input}
                value={form().timeout}
                min={0} max={300}
                style={{ width: "70px" }}
                onInput={e => setForm(f => ({ ...f, timeout: parseInt(e.currentTarget.value) || 30 }))}
              />
              <button
                class={s.copyBtn}
                onClick={addUpstream}
                disabled={saving()}
                style={{ "margin-left": "auto" }}
              >
                {saving() ? "Adding…" : "Add"}
              </button>
              <button
                class={s.copyBtn}
                onClick={() => { setShowAdd(false); setForm(emptyForm()); setError(""); }}
              >
                Cancel
              </button>
            </div>
            <Show when={error()}>
              <p class={s.hint} style={{ color: "var(--error, #e06c75)" }}>{error()}</p>
            </Show>
          </div>
        </div>
      </Show>

      {/* Upstream list */}
      <Show when={upstreams().length === 0 && !showAdd()}>
        <p class={s.hint} style={{ color: "var(--text-dimmed)" }}>
          No upstream servers configured. Click <strong>+</strong> to add one.
        </p>
      </Show>

      <For each={upstreams()}>
        {(server) => {
          const st = () => getStatus(server.name);
          const isEditing = () => editingId() === server.id;
          return (
            <div style={{ "border-bottom": "1px solid var(--border-subtle, rgba(255,255,255,0.06))" }}>
              <div class={s.group} style={{ display: "flex", "align-items": "center", gap: "8px", padding: "8px 0" }}>
                {/* Enable toggle */}
                <div class={s.toggle} style={{ "margin-right": "4px" }}>
                  <input
                    type="checkbox"
                    checked={server.enabled}
                    onChange={e => toggleUpstream(server.id, e.currentTarget.checked)}
                  />
                </div>
                {/* Info */}
                <div style={{ flex: 1, "min-width": 0 }}>
                  <div style={{ display: "flex", "align-items": "center", gap: "6px", "flex-wrap": "wrap" }}>
                    <span style={{ "font-weight": 500, "font-size": "13px" }}>{server.name}</span>
                    <span style={{
                      "font-size": "10px", padding: "1px 5px", "border-radius": "3px",
                      background: server.transport.type === "http" ? "rgba(97,175,239,0.15)" : "rgba(152,195,121,0.15)",
                      color: server.transport.type === "http" ? "#61afef" : "#98c379",
                    }}>
                      {server.transport.type.toUpperCase()}
                    </span>
                    {/* Status dot */}
                    <Show when={st()}>
                      {(entry) => (
                        <span style={{
                          display: "inline-block", width: "7px", height: "7px", "border-radius": "50%",
                          background: statusColor(entry().status),
                        }} title={entry().status.replace("_", " ")} />
                      )}
                    </Show>
                    <Show when={!server.enabled}>
                      <span style={{ "font-size": "10px", padding: "1px 5px", "border-radius": "3px", background: "rgba(255,255,255,0.05)", color: "var(--text-dimmed)" }}>
                        Disabled
                      </span>
                    </Show>
                  </div>
                  <div class={s.hint} style={{ margin: 0, "font-family": "monospace", "font-size": "11px" }}>
                    {server.transport.type === "http"
                      ? server.transport.url
                      : server.transport.command + (server.transport.args?.length ? " " + server.transport.args.join(" ") : "")}
                  </div>
                  {/* Metrics line */}
                  <Show when={st()?.metrics}>
                    {(m) => (
                      <div class={s.hint} style={{ margin: 0, "font-size": "11px" }}>
                        {st()!.tool_count} tools · {m().call_count} calls · {m().error_count} errors
                        {m().last_latency_ms > 0 ? ` · ${m().last_latency_ms}ms` : ""}
                      </div>
                    )}
                  </Show>
                </div>
                {/* Edit */}
                <button
                  class={s.copyBtn}
                  title="Edit"
                  onClick={() => isEditing() ? setEditingId(null) : startEdit(server)}
                >
                  <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor">
                    <path d="M12.146.146a.5.5 0 0 1 .708 0l3 3a.5.5 0 0 1 0 .708l-10 10a.5.5 0 0 1-.168.11l-5 2a.5.5 0 0 1-.65-.65l2-5a.5.5 0 0 1 .11-.168zM11.207 2.5 13.5 4.793 14.793 3.5 12.5 1.207zm1.586 3L10.5 3.207 4 9.707V10h.5a.5.5 0 0 1 .5.5v.5h.5a.5.5 0 0 1 .5.5v.5h.293z"/>
                  </svg>
                </button>
                {/* Reconnect */}
                <button
                  class={s.copyBtn}
                  title="Reconnect"
                  onClick={() => rpc("reconnect_mcp_upstream", { name: server.name }).catch(e => appLogger.warn("network", String(e)))}
                >
                  <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor">
                    <path d="M8 3a5 5 0 1 0 4.546 2.914.5.5 0 0 1 .908-.417A6 6 0 1 1 8 2z"/>
                    <path d="M8 4.466V.534a.25.25 0 0 1 .41-.192l2.36 1.966c.12.1.12.284 0 .384L8.41 4.658A.25.25 0 0 1 8 4.466"/>
                  </svg>
                </button>
                {/* Remove */}
                <button
                  class={s.copyBtn}
                  title="Remove"
                  onClick={() => removeUpstream(server.id, server.name)}
                  style={{ color: "var(--error, #e06c75)" }}
                >
                  <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor">
                    <path d="M5.5 5.5A.5.5 0 0 1 6 6v6a.5.5 0 0 1-1 0V6a.5.5 0 0 1 .5-.5m2.5 0a.5.5 0 0 1 .5.5v6a.5.5 0 0 1-1 0V6a.5.5 0 0 1 .5-.5m3 .5a.5.5 0 0 0-1 0v6a.5.5 0 0 0 1 0z"/>
                    <path d="M14.5 3a1 1 0 0 1-1 1H13v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V4h-.5a1 1 0 0 1-1-1V2a1 1 0 0 1 1-1H6a1 1 0 0 1 1-1h2a1 1 0 0 1 1 1h3.5a1 1 0 0 1 1 1zM4.118 4 4 4.059V13a1 1 0 0 0 1 1h6a1 1 0 0 0 1-1V4.059L11.882 4zM2.5 3h11V2h-11z"/>
                  </svg>
                </button>
              </div>
              {/* Edit inline panel */}
              <Show when={isEditing()}>
                <div style={{ background: "var(--bg-secondary, rgba(255,255,255,0.03))", padding: "12px", "border-radius": "6px", "margin-bottom": "8px" }}>
                  <div style={{ display: "grid", gap: "8px" }}>
                    <Show when={editForm().transportType === "http"}>
                      <div>
                        <label style={{ "font-size": "12px", color: "var(--text-dimmed)" }}>URL</label>
                        <input type="text" class={s.input} value={editForm().url}
                          onInput={e => setEditForm(f => ({ ...f, url: e.currentTarget.value }))} />
                      </div>
                      <div>
                        <label style={{ "font-size": "12px", color: "var(--text-dimmed)" }}>Bearer token</label>
                        <input type="password" class={s.input}
                          placeholder="Enter new token (leave blank to keep current)"
                          value={editForm().credential}
                          onInput={e => setEditForm(f => ({ ...f, credential: e.currentTarget.value }))} />
                        <p class={s.hint} style={{ margin: "2px 0 0" }}>Stored in OS keychain. Leave blank to keep current token.</p>
                      </div>
                    </Show>
                    <Show when={editForm().transportType === "stdio"}>
                      <div>
                        <label style={{ "font-size": "12px", color: "var(--text-dimmed)" }}>Command</label>
                        <input type="text" class={s.input} value={editForm().command}
                          onInput={e => setEditForm(f => ({ ...f, command: e.currentTarget.value }))} />
                      </div>
                      <div>
                        <label style={{ "font-size": "12px", color: "var(--text-dimmed)" }}>Args</label>
                        <input type="text" class={s.input} value={editForm().args}
                          onInput={e => setEditForm(f => ({ ...f, args: e.currentTarget.value }))} />
                      </div>
                    </Show>
                    <div style={{ display: "flex", gap: "8px", "align-items": "center" }}>
                      <label style={{ "font-size": "12px", color: "var(--text-dimmed)" }}>Timeout (s):</label>
                      <input type="number" class={s.input} value={editForm().timeout}
                        min={0} max={300} style={{ width: "70px" }}
                        onInput={e => setEditForm(f => ({ ...f, timeout: parseInt(e.currentTarget.value) || 30 }))} />
                    </div>
                    {/* Discovered tools */}
                    <Show when={st()?.tools?.length}>
                      <div>
                        <label style={{ "font-size": "12px", color: "var(--text-dimmed)" }}>Discovered tools ({st()!.tools.length})</label>
                        <div style={{ "font-family": "monospace", "font-size": "11px", color: "var(--text-dimmed)", "margin-top": "4px", "line-height": "1.6" }}>
                          {st()!.tools.join(", ")}
                        </div>
                      </div>
                    </Show>
                    <div style={{ display: "flex", gap: "8px", "justify-content": "flex-end" }}>
                      <button class={s.copyBtn} onClick={() => saveEdit(server)} disabled={saving()}>
                        {saving() ? "Saving…" : "Save"}
                      </button>
                      <button class={s.copyBtn} onClick={() => setEditingId(null)}>Cancel</button>
                    </div>
                  </div>
                </div>
              </Show>
            </div>
          );
        }}
      </For>
    </div>
  );
};
