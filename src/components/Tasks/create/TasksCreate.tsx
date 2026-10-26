import { For, Show } from "solid-js";
import type { WorkItemFieldSchema } from "../../../types/work-items.gen";
import { Button } from "../../ui/Button";
import { Input } from "../../ui/Input";
import { Select } from "../../ui/Select";
import { Textarea } from "../../ui/Textarea";
import { CreateField as SettingField } from "./CreateField";
import { TaskCreateField } from "./TaskCreateField";
import { TaskCreateOptional } from "./TaskCreateOptional";
import styles from "./TasksCreate.module.css";
import { type TasksCreateOptions, useTasksCreate } from "./useTasksCreate";

export interface TasksCreateProps extends TasksCreateOptions {
	onCancel?: () => void;
}

/** Core-owned create form. The provider supplies item types and field schemas, never markup. */
export function TasksCreate(props: TasksCreateProps) {
	const model = useTasksCreate(props);
	const busy = () => model.loading() || model.submitting();
	const required = () => model.schema().filter((entry) => entry.required);
	const optional = () => model.schema().filter((entry) => !entry.required);
	const field = (schema: WorkItemFieldSchema) => (
		<TaskCreateField
			schema={schema}
			provider={props.provider}
			error={model.fieldError(schema)}
			value={model.draft()[schema.id]}
			disabled={busy()}
			onInput={(value) => model.setValue(schema.id, value)}
		/>
	);
	return (
		<form
			class={styles.create}
			aria-label="Create task"
			onSubmit={(event) => {
				event.preventDefault();
				void model.submit();
			}}
		>
			<h2>New task in {props.provider.name}</h2>
			<SettingField label="Type *" error={!model.itemType() ? "Required: Type" : undefined}>
				{(id, hintId) => (
					<Select
						id={id}
						aria-describedby={hintId}
						aria-required="true"
						aria-invalid={!model.itemType()}
						disabled={model.submitting() || model.types().length < 2}
						onChange={(event) => model.chooseType(event.currentTarget.value)}
					>
						<option value="" selected={model.itemType() === ""}>
							Choose a type
						</option>
						<For each={model.types()}>
							{(type) => (
								<option value={type.id} selected={model.itemType() === type.id}>
									{type.name}
								</option>
							)}
						</For>
					</Select>
				)}
			</SettingField>
			<SettingField label="Title *" error={!model.title().trim() ? "Required: Title" : undefined}>
				{(id, hintId) => (
					<Input
						id={id}
						type="text"
						aria-describedby={hintId}
						aria-required="true"
						aria-invalid={!model.title().trim()}
						value={model.title()}
						disabled={model.submitting()}
						onInput={(event) => model.setTitle(event.currentTarget.value)}
					/>
				)}
			</SettingField>
			<SettingField label="Description">
				{(id) => (
					<Textarea
						id={id}
						value={model.body()}
						disabled={model.submitting()}
						onInput={(event) => model.setBody(event.currentTarget.value)}
					/>
				)}
			</SettingField>
			<div class={styles.fields}>
				<For each={required()}>{field}</For>
			</div>
			<TaskCreateOptional fields={optional()} error={model.fieldError}>
				{field}
			</TaskCreateOptional>
			<Show when={model.loading()}>
				<p class={styles.muted} role="status">
					Loading fields…
				</p>
			</Show>
			<Show when={model.error()}>
				{(error) => (
					<p class={styles.error} role="alert">
						{error()}
						<Button type="button" onClick={model.retry} disabled={busy()}>
							Retry
						</Button>
					</p>
				)}
			</Show>
			<div class={styles.actions}>
				<Show when={props.onCancel}>
					<Button type="button" onClick={props.onCancel} disabled={model.submitting()}>
						Cancel
					</Button>
				</Show>
				<Button type="submit" variant="primary" disabled={!model.canSubmit()}>
					{model.submitting() ? "Creating…" : "Create"}
				</Button>
			</div>
		</form>
	);
}
