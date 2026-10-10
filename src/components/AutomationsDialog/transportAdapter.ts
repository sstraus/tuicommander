import { invoke } from "../../invoke";
import type {
	AutomationAction,
	AutomationAdapter,
	AutomationDefinition,
	AutomationListItem,
	AutomationRun,
	DefinitionSchedulePreview,
	RunReceipt,
	SchedulePreview,
} from "./contract";

/** The only dependency on Step 8's command envelope. Local machine only.
 * DEFERRED (2026-10-09): reconcile reply envelopes with story 1617 when it lands.
 * No direct fetch or remote fallback: transport routing stays in invoke/COMMAND_TABLE.
 */
function call<T>(input: AutomationAction): Promise<T> {
	return invoke<T>("automation_action", { input });
}

export const automationAdapter: AutomationAdapter = {
	list: () => call<AutomationListItem[]>({ action: "list" }),
	save: (definition, create) => call<AutomationDefinition>({ action: create ? "create" : "update", definition }),
	setEnabled: (id, enabled) => call<void>({ action: enabled ? "resume" : "pause", id }),
	remove: (id) => call<void>({ action: "delete", id }),
	runNow: (id) => call<RunReceipt>({ action: "run_now", id }),
	history: (id) => call<AutomationRun[]>({ action: "list_runs", id, limit: 50 }),
	preview: (cron, timezone) => call<SchedulePreview>({ action: "preview", cron, timezone: timezone || null, count: 4 }),
	previewDefinition: (definition) =>
		call<DefinitionSchedulePreview>({ action: "preview_definition", definition, count: 4 }),
	preset: (preset) => call<{ cron: string }>({ action: "preset", preset }),
};
