import { Component, For, createSignal, onMount } from "solid-js";
import { invoke } from "../../../invoke";
import { appLogger } from "../../../stores/appLogger";
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
  const [newCron, setNewCron] = createSignal("");
  const [newGoal, setNewGoal] = createSignal("");

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
    const cron = newCron().trim();
    const goal = newGoal().trim();
    if (!cron || !goal) return;
    const id = `job-${Date.now().toString(36)}`;
    const job: ScheduledJob = {
      id,
      cron_expr: cron,
      goal,
      target_session: null,
      max_duration_secs: 300,
      enabled: true,
    };
    await saveScheduler([...schedulerJobs(), job]);
    setNewCron("");
    setNewGoal("");
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
              <code class={s.schedulerCron}>{job.cron_expr}</code>
              <span class={s.schedulerGoal}>{job.goal}</span>
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
            type="text"
            class={s.schedulerCronInput}
            value={newCron()}
            placeholder="0 0 * * * *"
            onInput={(e) => setNewCron(e.currentTarget.value)}
          />
          <input
            type="text"
            class={s.schedulerGoalInput}
            value={newGoal()}
            placeholder="Goal (e.g. run tests and report)"
            onInput={(e) => setNewGoal(e.currentTarget.value)}
          />
          <button
            class={s.testBtn}
            disabled={!newCron().trim() || !newGoal().trim()}
            onClick={handleAddJob}
          >
            Add
          </button>
        </div>
        <p class={s.hint}>
          Cron format: sec min hour day month weekday (6 fields).
          Example: <code>0 0 * * * *</code> = top of every hour.
        </p>
      </div>

      <p class={s.hint} style={{ "margin-top": "16px", color: "var(--fg-muted)" }}>
        Settings are saved automatically when changed
      </p>
    </div>
  );
};
