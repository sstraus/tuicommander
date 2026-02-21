import { Component, Show, createSignal, createResource, createMemo, createEffect, onMount, onCleanup } from "solid-js";
import { rpc } from "../../../transport";
import QRCode from "qrcode";

interface McpStatus {
  enabled: boolean;
  running: boolean;
  port: number | null;
  active_sessions: number;
  max_sessions: number;
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
}

export const ServicesTab: Component = () => {
  const [status, setStatus] = createSignal<McpStatus | null>(null);
  const [localIp] = createResource(() => rpc<string | null>("get_local_ip"));
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

  /** URL to embed in QR: includes credentials if plaintext password is available */
  const qrContent = createMemo(() => {
    const ip = localIp();
    if (!ip) return null;
    const base = `http://${ip}:${raPort()}`;
    const user = raUsername();
    const pass = raPassword(); // only set if user typed it this session
    if (user && pass) {
      return `http://${encodeURIComponent(user)}:${encodeURIComponent(pass)}@${ip}:${raPort()}`;
    }
    return base;
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

  return (
    <div class="settings-section">
      <h3>MCP Services</h3>

      <div class="settings-group">
        <label>HTTP API Server</label>
        <div class="settings-toggle">
          <input
            type="checkbox"
            checked={status()?.enabled ?? false}
            disabled={saving()}
            onChange={(e) => toggleMcp(e.currentTarget.checked)}
          />
          <span>Enable MCP HTTP API on localhost for external tool integration</span>
        </div>
        <p class="settings-hint">
          Exposes terminal sessions, git operations, and agent spawning to Claude Code, Cursor, and other MCP-capable tools.
        </p>
      </div>

      <Show when={status()}>
        {(s) => (
          <>
            <div class="settings-group">
              <label>Server Status</label>
              <div class="mcp-status-row">
                <span class={`mcp-status-dot ${s().running ? "running" : "stopped"}`} />
                <span class="mcp-status-text">
                  {s().running ? "Running" : s().enabled ? "Pending restart" : "Stopped"}
                </span>
                <Show when={s().running && s().port}>
                  <span class="mcp-status-port">Port {s().port}</span>
                </Show>
              </div>
            </div>

            <Show when={s().running}>
              <div class="settings-group">
                <label>Active Sessions</label>
                <div class="mcp-sessions-bar">
                  <div
                    class="mcp-sessions-fill"
                    style={{ width: `${Math.min(100, (s().active_sessions / s().max_sessions) * 100)}%` }}
                  />
                  <span class="mcp-sessions-label">
                    {s().active_sessions} / {s().max_sessions}
                  </span>
                </div>
              </div>

              <div class="settings-group">
                <label>API Endpoints</label>
                <p class="settings-hint">
                  {21} HTTP routes available — sessions, git, config, agents.
                  See <code>pty.md</code> for complete API reference.
                </p>
              </div>

              <div class="settings-group">
                <label>MCP Bridge</label>
                <p class="settings-hint">
                  Configure in Claude Code: add <code>tui-mcp-bridge</code> binary to MCP server settings.
                  20 tools available for terminal control, git operations, and agent orchestration.
                </p>
              </div>
            </Show>
          </>
        )}
      </Show>

      <h3 style={{ "margin-top": "24px" }}>Remote Access</h3>

      <div class="settings-group">
        <div class="settings-toggle">
          <input
            type="checkbox"
            checked={raEnabled()}
            onChange={(e) => setRaEnabled(e.currentTarget.checked)}
          />
          <span>Enable remote access via HTTP/WebSocket</span>
        </div>
        <p class="settings-hint" style={{ color: "var(--warning, #e5c07b)" }}>
          Warning: This exposes your terminal sessions to the network. Only enable on trusted networks and always set a strong password.
        </p>
      </div>

      <Show when={raEnabled()}>
        <div class="settings-group">
          <label>Port</label>
          <input
            type="number"
            class="settings-input"
            value={raPort()}
            min={1024}
            max={65535}
            onInput={(e) => setRaPort(parseInt(e.currentTarget.value) || 9876)}
          />
          <p class="settings-hint">
            Port for remote HTTP/WebSocket connections (default: 9876)
          </p>
        </div>

        {/* Username + Password side by side */}
        <div class="settings-credentials-row">
          <div class="settings-group" style={{ flex: "1", "min-width": 0 }}>
            <label>Username</label>
            <input
              type="text"
              class="settings-input"
              value={raUsername()}
              placeholder="admin"
              onInput={(e) => setRaUsername(e.currentTarget.value)}
            />
          </div>
          <div class="settings-group" style={{ flex: "1", "min-width": 0 }}>
            <label>Password</label>
            <div class="settings-password-row">
              <input
                type={raShowPassword() ? "text" : "password"}
                class="settings-input"
                value={raPassword()}
                placeholder={raHasPassword() ? "(password set)" : "Enter password..."}
                onInput={(e) => setRaPassword(e.currentTarget.value)}
              />
              <button
                class="settings-toggle-btn"
                onClick={() => setRaShowPassword(!raShowPassword())}
                title={raShowPassword() ? "Hide password" : "Show password"}
              >
                {raShowPassword()
                  ? /* eye-slash */ <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor"><path d="M13.359 11.238C15.06 9.72 16 8 16 8s-3-5.5-8-5.5a7.028 7.028 0 0 0-2.79.588l.77.771A5.944 5.944 0 0 1 8 3.5c2.12 0 3.879 1.168 5.168 2.457A13.134 13.134 0 0 1 14.828 8c-.058.087-.122.183-.195.288-.335.48-.83 1.12-1.465 1.755-.165.165-.337.328-.517.486l.708.709z"/><path d="M11.297 9.176a3.5 3.5 0 0 0-4.474-4.474l.823.823a2.5 2.5 0 0 1 2.829 2.829l.822.822zm-2.943 1.299.822.822a3.5 3.5 0 0 1-4.474-4.474l.823.823a2.5 2.5 0 0 0 2.829 2.829z"/><path d="M3.35 5.47c-.18.16-.353.322-.518.487A13.134 13.134 0 0 0 1.172 8l.195.288c.335.48.83 1.12 1.465 1.755C4.121 11.332 5.881 12.5 8 12.5c.716 0 1.39-.133 2.02-.36l.77.772A7.029 7.029 0 0 1 8 13.5C3 13.5 0 8 0 8s.939-1.721 2.641-3.238l.708.709zm10.296 8.884-12-12 .708-.708 12 12-.708.708z"/></svg>
                  : /* eye */      <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor"><path d="M16 8s-3-5.5-8-5.5S0 8 0 8s3 5.5 8 5.5S16 8 16 8zM1.173 8a13.133 13.133 0 0 1 1.66-2.043C4.12 4.668 5.88 3.5 8 3.5c2.12 0 3.879 1.168 5.168 2.457A13.133 13.133 0 0 1 14.828 8c-.058.087-.122.183-.195.288-.335.48-.83 1.12-1.465 1.755C11.879 11.332 10.119 12.5 8 12.5c-2.12 0-3.879-1.168-5.168-2.457A13.134 13.134 0 0 1 1.172 8z"/><path d="M8 5.5a2.5 2.5 0 1 0 0 5 2.5 2.5 0 0 0 0-5zM4.5 8a3.5 3.5 0 1 1 7 0 3.5 3.5 0 0 1-7 0z"/></svg>
                }
              </button>
            </div>
            <p class="settings-hint">Leave blank to keep existing.</p>
          </div>
        </div>

        <Show when={status()?.running && status()?.port}>
          <div class="settings-group">
            <div class="settings-connection-row">
              <div class="settings-connection-info">
                <label>Connection URL</label>
                <code class="settings-url">
                  http://{localIp() ?? "&lt;your-ip&gt;"}:{raPort()}
                </code>
                <Show when={raUsername()}>
                  <p class="settings-hint" style={{ "margin-top": "4px" }}>
                    User: <strong>{raUsername()}</strong>
                    {raPassword() ? "" : raHasPassword() ? " · password set" : ""}
                  </p>
                </Show>
              </div>
              <Show when={qrDataUrl()}>
                {(url) => (
                  <div class="settings-qr">
                    <img src={url()} width={120} height={120} alt="QR code for remote access" title="Scan to open on tablet" />
                    <span class="settings-qr-label">Scan to connect</span>
                  </div>
                )}
              </Show>
            </div>
          </div>
        </Show>
      </Show>

      <div class="settings-actions" style={{ "margin-top": "12px" }}>
        <button
          class="settings-save-btn"
          disabled={raSaving()}
          onClick={saveRemoteAccess}
        >
          {raSaving() ? "Saving..." : raSaved() ? "Saved" : "Save Remote Access"}
        </button>
      </div>

      <p class="settings-hint" style={{ "margin-top": "16px" }}>
        The HTTP server starts automatically when either MCP or Remote Access is enabled. Changes take effect after app restart.
      </p>
    </div>
  );
};
