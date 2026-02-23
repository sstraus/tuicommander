import { describe, it, expect, beforeEach, vi, afterEach } from "vitest";

// Mock invoke and listen before any imports that use them
vi.mock("../../invoke", () => ({
  invoke: vi.fn(),
  listen: vi.fn().mockResolvedValue(() => {}),
}));

// Mock @tauri-apps/plugin-dialog for credential consent
vi.mock("@tauri-apps/plugin-dialog", () => ({
  confirm: vi.fn().mockResolvedValue(true),
}));

import { invoke } from "../../invoke";
import { pluginRegistry } from "../../plugins/pluginRegistry";
import { activityStore } from "../../stores/activityStore";
import { statusBarTicker } from "../../stores/statusBarTicker";
import { claudeUsagePlugin } from "../../plugins/claudeUsagePlugin";
import {
  parseBudget,
  parseStatsCache,
  parseTrackingTail,
  parseUsageApiResponse,
  buildTickerText,
  getTickerPriority,
  formatResetTime,
  formatNumber,
  renderDashboardHtml,
} from "../../plugins/claudeUsagePlugin";
import type { UsageApiResponse, DashboardState } from "../../plugins/claudeUsagePlugin";

const mockedInvoke = vi.mocked(invoke);

beforeEach(() => {
  pluginRegistry.clear();
  activityStore.clearAll();
  statusBarTicker.clear();
  mockedInvoke.mockReset();
  vi.useFakeTimers({ shouldAdvanceTime: true });
});

afterEach(() => {
  vi.useRealTimers();
});

// ---------------------------------------------------------------------------
// Parsers
// ---------------------------------------------------------------------------

describe("parseBudget", () => {
  it("parses valid hud-budget.json", () => {
    const json = JSON.stringify({
      five_hour: { used_minutes: 45, limit_minutes: 300 },
      seven_day: { used_minutes: 600, limit_minutes: 2100 },
      updated_at: "2026-02-23T12:00:00Z",
    });
    const result = parseBudget(json);
    expect(result).toEqual({
      fiveHourMinutes: 45,
      fiveHourLimit: 300,
      sevenDayMinutes: 600,
      sevenDayLimit: 2100,
      updatedAt: "2026-02-23T12:00:00Z",
    });
  });

  it("handles missing fields with defaults", () => {
    const result = parseBudget("{}");
    expect(result).toEqual({
      fiveHourMinutes: 0,
      fiveHourLimit: 300,
      sevenDayMinutes: 0,
      sevenDayLimit: 2100,
      updatedAt: null,
    });
  });

  it("returns null for invalid JSON", () => {
    expect(parseBudget("not json")).toBeNull();
  });

  it("returns null for empty string", () => {
    expect(parseBudget("")).toBeNull();
  });
});

describe("parseStatsCache", () => {
  it("parses valid stats-cache.json", () => {
    const json = JSON.stringify({
      daily_activity: [
        { date: "2026-02-22", messages: 50, tokens: 10000, sessions: 3 },
        { date: "2026-02-23", messages: 30, tokens: 8000, sessions: 2 },
      ],
      model_usage: [
        { model: "claude-sonnet-4-6", input_tokens: 5000, output_tokens: 3000 },
        { model: "claude-opus-4-6", input_tokens: 2000, output_tokens: 1000 },
      ],
      total_sessions: 100,
      total_messages: 2000,
    });
    const result = parseStatsCache(json);
    expect(result).not.toBeNull();
    expect(result!.dailyActivity).toHaveLength(2);
    expect(result!.modelUsage).toHaveLength(2);
    expect(result!.totalSessions).toBe(100);
    expect(result!.totalMessages).toBe(2000);
    expect(result!.totalTokens).toBe(11000);
  });

  it("handles empty data gracefully", () => {
    const result = parseStatsCache("{}");
    expect(result).not.toBeNull();
    expect(result!.dailyActivity).toEqual([]);
    expect(result!.modelUsage).toEqual([]);
    expect(result!.totalSessions).toBe(0);
    expect(result!.totalTokens).toBe(0);
  });

  it("returns null for invalid JSON", () => {
    expect(parseStatsCache("broken")).toBeNull();
  });
});

describe("parseTrackingTail", () => {
  it("parses JSONL lines", () => {
    const text = [
      JSON.stringify({ session_id: "s1", model: "opus", input_tokens: 100, output_tokens: 50 }),
      JSON.stringify({ session_id: "s2", model: "sonnet", input_tokens: 200, output_tokens: 100 }),
    ].join("\n");
    const result = parseTrackingTail(text);
    expect(result).toHaveLength(2);
    expect(result[0].sessionId).toBe("s1");
    expect(result[0].model).toBe("opus");
    expect(result[1].sessionId).toBe("s2");
  });

  it("skips malformed lines", () => {
    const text = [
      JSON.stringify({ session_id: "s1" }),
      "not json at all",
      JSON.stringify({ session_id: "s3" }),
    ].join("\n");
    const result = parseTrackingTail(text);
    expect(result).toHaveLength(2);
    expect(result[0].sessionId).toBe("s1");
    expect(result[1].sessionId).toBe("s3");
  });

  it("handles empty input", () => {
    expect(parseTrackingTail("")).toEqual([]);
    expect(parseTrackingTail("  \n  ")).toEqual([]);
  });
});

describe("parseUsageApiResponse", () => {
  const REAL_RESPONSE = JSON.stringify({
    five_hour: { utilization: 11.0, resets_at: "2026-02-23T16:00:00.921418+00:00" },
    seven_day: { utilization: 67.0, resets_at: "2026-02-25T07:59:59.921441+00:00" },
    seven_day_oauth_apps: null,
    seven_day_opus: null,
    seven_day_sonnet: { utilization: 17.0, resets_at: "2026-02-25T10:00:00.921454+00:00" },
    seven_day_cowork: null,
    iguana_necktie: null,
    extra_usage: { is_enabled: true, monthly_limit: null, used_credits: 10391.0, utilization: null },
  });

  it("parses real API response shape", () => {
    const result = parseUsageApiResponse(REAL_RESPONSE);
    expect(result).not.toBeNull();
    expect(result!.five_hour?.utilization).toBe(11.0);
    expect(result!.seven_day?.utilization).toBe(67.0);
    expect(result!.seven_day_sonnet?.utilization).toBe(17.0);
    expect(result!.seven_day_opus).toBeNull();
    expect(result!.extra_usage?.is_enabled).toBe(true);
    expect(result!.extra_usage?.used_credits).toBe(10391.0);
  });

  it("handles null buckets gracefully", () => {
    const result = parseUsageApiResponse(JSON.stringify({ five_hour: null }));
    expect(result).not.toBeNull();
    expect(result!.five_hour).toBeNull();
    expect(result!.seven_day).toBeNull();
  });

  it("returns null for invalid JSON", () => {
    expect(parseUsageApiResponse("broken")).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// Format helpers
// ---------------------------------------------------------------------------

describe("formatResetTime", () => {
  it("formats future time as hours and minutes", () => {
    const future = new Date(Date.now() + 2 * 3600_000 + 15 * 60_000).toISOString();
    expect(formatResetTime(future)).toBe("2h 15m");
  });

  it("formats < 1 hour as minutes only", () => {
    const future = new Date(Date.now() + 30 * 60_000).toISOString();
    expect(formatResetTime(future)).toBe("30m");
  });

  it("formats >= 24 hours as days", () => {
    const future = new Date(Date.now() + 1 * 24 * 3600_000 + 5 * 3600_000).toISOString();
    expect(formatResetTime(future)).toBe("1d 5h");
  });

  it("formats exactly 24h as days", () => {
    const future = new Date(Date.now() + 24 * 3600_000 + 10 * 60_000).toISOString();
    expect(formatResetTime(future)).toBe("1d 0h");
  });

  it("formats multi-day with 0 remaining hours", () => {
    const future = new Date(Date.now() + 5 * 24 * 3600_000).toISOString();
    expect(formatResetTime(future)).toBe("5d 0h");
  });

  it("formats 23h as hours", () => {
    const future = new Date(Date.now() + 23 * 3600_000 + 30 * 60_000).toISOString();
    expect(formatResetTime(future)).toBe("23h 30m");
  });

  it("returns 'now' for past times", () => {
    const past = new Date(Date.now() - 60_000).toISOString();
    expect(formatResetTime(past)).toBe("now");
  });
});

describe("formatNumber", () => {
  it("formats thousands with K suffix", () => {
    expect(formatNumber(1500)).toBe("1.5K");
    expect(formatNumber(10000)).toBe("10.0K");
  });

  it("formats millions with M suffix", () => {
    expect(formatNumber(1500000)).toBe("1.5M");
  });

  it("formats small numbers as-is", () => {
    expect(formatNumber(42)).toBe("42");
    expect(formatNumber(999)).toBe("999");
  });
});

describe("buildTickerText", () => {
  it("builds text from both buckets", () => {
    const api: UsageApiResponse = {
      five_hour: { utilization: 29, resets_at: "" },
      seven_day: { utilization: 56, resets_at: "" },
      seven_day_opus: null,
      seven_day_sonnet: null,
      seven_day_cowork: null,
      seven_day_oauth_apps: null,
      extra_usage: null,
    };
    expect(buildTickerText(api)).toBe("Claude: 5h: 29% · 7d: 56%");
  });

  it("handles missing buckets", () => {
    const api: UsageApiResponse = {
      five_hour: null,
      seven_day: null,
      seven_day_opus: null,
      seven_day_sonnet: null,
      seven_day_cowork: null,
      seven_day_oauth_apps: null,
      extra_usage: null,
    };
    expect(buildTickerText(api)).toBe("Claude: --");
  });
});

describe("getTickerPriority", () => {
  it("returns 90 for critical utilization (>= 90%)", () => {
    const api: UsageApiResponse = {
      five_hour: { utilization: 95, resets_at: "" },
      seven_day: { utilization: 50, resets_at: "" },
      seven_day_opus: null,
      seven_day_sonnet: null,
      seven_day_cowork: null,
      seven_day_oauth_apps: null,
      extra_usage: null,
    };
    expect(getTickerPriority(api)).toBe(90);
  });

  it("returns 80 for warning utilization (>= 80%)", () => {
    const api: UsageApiResponse = {
      five_hour: { utilization: 85, resets_at: "" },
      seven_day: { utilization: 50, resets_at: "" },
      seven_day_opus: null,
      seven_day_sonnet: null,
      seven_day_cowork: null,
      seven_day_oauth_apps: null,
      extra_usage: null,
    };
    expect(getTickerPriority(api)).toBe(80);
  });

  it("returns 10 for normal utilization", () => {
    const api: UsageApiResponse = {
      five_hour: { utilization: 30, resets_at: "" },
      seven_day: { utilization: 40, resets_at: "" },
      seven_day_opus: null,
      seven_day_sonnet: null,
      seven_day_cowork: null,
      seven_day_oauth_apps: null,
      extra_usage: null,
    };
    expect(getTickerPriority(api)).toBe(10);
  });
});

// ---------------------------------------------------------------------------
// Dashboard HTML
// ---------------------------------------------------------------------------

describe("renderDashboardHtml", () => {
  const emptyState: DashboardState = {
    apiData: null,
    budget: null,
    stats: null,
    recentSessions: [],
    apiError: null,
    lastApiPoll: null,
  };

  it("renders loading state when no data", () => {
    const html = renderDashboardHtml(emptyState);
    expect(html).toContain("Loading rate limits...");
    expect(html).toContain("Claude Usage Dashboard");
  });

  it("renders error message", () => {
    const html = renderDashboardHtml({ ...emptyState, apiError: "Token expired" });
    expect(html).toContain("Token expired");
  });

  it("renders progress bars when API data present", () => {
    const html = renderDashboardHtml({
      ...emptyState,
      apiData: {
        five_hour: { utilization: 45, resets_at: new Date(Date.now() + 3600_000).toISOString() },
        seven_day: { utilization: 72, resets_at: new Date(Date.now() + 86400_000).toISOString() },
        seven_day_opus: null,
        seven_day_sonnet: null,
        seven_day_cowork: null,
        seven_day_oauth_apps: null,
        extra_usage: null,
      },
    });
    expect(html).toContain("5-Hour Limit");
    expect(html).toContain("7-Day Limit");
    expect(html).toContain("45%");
    expect(html).toContain("72%");
  });

  it("renders stats cards", () => {
    const html = renderDashboardHtml({
      ...emptyState,
      stats: {
        dailyActivity: [],
        modelUsage: [],
        totalSessions: 150,
        totalMessages: 3500,
        totalTokens: 2_500_000,
      },
    });
    expect(html).toContain("150");
    expect(html).toContain("3.5K");
    expect(html).toContain("2.5M");
    expect(html).toContain("Sessions");
    expect(html).toContain("Messages");
    expect(html).toContain("Tokens");
  });

  it("renders model usage table", () => {
    const html = renderDashboardHtml({
      ...emptyState,
      stats: {
        dailyActivity: [],
        modelUsage: [
          { model: "claude-opus-4-6", inputTokens: 5000, outputTokens: 3000 },
        ],
        totalSessions: 0,
        totalMessages: 0,
        totalTokens: 8000,
      },
    });
    expect(html).toContain("claude-opus-4-6");
    expect(html).toContain("Token Usage by Model");
  });

  it("escapes HTML in error messages", () => {
    const html = renderDashboardHtml({
      ...emptyState,
      apiError: '<script>alert("xss")</script>',
    });
    expect(html).not.toContain("<script>");
    expect(html).toContain("&lt;script&gt;");
  });

  it("renders extra usage info", () => {
    const html = renderDashboardHtml({
      ...emptyState,
      apiData: {
        five_hour: null,
        seven_day: null,
        seven_day_opus: null,
        seven_day_sonnet: null,
        seven_day_cowork: null,
        seven_day_oauth_apps: null,
        extra_usage: { is_enabled: true, monthly_limit: null, used_credits: 10391, utilization: null },
      },
    });
    expect(html).toContain("Extra Usage");
    expect(html).toContain("$103.91");
  });
});

// ---------------------------------------------------------------------------
// Plugin lifecycle
// ---------------------------------------------------------------------------

describe("claudeUsagePlugin lifecycle", () => {
  beforeEach(() => {
    // Mock all invoke calls to prevent real Tauri calls
    mockedInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "plugin_read_file") return "{}";
      if (cmd === "plugin_read_file_tail") return "";
      if (cmd === "plugin_read_credential") return null;
      if (cmd === "plugin_watch_path") return "watch-1";
      if (cmd === "plugin_unwatch") return undefined;
      if (cmd === "plugin_http_fetch") {
        return { status: 200, headers: {}, body: "{}" };
      }
      return undefined;
    });
  });

  it("registers an activity section on load", () => {
    pluginRegistry.register(claudeUsagePlugin);
    const sections = activityStore.getSections();
    expect(sections.some((s) => s.id === "claude-usage")).toBe(true);
  });

  it("adds a summary activity item on load", async () => {
    pluginRegistry.register(claudeUsagePlugin);
    // Wait for async operations
    await vi.advanceTimersByTimeAsync(100);
    const items = activityStore.getForSection("claude-usage");
    expect(items.length).toBeGreaterThanOrEqual(1);
    expect(items[0].title).toBe("Claude Usage Dashboard");
  });

  it("unregistering cleans up section", () => {
    pluginRegistry.register(claudeUsagePlugin);
    pluginRegistry.unregister("claude-usage");
    expect(activityStore.getSections().some((s) => s.id === "claude-usage")).toBe(false);
  });

  it("reads filesystem data on load", async () => {
    pluginRegistry.register(claudeUsagePlugin);
    await vi.advanceTimersByTimeAsync(100);

    // Should have tried to read budget, stats, and tracking files
    const readCalls = mockedInvoke.mock.calls.filter(
      (c) => c[0] === "plugin_read_file" || c[0] === "plugin_read_file_tail",
    );
    expect(readCalls.length).toBeGreaterThanOrEqual(2);
  });

  it("attempts to read credentials and call API on load", async () => {
    pluginRegistry.register(claudeUsagePlugin);
    await vi.advanceTimersByTimeAsync(100);

    const credCalls = mockedInvoke.mock.calls.filter(
      (c) => c[0] === "plugin_read_credential",
    );
    expect(credCalls.length).toBeGreaterThanOrEqual(1);
  });

  it("sets up file watcher on load", async () => {
    pluginRegistry.register(claudeUsagePlugin);
    await vi.advanceTimersByTimeAsync(100);

    const watchCalls = mockedInvoke.mock.calls.filter(
      (c) => c[0] === "plugin_watch_path",
    );
    expect(watchCalls.length).toBeGreaterThanOrEqual(1);
  });
});
