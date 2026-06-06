import { type Component, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { invoke, listen } from "../../../invoke";
import { appLogger } from "../../../stores/appLogger";
import { toastsStore } from "../../../stores/toasts";
import s from "../Settings.module.css";

type ReasoningEffort = "auto" | "off" | "low" | "medium" | "high";

interface AiChatConfig {
	temperature: number;
	reasoning_effort?: ReasoningEffort;
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

type IntervalUnit = "min" | "hr" | "day";

interface ParsedInterval {
	value: number;
	unit: IntervalUnit;
	totalMinutes: number;
}

function parseCronInterval(expr: string): ParsedInterval | null {
	// minutes: `0 0/{n} * * * *`
	const mMin = expr.match(/^0\s+0\/(\d+)\s+\*\s+\*\s+\*\s+\*$/);
	if (mMin) {
		const mins = parseInt(mMin[1], 10);
		if (mins >= 60 && mins % 60 === 0) return { value: mins / 60, unit: "hr", totalMinutes: mins };
		return { value: mins, unit: "min", totalMinutes: mins };
	}
	// hourly: `0 0 0/{n} * * *` or `0 0 * * * *`
	const mHr = expr.match(/^0\s+0\s+(?:0\/)?(\d+|\*)\s+\*\s+\*\s+\*$/);
	if (mHr) {
		const hrs = mHr[1] === "*" ? 1 : parseInt(mHr[1], 10);
		return { value: hrs, unit: "hr", totalMinutes: hrs * 60 };
	}
	// daily: `0 0 {h} */{n} * *`
	const mDay = expr.match(/^0\s+0\s+\d+\s+\*\/(\d+)\s+\*\s+\*$/);
	if (mDay) {
		const days = parseInt(mDay[1], 10);
		return { value: days, unit: "day", totalMinutes: days * 1440 };
	}
	return null;
}

function humanInterval(expr: string): string {
	const p = parseCronInterval(expr);
	if (!p) return expr;
	const labels: Record<IntervalUnit, [string, string]> = {
		min: ["min", "min"],
		hr: ["hour", "hr"],
		day: ["day", "days"],
	};
	const [singular, plural] = labels[p.unit];
	return p.value === 1 ? `every ${singular}` : `every ${p.value} ${plural}`;
}

function nextRunLabel(expr: string): string {
	const p = parseCronInterval(expr);
	if (!p) return "";
	// Daily intervals: cron fires at a fixed hour — next-run calc not meaningful here
	if (p.unit === "day") return "";
	const mins = p.totalMinutes;
	const now = new Date();
	const currentMin = now.getHours() * 60 + now.getMinutes();
	const nextMin = (Math.floor(currentMin / mins) + 1) * mins;
	const nextDate = new Date(now);
	nextDate.setHours(Math.floor(nextMin / 60) % 24, nextMin % 60, 0, 0);
	if (nextMin >= 24 * 60) {
		nextDate.setDate(nextDate.getDate() + 1);
		nextDate.setHours(0, (nextMin % (24 * 60)) % 60, 0, 0);
	}
	return nextDate.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

function buildCronExpr(value: number, unit: IntervalUnit): string {
	switch (unit) {
		case "min":
			return `0 0/${value} * * * *`;
		case "hr":
			return `0 0 0/${value} * * *`;
		case "day":
			return `0 0 9 */${value} * *`;
	}
}

const UNIT_LIMITS: Record<IntervalUnit, { min: number; max: number }> = {
	min: { min: 5, max: 59 },
	hr: { min: 1, max: 23 },
	day: { min: 1, max: 1 },
};

interface SchedulerConfig {
	jobs: ScheduledJob[];
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export const AiChatTab: Component = () => {
	const [temperature, setTemperature] = createSignal(0.7);
	const [reasoningEffort, setReasoningEffort] = createSignal<ReasoningEffort>("auto");

	// Scheduler state
	const [schedulerJobs, setSchedulerJobs] = createSignal<ScheduledJob[]>([]);
	const [newIntervalValue, setNewIntervalValue] = createSignal(15);
	const [newIntervalUnit, setNewIntervalUnit] = createSignal<IntervalUnit>("min");
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
						reasoning_effort: reasoningEffort(),
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
			setReasoningEffort(config.reasoning_effort ?? "auto");
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
					.catch((e) => appLogger.warn("config", "load_scheduler_config refresh failed", { error: String(e) }));
			},
		);
		onCleanup(() => {
			unlisten();
			clearTimeout(saveTimer);
		});
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
		const value = newIntervalValue();
		const unit = newIntervalUnit();
		const limits = UNIT_LIMITS[unit];
		if (!goal || value < limits.min) return;
		const id = `job-${Date.now().toString(36)}`;
		const cron_expr = buildCronExpr(value, unit);
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
		const jobs = schedulerJobs().map((j) => (j.id === id ? { ...j, enabled: !j.enabled } : j));
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
				<p class={s.hint}>Controls randomness of responses (0.0 = deterministic, 1.0 = creative)</p>
			</div>

			<div class={s.group}>
				<label>Extended thinking</label>
				<select
					value={reasoningEffort()}
					onChange={(e) => {
						setReasoningEffort(e.currentTarget.value as ReasoningEffort);
						saveConfig();
					}}
				>
					<option value="auto">Auto (on for Opus 4.7+)</option>
					<option value="off">Off</option>
					<option value="low">Low</option>
					<option value="medium">Medium</option>
					<option value="high">High</option>
				</select>
				<p class={s.hint}>
					Streams the model's reasoning into a collapsible "Thinking" block. Only models that support extended thinking
					(Claude Opus 4.7+) are affected; higher effort costs more tokens and latency.
				</p>
			</div>

			{/* ── Scheduled Tasks ── */}
			<h3>Scheduled Tasks</h3>

			<div class={s.group}>
				<p class={s.hint} style={{ "margin-bottom": "8px" }}>
					Cron-triggered agent tasks. The agent runs with standard trust level (destructive commands require approval).
				</p>

				<For each={schedulerJobs()}>
					{(job) => (
						<div class={s.schedulerRow}>
							<label class={s.schedulerToggle}>
								<input type="checkbox" checked={job.enabled} onChange={() => handleToggleJob(job.id)} />
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
							<button class={s.schedulerRemove} onClick={() => handleRemoveJob(job.id)} title="Remove">
								×
							</button>
						</div>
					)}
				</For>

				<div class={s.schedulerAdd}>
					<input
						type="number"
						class={s.schedulerCronInput}
						value={newIntervalValue()}
						min={UNIT_LIMITS[newIntervalUnit()].min}
						max={UNIT_LIMITS[newIntervalUnit()].max}
						onInput={(e) => setNewIntervalValue(parseInt(e.currentTarget.value, 10) || 1)}
					/>
					<select
						class={s.schedulerUnitSelect}
						value={newIntervalUnit()}
						onChange={(e) => {
							const unit = e.currentTarget.value as IntervalUnit;
							setNewIntervalUnit(unit);
							const limits = UNIT_LIMITS[unit];
							const cur = newIntervalValue();
							if (cur < limits.min) setNewIntervalValue(limits.min);
							else if (cur > limits.max) setNewIntervalValue(limits.max);
						}}
					>
						<option value="min">min</option>
						<option value="hr">hr</option>
						<option value="day">day</option>
					</select>
					<input
						type="text"
						class={s.schedulerGoalInput}
						value={newGoal()}
						placeholder="Goal (e.g. run tests and report)"
						onInput={(e) => setNewGoal(e.currentTarget.value)}
					/>
					<label class={s.schedulerOneShotLabel} title="Run once then disable">
						<input type="checkbox" checked={newOneShot()} onChange={(e) => setNewOneShot(e.currentTarget.checked)} />
						once
					</label>
					<button
						class={s.testBtn}
						disabled={!newGoal().trim() || newIntervalValue() < UNIT_LIMITS[newIntervalUnit()].min}
						onClick={handleAddJob}
					>
						Add
					</button>
				</div>
				<p class={s.hint}>Jobs run with standard trust — destructive commands require approval.</p>
			</div>

			<p class={s.hint} style={{ "margin-top": "16px", color: "var(--fg-muted)" }}>
				Settings are saved automatically when changed
			</p>
		</div>
	);
};
