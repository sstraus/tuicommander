import type { Component } from "solid-js";
import { storiesUi } from "../../stores/storiesUi";
import { workflowsUi } from "../../stores/workflowsUi";
import s from "../StoriesDialog/StoriesDialog.module.css";
import d from "../shared/dialog.module.css";
import { Button } from "../ui/Button";
import { WorkflowDesigner } from "../WorkflowDesigner/WorkflowDesigner";

/** Workflows space (Boss Q6=A): neutral definitions next to Plans and stories.
 *  It never needs a selected plan or story. */
export const WorkflowsDialog: Component<{ project: string; onClose: () => void }> = (props) => (
	<div class={d.overlay} onClick={() => props.onClose()}>
		<div
			class={s.dialog}
			role="dialog"
			aria-modal="true"
			aria-label="Workflows"
			onClick={(event) => event.stopPropagation()}
		>
			<header class={s.header}>
				<div>
					<h2>Workflows</h2>
					<span class={s.project} title={props.project}>
						{props.project}
					</span>
				</div>
				<div class={s.headerActions}>
					<Button
						size="compact"
						type="button"
						onClick={() => {
							storiesUi.open(props.project);
							workflowsUi.close();
						}}
					>
						Plans and stories
					</Button>
				</div>
				<Button size="compact" type="button" aria-label="Close Workflows" onClick={() => props.onClose()}>
					Close
				</Button>
			</header>
			<WorkflowDesigner project={props.project} />
		</div>
	</div>
);
