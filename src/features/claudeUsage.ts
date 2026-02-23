/**
 * Claude Usage Dashboard — native lifecycle manager.
 *
 * Manages the Activity Center section, ticker message, and API polling.
 * Called from App.tsx or a top-level effect when the feature is enabled/disabled.
 */

import { activityStore } from "../stores/activityStore";
import { statusBarTicker } from "../stores/statusBarTicker";
import { mdTabsStore } from "../stores/mdTabs";
import { invoke } from "../invoke";
import type { Disposable } from "../plugins/types";

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const FEATURE_ID = "claude-usage";
const SECTION_ID = "claude-usage";
const ACTIVITY_ITEM_ID = "claude-usage:summary";
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

/** Format a reset time from ISO string to short relative format */
function formatResetShort(iso: string): string {
  const diffMs = new Date(iso).getTime() - Date.now();
  if (diffMs <= 0) return "now";
  const min = Math.floor(diffMs / 60_000);
  if (min < 60) return `${min}m`;
  const hrs = Math.floor(min / 60);
  if (hrs < 48) return `${hrs}h`;
  const days = Math.floor(hrs / 24);
  return `${days}d`;
}

/** Build status bar ticker text from API data */
export function buildTickerText(api: UsageApiResponse): string {
  const parts: string[] = [];
  if (api.five_hour) {
    parts.push(`5h: ${Math.round(api.five_hour.utilization * 100)}%`);
  }
  if (api.seven_day) {
    parts.push(`7d: ${Math.round(api.seven_day.utilization * 100)}%`);
  }
  return parts.length > 0 ? `Claude: ${parts.join(" · ")}` : "Claude Usage";
}

/** Determine ticker priority from usage levels */
export function getTickerPriority(api: UsageApiResponse): number {
  const utils = [api.five_hour, api.seven_day, api.seven_day_opus, api.seven_day_sonnet]
    .filter((b): b is RateBucket => b !== null)
    .map((b) => b.utilization);
  const maxUtil = Math.max(0, ...utils);
  if (maxUtil >= 0.9) return 90;
  if (maxUtil >= 0.7) return 50;
  return 10;
}

/** Build subtitle for the activity center item */
function buildSubtitle(api: UsageApiResponse): string {
  const parts: string[] = [];
  if (api.five_hour) {
    const pct = Math.round(api.five_hour.utilization * 100);
    parts.push(`5h: ${pct}% (resets ${formatResetShort(api.five_hour.resets_at)})`);
  }
  if (api.seven_day) {
    const pct = Math.round(api.seven_day.utilization * 100);
    parts.push(`7d: ${pct}% (resets ${formatResetShort(api.seven_day.resets_at)})`);
  }
  return parts.join(" · ") || "No data";
}

// ---------------------------------------------------------------------------
// Lifecycle
// ---------------------------------------------------------------------------

let sectionDisposable: Disposable | null = null;
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
      text: buildTickerText(api),
      icon: CHART_SVG,
      priority: getTickerPriority(api),
      ttlMs: API_POLL_MS + 30_000,
      onClick: openDashboard,
    });

    // Update activity center item
    activityStore.updateItem(ACTIVITY_ITEM_ID, {
      subtitle: buildSubtitle(api),
    });
  } catch (err) {
    console.error("[claudeUsage] API poll failed:", err);
    statusBarTicker.addMessage({
      id: TICKER_ID,
      pluginId: FEATURE_ID,
      text: "Claude: offline",
      icon: CHART_SVG,
      priority: 5,
      ttlMs: API_POLL_MS + 30_000,
      onClick: openDashboard,
    });
  }
}

/** Initialize the Claude Usage feature (activity center, ticker, polling). */
export function initClaudeUsage(): void {
  if (initialized) return;
  initialized = true;

  // Register activity center section
  sectionDisposable = activityStore.registerSection({
    id: SECTION_ID,
    label: "CLAUDE USAGE",
    priority: 5,
    canDismissAll: false,
  });

  // Add summary item
  activityStore.addItem({
    id: ACTIVITY_ITEM_ID,
    pluginId: FEATURE_ID,
    sectionId: SECTION_ID,
    title: "Claude Usage Dashboard",
    subtitle: "Loading...",
    icon: CHART_SVG,
    dismissible: false,
    onClick: openDashboard,
  });

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
  activityStore.removeItem(ACTIVITY_ITEM_ID);
  sectionDisposable?.dispose();
  sectionDisposable = null;
}

/** Check if the feature is currently active. */
export function isClaudeUsageActive(): boolean {
  return initialized;
}
