import { Component, For, Show, createSignal, onCleanup, onMount } from "solid-js";
import { invoke, listen } from "../../../invoke";
import { appLogger } from "../../../stores/appLogger";
import { toastsStore } from "../../../stores/toasts";
import s from "../Settings.module.css";

interface AiChatConfig {
  temperature: number;
}

interface ScheduledJob {
  id: string;
  cron_expr: string;
  goal: string;
  target_session?: string | null;
  max_duration_secs: number;
  enabled: boolean;
  one_shot?: boolean;
}

/** Parse agent-generated cron `0 0/{n} * * * *` → interval in minutes, or null. */
function parseCronIntervalMinutes(expr: string): number | null {
  const m = expr.match(/^0\s+0\/(\d+)\s+\*\s+\*\s+\*\s+\*$/);
  if (m) return parseInt(m[1], 10);
  // also match hourly `0 0 * * * *` → 60 min
  if (/^0\s+0\s+\*\s+\*\s+\*\s+\*$/.test(expr)) return 60;
  return null;
}

function humanInterval(expr: string): string {
  const mins = parseCronIntervalMinutes(expr);
  if (mins === null) return expr;
  if (mins < 60) return `every ${mins} min`;
  const hrs = mins / 60;
  return hrs === 1 ? "every hour" : `every ${hrs} hr`;
}

function nextRunLabel(expr: string): string {
  const mins = parseCronIntervalMinutes(expr);
  if (mins === null) return "";
  const now = new Date();
  // Compute next multiple-of-interval boundary from the current minute
  const currentMin = now.getHours() * 60 + now.getMinutes();
  const nextMin = (Math.floor(currentMin / mins) + 1) * mins;
  const nextDate = new Date(now);
  nextDate.setHours(Math.floor(nextMin / 60) % 24, nextMin % 60, 0, 0);
  if (nextMin >= 24 * 60) {
    nextDate.setDate(nextDate.getDate() + 1);
    nextDate.setHours(0, nextMin % (24 * 60) % 60, 0, 0);
  }
  return nextDate.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

interface SchedulerConfig {
  jobs: ScheduledJob[];
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export const AiChatTab: Component = () => {
  const [temperature, setTemperature] = createSignal(0.7);

  // Scheduler state
  const [schedulerJobs, setSchedulerJobs] = createSignal<ScheduledJob[]>([]);
  const [newIntervalMinutes, setNewIntervalMinutes] = createSignal(15);
  const [newGoal, setNewGoal] = createSignal("");
  const [newOneShot, setNewOneShot] = createSignal(false);

  let saveTimer: ReturnType<typeof setTimeout> | undefined;

  // ---------------------------------------------------------------------------
  // Config persistence
  // ---------------------------------------------------------------------------

  const saveConfig = () => {
    clearTimeout(saveTimer);
    saveTimer = setTimeout(async () => {
      try {
        await invoke("save_ai_chat_config", {
          config: {
            temperature: temperature(),
          },
        });
      } catch (e) {
        appLogger.error("config", "Failed to save AI Chat config", e);
      }
    }, 500);
  };

  onMount(async () => {
    try {
      const config = await invoke<AiChatConfig>("load_ai_chat_config");
      setTemperature(config.temperature ?? 0.7);
    } catch (e) {
      appLogger.warn("config", "Failed to load AI Chat config", e);
    }

    try {
      const sc = await invoke<SchedulerConfig>("load_scheduler_config");
      setSchedulerJobs(sc.jobs);
    } catch (e) {
      appLogger.warn("config", "Failed to load scheduler config", e);
    }

    const unlisten = await listen<{ job_id: string; goal: string; timed_out: boolean }>(
      "scheduled-job-completed",
      (event) => {
        const { goal, timed_out } = event.payload;
        if (timed_out) {
          toastsStore.add("Scheduled task timed out", goal, "warn");
        } else {
          toastsStore.add("Scheduled task completed", goal, "info");
        }
        // Refresh job list (one-shot jobs may now be disabled)
        invoke<SchedulerConfig>("load_scheduler_config")
          .then((sc) => setSchedulerJobs(sc.jobs))
          .catch(() => {});
      }
    );
    onCleanup(() => unlisten());
  });

  // ---------------------------------------------------------------------------
  // Scheduler handlers
  // ---------------------------------------------------------------------------

  const saveScheduler = async (jobs: ScheduledJob[]) => {
    try {
      await invoke("save_scheduler_config", { config: { jobs } });
      setSchedulerJobs(jobs);
    } catch (e) {
      appLogger.error("config", "Failed to save scheduler config", e);
    }
  };

  const handleAddJob = async () => {
    const goal = newGoal().trim();
    const interval = newIntervalMinutes();
    if (!goal || interval < 5) return;
    const id = `job-${Date.now().toString(36)}`;
    const cron_expr = `0 0/${interval} * * * *`;
    const job: ScheduledJob = {
      id,
      cron_expr,
      goal,
      target_session: null,
      max_duration_secs: 300,
      enabled: true,
      one_shot: newOneShot(),
    };
    await saveScheduler([...schedulerJobs(), job]);
    setNewGoal("");
    setNewOneShot(false);
  };

  const handleToggleJob = async (id: string) => {
    const jobs = schedulerJobs().map((j) =>
      j.id === id ? { ...j, enabled: !j.enabled } : j,
    );
    await saveScheduler(jobs);
  };

  const handleRemoveJob = async (id: string) => {
    await saveScheduler(schedulerJobs().filter((j) => j.id !== id));
  };

  // ---------------------------------------------------------------------------
  // Render
  // ---------------------------------------------------------------------------

  return (
    <div class={s.section}>
      {/* ── Parameters ── */}
      <h3>Parameters</h3>

      <div class={s.group}>
        <label>Temperature</label>
        <div class={s.slider}>
          <input
            type="range"
            min={0}
            max={1}
            step={0.1}
            value={temperature()}
            onInput={(e) => {
              setTemperature(parseFloat(e.currentTarget.value));
              saveConfig();
            }}
          />
          <span>{temperature().toFixed(1)}</span>
        </div>
        <p class={s.hint}>
          Controls randomness of responses (0.0 = deterministic, 1.0 = creative)
        </p>
      </div>

      {/* ── Scheduled Tasks ── */}
      <h3>Scheduled Tasks</h3>

      <div class={s.group}>
        <p class={s.hint} style={{ "margin-bottom": "8px" }}>
          Cron-triggered agent tasks. The agent runs with standard trust level
          (destructive commands require approval).
        </p>

        <For each={schedulerJobs()}>
          {(job) => (
            <div class={s.schedulerRow}>
              <label class={s.schedulerToggle}>
                <input
                  type="checkbox"
                  checked={job.enabled}
                  onChange={() => handleToggleJob(job.id)}
                />
              </label>
              <div class={s.schedulerInfo}>
                <span class={s.schedulerGoal}>{job.goal}</span>
                <div class={s.schedulerMeta}>
                  <code class={s.schedulerCron}>{humanInterval(job.cron_expr)}</code>
                  <Show when={nextRunLabel(job.cron_expr)}>
                    <span class={s.schedulerNext}>next {nextRunLabel(job.cron_expr)}</span>
                  </Show>
                  <Show when={job.one_shot}>
                    <span class={s.schedulerBadge}>once</span>
                  </Show>
                  <Show when={job.id.startsWith("agent-")}>
                    <span class={s.schedulerBadge}>AI</span>
                  </Show>
                </div>
              </div>
              <button
                class={s.schedulerRemove}
                onClick={() => handleRemoveJob(job.id)}
                title="Remove"
              >
                ×
              </button>
            </div>
          )}
        </For>

        <div class={s.schedulerAdd}>
          <input
            type="number"
            class={s.schedulerCronInput}
            value={newIntervalMinutes()}
            min={5}
            max={1440}
            title="Interval in minutes (min 5)"
            onInput={(e) => setNewIntervalMinutes(parseInt(e.currentTarget.value, 10) || 15)}
          />
          <span class={s.schedulerAddLabel}>min</span>
          <input
            type="text"
            class={s.schedulerGoalInput}
            value={newGoal()}
            placeholder="Goal (e.g. run tests and report)"
            onInput={(e) => setNewGoal(e.currentTarget.value)}
          />
          <label class={s.schedulerOneShotLabel} title="Run once then disable">
            <input
              type="checkbox"
              checked={newOneShot()}
              onChange={(e) => setNewOneShot(e.currentTarget.checked)}
            />
            once
          </label>
          <button
            class={s.testBtn}
            disabled={!newGoal().trim() || newIntervalMinutes() < 5}
            onClick={handleAddJob}
          >
            Add
          </button>
        </div>
        <p class={s.hint}>
          Interval in minutes (minimum 5). Jobs run with standard trust — destructive commands require approval.
        </p>
      </div>

      <p class={s.hint} style={{ "margin-top": "16px", color: "var(--fg-muted)" }}>
        Settings are saved automatically when changed
      </p>
    </div>
  );
};
