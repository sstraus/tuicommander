import type { Disposable, PanelHandle, PluginHost, TuiPlugin } from "./types";

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const PLUGIN_ID = "claude-usage";
const SECTION_ID = "claude-usage";
const ACTIVITY_ITEM_ID = "claude-usage:summary";
const TICKER_ID = "claude-usage:rate";
const PANEL_ID = "claude-usage-dashboard";

/** Poll API every 5 minutes */
const API_POLL_MS = 5 * 60 * 1000;
/** Debounce re-renders to max 1/second */
const RENDER_DEBOUNCE_MS = 1000;

const CLAUDE_DIR = `${
  typeof process !== "undefined" && process.env?.HOME
    ? process.env.HOME
    : "~"
}/.claude`;

// Dashboard chart icon
const CHART_SVG = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="currentColor">
  <path d="M0 11.5a.5.5 0 0 1 .5-.5h4a.5.5 0 0 1 .5.5v4a.5.5 0 0 1-.5.5h-4a.5.5 0 0 1-.5-.5v-4zm6-4a.5.5 0 0 1 .5-.5h4a.5.5 0 0 1 .5.5v8a.5.5 0 0 1-.5.5h-4a.5.5 0 0 1-.5-.5v-8zm6-7a.5.5 0 0 1 .5-.5h3a.5.5 0 0 1 .5.5v15a.5.5 0 0 1-.5.5h-3a.5.5 0 0 1-.5-.5V.5z"/>
</svg>`;

// ---------------------------------------------------------------------------
// Types — API response shape (verified live 2026-02-23)
// ---------------------------------------------------------------------------

/** A single rate limit bucket from the usage API */
export interface RateBucket {
  utilization: number;
  resets_at: string;
}

/** Full API response from api.anthropic.com/api/oauth/usage */
export interface UsageApiResponse {
  five_hour: RateBucket | null;
  seven_day: RateBucket | null;
  seven_day_opus: RateBucket | null;
  seven_day_sonnet: RateBucket | null;
  seven_day_cowork: RateBucket | null;
  seven_day_oauth_apps: RateBucket | null;
  extra_usage: {
    is_enabled: boolean;
    monthly_limit: number | null;
    used_credits: number | null;
    utilization: number | null;
  } | null;
}

// ---------------------------------------------------------------------------
// Types — local filesystem data
// ---------------------------------------------------------------------------

/** Parsed hud-budget.json */
export interface BudgetData {
  fiveHourMinutes: number;
  fiveHourLimit: number;
  sevenDayMinutes: number;
  sevenDayLimit: number;
  updatedAt: string | null;
}

/** A single day's activity from stats-cache.json */
export interface DailyActivity {
  date: string;
  messages: number;
  tokens: number;
  sessions: number;
}

/** Per-model token usage from stats-cache.json */
export interface ModelUsage {
  model: string;
  inputTokens: number;
  outputTokens: number;
}

/** Parsed stats-cache.json */
export interface StatsData {
  dailyActivity: DailyActivity[];
  modelUsage: ModelUsage[];
  totalSessions: number;
  totalMessages: number;
  totalTokens: number;
}

/** A single session from hud-tracking.jsonl */
export interface TrackingSession {
  sessionId: string;
  model?: string;
  inputTokens?: number;
  outputTokens?: number;
  costUsd?: number;
  timestamp?: string;
}

/** Combined dashboard state */
export interface DashboardState {
  apiData: UsageApiResponse | null;
  budget: BudgetData | null;
  stats: StatsData | null;
  recentSessions: TrackingSession[];
  apiError: string | null;
  lastApiPoll: number | null;
}

// ---------------------------------------------------------------------------
// Parsers (exported for testing)
// ---------------------------------------------------------------------------

/**
 * Parse hud-budget.json content.
 * Expected shape: { five_hour: { used_minutes, limit_minutes }, seven_day: { ... }, updated_at? }
 */
export function parseBudget(json: string): BudgetData | null {
  try {
    const raw = JSON.parse(json);
    return {
      fiveHourMinutes: raw?.five_hour?.used_minutes ?? 0,
      fiveHourLimit: raw?.five_hour?.limit_minutes ?? 300,
      sevenDayMinutes: raw?.seven_day?.used_minutes ?? 0,
      sevenDayLimit: raw?.seven_day?.limit_minutes ?? 2100,
      updatedAt: raw?.updated_at ?? null,
    };
  } catch {
    return null;
  }
}

/**
 * Parse stats-cache.json content.
 * Expected shape: { daily_activity: [...], model_usage: [...], total_sessions, total_messages }
 */
export function parseStatsCache(json: string): StatsData | null {
  try {
    const raw = JSON.parse(json);
    const dailyActivity: DailyActivity[] = (raw?.daily_activity ?? []).map(
      (d: Record<string, unknown>) => ({
        date: String(d.date ?? ""),
        messages: Number(d.messages ?? 0),
        tokens: Number(d.tokens ?? 0),
        sessions: Number(d.sessions ?? 0),
      }),
    );
    const modelUsage: ModelUsage[] = (raw?.model_usage ?? []).map(
      (m: Record<string, unknown>) => ({
        model: String(m.model ?? "unknown"),
        inputTokens: Number(m.input_tokens ?? 0),
        outputTokens: Number(m.output_tokens ?? 0),
      }),
    );
    const totalTokens = modelUsage.reduce(
      (sum, m) => sum + m.inputTokens + m.outputTokens,
      0,
    );
    return {
      dailyActivity,
      modelUsage,
      totalSessions: Number(raw?.total_sessions ?? 0),
      totalMessages: Number(raw?.total_messages ?? 0),
      totalTokens,
    };
  } catch {
    return null;
  }
}

/**
 * Parse the tail of hud-tracking.jsonl (one JSON object per line).
 * Skips malformed lines gracefully.
 */
export function parseTrackingTail(text: string): TrackingSession[] {
  const sessions: TrackingSession[] = [];
  const lines = text.split("\n").filter((l) => l.trim().length > 0);
  for (const line of lines) {
    try {
      const raw = JSON.parse(line);
      sessions.push({
        sessionId: String(raw.session_id ?? raw.sessionId ?? ""),
        model: raw.model ?? undefined,
        inputTokens: raw.input_tokens ?? raw.inputTokens ?? undefined,
        outputTokens: raw.output_tokens ?? raw.outputTokens ?? undefined,
        costUsd: raw.cost_usd ?? raw.costUsd ?? undefined,
        timestamp: raw.timestamp ?? raw.created_at ?? undefined,
      });
    } catch {
      // Skip malformed lines
    }
  }
  return sessions;
}

/**
 * Parse the usage API response. Accepts the raw JSON string from httpFetch.
 */
export function parseUsageApiResponse(json: string): UsageApiResponse | null {
  try {
    const raw = JSON.parse(json);
    return {
      five_hour: raw.five_hour ?? null,
      seven_day: raw.seven_day ?? null,
      seven_day_opus: raw.seven_day_opus ?? null,
      seven_day_sonnet: raw.seven_day_sonnet ?? null,
      seven_day_cowork: raw.seven_day_cowork ?? null,
      seven_day_oauth_apps: raw.seven_day_oauth_apps ?? null,
      extra_usage: raw.extra_usage ?? null,
    };
  } catch {
    return null;
  }
}

// ---------------------------------------------------------------------------
// Format helpers (exported for testing)
// ---------------------------------------------------------------------------

/** Format a reset time as relative countdown like "2h 15m" or "3d 5h" for >48h */
export function formatResetTime(isoString: string): string {
  const resetMs = new Date(isoString).getTime();
  const nowMs = Date.now();
  const diffMs = resetMs - nowMs;
  if (diffMs <= 0) return "now";
  const totalHours = Math.floor(diffMs / 3_600_000);
  if (totalHours > 48) {
    const days = Math.floor(totalHours / 24);
    const remainingHours = totalHours % 24;
    return `${days}d ${remainingHours}h`;
  }
  const minutes = Math.floor((diffMs % 3_600_000) / 60_000);
  if (totalHours > 0) return `${totalHours}h ${minutes}m`;
  return `${minutes}m`;
}

/** Format a number with K/M suffixes */
export function formatNumber(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return String(n);
}

/** Build ticker text from API data */
export function buildTickerText(api: UsageApiResponse): string {
  const parts: string[] = [];
  if (api.five_hour) parts.push(`5h: ${Math.round(api.five_hour.utilization)}%`);
  if (api.seven_day) parts.push(`7d: ${Math.round(api.seven_day.utilization)}%`);
  return parts.length > 0 ? `Claude: ${parts.join(" · ")}` : "Claude: --";
}

/** Determine ticker priority based on utilization */
export function getTickerPriority(api: UsageApiResponse): number {
  const maxUtil = Math.max(
    api.five_hour?.utilization ?? 0,
    api.seven_day?.utilization ?? 0,
  );
  if (maxUtil >= 90) return 90;
  if (maxUtil >= 80) return 80;
  return 10;
}

// ---------------------------------------------------------------------------
// Dashboard HTML renderer (exported for testing)
// ---------------------------------------------------------------------------

/** Color for a progress bar based on utilization percentage */
function progressColor(pct: number): string {
  if (pct >= 90) return "#f48771";
  if (pct >= 70) return "#dcdcaa";
  if (pct >= 50) return "#59a8dd";
  return "#3fb950";
}

/** Generate a single progress bar row */
function renderProgressBar(label: string, pct: number, resetTime: string | null): string {
  const color = progressColor(pct);
  const rounded = Math.round(pct);
  const resetHtml = resetTime
    ? `<span style="color:#888;font-size:11px;margin-left:8px">resets in ${formatResetTime(resetTime)}</span>`
    : "";
  return `
    <div style="margin-bottom:12px">
      <div style="display:flex;justify-content:space-between;margin-bottom:4px">
        <span>${label}</span>
        <span style="color:${color};font-weight:600">${rounded}%${resetHtml}</span>
      </div>
      <div style="background:#333;border-radius:4px;height:8px;overflow:hidden">
        <div style="background:${color};height:100%;width:${Math.min(rounded, 100)}%;border-radius:4px;transition:width 0.3s"></div>
      </div>
    </div>`;
}

/** Generate SVG activity heatmap (GitHub-style, last 12 weeks) */
function renderHeatmap(dailyActivity: DailyActivity[]): string {
  // Build a map of date → messages for quick lookup
  const dateMap = new Map<string, number>();
  let maxMessages = 1;
  for (const day of dailyActivity) {
    dateMap.set(day.date, day.messages);
    if (day.messages > maxMessages) maxMessages = day.messages;
  }

  const weeks = 12;
  const cellSize = 12;
  const gap = 2;
  const cols = weeks;
  const rows = 7;
  const svgW = cols * (cellSize + gap) + 30;
  const svgH = rows * (cellSize + gap) + 20;

  const dayLabels = ["", "M", "", "W", "", "F", ""];
  let cells = "";

  // Start from (weeks) weeks ago
  const now = new Date();
  const startDate = new Date(now);
  startDate.setDate(startDate.getDate() - (weeks * 7 - 1));
  // Align to Monday
  const dayOfWeek = startDate.getDay();
  startDate.setDate(startDate.getDate() - ((dayOfWeek + 6) % 7));

  for (let week = 0; week < cols; week++) {
    for (let day = 0; day < rows; day++) {
      const d = new Date(startDate);
      d.setDate(d.getDate() + week * 7 + day);
      const dateStr = d.toISOString().slice(0, 10);
      const messages = dateMap.get(dateStr) ?? 0;
      const intensity = messages === 0 ? 0 : Math.min(messages / maxMessages, 1);
      const alpha = messages === 0 ? 0.05 : 0.15 + intensity * 0.85;
      const color = messages === 0 ? "rgba(255,255,255,0.05)" : `rgba(63,185,80,${alpha.toFixed(2)})`;
      const x = 30 + week * (cellSize + gap);
      const y = day * (cellSize + gap);
      cells += `<rect x="${x}" y="${y}" width="${cellSize}" height="${cellSize}" rx="2" fill="${color}"><title>${dateStr}: ${messages} msgs</title></rect>`;
    }
  }

  // Day labels on left
  let labels = "";
  for (let i = 0; i < rows; i++) {
    if (dayLabels[i]) {
      labels += `<text x="20" y="${i * (cellSize + gap) + cellSize - 1}" text-anchor="end" fill="#888" font-size="10">${dayLabels[i]}</text>`;
    }
  }

  return `
    <div style="margin-top:16px">
      <h3 style="margin:0 0 8px;font-size:13px;color:#ccc">Activity (12 weeks)</h3>
      <svg width="${svgW}" height="${svgH}" style="display:block">${labels}${cells}</svg>
    </div>`;
}

/** Render model usage breakdown */
function renderModelUsage(modelUsage: ModelUsage[]): string {
  if (modelUsage.length === 0) return "";
  const sorted = [...modelUsage].sort(
    (a, b) => b.inputTokens + b.outputTokens - (a.inputTokens + a.outputTokens),
  );
  const rows = sorted
    .map(
      (m) =>
        `<tr><td style="padding:2px 8px 2px 0;color:#ccc">${m.model}</td><td style="padding:2px 8px;text-align:right">${formatNumber(m.inputTokens)}</td><td style="padding:2px 8px;text-align:right">${formatNumber(m.outputTokens)}</td></tr>`,
    )
    .join("");
  return `
    <div style="margin-top:16px">
      <h3 style="margin:0 0 8px;font-size:13px;color:#ccc">Token Usage by Model</h3>
      <table style="font-size:12px;border-collapse:collapse;width:100%">
        <thead><tr style="color:#888"><th style="text-align:left;padding:2px 8px 2px 0">Model</th><th style="text-align:right;padding:2px 8px">Input</th><th style="text-align:right;padding:2px 8px">Output</th></tr></thead>
        <tbody>${rows}</tbody>
      </table>
    </div>`;
}

/** Render the complete dashboard HTML */
export function renderDashboardHtml(state: DashboardState): string {
  const { apiData, budget, stats } = state;

  // Rate limits section
  let rateLimitsHtml = "";
  if (apiData) {
    if (apiData.five_hour) {
      rateLimitsHtml += renderProgressBar(
        "5-Hour Limit",
        apiData.five_hour.utilization,
        apiData.five_hour.resets_at,
      );
    }
    if (apiData.seven_day) {
      rateLimitsHtml += renderProgressBar(
        "7-Day Limit",
        apiData.seven_day.utilization,
        apiData.seven_day.resets_at,
      );
    }
    if (apiData.seven_day_sonnet) {
      rateLimitsHtml += renderProgressBar(
        "7-Day Sonnet",
        apiData.seven_day_sonnet.utilization,
        apiData.seven_day_sonnet.resets_at,
      );
    }
    if (apiData.seven_day_opus) {
      rateLimitsHtml += renderProgressBar(
        "7-Day Opus",
        apiData.seven_day_opus.utilization,
        apiData.seven_day_opus.resets_at,
      );
    }
    if (apiData.extra_usage?.is_enabled && apiData.extra_usage.utilization != null) {
      rateLimitsHtml += renderProgressBar(
        "Extra Usage",
        apiData.extra_usage.utilization,
        null,
      );
    }
  } else if (state.apiError) {
    rateLimitsHtml = `<div style="color:#f48771;font-size:12px;margin-bottom:12px">${escapeHtml(state.apiError)}</div>`;
  } else {
    rateLimitsHtml = `<div style="color:#888;font-size:12px;margin-bottom:12px">Loading rate limits...</div>`;
  }

  // Budget section (from local file)
  let budgetHtml = "";
  if (budget) {
    const fiveHourPct = budget.fiveHourLimit > 0 ? (budget.fiveHourMinutes / budget.fiveHourLimit) * 100 : 0;
    const sevenDayPct = budget.sevenDayLimit > 0 ? (budget.sevenDayMinutes / budget.sevenDayLimit) * 100 : 0;
    budgetHtml = `
      <div style="margin-top:16px">
        <h3 style="margin:0 0 8px;font-size:13px;color:#ccc">Local Budget (minutes used)</h3>
        ${renderProgressBar(`5h: ${Math.round(budget.fiveHourMinutes)}/${budget.fiveHourLimit} min`, fiveHourPct, null)}
        ${renderProgressBar(`7d: ${Math.round(budget.sevenDayMinutes)}/${budget.sevenDayLimit} min`, sevenDayPct, null)}
      </div>`;
  }

  // Stats summary
  let statsHtml = "";
  if (stats) {
    statsHtml = `
      <div style="margin-top:16px;display:flex;gap:16px;flex-wrap:wrap">
        <div style="background:#252525;border-radius:6px;padding:8px 14px;min-width:80px;text-align:center">
          <div style="font-size:18px;font-weight:600;color:#59a8dd">${formatNumber(stats.totalSessions)}</div>
          <div style="font-size:11px;color:#888">Sessions</div>
        </div>
        <div style="background:#252525;border-radius:6px;padding:8px 14px;min-width:80px;text-align:center">
          <div style="font-size:18px;font-weight:600;color:#c586c0">${formatNumber(stats.totalMessages)}</div>
          <div style="font-size:11px;color:#888">Messages</div>
        </div>
        <div style="background:#252525;border-radius:6px;padding:8px 14px;min-width:80px;text-align:center">
          <div style="font-size:18px;font-weight:600;color:#4ec9b0">${formatNumber(stats.totalTokens)}</div>
          <div style="font-size:11px;color:#888">Tokens</div>
        </div>
      </div>`;
  }

  // Activity heatmap
  const heatmapHtml = stats?.dailyActivity.length ? renderHeatmap(stats.dailyActivity) : "";

  // Model usage table
  const modelHtml = stats?.modelUsage.length ? renderModelUsage(stats.modelUsage) : "";

  // Extra usage info
  let extraHtml = "";
  if (apiData?.extra_usage?.is_enabled && apiData.extra_usage.used_credits != null) {
    const limit = apiData.extra_usage.monthly_limit;
    const limitStr = limit != null ? ` / $${(limit / 100).toFixed(2)}` : " (no limit)";
    extraHtml = `
      <div style="margin-top:16px">
        <h3 style="margin:0 0 8px;font-size:13px;color:#ccc">Extra Usage</h3>
        <div style="font-size:12px;color:#dcdcaa">$${(apiData.extra_usage.used_credits / 100).toFixed(2)}${limitStr} this month</div>
      </div>`;
  }

  // Last poll info
  const lastPollStr = state.lastApiPoll
    ? new Date(state.lastApiPoll).toLocaleTimeString()
    : "never";

  return `<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<style>
  * { box-sizing: border-box; margin: 0; padding: 0; }
  body {
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, monospace;
    background: #1e1e1e;
    color: #d4d4d4;
    padding: 16px;
    font-size: 13px;
    line-height: 1.5;
  }
  h2 { font-size: 15px; font-weight: 600; color: #e0e0e0; margin-bottom: 12px; }
  h3 { font-size: 13px; font-weight: 500; }
</style>
</head>
<body>
  <h2>Claude Usage Dashboard</h2>
  ${rateLimitsHtml}
  ${budgetHtml}
  ${statsHtml}
  ${heatmapHtml}
  ${modelHtml}
  ${extraHtml}
  <div style="margin-top:20px;font-size:10px;color:#555;text-align:right">
    Last API poll: ${lastPollStr}
  </div>
</body>
</html>`;
}

// ---------------------------------------------------------------------------
// HTML escaping
// ---------------------------------------------------------------------------

function escapeHtml(str: string): string {
  return str
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

// ---------------------------------------------------------------------------
// Plugin implementation
// ---------------------------------------------------------------------------

class ClaudeUsagePlugin implements TuiPlugin {
  readonly id = PLUGIN_ID;

  private host: PluginHost | null = null;
  private state: DashboardState = {
    apiData: null,
    budget: null,
    stats: null,
    recentSessions: [],
    apiError: null,
    lastApiPoll: null,
  };
  private panelHandle: PanelHandle | null = null;
  private apiPollTimer: ReturnType<typeof setInterval> | null = null;
  private renderDebounceTimer: ReturnType<typeof setTimeout> | null = null;
  private disposables: Disposable[] = [];

  onload(host: PluginHost): void {
    this.host = host;

    // Register Activity Center section
    host.registerSection({
      id: SECTION_ID,
      label: "CLAUDE USAGE",
      priority: 5,
      canDismissAll: false,
    });

    // Add summary item to activity center
    host.addItem({
      id: ACTIVITY_ITEM_ID,
      pluginId: PLUGIN_ID,
      sectionId: SECTION_ID,
      title: "Claude Usage Dashboard",
      subtitle: "Loading...",
      icon: CHART_SVG,
      dismissible: false,
      onClick: () => this.openDashboard(),
    });

    // Start data loading
    this.loadFilesystemData();
    this.pollApi();

    // Set up API polling interval
    this.apiPollTimer = setInterval(() => this.pollApi(), API_POLL_MS);

    // Set up file watchers
    this.setupFileWatchers();
  }

  onunload(): void {
    if (this.apiPollTimer) {
      clearInterval(this.apiPollTimer);
      this.apiPollTimer = null;
    }
    if (this.renderDebounceTimer) {
      clearTimeout(this.renderDebounceTimer);
      this.renderDebounceTimer = null;
    }
    if (this.panelHandle) {
      this.panelHandle.close();
      this.panelHandle = null;
    }
    for (const d of this.disposables) {
      try { d.dispose(); } catch { /* best-effort */ }
    }
    this.disposables = [];
    this.host = null;
  }

  // -----------------------------------------------------------------------
  // Data loading
  // -----------------------------------------------------------------------

  private async loadFilesystemData(): Promise<void> {
    if (!this.host) return;

    // Load budget
    try {
      const budgetJson = await this.host.readFile(`${CLAUDE_DIR}/hud-budget.json`);
      this.state.budget = parseBudget(budgetJson);
    } catch {
      this.host?.log("debug", "hud-budget.json not found or unreadable");
    }

    if (!this.host) return;

    // Load stats cache
    try {
      const statsJson = await this.host.readFile(`${CLAUDE_DIR}/stats-cache.json`);
      this.state.stats = parseStatsCache(statsJson);
    } catch {
      this.host?.log("debug", "stats-cache.json not found or unreadable");
    }

    if (!this.host) return;

    // Load recent tracking sessions (last 512KB)
    try {
      const trackingTail = await this.host.readFileTail(
        `${CLAUDE_DIR}/hud-tracking.jsonl`,
        512 * 1024,
      );
      this.state.recentSessions = parseTrackingTail(trackingTail);
    } catch {
      this.host?.log("debug", "hud-tracking.jsonl not found or unreadable");
    }

    this.scheduleRender();
  }

  private async pollApi(): Promise<void> {
    if (!this.host) return;

    try {
      // Read credential
      const credJson = await this.host.readCredential("Claude Code-credentials");
      if (!this.host) return;
      if (!credJson) {
        this.state.apiError = "No Claude Code credentials found. Please authenticate Claude Code first.";
        this.state.apiData = null;
        this.scheduleRender();
        return;
      }

      let accessToken: string;
      try {
        const creds = JSON.parse(credJson);
        accessToken = creds?.claudeAiOauth?.accessToken;
        if (!accessToken) {
          this.state.apiError = "Credentials found but missing OAuth access token.";
          this.state.apiData = null;
          this.scheduleRender();
          return;
        }
      } catch {
        this.state.apiError = "Failed to parse credential JSON.";
        this.state.apiData = null;
        this.scheduleRender();
        return;
      }

      if (!this.host) return;

      // Fetch usage API
      const resp = await this.host.httpFetch(
        "https://api.anthropic.com/api/oauth/usage",
        {
          headers: {
            Authorization: `Bearer ${accessToken}`,
            "anthropic-beta": "oauth-2025-04-20",
          },
        },
      );

      if (resp.status === 401) {
        this.state.apiError = "OAuth token expired. Please re-authenticate Claude Code.";
        this.state.apiData = null;
      } else if (resp.status === 429) {
        this.state.apiError = "Rate limited by Anthropic API. Will retry.";
        // Keep stale data if we have it
      } else if (resp.status >= 200 && resp.status < 300) {
        const parsed = parseUsageApiResponse(resp.body);
        if (parsed) {
          this.state.apiData = parsed;
          this.state.apiError = null;
        } else {
          this.state.apiError = "Unexpected API response format.";
        }
      } else {
        this.state.apiError = `API returned status ${resp.status}`;
      }

      this.state.lastApiPoll = Date.now();
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      this.state.apiError = `API fetch failed: ${msg}`;
      this.host?.log("warn", `API poll failed: ${msg}`);
    }

    if (!this.host) return;
    this.updateTickerAndActivity();
    this.scheduleRender();
  }

  // -----------------------------------------------------------------------
  // File watchers
  // -----------------------------------------------------------------------

  private async setupFileWatchers(): Promise<void> {
    if (!this.host) return;

    try {
      const watcher = await this.host.watchPath(
        `${CLAUDE_DIR}`,
        (events) => {
          const relevant = events.some(
            (e) =>
              e.path.endsWith("hud-budget.json") ||
              e.path.endsWith("stats-cache.json") ||
              e.path.endsWith("hud-tracking.jsonl"),
          );
          if (relevant) {
            this.loadFilesystemData();
          }
        },
        { recursive: false, debounceMs: 500 },
      );
      this.disposables.push(watcher);
    } catch {
      this.host?.log("warn", "Could not set up file watcher for ~/.claude/");
    }
  }

  // -----------------------------------------------------------------------
  // Rendering
  // -----------------------------------------------------------------------

  private scheduleRender(): void {
    if (this.renderDebounceTimer) return;
    this.renderDebounceTimer = setTimeout(() => {
      this.renderDebounceTimer = null;
      this.render();
    }, RENDER_DEBOUNCE_MS);
  }

  private render(): void {
    if (!this.panelHandle) return;
    const html = renderDashboardHtml(this.state);
    this.panelHandle.update(html);
  }

  // -----------------------------------------------------------------------
  // Activity Center + Ticker
  // -----------------------------------------------------------------------

  private updateTickerAndActivity(): void {
    if (!this.host) return;
    const api = this.state.apiData;

    if (api) {
      // Update ticker
      const text = buildTickerText(api);
      const priority = getTickerPriority(api);
      this.host.postTickerMessage({
        id: TICKER_ID,
        text,
        icon: CHART_SVG,
        priority,
        ttlMs: API_POLL_MS + 30_000, // Slightly longer than poll interval
      });

      // Update activity item subtitle
      const parts: string[] = [];
      if (api.five_hour) parts.push(`5h: ${Math.round(api.five_hour.utilization)}%`);
      if (api.seven_day) parts.push(`7d: ${Math.round(api.seven_day.utilization)}%`);
      this.host.updateItem(ACTIVITY_ITEM_ID, {
        subtitle: parts.join(" | ") || "Connected",
      });
    } else if (this.state.apiError) {
      this.host.updateItem(ACTIVITY_ITEM_ID, {
        subtitle: this.state.apiError,
      });
    }
  }

  // -----------------------------------------------------------------------
  // Dashboard panel
  // -----------------------------------------------------------------------

  private openDashboard(): void {
    if (!this.host) return;

    if (this.panelHandle) {
      // Panel already open — just refresh
      this.render();
      return;
    }

    const html = renderDashboardHtml(this.state);
    this.panelHandle = this.host.openPanel({
      id: PANEL_ID,
      title: "Claude Usage",
      html,
    });
  }
}

export const claudeUsagePlugin: TuiPlugin = new ClaudeUsagePlugin();
