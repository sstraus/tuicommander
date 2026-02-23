import { Component, For, Show, createSignal, createResource, createMemo, createEffect, onMount, onCleanup } from "solid-js";
import { rpc } from "../../../transport";
import QRCode from "qrcode";
import { t } from "../../../i18n";
import { cx } from "../../../utils";
import s from "../Settings.module.css";

interface McpStatus {
  enabled: boolean;
  running: boolean;
  port: number | null;
  active_sessions: number;
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
  worktree_dir: string | null;
  mcp_server_enabled: boolean;
  remote_access_enabled: boolean;
  remote_access_port: number;
  remote_access_username: string;
  remote_access_password_hash: string;
  session_token_duration_secs: number;
}

interface LocalIpEntry { ip: string; label: string; }

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
  const [raSaving, setRaSaving] = createSignal(false);
  const [raSaved, setRaSaved] = createSignal(false);
  const [qrDataUrl, setQrDataUrl] = createSignal<string | null>(null);
  const [tokenDuration, setTokenDuration] = createSignal(86400);
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
    return `http://${ip}:${raPort()}/?token=${token}`;
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
    } catch {
      // Ignore errors during refresh
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
    } catch {
      // Ignore
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
      console.error("Failed to save MCP config:", e);
    } finally {
      setSaving(false);
    }
  };

  const saveRemoteAccess = async () => {
    setRaSaving(true);
    setRaSaved(false);
    try {
      const config = await rpc<AppConfig>("load_config");
      config.remote_access_enabled = raEnabled();
      config.remote_access_port = raPort();
      config.remote_access_username = raUsername();
      config.session_token_duration_secs = tokenDuration();

      // Hash password if a new one was entered
      if (raPassword()) {
        const hash = await rpc<string>("hash_password", { password: raPassword() });
        config.remote_access_password_hash = hash;
        setRaPassword("");
        setRaHasPassword(true);
      }

      await rpc("save_config", { config });
      setRaSaved(true);
      setTimeout(() => setRaSaved(false), 2000);
    } catch (e) {
      console.error("Failed to save remote access config:", e);
    } finally {
      setRaSaving(false);
    }
  };

  const regenerateToken = async () => {
    setRegenerating(true);
    try {
      await rpc("regenerate_session_token");
      await refreshStatus();
    } catch (e) {
      console.error("Failed to regenerate token:", e);
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
            onChange={(e) => setRaEnabled(e.currentTarget.checked)}
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
                onChange={(e) => setTokenDuration(parseInt(e.currentTarget.value))}
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
              <a
                class={s.qrUrl}
                href={qrContent()!}
                target="_blank"
                rel="noopener"
                title={t("services.title.openInBrowser", "Open in browser")}
              >
                {qrContent()}
              </a>
            </div>
          </Show>
        </div>
      </Show>

      <div class={s.actions} style={{ "margin-top": "12px" }}>
        <button
          class={s.saveBtn}
          disabled={raSaving()}
          onClick={saveRemoteAccess}
        >
          {raSaving() ? t("services.btn.saving", "Saving...") : raSaved() ? t("services.btn.saved", "Saved!") : t("services.btn.saveRemoteAccess", "Save Remote Access Settings")}
        </button>
      </div>

      <p class={s.hint} style={{ "margin-top": "16px" }}>
        {t("services.hint.serverStartsAutomatically", "The server starts automatically when the app launches if enabled")}
      </p>
    </div>
  );
};
