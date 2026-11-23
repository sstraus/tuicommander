import type { JsonValue } from "../components/WorkflowDesigner/catalogTypes";
import { type NeutralDefinition, neutralClient } from "../components/WorkflowDesigner/neutralClient";
import { t } from "../i18n";
import { toastsStore } from "../stores/toasts";
import { workflowsUi } from "../stores/workflowsUi";
import { randomUuid } from "../utils/randomId";

/** A published workflow that a panel can start for its issue or pull request. */
export interface LaunchableWorkflow {
	id: string;
	name: string;
	revision: number;
}

/** The limits the Workflows space proposes for a new run. */
const DEFAULT_LIMITS = { max_activations: 100, max_concurrency: 4, max_duration_ms: 10 * 60_000 };

/** Workflows of the project with at least one published revision; a run always uses a publication. */
export async function listLaunchableWorkflows(project: string): Promise<LaunchableWorkflow[]> {
	const drafts = await neutralClient(() => project)({ action: "list_drafts" }, "drafts");
	return drafts
		.filter((draft) => draft.latest_published_revision > 0)
		.map((draft) => ({ id: draft.id, name: draft.name, revision: draft.latest_published_revision }));
}

/** Input values from the field defaults, or null when a required field has no default to use. */
function defaultInputs(definition: NeutralDefinition): Record<string, JsonValue> | null {
	const inputs: Record<string, JsonValue> = {};
	for (const [name, field] of Object.entries(definition.inputs.fields)) {
		if (field.default !== null) inputs[name] = field.default;
		else if (field.required) return null;
	}
	return inputs;
}

/** Starts the published revision with `subject` as its subject. A workflow that needs input
 *  values nobody can guess opens in the Workflows space instead, where the run form asks for them. */
export async function startWorkflow(
	project: string,
	workflow: LaunchableWorkflow,
	subject: Record<string, JsonValue>,
): Promise<void> {
	const call = neutralClient(() => project);
	const publication = await call(
		{ action: "get_publication", id: workflow.id, revision: workflow.revision },
		"publication",
	);
	const inputs = defaultInputs(publication.definition);
	if (!inputs) {
		workflowsUi.open(project);
		toastsStore.add(
			t("workflowLaunch.needsInputs", "{name} needs input values", { name: workflow.name }),
			t("workflowLaunch.fillInputs", "Fill them in the Workflows space to start the run."),
		);
		return;
	}
	await call(
		{
			action: "start",
			request: {
				publication: { project, id: workflow.id, revision: workflow.revision },
				request_id: randomUuid(),
				inputs,
				subject: publication.definition.subject ? subject : null,
				limits: DEFAULT_LIMITS,
			},
		},
		"inspection",
	);
	toastsStore.add(t("workflowLaunch.started", "Started {name}", { name: workflow.name }), "", "info", false, {
		label: t("workflowLaunch.open", "Open"),
		onClick: () => workflowsUi.open(project),
	});
}
