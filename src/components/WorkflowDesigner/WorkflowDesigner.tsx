import { type Component, Show } from "solid-js";
import { NeutralDesigner } from "./NeutralDesigner";

export interface WorkflowDesignerProps {
	project: string;
}

export const WorkflowDesigner: Component<WorkflowDesignerProps> = (props) => (
	<Show when={props.project} keyed>
		{(project) => <NeutralDesigner project={project} />}
	</Show>
);
