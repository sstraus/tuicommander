import type { AutomationAdapter, AutomationDefinition, AutomationRun } from "../components/AutomationsDialog/contract";
export const definition: AutomationDefinition = {
	id: "a1",
	name: "Sentry",
	prompt: "Review new issues",
	repository: "/repo",
	run_config: "claude:unattended",
	workspace: { mode: "existing" },
	cron: "*/30 * * * *",
	timezone: "Europe/Madrid",
	enabled: true,
	grace_secs: 43200,
	overlap: "skip",
	max_duration_secs: 1500,
	precheck: null,
};
export function fakeAdapter(overrides: Partial<AutomationAdapter> = {}): AutomationAdapter {
	return {
		list: async () => [{ definition, next_run_ms: 1791568800000, last_status: "skipped_overlap" }],
		save: async (value) => value,
		setEnabled: async () => {},
		remove: async () => {},
		runNow: async () => ({ status: "skipped_concurrency", reason: "All slots are busy" }),
		history: async () => [],
		preview: async (cron, timezone) => ({
			cron,
			timezone: timezone || "Europe/Madrid",
			occurrences: ["2026-10-09T19:00:00Z"],
		}),
		previewDefinition: async (value) => ({
			cron: value.cron,
			timezone: value.timezone,
			once_local: value.once_local ?? null,
			occurrences: ["2026-10-16T08:00:00Z"],
			completed: false,
		}),
		preset: async () => ({ cron: "30 7 * * 1-5" }),
		...overrides,
	};
}

export function runFixture(id: string, automationId: string): AutomationRun {
	return {
		id,
		definition: { ...definition, id: automationId },
		status: "completed",
		trigger: { kind: "manual" },
		created_ms: 1791568800000,
		updated_ms: 1791568800000,
		finished_ms: 1791568800000,
		task_id: null,
		session_id: null,
		workspace: null,
		stdout: { text: "", truncated: false },
		stderr: { text: "", truncated: false },
		precheck: null,
		reason: null,
	};
}
