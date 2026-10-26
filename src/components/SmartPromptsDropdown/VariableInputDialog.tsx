import { type Component, createSignal, For, onMount } from "solid-js";
import { VARIABLE_DESCRIPTIONS } from "../../data/smartPromptsBuiltIn";
import { registerModal } from "../../stores/modalStack";
import { Button, Input } from "../ui";
import s from "./VariableInputDialog.module.css";

export interface VariableInputDialogProps {
	/** Variable names that need user input */
	variables: string[];
	/** Pre-populated values (partial auto-resolve results) */
	suggestions?: Record<string, string>;
	/** Prompt name for context */
	promptName: string;
	onSubmit: (values: Record<string, string>) => void;
	onCancel: () => void;
}

export const VariableInputDialog: Component<VariableInputDialogProps> = (props) => {
	const [values, setValues] = createSignal<Record<string, string>>({});
	let firstInputRef: HTMLInputElement | undefined;

	onMount(() => {
		// Pre-populate with suggestions
		const initial: Record<string, string> = {};
		for (const v of props.variables) {
			initial[v] = props.suggestions?.[v] ?? "";
		}
		setValues(initial);
		// Focus first input
		requestAnimationFrame(() => firstInputRef?.focus());
	});

	const updateValue = (name: string, value: string) => {
		setValues((prev) => ({ ...prev, [name]: value }));
	};

	const allFilled = () => props.variables.every((v) => values()[v]?.trim());

	const handleSubmit = (e: Event) => {
		e.preventDefault();
		if (allFilled()) {
			props.onSubmit(values());
		}
	};

	// Escape-to-close is handled centrally (stores/modalStack): registering routes
	// Escape to props.onCancel AND stops it reaching the terminal underneath.
	registerModal(props.onCancel);

	return (
		<div class={s.overlay} onClick={props.onCancel}>
			<form
				class={s.dialog}
				role="dialog"
				aria-modal="true"
				aria-label={props.promptName}
				onClick={(e) => e.stopPropagation()}
				onSubmit={handleSubmit}
			>
				<div class={s.header}>
					<span class={s.title}>{props.promptName}</span>
				</div>

				<div class={s.fields}>
					<For each={props.variables}>
						{(varName, i) => {
							const desc = VARIABLE_DESCRIPTIONS[varName] ?? `Value for ${varName}`;
							return (
								<label class={s.field}>
									<span class={s.fieldName}>{`{${varName}}`}</span>
									<Input
										ref={i() === 0 ? (el) => (firstInputRef = el) : undefined}
										type="text"
										placeholder={desc}
										value={values()[varName] ?? ""}
										onInput={(e) => updateValue(varName, e.currentTarget.value)}
									/>
								</label>
							);
						}}
					</For>
				</div>

				<div class={s.actions}>
					<Button type="button" onClick={props.onCancel}>
						Cancel
					</Button>
					<Button type="submit" variant="primary" disabled={!allFilled()}>
						Run
					</Button>
				</div>
			</form>
		</div>
	);
};
