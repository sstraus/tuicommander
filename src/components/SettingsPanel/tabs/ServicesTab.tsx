import { Component, Show, createSignal, onCleanup } from "solid-js";
import { rpc } from "../../../transport";

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

  // Load on mount
  refreshStatus();
  loadRemoteConfig();
  const interval = setInterval(refreshStatus, 3000);
  onCleanup(() => clearInterval(interval));

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

        <div class="settings-group">
          <label>Username</label>
          <input
            type="text"
            class="settings-input"
            value={raUsername()}
            placeholder="admin"
            onInput={(e) => setRaUsername(e.currentTarget.value)}
          />
        </div>

        <div class="settings-group">
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
              title={raShowPassword() ? "Hide" : "Show"}
            >
              {raShowPassword() ? "Hide" : "Show"}
            </button>
          </div>
          <p class="settings-hint">
            Required for remote connections. Leave blank to keep existing password.
          </p>
        </div>

        <Show when={status()?.running && status()?.port}>
          <div class="settings-group">
            <label>Connection URL</label>
            <code class="settings-url">
              http://&lt;your-ip&gt;:{raPort()}
            </code>
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
