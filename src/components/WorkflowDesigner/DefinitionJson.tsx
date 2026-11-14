import { Textarea } from "../ui/Textarea";
import type { NeutralDefinition } from "./neutralClient";
import s from "./WorkflowDesigner.module.css";

export function DefinitionJson(props: {
	definition: NeutralDefinition;
	onApply: (value: unknown) => Promise<void>;
	onError: (error: string) => void;
}) {
	return (
		<details class={s.canvas}>
			<summary>Structured definition: inputs, outputs, conditions, scopes and subflows</summary>
			<p>Complete JSON edits are checked by the backend before applying.</p>
			<Textarea
				aria-label="Neutral definition JSON"
				value={JSON.stringify(props.definition, null, 2)}
				onChange={(event) => {
					try {
						void props.onApply(JSON.parse(event.currentTarget.value));
					} catch (cause) {
						props.onError(String(cause));
					}
				}}
			/>
		</details>
	);
}
