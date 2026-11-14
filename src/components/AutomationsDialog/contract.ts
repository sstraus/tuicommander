/** Shared backend boundary. Rust model fields retain their serde spelling.
 * Keep API integration changes in transportAdapter.ts; Rust owns cron validation and occurrence calculation; UI may format labels.
 */
export interface AutomationDefinition {
	id: string;
	name: string;
	prompt: string;
	run_config: string;
	repository: string;
	workspace: { mode: "existing" } | { mode: "new_per_run"; base_branch: string };
	cron: string;
	once_local?: string | null;
	once_at?: string | null;
	timezone: string;
	enabled: boolean;
	grace_secs: number;
	overlap: "skip";
	max_duration_secs: number;
	close_on_finish?: boolean;
	precheck: { command: string; timeout_secs: number; inject_output?: boolean } | null;
}
export type RunStatus =
	| "reserved"
	| "prechecking"
	| "running"
	| "needs_you"
	| "completed"
	| "failed"
	| "unknown"
	| "timed_out"
	| "interrupted"
	| "skipped_precheck"
	| "skipped_overlap"
	| "skipped_concurrency"
	| "skipped_expired"
	| "skipped_missed"
	| "skipped_filtered"
	| "skipped_duplicate";
export interface AutomationListItem {
	definition: AutomationDefinition;
	next_run_ms: number | null;
	last_status: RunStatus | null;
	run_config_error?: string | null;
}
export interface SavedOutput {
	text: string;
	truncated: boolean;
}
export interface AutomationRun {
	failure_count?: number;
	id: string;
	definition: AutomationDefinition;
	status: RunStatus;
	trigger: { kind: "manual" } | { kind: "scheduled"; occurrence_ms: number };
	created_ms: number;
	updated_ms: number;
	finished_ms: number | null;
	task_id: string | null;
	session_id: string | null;
	workspace: string | null;
	workspace_id?: string | null;
	stdout: SavedOutput;
	stderr: SavedOutput;
	reason: string | null;
	precheck_context?: { injected: boolean; bytes: number } | null;
	precheck: { exit_code: number | null; timed_out: boolean; stdout: SavedOutput; stderr: SavedOutput } | null;
}
export interface SchedulePreview {
	cron: string;
	timezone: string;
	occurrences: string[];
}
export interface DefinitionSchedulePreview extends SchedulePreview {
	once_local: string | null;
	completed: boolean;
}
export type SchedulePreset =
	| { kind: "hourly"; minute: number }
	| { kind: "daily" | "weekdays"; hour: number; minute: number }
	| { kind: "weekly"; weekday: number; hour: number; minute: number };
export interface RunReceipt {
	status: RunStatus;
	reason: string | null;
}
export interface RunSummary {
	total: number;
	by_status: Partial<Record<RunStatus, number>>;
}
export interface AutomationTemplate {
	id: string;
	name: string;
}
export interface AutomationAdapter {
	templates(): Promise<AutomationTemplate[]>;
	profiles(): Promise<string[]>;
	template(
		template_id: string,
		repository: string,
		run_config: string,
		timezone: string,
	): Promise<AutomationDefinition>;
	list(): Promise<AutomationListItem[]>;
	run(id: string): Promise<AutomationRun>;
	save(definition: AutomationDefinition, create: boolean): Promise<AutomationDefinition>;
	setEnabled(id: string, enabled: boolean): Promise<void>;
	remove(id: string): Promise<void>;
	runNow(id: string): Promise<RunReceipt>;
	history(id?: string, offset?: number, limit?: number, groupFailures?: boolean): Promise<AutomationRun[]>;
	summary(window: "24h" | "7d"): Promise<RunSummary>;
	preview(cron: string, timezone: string): Promise<SchedulePreview>;
	previewDefinition(definition: AutomationDefinition): Promise<DefinitionSchedulePreview>;
	preset(preset: SchedulePreset): Promise<{ cron: string }>;
}

/** Request contract for automation_action; serde model payloads are unchanged. */
export type AutomationAction =
	| { action: "list" | "templates" | "profiles" }
	| { action: "template"; template_id: string; repository: string; run_config: string; timezone: string | null }
	| { action: "create" | "update"; definition: AutomationDefinition }
	| { action: "update"; id: string; enabled: boolean }
	| { action: "get" | "run" | "worktree_preview" | "cleanup_worktree"; id: string }
	| { action: "summary"; window: "24h" | "7d" }
	| { action: "delete" | "run_now" | "pause" | "resume"; id: string }
	| { action: "list_runs"; id?: string; limit: number; offset?: number; group_failures?: boolean }
	| { action: "preview"; cron: string; timezone: string | null; count: number }
	| { action: "preview_definition"; definition: AutomationDefinition; count: number }
	| { action: "preset"; preset: SchedulePreset };
