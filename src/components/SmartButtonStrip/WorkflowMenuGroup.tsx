import { createResource, For, Show } from "solid-js";
import { t } from "../../i18n";
import { listLaunchableWorkflows, startWorkflow } from "../../services/workflowLaunch";
import { appLogger } from "../../stores/appLogger";
import { workflowsUi } from "../../stores/workflowsUi";
import { cx } from "../../utils";
import type { JsonValue } from "../WorkflowDesigner/catalogTypes";
import { plainMessage } from "../WorkflowDesigner/neutralClient";
import s from "./SmartButtonStrip.module.css";

/** The "Workflow" group of a split menu: the project's published workflows, started with the
 *  panel's item as subject. Mounted only while the menu is open, so the list is fetched on demand. */
export function WorkflowMenuGroup(props: {
	project: string;
	subject: () => Record<string, JsonValue>;
	onDone: () => void;
	onError?: (msg: string) => void;
}) {
	const [workflows] = createResource(
		() => props.project,
		(project) =>
			listLaunchableWorkflows(project).catch((cause) => {
				appLogger.warn("prompts", "Failed to list workflows for the split menu", cause);
				return [];
			}),
	);
	const run = (workflow: { id: string; name: string; revision: number }) => {
		props.onDone();
		startWorkflow(props.project, workflow, props.subject()).catch((cause) => {
			const message = plainMessage(cause);
			props.onError?.(message);
			appLogger.error("prompts", `Failed to start workflow "${workflow.name}"`, cause);
		});
	};
	return (
		<>
			<div class={s.menuGroup}>{t("smartStrip.workflow", "Workflow")}</div>
			<Show when={!workflows.loading} fallback={<div class={s.menuNote}>{t("smartStrip.loading", "Loading…")}</div>}>
				<For
					each={workflows() ?? []}
					fallback={<div class={s.menuNote}>{t("smartStrip.noWorkflows", "No published workflows")}</div>}
				>
					{(workflow) => (
						<button
							type="button"
							data-control-layout="row"
							class={s.menuItem}
							onClick={(e) => {
								e.stopPropagation();
								run(workflow);
							}}
						>
							{workflow.name}
						</button>
					)}
				</For>
			</Show>
		</>
	);
}

/** The last entry of a split menu that lists workflows: opens the Workflows space. */
export function ManageWorkflowsItem(props: { project: string; onDone: () => void }) {
	return (
		<>
			<div class={s.menuDivider} />
			<button
				type="button"
				data-control-layout="row"
				class={cx(s.menuItem, s.menuItemMuted)}
				onClick={(e) => {
					e.stopPropagation();
					props.onDone();
					workflowsUi.open(props.project);
				}}
			>
				{t("smartStrip.manageWorkflows", "Manage workflows…")}
			</button>
		</>
	);
}
