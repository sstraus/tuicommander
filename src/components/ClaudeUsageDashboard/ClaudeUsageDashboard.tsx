import { Component, createSignal, createEffect, For, Show, onCleanup } from "solid-js";
import { invoke } from "../../invoke";
import s from "./ClaudeUsageDashboard.module.css";

// ---------------------------------------------------------------------------
// Types (mirrors Rust structs)
// ---------------------------------------------------------------------------

interface RateBucket {
  utilization: number;
  resets_at: string;
}

interface ExtraUsage {
  enabled: boolean;
  spend_limit_cents: number | null;
  current_spend_cents: number | null;
}

interface UsageApiResponse {
  five_hour: RateBucket | null;
  seven_day: RateBucket | null;
  seven_day_opus: RateBucket | null;
  seven_day_sonnet: RateBucket | null;
  seven_day_cowork: RateBucket | null;
  extra_usage: ExtraUsage | null;
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
}

interface ProjectEntry {
  slug: string;
  session_count: number;
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
function rateClass(util: number): string {
  if (util >= 0.9) return s.rateCritical;
  if (util >= 0.7) return s.rateWarn;
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
// Component
// ---------------------------------------------------------------------------

export const ClaudeUsageDashboard: Component = () => {
  const [apiData, setApiData] = createSignal<UsageApiResponse | null>(null);
  const [apiError, setApiError] = createSignal<string | null>(null);
  const [sessionStats, setSessionStats] = createSignal<SessionStats | null>(null);
  const [sessionError, setSessionError] = createSignal<string | null>(null);
  const [projects, setProjects] = createSignal<ProjectEntry[]>([]);
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

  // Fetch project list for dropdown
  const fetchProjects = async () => {
    try {
      const list = await invoke<ProjectEntry[]>("get_claude_project_list");
      setProjects(list);
    } catch {
      // Non-critical — dropdown just won't have project options
    }
  };

  // Initial load
  createEffect(() => {
    setLoading(true);
    Promise.all([fetchApi(), fetchStats(scope()), fetchProjects()]).finally(() =>
      setLoading(false),
    );
  });

  // Re-fetch stats when scope changes (not on initial mount — handled above)
  let mounted = false;
  createEffect(() => {
    const currentScope = scope();
    if (!mounted) {
      mounted = true;
      return;
    }
    fetchStats(currentScope);
  });

  // Auto-refresh API every 5 minutes
  const timer = setInterval(fetchApi, 5 * 60 * 1000);
  onCleanup(() => clearInterval(timer));

  // Computed data for rate limit cards
  const rateBuckets = () => {
    const api = apiData();
    if (!api) return [];
    const buckets: { label: string; bucket: RateBucket }[] = [];
    if (api.five_hour) buckets.push({ label: "5-Hour", bucket: api.five_hour });
    if (api.seven_day) buckets.push({ label: "7-Day", bucket: api.seven_day });
    if (api.seven_day_opus) buckets.push({ label: "7-Day Opus", bucket: api.seven_day_opus });
    if (api.seven_day_sonnet) buckets.push({ label: "7-Day Sonnet", bucket: api.seven_day_sonnet });
    if (api.seven_day_cowork) buckets.push({ label: "7-Day Cowork", bucket: api.seven_day_cowork });
    return buckets;
  };

  // Sorted models
  const sortedModels = () => {
    const stats = sessionStats();
    if (!stats) return [];
    return Object.entries(stats.model_usage).sort(
      (a, b) => (b[1].input_tokens + b[1].output_tokens) - (a[1].input_tokens + a[1].output_tokens),
    );
  };

  // Sorted projects by tokens
  const sortedProjects = () => {
    const stats = sessionStats();
    if (!stats) return [];
    return Object.entries(stats.per_project).sort(
      (a, b) => (b[1].input_tokens + b[1].output_tokens) - (a[1].input_tokens + a[1].output_tokens),
    );
  };

  // Heatmap weeks
  const heatmapWeeks = () => {
    const stats = sessionStats();
    if (!stats) return [];
    return buildHeatmap(stats.daily_activity);
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
            {(p) => <option value={p.slug}>{p.slug} ({p.session_count})</option>}
          </For>
        </select>
      </div>

      {/* Loading state */}
      <Show when={loading()}>
        <div class={s.loadingState}>Loading usage data...</div>
      </Show>

      {/* API error */}
      <Show when={apiError()}>
        <div class={s.errorState}>API: {apiError()}</div>
      </Show>

      {/* Rate limits */}
      <Show when={rateBuckets().length > 0}>
        <div class={s.section}>
          <div class={s.sectionTitle}>Rate Limits</div>
          <div class={s.rateGrid}>
            <For each={rateBuckets()}>
              {(item) => (
                <div class={s.rateCard}>
                  <div class={s.rateLabel}>
                    <span>{item.label}</span>
                    <span class={s.rateValue}>{Math.round(item.bucket.utilization * 100)}%</span>
                  </div>
                  <div class={s.rateBar}>
                    <div
                      class={`${s.rateFill} ${rateClass(item.bucket.utilization)}`}
                      style={{ width: `${Math.min(100, item.bucket.utilization * 100)}%` }}
                    />
                  </div>
                  <span class={s.rateReset}>
                    Resets in {formatResetTime(item.bucket.resets_at)}
                  </span>
                </div>
              )}
            </For>
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
                                title={cell.date}
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
                        title={`Click to filter by ${slug}`}
                      >
                        <span class={s.projectSlug}>{slug}</span>
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
