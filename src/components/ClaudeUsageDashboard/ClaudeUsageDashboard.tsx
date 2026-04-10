import { Component, createSignal, createEffect, createMemo, For, Show, onCleanup, onMount } from "solid-js";
import { invoke } from "../../invoke";
import { appLogger } from "../../stores/appLogger";
import s from "./ClaudeUsageDashboard.module.css";

// ---------------------------------------------------------------------------
// Types (mirrors Rust structs)
// ---------------------------------------------------------------------------

interface RateBucket {
  utilization: number;
  resets_at: string | null;
}

interface ExtraUsage {
  is_enabled: boolean;
  monthly_limit: number | null;
  used_credits: number | null;
  utilization: number | null;
  /** From overage-reset header; not available via primary API body. */
  resets_at: string | null;
  /** From overage-in-use header. */
  in_use: boolean;
}

interface RateLimitMeta {
  unified_status: string | null;
  representative_claim: string | null;
}

interface PlanInfo {
  subscription_type: string | null;
  rate_limit_tier: string | null;
  scopes: string[];
}

interface UsageApiResponse {
  five_hour: RateBucket | null;
  seven_day: RateBucket | null;
  seven_day_oauth_apps: RateBucket | null;
  seven_day_opus: RateBucket | null;
  seven_day_sonnet: RateBucket | null;
  seven_day_cowork: RateBucket | null;
  extra_usage: ExtraUsage | null;
  plan: PlanInfo | null;
  meta: RateLimitMeta | null;
}

interface ModelTokens {
  input_tokens: number;
  output_tokens: number;
  cache_creation_tokens: number;
  cache_read_tokens: number;
  message_count: number;
}

interface DayStats {
  input_tokens: number;
  output_tokens: number;
  message_count: number;
  session_count: number;
}

interface ProjectStats {
  session_count: number;
  assistant_message_count: number;
  user_message_count: number;
  input_tokens: number;
  output_tokens: number;
}

interface SessionStats {
  total_sessions: number;
  total_assistant_messages: number;
  total_user_messages: number;
  total_input_tokens: number;
  total_output_tokens: number;
  total_cache_creation_tokens: number;
  total_cache_read_tokens: number;
  model_usage: Record<string, ModelTokens>;
  daily_activity: Record<string, DayStats>;
  per_project: Record<string, ProjectStats>;
  per_project_daily: Record<string, Record<string, DayStats>>;
  active_hours: number;
}

interface ProjectEntry {
  slug: string;
  session_count: number;
  display_path: string | null;
}

interface TimelinePoint {
  hour: string;
  input_tokens: number;
  output_tokens: number;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Format a number with K/M suffixes for readability */
function formatTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return n.toString();
}

/** Format a "resets_at" ISO string as a human-readable relative time */
function formatResetTime(isoStr: string): string {
  const reset = new Date(isoStr);
  const now = new Date();
  const diffMs = reset.getTime() - now.getTime();
  if (diffMs <= 0) return "now";
  const diffMin = Math.floor(diffMs / 60_000);
  if (diffMin < 60) return `${diffMin}m`;
  const diffHrs = Math.floor(diffMin / 60);
  if (diffHrs < 48) return `${diffHrs}h ${diffMin % 60}m`;
  const diffDays = Math.floor(diffHrs / 24);
  return `${diffDays}d ${diffHrs % 24}h`;
}

/** Pick CSS class for rate bar based on utilization */
/** Utilization values are direct percentages (0-100). */
function rateClass(util: number): string {
  if (util >= 90) return s.rateCritical;
  if (util >= 70) return s.rateWarn;
  return s.rateOk;
}

/** Build 52-week heatmap data from daily_activity */
function buildHeatmap(daily: Record<string, DayStats>): { date: string; level: number }[][] {
  const today = new Date();
  const weeks: { date: string; level: number }[][] = [];

  // Find max messages for scaling
  const values = Object.values(daily).map((d) => d.message_count);
  const maxVal = Math.max(1, ...values);

  // Go back 52 weeks (364 days)
  const startDate = new Date(today);
  startDate.setDate(startDate.getDate() - 363);
  // Align to start of week (Sunday)
  startDate.setDate(startDate.getDate() - startDate.getDay());

  let currentDate = new Date(startDate);
  while (currentDate <= today) {
    const week: { date: string; level: number }[] = [];
    for (let d = 0; d < 7; d++) {
      const dateStr = currentDate.toISOString().slice(0, 10);
      const count = daily[dateStr]?.message_count || 0;
      let level = 0;
      if (count > 0) {
        const ratio = count / maxVal;
        if (ratio > 0.75) level = 4;
        else if (ratio > 0.5) level = 3;
        else if (ratio > 0.25) level = 2;
        else level = 1;
      }
      week.push({ date: dateStr, level });
      currentDate.setDate(currentDate.getDate() + 1);
    }
    weeks.push(week);
  }

  return weeks;
}

/** Map heatmap level to CSS class */
function heatmapClass(level: number): string {
  switch (level) {
    case 1: return `${s.heatmapCell} ${s.heatmapLevel1}`;
    case 2: return `${s.heatmapCell} ${s.heatmapLevel2}`;
    case 3: return `${s.heatmapCell} ${s.heatmapLevel3}`;
    case 4: return `${s.heatmapCell} ${s.heatmapLevel4}`;
    default: return s.heatmapCell;
  }
}

// ---------------------------------------------------------------------------
// Usage Over Time chart (inline SVG — token usage from session transcripts)
// ---------------------------------------------------------------------------

const CHART_W = 700;
const CHART_H = 200;
const CHART_PAD = { top: 20, right: 20, bottom: 30, left: 55 };
const PLOT_W = CHART_W - CHART_PAD.left - CHART_PAD.right;
const PLOT_H = CHART_H - CHART_PAD.top - CHART_PAD.bottom;

/** Build an SVG path string from (x, y) points */
function svgPath(points: { x: number; y: number }[]): string {
  if (points.length === 0) return "";
  return points.map((p, i) => `${i === 0 ? "M" : "L"}${p.x.toFixed(1)},${p.y.toFixed(1)}`).join(" ");
}

/** Build an SVG area path (line + close to bottom) */
function svgAreaPath(points: { x: number; y: number }[]): string {
  if (points.length === 0) return "";
  const line = svgPath(points);
  const lastX = points[points.length - 1].x;
  const firstX = points[0].x;
  return `${line} L${lastX.toFixed(1)},${PLOT_H.toFixed(1)} L${firstX.toFixed(1)},${PLOT_H.toFixed(1)} Z`;
}

/** Format Y axis token values */
function formatYAxis(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(0)}K`;
  return n.toString();
}

interface ChartPoint {
  x: number;
  inputY: number;
  outputY: number;
  raw: TimelinePoint;
}

const UsageChart: Component<{ timeline: TimelinePoint[]; days: number }> = (props) => {
  const [hoverIdx, setHoverIdx] = createSignal<number | null>(null);

  /** Compute chart data: map hour keys to x/y positions */
  const chartData = createMemo(() => {
    const data = props.timeline;
    if (data.length === 0) return { points: [] as ChartPoint[], yMax: 0, timeLabels: [] as { x: number; label: string }[] };

    // Find max total tokens for Y scale
    let yMax = 0;
    for (const pt of data) {
      const total = pt.input_tokens + pt.output_tokens;
      if (total > yMax) yMax = total;
    }
    if (yMax === 0) yMax = 1000;
    else yMax = Math.ceil(yMax / 1000) * 1000;

    // Build the time window
    const now = new Date();
    const windowMs = props.days * 24 * 60 * 60 * 1000;
    const startMs = now.getTime() - windowMs;

    const points: ChartPoint[] = [];
    for (const pt of data) {
      const hourDate = new Date(pt.hour + ":00:00Z");
      const t = hourDate.getTime();
      if (t < startMs) continue;
      const x = ((t - startMs) / windowMs) * PLOT_W;
      const total = pt.input_tokens + pt.output_tokens;
      points.push({
        x,
        inputY: PLOT_H - (total / yMax) * PLOT_H,
        outputY: PLOT_H - (pt.output_tokens / yMax) * PLOT_H,
        raw: pt,
      });
    }

    // Time labels: one per day
    const labels: { x: number; label: string }[] = [];
    const startDate = new Date(startMs);
    startDate.setUTCHours(0, 0, 0, 0);
    if (startDate.getTime() < startMs) startDate.setUTCDate(startDate.getUTCDate() + 1);
    let cursor = startDate.getTime();
    while (cursor <= now.getTime()) {
      const x = ((cursor - startMs) / windowMs) * PLOT_W;
      const d = new Date(cursor);
      const dayStr = `${(d.getUTCMonth() + 1).toString().padStart(2, "0")}/${d.getUTCDate().toString().padStart(2, "0")}`;
      labels.push({ x, label: dayStr });
      cursor += 24 * 60 * 60 * 1000;
    }

    return { points, yMax, timeLabels: labels };
  });

  /** Y gridlines */
  const yLines = () => {
    const { yMax } = chartData();
    if (yMax === 0) return [];
    const steps = [0, 0.25, 0.5, 0.75, 1];
    return steps.map((ratio) => ({
      y: PLOT_H - ratio * PLOT_H,
      label: formatYAxis(Math.round(yMax * ratio)),
    }));
  };

  /** Find nearest point index from mouse x in SVG coordinates */
  const onMouseMove = (e: MouseEvent) => {
    const svg = (e.currentTarget as SVGSVGElement);
    const rect = svg.getBoundingClientRect();
    // Convert DOM coords to SVG viewBox coords
    const svgX = ((e.clientX - rect.left) / rect.width) * CHART_W - CHART_PAD.left;
    const pts = chartData().points;
    if (pts.length === 0) { setHoverIdx(null); return; }
    let best = 0;
    let bestDist = Math.abs(pts[0].x - svgX);
    for (let i = 1; i < pts.length; i++) {
      const d = Math.abs(pts[i].x - svgX);
      if (d < bestDist) { best = i; bestDist = d; }
    }
    setHoverIdx(best);
  };

  /** Format hour key "2026-02-04T10" for tooltip display */
  const formatHour = (hour: string): string => {
    const date = hour.slice(5, 10).replace("-", "/"); // "02/04"
    const h = hour.slice(11); // "10"
    return `${date} ${h}:00`;
  };

  const inputPath = () => svgPath(chartData().points.map((p) => ({ x: p.x, y: p.inputY })));
  const inputArea = () => svgAreaPath(chartData().points.map((p) => ({ x: p.x, y: p.inputY })));
  const outputPath = () => svgPath(chartData().points.map((p) => ({ x: p.x, y: p.outputY })));
  const outputArea = () => svgAreaPath(chartData().points.map((p) => ({ x: p.x, y: p.outputY })));

  return (
    <svg
      class={s.usageChart}
      viewBox={`0 0 ${CHART_W} ${CHART_H}`}
      preserveAspectRatio="xMidYMid meet"
      onMouseMove={onMouseMove}
      onMouseLeave={() => setHoverIdx(null)}
    >
      <g transform={`translate(${CHART_PAD.left},${CHART_PAD.top})`}>
        {/* Y gridlines + labels */}
        {yLines().map((line) => (
          <>
            <line
              x1="0" y1={line.y} x2={PLOT_W} y2={line.y}
              stroke="var(--border)" stroke-width="0.5"
            />
            <text
              x="-8" y={line.y + 3}
              fill="var(--fg-muted)" font-size="9" text-anchor="end"
            >
              {line.label}
            </text>
          </>
        ))}

        {/* X axis labels (day) */}
        {chartData().timeLabels.map((tl) => (
          <>
            <line
              x1={tl.x} y1="0" x2={tl.x} y2={PLOT_H}
              stroke="var(--border)" stroke-width="0.5" stroke-dasharray="2,4"
            />
            <text
              x={tl.x} y={PLOT_H + 16}
              fill="var(--fg-muted)" font-size="9" text-anchor="middle"
            >
              {tl.label}
            </text>
          </>
        ))}

        {/* Output tokens area (bottom layer, pink) */}
        <Show when={chartData().points.length > 1}>
          <path d={outputArea()} fill="rgba(248, 81, 73, 0.15)" />
          <path d={outputPath()} fill="none" stroke="rgba(248, 81, 73, 0.6)" stroke-width="1.5" />
        </Show>

        {/* Input tokens area (top layer, blue/accent) */}
        <Show when={chartData().points.length > 1}>
          <path d={inputArea()} fill="rgba(88, 166, 255, 0.15)" />
          <path d={inputPath()} fill="none" stroke="var(--accent)" stroke-width="2" />
        </Show>

        {/* "Now" vertical line */}
        <line
          x1={PLOT_W} y1="0" x2={PLOT_W} y2={PLOT_H}
          stroke="var(--fg-muted)" stroke-width="1" opacity="0.5"
        />
        <text
          x={PLOT_W} y={PLOT_H + 16}
          fill="var(--fg-secondary)" font-size="9" text-anchor="middle" font-weight="600"
        >
          Now
        </text>

        {/* Hover crosshair + tooltip */}
        <Show when={hoverIdx() != null}>
          {(() => {
            const idx = hoverIdx();
            if (idx == null) return null;
            const pt = chartData().points[idx];
            if (!pt) return null;
            const total = pt.raw.input_tokens + pt.raw.output_tokens;
            // Position tooltip: flip to left side if too close to right edge
            const tipX = pt.x > PLOT_W * 0.75 ? pt.x - 8 : pt.x + 8;
            const anchor = pt.x > PLOT_W * 0.75 ? "end" : "start";
            return (
              <>
                {/* Vertical crosshair */}
                <line
                  x1={pt.x} y1="0" x2={pt.x} y2={PLOT_H}
                  stroke="var(--fg-secondary)" stroke-width="1" stroke-dasharray="3,2" opacity="0.7"
                />
                {/* Dot on input line */}
                <circle cx={pt.x} cy={pt.inputY} r="3" fill="var(--accent)" />
                {/* Dot on output line */}
                <circle cx={pt.x} cy={pt.outputY} r="3" fill="rgba(248, 81, 73, 0.8)" />
                {/* Tooltip text */}
                <text x={tipX} y="4" fill="var(--fg-primary)" font-size="10" font-weight="600" text-anchor={anchor}>
                  {formatHour(pt.raw.hour)}
                </text>
                <text x={tipX} y="16" fill="var(--accent)" font-size="9" text-anchor={anchor}>
                  In: {formatTokens(pt.raw.input_tokens)}
                </text>
                <text x={tipX} y="27" fill="rgba(248, 81, 73, 0.8)" font-size="9" text-anchor={anchor}>
                  Out: {formatTokens(pt.raw.output_tokens)}
                </text>
                <text x={tipX} y="38" fill="var(--fg-muted)" font-size="9" text-anchor={anchor}>
                  Total: {formatTokens(total)}
                </text>
              </>
            );
          })()}
        </Show>
      </g>
    </svg>
  );
};

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export const ClaudeUsageDashboard: Component = () => {
  const [apiData, setApiData] = createSignal<UsageApiResponse | null>(null);
  const [apiError, setApiError] = createSignal<string | null>(null);
  const [sessionStats, setSessionStats] = createSignal<SessionStats | null>(null);
  const [sessionError, setSessionError] = createSignal<string | null>(null);
  const [projects, setProjects] = createSignal<ProjectEntry[]>([]);
  const [timeline, setTimeline] = createSignal<TimelinePoint[]>([]);
  const [scope, setScope] = createSignal("all");
  const [loading, setLoading] = createSignal(true);

  // Fetch API usage data
  const fetchApi = async () => {
    try {
      const data = await invoke<UsageApiResponse>("get_claude_usage_api");
      setApiData(data);
      setApiError(null);
    } catch (err) {
      setApiError(String(err));
    }
  };

  // Fetch session stats
  const fetchStats = async (scopeValue: string) => {
    try {
      const data = await invoke<SessionStats>("get_claude_session_stats", { scope: scopeValue });
      setSessionStats(data);
      setSessionError(null);
    } catch (err) {
      setSessionError(String(err));
    }
  };

  // Fetch usage timeline from session transcripts
  const fetchTimeline = async (scopeValue: string) => {
    try {
      const data = await invoke<TimelinePoint[]>("get_claude_usage_timeline", { scope: scopeValue, days: 7 });
      setTimeline(data);
    } catch (err) {
      appLogger.warn("app", "Failed to fetch timeline", err);
    }
  };

  // Fetch project list for dropdown
  const fetchProjects = async () => {
    try {
      const list = await invoke<ProjectEntry[]>("get_claude_project_list");
      setProjects(list);
    } catch (err) {
      appLogger.warn("app", "Failed to fetch project list", err);
    }
  };

  // Fetch API usage and project list once on mount (not scope-dependent).
  onMount(() => {
    void Promise.all([fetchApi(), fetchProjects()]);
  });

  // Sequential by design: fetchStats writes the JSONL cache that fetchTimeline reads.
  const refreshStatsAndTimeline = async (scopeValue: string) => {
    await fetchStats(scopeValue);
    await fetchTimeline(scopeValue);
  };

  // Re-fetch stats and timeline whenever scope changes.
  createEffect(() => {
    const currentScope = scope();
    setLoading(true);
    void refreshStatsAndTimeline(currentScope).finally(() => setLoading(false));
  });

  // Auto-refresh every 5 minutes
  const timer = setInterval(() => { void fetchApi(); void refreshStatsAndTimeline(scope()); }, 5 * 60 * 1000);
  onCleanup(() => clearInterval(timer));

  // Computed data for rate limit cards.
  // `key` matches the `representative_claim` header value so we can flag the bottleneck bucket.
  const rateBuckets = () => {
    const api = apiData();
    if (!api) return [];
    const buckets: { key: string; label: string; bucket: RateBucket }[] = [];
    if (api.five_hour) buckets.push({ key: "five_hour", label: "5-Hour", bucket: api.five_hour });
    if (api.seven_day) buckets.push({ key: "seven_day", label: "7-Day", bucket: api.seven_day });
    if (api.seven_day_oauth_apps) buckets.push({ key: "seven_day_oauth_apps", label: "7-Day OAuth Apps", bucket: api.seven_day_oauth_apps });
    if (api.seven_day_opus) buckets.push({ key: "seven_day_opus", label: "7-Day Opus", bucket: api.seven_day_opus });
    if (api.seven_day_sonnet) buckets.push({ key: "seven_day_sonnet", label: "7-Day Sonnet", bucket: api.seven_day_sonnet });
    if (api.seven_day_cowork) buckets.push({ key: "seven_day_cowork", label: "7-Day Cowork", bucket: api.seven_day_cowork });
    return buckets;
  };

  /** Which bucket key is the active constraint — either from header meta or derived from the highest utilization (≥ 95%). */
  const bottleneckKey = (): string | null => {
    const api = apiData();
    if (!api) return null;
    if (api.meta?.representative_claim) return api.meta.representative_claim;
    // Fallback: mark the first bucket at or above 95% as the bottleneck.
    const over = rateBuckets().filter((b) => b.bucket.utilization >= 95);
    if (over.length === 0) return null;
    over.sort((a, b) => b.bucket.utilization - a.bucket.utilization);
    return over[0].key;
  };

  /** Global status — prefer the backend meta, else derive from utilizations. */
  type StatusLevel = "allowed" | "warning" | "rejected";
  const globalStatus = (): StatusLevel | null => {
    const api = apiData();
    if (!api) return null;
    const metaStatus = api.meta?.unified_status;
    if (metaStatus === "rejected") return "rejected";
    if (metaStatus === "allowed_warning") return "warning";
    if (metaStatus === "allowed") return "allowed";
    // Derive from buckets as fallback when the primary endpoint is used.
    const maxUtil = Math.max(0, ...rateBuckets().map((b) => b.bucket.utilization));
    if (maxUtil >= 100) return "rejected";
    if (maxUtil >= 90) return "warning";
    return "allowed";
  };

  /** Pretty-print a claim key like "seven_day_sonnet" → "7-Day Sonnet". */
  const claimLabel = (key: string): string => {
    const match = rateBuckets().find((b) => b.key === key);
    return match ? match.label : key.replace(/_/g, " ");
  };

  /** Format a number with thousand separators (no decimal for integers). */
  const formatCount = (n: number): string => n.toLocaleString("en-US", { maximumFractionDigits: 0 });

  // Sorted models
  const sortedModels = () => {
    const stats = sessionStats();
    if (!stats) return [];
    return Object.entries(stats.model_usage).sort(
      (a, b) => (b[1].input_tokens + b[1].output_tokens) - (a[1].input_tokens + a[1].output_tokens),
    );
  };

  // Lookup: slug → resolved display path (from Rust)
  const displayPathMap = () => {
    const map = new Map<string, string>();
    for (const p of projects()) {
      if (p.display_path) map.set(p.slug, p.display_path);
    }
    return map;
  };

  /** Format a project slug for display, using resolved path if available */
  const formatSlug = (slug: string): string => {
    const path = displayPathMap().get(slug) ?? slug;
    const segments = path.split("/").filter(Boolean);
    if (segments.length <= 2) return segments.join("/");
    return segments.slice(-2).join("/");
  };

  // Sorted projects by tokens, excluding zero-token entries
  const sortedProjects = () => {
    const stats = sessionStats();
    if (!stats) return [];
    return Object.entries(stats.per_project)
      .filter(([, proj]) => proj.input_tokens + proj.output_tokens > 0)
      .sort(
        (a, b) => (b[1].input_tokens + b[1].output_tokens) - (a[1].input_tokens + a[1].output_tokens),
      );
  };

  // Heatmap weeks
  const heatmapWeeks = () => {
    const stats = sessionStats();
    if (!stats) return [];
    return buildHeatmap(stats.daily_activity);
  };

  /** Build a tooltip string for a heatmap cell, showing date, messages, and top 3 projects */
  const heatmapTooltip = (date: string): string => {
    const stats = sessionStats();
    if (!stats) return date;
    const day = stats.daily_activity[date];
    if (!day || day.message_count === 0) return `${date}: no activity`;

    const lines = [`${date}: ${day.message_count} messages`];

    // Find top 3 projects for this day by message count
    const projectActivity: { slug: string; count: number }[] = [];
    for (const [slug, daily] of Object.entries(stats.per_project_daily)) {
      const d = daily[date];
      if (d && d.message_count > 0) {
        projectActivity.push({ slug, count: d.message_count });
      }
    }
    projectActivity.sort((a, b) => b.count - a.count);
    for (const { slug, count } of projectActivity.slice(0, 3)) {
      lines.push(`  ${formatSlug(slug)}: ${count}`);
    }

    return lines.join("\n");
  };

  return (
    <div class={s.dashboard}>
      {/* Header with scope selector */}
      <div class={s.header}>
        <span class={s.title}>Claude Usage Dashboard</span>
        <select
          class={s.scopeSelect}
          value={scope()}
          onChange={(e) => setScope(e.currentTarget.value)}
        >
          <option value="all">All Projects</option>
          <For each={projects()}>
            {(p) => <option value={p.slug}>{formatSlug(p.slug)} ({p.session_count})</option>}
          </For>
        </select>
      </div>

      {/* Loading state */}
      <Show when={loading()}>
        <div class={s.loadingState}>Loading usage data...</div>
      </Show>

      {/* Plan strip — subscription + rate-limit tier + global status */}
      <Show when={apiData()?.plan}>
        {(plan) => (
          <div class={s.planStrip}>
            <div class={s.planIdentity}>
              <span class={s.planLabel}>Plan</span>
              <span class={s.planName}>{plan().subscription_type ?? "unknown"}</span>
              <Show when={plan().rate_limit_tier}>
                <span class={s.planTier}>{plan().rate_limit_tier}</span>
              </Show>
            </div>
            <Show when={globalStatus()}>
              {(status) => (
                <div class={s.planStatusWrap}>
                  <span class={`${s.planStatus} ${s[`status_${status()}`]}`}>
                    <span class={s.statusDot} />
                    <span class={s.statusLabel}>
                      {status() === "rejected"
                        ? "At limit"
                        : status() === "warning"
                          ? "Warning"
                          : "Allowed"}
                    </span>
                  </span>
                  <Show when={bottleneckKey()}>
                    <span class={s.statusConstraint}>
                      {claimLabel(bottleneckKey()!)} is the bottleneck
                    </span>
                  </Show>
                </div>
              )}
            </Show>
          </div>
        )}
      </Show>

      {/* Rate limits — show section whenever we have data OR an error to report */}
      <Show when={rateBuckets().length > 0 || (apiError() && !loading())}>
        <div class={s.section}>
          <div class={s.sectionTitle}>
            <span>Rate Limits</span>
            <Show when={apiError() && rateBuckets().length > 0}>
              <span class={s.sectionHint}>(from response headers)</span>
            </Show>
          </div>
          <Show when={rateBuckets().length > 0}>
            <div class={s.rateGrid}>
              <For each={rateBuckets()}>
                {(item) => (
                  <div class={`${s.rateCard} ${bottleneckKey() === item.key ? s.rateCardBottleneck : ""}`}>
                    <div class={s.rateLabel}>
                      <span>
                        {item.label}
                        <Show when={bottleneckKey() === item.key}>
                          <span class={s.bottleneckArrow} title="Active constraint">←</span>
                        </Show>
                      </span>
                      <span class={s.rateValue}>{Math.round(item.bucket.utilization)}%</span>
                    </div>
                    <div class={s.rateBar}>
                      <div
                        class={`${s.rateFill} ${rateClass(item.bucket.utilization)}`}
                        style={{ width: `${Math.min(100, item.bucket.utilization)}%` }}
                      />
                    </div>
                    <Show when={item.bucket.resets_at}>
                      <span class={s.rateReset}>
                        Resets in {formatResetTime(item.bucket.resets_at!)}
                      </span>
                    </Show>
                  </div>
                )}
              </For>
            </div>
          </Show>
          <Show when={apiError() && rateBuckets().length === 0}>
            <div class={s.rateLimitHint}>
              <span>Rate limit data unavailable</span>
              <button class={s.retryButton} onClick={() => fetchApi()}>Retry</button>
            </div>
          </Show>

          {/* Extra Usage card — shown always when we have API data, enabled or not */}
          <Show when={apiData()?.extra_usage || (apiData() && !apiError())}>
            {(_) => {
              const extra = (): ExtraUsage | null => apiData()?.extra_usage ?? null;
              const enabled = () => extra()?.is_enabled === true;
              const util = () => extra()?.utilization ?? 0;
              const limit = () => extra()?.monthly_limit;
              const used = () => extra()?.used_credits;
              return (
                <div class={`${s.extraUsageCard} ${enabled() ? s.extraEnabled : s.extraDisabled}`}>
                  <div class={s.extraHeader}>
                    <span class={s.extraTitle}>Extra Usage</span>
                    <Show when={enabled() && extra()?.in_use}>
                      <span class={s.extraInUse}>
                        <span class={s.extraInUseDot} /> IN USE NOW
                      </span>
                    </Show>
                    <Show when={!enabled()}>
                      <span class={s.extraOff}>Disabled</span>
                    </Show>
                  </div>
                  <Show when={enabled()}>
                    <Show
                      when={limit() != null && used() != null}
                      fallback={
                        <div class={s.extraBody}>
                          <span class={s.extraValue}>
                            {Math.round(util()).toString()}%
                          </span>
                          <span class={s.extraSub}>
                            (credit detail available via primary API only)
                          </span>
                        </div>
                      }
                    >
                      <div class={s.extraBody}>
                        <span class={s.extraValue}>
                          {formatCount(used()!)} / {formatCount(limit()!)}
                          <span class={s.extraUnit}> credits</span>
                        </span>
                        <div class={s.extraBar}>
                          <div
                            class={`${s.extraFill} ${rateClass(util())}`}
                            style={{ width: `${Math.min(100, util())}%` }}
                          />
                        </div>
                        <span class={s.extraSub}>{util().toFixed(1)}% used</span>
                      </div>
                    </Show>
                    <Show when={extra()?.resets_at}>
                      <span class={s.extraReset}>
                        Resets in {formatResetTime(extra()!.resets_at!)}
                      </span>
                    </Show>
                  </Show>
                  <Show when={!enabled()}>
                    <span class={s.extraHint}>
                      Enable extra usage at claude.ai/settings to keep working past your plan's limits.
                    </span>
                  </Show>
                </div>
              );
            }}
          </Show>
        </div>
      </Show>

      {/* Usage Over Time chart (from session transcripts) */}
      <Show when={timeline().length > 1}>
        <div class={s.section}>
          <div class={s.sectionTitle}>Usage Over Time (7 days)</div>
          <div class={s.chartContainer}>
            <UsageChart timeline={timeline()} days={7} />
            <div class={s.chartLegend}>
              <span class={s.legendItem}>
                <span class={s.legendDot} style={{ background: "var(--accent)" }} />
                Input Tokens
              </span>
              <span class={s.legendItem}>
                <span class={s.legendDot} style={{ background: "rgba(248, 81, 73, 0.6)" }} />
                Output Tokens
              </span>
            </div>
          </div>
        </div>
      </Show>

      {/* Session error */}
      <Show when={sessionError()}>
        <div class={s.errorState}>Sessions: {sessionError()}</div>
      </Show>

      {/* Insights */}
      <Show when={sessionStats()}>
        {(stats) => (
          <>
            <div class={s.section}>
              <div class={s.sectionTitle}>Insights</div>
              <div class={s.insightsGrid}>
                <div class={s.insightCard}>
                  <span class={s.insightLabel}>Sessions</span>
                  <span class={s.insightValue}>{stats().total_sessions}</span>
                </div>
                <div class={s.insightCard}>
                  <span class={s.insightLabel}>Messages</span>
                  <span class={s.insightValue}>
                    {formatTokens(stats().total_assistant_messages + stats().total_user_messages)}
                  </span>
                  <span class={s.insightSub}>
                    {stats().total_user_messages} user / {stats().total_assistant_messages} assistant
                  </span>
                </div>
                <div class={s.insightCard}>
                  <span class={s.insightLabel}>Input Tokens</span>
                  <span class={s.insightValue}>{formatTokens(stats().total_input_tokens)}</span>
                </div>
                <div class={s.insightCard}>
                  <span class={s.insightLabel}>Output Tokens</span>
                  <span class={s.insightValue}>{formatTokens(stats().total_output_tokens)}</span>
                </div>
                <div class={s.insightCard}>
                  <span class={s.insightLabel}>Cache Created</span>
                  <span class={s.insightValue}>
                    {formatTokens(stats().total_cache_creation_tokens)}
                  </span>
                </div>
                <div class={s.insightCard}>
                  <span class={s.insightLabel}>Cache Read</span>
                  <span class={s.insightValue}>
                    {formatTokens(stats().total_cache_read_tokens)}
                  </span>
                </div>
                <div class={s.insightCard}>
                  <span class={s.insightLabel}>Tokens/Hour</span>
                  <span class={s.insightValue}>
                    {stats().active_hours > 0
                      ? formatTokens(
                          Math.round(
                            (stats().total_input_tokens + stats().total_output_tokens) /
                              stats().active_hours,
                          ),
                        )
                      : "—"}
                  </span>
                  <span class={s.insightSub}>
                    {stats().active_hours} active hours
                  </span>
                </div>
              </div>
            </div>

            {/* Activity heatmap */}
            <Show when={heatmapWeeks().length > 0}>
              <div class={s.section}>
                <div class={s.sectionTitle}>Activity (52 weeks)</div>
                <div class={s.heatmapContainer}>
                  <div class={s.heatmapGrid}>
                    <For each={heatmapWeeks()}>
                      {(week) => (
                        <div class={s.heatmapWeek}>
                          <For each={week}>
                            {(cell) => (
                              <div
                                class={heatmapClass(cell.level)}
                                title={heatmapTooltip(cell.date)}
                              />
                            )}
                          </For>
                        </div>
                      )}
                    </For>
                  </div>
                </div>
              </div>
            </Show>

            {/* Model breakdown */}
            <Show when={sortedModels().length > 0}>
              <div class={s.section}>
                <div class={s.sectionTitle}>Model Usage</div>
                <table class={s.modelTable}>
                  <thead>
                    <tr>
                      <th>Model</th>
                      <th>Messages</th>
                      <th>Input</th>
                      <th>Output</th>
                      <th>Cache Created</th>
                      <th>Cache Read</th>
                    </tr>
                  </thead>
                  <tbody>
                    <For each={sortedModels()}>
                      {([model, tokens]) => (
                        <tr>
                          <td>{model}</td>
                          <td>{formatTokens(tokens.message_count)}</td>
                          <td>{formatTokens(tokens.input_tokens)}</td>
                          <td>{formatTokens(tokens.output_tokens)}</td>
                          <td>{formatTokens(tokens.cache_creation_tokens)}</td>
                          <td>{formatTokens(tokens.cache_read_tokens)}</td>
                        </tr>
                      )}
                    </For>
                  </tbody>
                </table>
              </div>
            </Show>

            {/* Per-project breakdown */}
            <Show when={scope() === "all" && sortedProjects().length > 0}>
              <div class={s.section}>
                <div class={s.sectionTitle}>Projects ({sortedProjects().length})</div>
                <div class={s.projectList}>
                  <For each={sortedProjects()}>
                    {([slug, proj]) => (
                      <div
                        class={s.projectRow}
                        onClick={() => setScope(slug)}
                        style={{ cursor: "pointer" }}
                        title={displayPathMap().get(slug) ?? slug}
                      >
                        <span class={s.projectSlug}>{formatSlug(slug)}</span>
                        <span class={s.projectTokens}>
                          {proj.session_count} sessions · {formatTokens(proj.input_tokens + proj.output_tokens)} tokens
                        </span>
                      </div>
                    )}
                  </For>
                </div>
              </div>
            </Show>
          </>
        )}
      </Show>
    </div>
  );
};
