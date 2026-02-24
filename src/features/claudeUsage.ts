/**
 * Claude Usage Dashboard — native lifecycle manager.
 *
 * Manages the status bar ticker message and API polling.
 * Called from plugins/index.ts when the feature is enabled/disabled.
 */

import { statusBarTicker } from "../stores/statusBarTicker";
import { mdTabsStore } from "../stores/mdTabs";
import { invoke } from "../invoke";

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const FEATURE_ID = "claude-usage";
const TICKER_ID = "claude-usage:rate";

/** Poll API every 5 minutes */
const API_POLL_MS = 5 * 60 * 1000;

/** Chart icon (inline SVG, monochrome) */
const CHART_SVG = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="currentColor"><path d="M0 11.5a.5.5 0 0 1 .5-.5h4a.5.5 0 0 1 .5.5v4a.5.5 0 0 1-.5.5h-4a.5.5 0 0 1-.5-.5v-4zm6-4a.5.5 0 0 1 .5-.5h4a.5.5 0 0 1 .5.5v8a.5.5 0 0 1-.5.5h-4a.5.5 0 0 1-.5-.5v-8zm6-7a.5.5 0 0 1 .5-.5h3a.5.5 0 0 1 .5.5v15a.5.5 0 0 1-.5.5h-3a.5.5 0 0 1-.5-.5V.5z"/></svg>`;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface RateBucket {
  utilization: number;
  resets_at: string;
}

interface UsageApiResponse {
  five_hour: RateBucket | null;
  seven_day: RateBucket | null;
  seven_day_opus: RateBucket | null;
  seven_day_sonnet: RateBucket | null;
  seven_day_cowork: RateBucket | null;
  extra_usage: unknown;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Build status bar ticker text from API data.
 * The API returns utilization as a direct percentage (e.g. 3.0 = 3%, 68.0 = 68%). */
export function buildTickerText(api: UsageApiResponse): string {
  const parts: string[] = [];
  if (api.five_hour) {
    parts.push(`5h: ${Math.round(api.five_hour.utilization)}%`);
  }
  if (api.seven_day) {
    parts.push(`7d: ${Math.round(api.seven_day.utilization)}%`);
  }
  return parts.length > 0 ? parts.join(" · ") : "no data";
}

/** Determine ticker priority from usage levels.
 * Utilization values are direct percentages (0-100). */
export function getTickerPriority(api: UsageApiResponse): number {
  const utils = [api.five_hour, api.seven_day, api.seven_day_opus, api.seven_day_sonnet]
    .filter((b): b is RateBucket => b !== null)
    .map((b) => b.utilization);
  const maxUtil = Math.max(0, ...utils);
  if (maxUtil >= 90) return 90;
  if (maxUtil >= 70) return 50;
  return 10;
}

// ---------------------------------------------------------------------------
// Lifecycle
// ---------------------------------------------------------------------------

let pollTimer: ReturnType<typeof setInterval> | null = null;
let initialized = false;

function openDashboard(): void {
  mdTabsStore.addClaudeUsage();
}

async function poll(): Promise<void> {
  try {
    const api = await invoke<UsageApiResponse>("get_claude_usage_api");

    // Update ticker
    statusBarTicker.addMessage({
      id: TICKER_ID,
      pluginId: FEATURE_ID,
      label: "Usage",
      text: buildTickerText(api),
      icon: CHART_SVG,
      priority: getTickerPriority(api),
      ttlMs: API_POLL_MS + 30_000,
      onClick: openDashboard,
    });

  } catch (err) {
    console.error("[claudeUsage] API poll failed:", err);
    const errStr = String(err);
    const isTokenMissing = errStr.includes("No Claude OAuth token");
    const isAuthError = errStr.includes("401") || errStr.includes("403");
    const isParseError = errStr.includes("Failed to parse");
    const text = isTokenMissing
      ? "no token"
      : isAuthError
        ? "token expired"
        : isParseError
          ? "API changed"
          : "offline";
    statusBarTicker.addMessage({
      id: TICKER_ID,
      pluginId: FEATURE_ID,
      label: "Usage",
      text,
      icon: CHART_SVG,
      priority: 5,
      ttlMs: API_POLL_MS + 30_000,
      onClick: openDashboard,
    });
  }
}

/** Initialize the Claude Usage feature (status bar ticker + polling). */
export function initClaudeUsage(): void {
  if (initialized) return;
  initialized = true;

  // Initial poll + interval
  poll();
  pollTimer = setInterval(poll, API_POLL_MS);
}

/** Tear down the Claude Usage feature. */
export function destroyClaudeUsage(): void {
  if (!initialized) return;
  initialized = false;

  if (pollTimer) {
    clearInterval(pollTimer);
    pollTimer = null;
  }

  statusBarTicker.removeMessage(TICKER_ID, FEATURE_ID);
}

/** Check if the feature is currently active. */
export function isClaudeUsageActive(): boolean {
  return initialized;
}
