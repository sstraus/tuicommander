/** Provisional Step 8 boundary. Rust model fields retain their serde spelling.
 * Keep API integration changes in transportAdapter.ts; UI never interprets cron.
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
	timezone: string;
	enabled: boolean;
	grace_secs: number;
	overlap: "skip";
	max_duration_secs: number;
	precheck: { command: string; timeout_secs: number } | null;
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
	| "skipped_expired";
export interface AutomationListItem {
	definition: AutomationDefinition;
	next_run_ms: number | null;
	last_status: RunStatus | null;
}
export interface SavedOutput {
	text: string;
	truncated: boolean;
}
export interface AutomationRun {
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
	stdout: SavedOutput;
	stderr: SavedOutput;
	reason: string | null;
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
export interface AutomationAdapter {
	list(): Promise<AutomationListItem[]>;
	save(definition: AutomationDefinition, create: boolean): Promise<AutomationDefinition>;
	setEnabled(id: string, enabled: boolean): Promise<void>;
	remove(id: string): Promise<void>;
	runNow(id: string): Promise<RunReceipt>;
	history(id: string): Promise<AutomationRun[]>;
	preview(cron: string, timezone: string): Promise<SchedulePreview>;
	previewDefinition(definition: AutomationDefinition): Promise<DefinitionSchedulePreview>;
	preset(preset: SchedulePreset): Promise<{ cron: string }>;
}

/** Proposed request contract for Step 8; serde model payloads are unchanged. */
export type AutomationAction =
	| { action: "list" }
	| { action: "create" | "update"; definition: AutomationDefinition }
	| { action: "delete" | "run_now" | "pause" | "resume"; id: string }
	| { action: "list_runs"; id: string; limit: number }
	| { action: "preview"; cron: string; timezone: string | null; count: number }
	| { action: "preview_definition"; definition: AutomationDefinition; count: number }
	| { action: "preset"; preset: SchedulePreset };
