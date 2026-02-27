/**
 * Telegram Notifier Plugin — Universal (all terminals)
 *
 * Forwards terminal events to a Telegram bot. Each notification type is
 * individually toggleable and controls whether Telegram delivers it with
 * sound (normal push) or silently (no vibration/sound on the phone).
 *
 * Capabilities: net:http, ui:panel, ui:ticker
 *
 * Setup:
 *   1. Create a bot via @BotFather on Telegram, copy the token.
 *   2. Send any message to your bot, then use the Settings panel to enter
 *      the bot token — the plugin auto-detects your chat ID.
 *   3. Toggle which events get forwarded and whether each is silent or audible.
 */

const PLUGIN_ID = "telegram-notifier";
const SECTION_ID = "telegram";
const DATA_FILE = "config.json";
const TELEGRAM_API = "https://api.telegram.org";

// Cooldown per event type to avoid spam (ms)
const COOLDOWN_MS = 10_000;

// ── Icons ──────────────────────────────────────────────────────────────────

const ICON_TELEGRAM = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="currentColor"><path d="M14.05 2.43a.72.72 0 0 0-.76-.05L1.87 7.62a.68.68 0 0 0 .04 1.24l2.81.93 1.09 3.45a.67.67 0 0 0 1.12.28l1.58-1.55 2.87 2.13a.7.7 0 0 0 1.08-.38l2.5-10.56a.72.72 0 0 0-.41-.73zM6.25 9.97l-.36 2.08-.76-2.4 6.3-5.2-5.18 5.52z"/></svg>`;

const ICON_WARN = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="currentColor"><path d="M6.457 1.047c.659-1.234 2.427-1.234 3.086 0l5.082 9.524A1.7 1.7 0 0 1 13.082 13H2.918a1.7 1.7 0 0 1-1.543-2.429zM8 5a.75.75 0 0 0-.75.75v2.5a.75.75 0 0 0 1.5 0v-2.5A.75.75 0 0 0 8 5m1 6a1 1 0 1 0-2 0 1 1 0 0 0 2 0"/></svg>`;

// ── Default config ─────────────────────────────────────────────────────────

const EVENT_LABELS = {
  "question":      "Question / Input Prompt",
  "rate-limit":    "Rate Limit Hit",
  "usage-limit":   "Usage Limit Warning",
  "ci-failure":    "CI Failure",
  "agent-stopped": "Agent Stopped",
  "pr-url":        "PR/MR Created",
};

// enabled = send to Telegram at all, silent = Telegram delivers without sound
const DEFAULT_EVENTS = {
  "question":      { enabled: true,  silent: false },
  "rate-limit":    { enabled: true,  silent: false },
  "usage-limit":   { enabled: true,  silent: true },
  "ci-failure":    { enabled: true,  silent: false },
  "agent-stopped": { enabled: true,  silent: false },
  "pr-url":        { enabled: true,  silent: true },
};

function defaultConfig() {
  return {
    botToken: "",
    chatId: "",
    events: JSON.parse(JSON.stringify(DEFAULT_EVENTS)),
  };
}

// ── Plugin state ───────────────────────────────────────────────────────────

let hostRef = null;
let config = defaultConfig();
let lastSent = {};  // eventType -> timestamp
let sentCount = 0;

async function loadConfig() {
  try {
    const raw = await hostRef.invoke("read_plugin_data", {
      plugin_id: PLUGIN_ID,
      path: DATA_FILE,
    });
    const saved = JSON.parse(raw);
    config = {
      ...defaultConfig(),
      ...saved,
      events: { ...defaultConfig().events, ...(saved.events || {}) },
    };
  } catch {
    config = defaultConfig();
  }
}

async function saveConfig() {
  await hostRef.invoke("write_plugin_data", {
    plugin_id: PLUGIN_ID,
    path: DATA_FILE,
    content: JSON.stringify(config, null, 2),
  });
}

function isConfigured() {
  return config.botToken.length > 10 && config.chatId.length > 0;
}

function isOnCooldown(eventType) {
  const last = lastSent[eventType] || 0;
  return Date.now() - last < COOLDOWN_MS;
}

async function sendTelegram(text, silent) {
  if (!isConfigured()) return false;

  const url = `${TELEGRAM_API}/bot${config.botToken}/sendMessage`;
  try {
    const resp = await hostRef.httpFetch(url, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        chat_id: config.chatId,
        text,
        parse_mode: "HTML",
        disable_web_page_preview: true,
        disable_notification: !!silent,
      }),
    });
    if (resp.status === 200) {
      sentCount++;
      updateTicker();
      return true;
    }
    hostRef.log("error", `Telegram API error ${resp.status}`, resp.body);
    return false;
  } catch (err) {
    hostRef.log("error", "Failed to send Telegram message", String(err));
    return false;
  }
}

async function autoDetectChatId() {
  if (!config.botToken || config.botToken.length < 10) return null;

  const url = `${TELEGRAM_API}/bot${config.botToken}/getUpdates?limit=5`;
  try {
    const resp = await hostRef.httpFetch(url, { method: "GET" });
    if (resp.status !== 200) return null;
    const data = JSON.parse(resp.body);
    if (!data.ok || !data.result || data.result.length === 0) return null;
    for (let i = data.result.length - 1; i >= 0; i--) {
      const msg = data.result[i].message;
      if (msg && msg.chat && msg.chat.id) {
        return String(msg.chat.id);
      }
    }
    return null;
  } catch {
    return null;
  }
}

function updateTicker() {
  if (!hostRef) return;
  if (!isConfigured()) {
    hostRef.setTicker({
      id: "status",
      text: "Not configured",
      label: "Telegram",
      icon: ICON_WARN,
      priority: 0,
      ttlMs: 0,
    });
    return;
  }
  hostRef.setTicker({
    id: "status",
    text: sentCount > 0 ? `${sentCount} sent` : "Ready",
    label: "Telegram",
    icon: ICON_TELEGRAM,
    priority: 0,
    ttlMs: 0,
  });
}

async function notify(eventType, message) {
  const evtConfig = config.events[eventType];
  if (!evtConfig || !evtConfig.enabled) return;
  if (isOnCooldown(eventType)) return;

  lastSent[eventType] = Date.now();
  await sendTelegram(message, evtConfig.silent);
}

function repoLabel() {
  const repo = hostRef.getActiveRepo();
  return repo ? repo.displayName : "unknown";
}

// ── Settings panel HTML ────────────────────────────────────────────────────

function buildSettingsHtml() {
  const eventRows = Object.keys(EVENT_LABELS).map((key) => {
    const evt = config.events[key] || DEFAULT_EVENTS[key];
    const label = EVENT_LABELS[key];
    const enabledChecked = evt.enabled ? "checked" : "";
    const silentChecked = evt.silent ? "checked" : "";

    return `
      <tr>
        <td style="padding:6px 10px">${label}</td>
        <td style="padding:6px 10px;text-align:center">
          <input type="checkbox" data-event="${key}" data-field="enabled" ${enabledChecked}>
        </td>
        <td style="padding:6px 10px;text-align:center">
          <input type="checkbox" data-event="${key}" data-field="silent" ${silentChecked}>
        </td>
      </tr>`;
  }).join("");

  return `<!DOCTYPE html>
<html>
<head>
<style>
  * { box-sizing: border-box; margin: 0; padding: 0; }
  body {
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
    font-size: 13px;
    color: #cdd6f4;
    background: #1e1e2e;
    padding: 16px;
  }
  h2 { font-size: 15px; font-weight: 600; margin-bottom: 12px; color: #cdd6f4; }
  h3 { font-size: 13px; font-weight: 600; margin: 16px 0 8px; color: #a6adc8; }
  .field { margin-bottom: 10px; }
  label { display: block; font-size: 12px; color: #a6adc8; margin-bottom: 3px; }
  input[type="text"], input[type="password"] {
    width: 100%;
    padding: 6px 8px;
    background: #313244;
    border: 1px solid #45475a;
    border-radius: 4px;
    color: #cdd6f4;
    font-size: 13px;
    font-family: monospace;
  }
  input[type="text"]:focus, input[type="password"]:focus {
    outline: none;
    border-color: #89b4fa;
  }
  button {
    padding: 6px 14px;
    background: #89b4fa;
    color: #1e1e2e;
    border: none;
    border-radius: 4px;
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
    margin-right: 6px;
    margin-top: 6px;
  }
  button:hover { background: #74c7ec; }
  button.secondary {
    background: #45475a;
    color: #cdd6f4;
  }
  button.secondary:hover { background: #585b70; }
  button.danger {
    background: #f38ba8;
    color: #1e1e2e;
  }
  button.danger:hover { background: #eba0ac; }
  table {
    width: 100%;
    border-collapse: collapse;
    margin-top: 6px;
  }
  th {
    text-align: left;
    padding: 6px 10px;
    font-size: 11px;
    font-weight: 600;
    color: #a6adc8;
    border-bottom: 1px solid #45475a;
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }
  tr:hover td { background: #313244; }
  .status {
    margin-top: 12px;
    padding: 8px 10px;
    border-radius: 4px;
    font-size: 12px;
    display: none;
  }
  .status.ok { display: block; background: #1e3a2f; color: #a6e3a1; border: 1px solid #2d5a3f; }
  .status.err { display: block; background: #3a1e2f; color: #f38ba8; border: 1px solid #5a2d3f; }
  .status.info { display: block; background: #1e2e3a; color: #89b4fa; border: 1px solid #2d3f5a; }
  .hint { font-size: 11px; color: #6c7086; margin-top: 3px; }
</style>
</head>
<body>
  <h2>Telegram Notifier Settings</h2>

  <div class="field">
    <label for="token">Bot Token</label>
    <input type="password" id="token" value="${escapeHtml(config.botToken)}" placeholder="123456:ABC-DEF1234ghIkl-zyx57W2v1u123ew11">
    <div class="hint">Create a bot via <strong>@BotFather</strong> on Telegram</div>
  </div>

  <div class="field">
    <label for="chatid">Chat ID</label>
    <input type="text" id="chatid" value="${escapeHtml(config.chatId)}" placeholder="Auto-detected after you message the bot">
    <div class="hint">Send any message to your bot, then click "Auto-detect"</div>
  </div>

  <div>
    <button id="btn-detect">Auto-detect Chat ID</button>
    <button id="btn-test" class="secondary">Send Test</button>
    <button id="btn-save">Save</button>
  </div>

  <div id="status" class="status"></div>

  <h3>Event Types</h3>
  <table>
    <thead>
      <tr>
        <th>Event</th>
        <th>Send</th>
        <th>Silent</th>
      </tr>
    </thead>
    <tbody>
      ${eventRows}
    </tbody>
  </table>
  <div class="hint" style="margin-top:6px">
    <strong>Send</strong> = forward to Telegram &nbsp;&bull;&nbsp;
    <strong>Silent</strong> = deliver without sound/vibration on the phone
  </div>

  <div style="margin-top:16px; border-top:1px solid #45475a; padding-top:12px">
    <button id="btn-reset" class="danger">Reset to Defaults</button>
  </div>

<script>
  const $ = (sel) => document.querySelector(sel);
  const $$ = (sel) => document.querySelectorAll(sel);

  function showStatus(cls, msg) {
    const el = $("#status");
    el.className = "status " + cls;
    el.textContent = msg;
    if (cls === "ok" || cls === "info") {
      setTimeout(() => { el.className = "status"; el.textContent = ""; }, 4000);
    }
  }

  function gatherConfig() {
    const events = {};
    $$("input[data-event]").forEach((inp) => {
      const key = inp.dataset.event;
      if (!events[key]) events[key] = {};
      events[key][inp.dataset.field] = inp.checked;
    });
    return {
      botToken: $("#token").value.trim(),
      chatId: $("#chatid").value.trim(),
      events,
    };
  }

  $("#btn-save").addEventListener("click", () => {
    const cfg = gatherConfig();
    window.parent.postMessage({ type: "tg-save", config: cfg }, "*");
    showStatus("ok", "Saved!");
  });

  $("#btn-detect").addEventListener("click", () => {
    showStatus("info", "Checking for messages...");
    window.parent.postMessage({ type: "tg-detect", botToken: $("#token").value.trim() }, "*");
  });

  $("#btn-test").addEventListener("click", () => {
    showStatus("info", "Sending test message...");
    window.parent.postMessage({ type: "tg-test" }, "*");
  });

  $("#btn-reset").addEventListener("click", () => {
    window.parent.postMessage({ type: "tg-reset" }, "*");
    showStatus("info", "Reset to defaults. Reopening...");
  });

  window.addEventListener("message", (e) => {
    if (e.data && e.data.type === "tg-result") {
      showStatus(e.data.ok ? "ok" : "err", e.data.message);
      if (e.data.chatId) {
        $("#chatid").value = e.data.chatId;
      }
    }
  });
</script>
</body>
</html>`;
}

function escapeHtml(str) {
  return String(str)
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

// ── Exports ────────────────────────────────────────────────────────────────

export default {
  id: PLUGIN_ID,

  async onload(host) {
    hostRef = host;
    await loadConfig();

    // ── Activity Center section ──
    host.registerSection({
      id: SECTION_ID,
      label: "TELEGRAM",
      priority: 50,
      canDismissAll: true,
    });

    // ── Settings panel ──
    let panelHandle = null;

    function openSettings() {
      panelHandle = host.openPanel({
        id: "telegram-settings",
        title: "Telegram Notifier",
        html: buildSettingsHtml(),
      });
    }

    // Persistent item to open settings
    host.addItem({
      id: `${PLUGIN_ID}:settings`,
      pluginId: PLUGIN_ID,
      sectionId: SECTION_ID,
      title: "Telegram Notifier",
      subtitle: isConfigured() ? "Configured" : "Click to set up",
      icon: ICON_TELEGRAM,
      iconColor: isConfigured() ? "#89b4fa" : "var(--fg-muted)",
      dismissible: false,
      onClick: openSettings,
    });

    // Handle messages from the settings panel iframe
    const onMessage = async (e) => {
      if (!e.data || typeof e.data.type !== "string") return;

      if (e.data.type === "tg-save") {
        config = { ...defaultConfig(), ...e.data.config };
        await saveConfig();
        updateTicker();
        host.updateItem(`${PLUGIN_ID}:settings`, {
          subtitle: isConfigured() ? "Configured" : "Click to set up",
          iconColor: isConfigured() ? "#89b4fa" : "var(--fg-muted)",
        });
        host.log("info", "Configuration saved");
      }

      if (e.data.type === "tg-detect") {
        const origToken = config.botToken;
        config.botToken = e.data.botToken || config.botToken;
        const chatId = await autoDetectChatId();
        config.botToken = origToken;
        if (chatId) {
          broadcastToIframe({
            type: "tg-result", ok: true,
            message: `Chat ID detected: ${chatId}`,
            chatId,
          });
        } else {
          broadcastToIframe({
            type: "tg-result", ok: false,
            message: "No messages found. Send a message to your bot first.",
          });
        }
      }

      if (e.data.type === "tg-test") {
        const ok = await sendTelegram(
          `<b>TUICommander</b>\nTest notification from <b>${escapeHtml(repoLabel())}</b>`,
          false
        );
        broadcastToIframe({
          type: "tg-result",
          ok,
          message: ok ? "Test message sent!" : "Failed to send. Check bot token and chat ID.",
        });
      }

      if (e.data.type === "tg-reset") {
        config = defaultConfig();
        await saveConfig();
        updateTicker();
        setTimeout(openSettings, 200);
      }
    };

    window.addEventListener("message", onMessage);
    host._tgCleanup = () => window.removeEventListener("message", onMessage);

    function broadcastToIframe(msg) {
      const iframes = document.querySelectorAll("iframe");
      for (const iframe of iframes) {
        try {
          iframe.contentWindow.postMessage(msg, "*");
        } catch { /* sandboxed iframe may reject */ }
      }
    }

    // ── Structured event: question ──
    host.registerStructuredEventHandler("question", async (payload) => {
      const prompt = payload.prompt_text || "Input requested";
      await notify(
        "question",
        `<b>${escapeHtml(repoLabel())}</b>\nInput requested:\n<code>${escapeHtml(prompt)}</code>`
      );

      host.addItem({
        id: `${PLUGIN_ID}:question:${Date.now()}`,
        pluginId: PLUGIN_ID,
        sectionId: SECTION_ID,
        title: "Input requested",
        subtitle: truncate(prompt, 60),
        icon: ICON_TELEGRAM,
        iconColor: "var(--warning)",
        dismissible: true,
      });
    });

    // ── Structured event: rate-limit ──
    host.registerStructuredEventHandler("rate-limit", async (payload) => {
      const pattern = payload.pattern_name || "unknown";
      const retryMs = payload.retry_after_ms;
      const retryStr = retryMs ? ` (retry in ${Math.round(retryMs / 1000)}s)` : "";
      await notify(
        "rate-limit",
        `<b>${escapeHtml(repoLabel())}</b>\nRate limited: <code>${escapeHtml(pattern)}</code>${retryStr}`
      );
    });

    // ── Structured event: usage-limit ──
    host.registerStructuredEventHandler("usage-limit", async (payload) => {
      const pct = payload.percentage || 0;
      const type = payload.limit_type || "unknown";
      await notify(
        "usage-limit",
        `<b>${escapeHtml(repoLabel())}</b>\nUsage: ${pct}% of ${type} limit`
      );
    });

    // ── Structured event: pr-url ──
    host.registerStructuredEventHandler("pr-url", async (payload) => {
      const num = payload.number;
      const url = payload.url;
      const platform = payload.platform || "github";
      await notify(
        "pr-url",
        `<b>${escapeHtml(repoLabel())}</b>\n${platform.toUpperCase()} PR #${num}\n${url}`
      );
    });

    // ── Output watcher: CI failure ──
    host.registerOutputWatcher({
      pattern: /(?:FAILED|FAILURE|ERROR):\s*(.+)/i,
      onMatch(match) {
        const step = match[1].trim();
        notify(
          "ci-failure",
          `<b>${escapeHtml(repoLabel())}</b>\nCI failure: <code>${escapeHtml(step)}</code>`
        ).catch(() => {});
      },
    });

    // ── State change: agent stopped ──
    host.onStateChange(async (event) => {
      if (event.type === "agent-stopped") {
        await notify(
          "agent-stopped",
          `<b>${escapeHtml(repoLabel())}</b>\nAgent finished`
        );
      }
    });

    // ── Initial ticker ──
    updateTicker();
    host.log("info", "Telegram Notifier loaded", { configured: isConfigured() });
  },

  onunload() {
    if (hostRef && hostRef._tgCleanup) {
      hostRef._tgCleanup();
      delete hostRef._tgCleanup;
    }
    hostRef = null;
    config = defaultConfig();
    lastSent = {};
    sentCount = 0;
  },
};

// ── Helpers ────────────────────────────────────────────────────────────────

function truncate(str, max) {
  if (!str) return "";
  return str.length > max ? str.slice(0, max - 1) + "\u2026" : str;
}
