import { Component, For, Show, createSignal, createResource, createMemo, createEffect, onMount, onCleanup } from "solid-js";
import { rpc } from "../../../transport";
import { appLogger } from "../../../stores/appLogger";
import QRCode from "qrcode";
import { t } from "../../../i18n";
import { cx } from "../../../utils";
import s from "../Settings.module.css";

interface McpStatus {
  enabled: boolean;
  running: boolean;
  port: number | null;
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
  mcp_port: number;
  remote_access_enabled: boolean;
  remote_access_port: number;
  remote_access_username: string;
  remote_access_password_hash: string;
  session_token_duration_secs: number;
  ipv6_enabled: boolean;
  lan_auth_bypass: boolean;
}

interface LocalIpEntry { ip: string; label: string; }

export const ServicesTab: Component = () => {
  const [status, setStatus] = createSignal<McpStatus | null>(null);
  const [localIps] = createResource(() => rpc<LocalIpEntry[]>("get_local_ips"));
  const [selectedIp, setSelectedIp] = createSignal<string>("");
  const [saving, setSaving] = createSignal(false);
  const [mcpPort, setMcpPort] = createSignal(3845);
  const [mcpUrlCopied, setMcpUrlCopied] = createSignal(false);

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
  };

  const loadRemoteConfig = async () => {
    try {
      const config = await rpc<AppConfig>("load_config");
      setMcpPort(config.mcp_port ?? 3845);
      setRaEnabled(config.remote_access_enabled);
      setRaPort(config.remote_access_port);
      setRaUsername(config.remote_access_username);
      setRaHasPassword(config.remote_access_password_hash.length > 0);
      setTokenDuration(config.session_token_duration_secs ?? 86400);
      setIpv6Enabled(config.ipv6_enabled ?? false);
      setLanAuthBypass(config.lan_auth_bypass ?? false);
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
      <h3>{t("services.heading.mcpServices", "MCP Services")}</h3>

      <div class={s.group}>
        <label>{t("services.label.httpApiServer", "HTTP API Server")}</label>
        <div class={s.toggle}>
          <input
            type="checkbox"
            checked={status()?.enabled ?? false}
            disabled={saving()}
            onChange={(e) => toggleMcp(e.currentTarget.checked)}
          />
          <span>{t("services.toggle.enableMcp", "Enable MCP server")}</span>
        </div>
        <p class={s.hint}>
          {t("services.hint.mcpDescription", "Exposes a local HTTP API for AI agents and automation tools")}
        </p>
      </div>

      <div class={s.group}>
        <label>{t("services.label.mcpPort", "MCP Port")}</label>
        <input
          type="number"
          class={s.input}
          value={mcpPort()}
          min={1024}
          max={65535}
          style={{ width: "100px" }}
          onInput={(e) => setMcpPort(parseInt(e.currentTarget.value) || 3845)}
          onChange={() => saveConfigField((c) => { c.mcp_port = mcpPort(); })}
        />
        <p class={s.hint}>
          {t("services.hint.mcpPort", "Fixed port for the MCP server. Change requires server restart.")}
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
                <Show when={st().running && st().port}>
                  <span class={s.mcpStatusPort}>{t("services.label.port", "Port")} {st().port}</span>
                </Show>
              </div>
            </div>

            <Show when={st().running}>
              <div class={s.group}>
                <label>{t("services.label.activeSessions", "Active Sessions")}</label>
                <div class={s.mcpSessionsBar}>
                  <div
                    class={s.mcpSessionsFill}
                    style={{ width: `${Math.min(100, (st().active_sessions / st().max_sessions) * 100)}%` }}
                  />
                  <span class={s.mcpSessionsLabel}>
                    {st().active_sessions} / {st().max_sessions}
                  </span>
                </div>
              </div>

              <div class={s.group}>
                <label>{t("services.label.apiEndpoints", "API Endpoints")}</label>
                <p class={s.hint}>
                  {t("services.hint.apiEndpoints", "Exposes {count} API endpoints.", { count: "21" })}
                  See <code>pty.md</code> for complete API reference.
                </p>
              </div>

              <div class={s.group}>
                <label>{t("services.label.mcpBridge", "MCP Bridge")}</label>
                <p class={s.hint}>
                  {t("services.hint.mcpBridge", "Connects AI coding assistants via the Model Context Protocol")}
                </p>
              </div>

              <div class={s.group}>
                <label>{t("services.label.mcpEndpoint", "MCP Endpoint")}</label>
                <div class={s.urlCopyRow}>
                  <code class={s.urlFull}>{`http://127.0.0.1:${st().port}/mcp`}</code>
                  <button
                    class={s.copyBtn}
                    onClick={async () => {
                      try {
                        await navigator.clipboard.writeText(`http://127.0.0.1:${st().port}/mcp`);
                        setMcpUrlCopied(true);
                        setTimeout(() => setMcpUrlCopied(false), 2000);
                      } catch { /* ignore */ }
                    }}
                    title={t("services.btn.copyUrl", "Copy URL to clipboard")}
                  >
                    {mcpUrlCopied()
                      ? <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor"><path d="M13.78 4.22a.75.75 0 0 1 0 1.06l-7.25 7.25a.75.75 0 0 1-1.06 0L2.22 9.28a.75.75 0 0 1 1.06-1.06L6 10.94l6.72-6.72a.75.75 0 0 1 1.06 0z"/></svg>
                      : <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor"><path d="M0 6.75C0 5.784.784 5 1.75 5h1.5a.75.75 0 0 1 0 1.5h-1.5a.25.25 0 0 0-.25.25v7.5c0 .138.112.25.25.25h7.5a.25.25 0 0 0 .25-.25v-1.5a.75.75 0 0 1 1.5 0v1.5A1.75 1.75 0 0 1 9.25 16h-7.5A1.75 1.75 0 0 1 0 14.25Z"/><path d="M5 1.75C5 .784 5.784 0 6.75 0h7.5C15.216 0 16 .784 16 1.75v7.5A1.75 1.75 0 0 1 14.25 11h-7.5A1.75 1.75 0 0 1 5 9.25Zm1.75-.25a.25.25 0 0 0-.25.25v7.5c0 .138.112.25.25.25h7.5a.25.25 0 0 0 .25-.25v-7.5a.25.25 0 0 0-.25-.25Z"/></svg>
                    }
                  </button>
                </div>
                <p class={s.hint}>
                  {t("services.hint.mcpEndpoint", "Use this URL to register TUICommander with any MCP-compatible AI client")}
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

            <Show when={status()?.running && status()?.port}>
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
            </div>
          </Show>
        </div>

        <Show when={status()?.running && qrContent()}>
          <div class={s.urlRow}>
            <label>{t("services.label.connectionUrl", "Connection URL")}</label>
            <div class={s.urlCopyRow}>
              <code class={s.urlFull}>{qrContent()}</code>
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

      <p class={s.hint} style={{ "margin-top": "12px", color: "var(--text-dimmed)" }}>
        {t("services.hint.autoSave", "Settings are saved automatically when changed")}
      </p>

      <p class={s.hint} style={{ "margin-top": "16px" }}>
        {t("services.hint.serverStartsAutomatically", "The server starts automatically when the app launches if enabled")}
      </p>
    </div>
  );
};
