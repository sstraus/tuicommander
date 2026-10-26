import { createSignal, Show } from "solid-js";
import { t } from "../../i18n";
import { Button } from "../ui/Button";
import { Input } from "../ui/Input";
import s from "./StoriesDialog.module.css";

export interface EditablePlan {
	id: string;
	revision: number;
	title: string;
	source: string;
}

export type PlanEditPatch = Partial<Pick<EditablePlan, "title" | "source">>;

/** Keep the initial revision and draft stable across refreshes and failed saves. */
export function PlanEditForm(props: {
	plan: EditablePlan;
	busy: boolean;
	error: string;
	onSave: (patch: PlanEditPatch) => void;
	onCancel: () => void;
}) {
	const baseline = props.plan;
	const [title, setTitle] = createSignal(baseline.title);
	const [source, setSource] = createSignal(baseline.source);
	const patch = (): PlanEditPatch => {
		const result: PlanEditPatch = {};
		if (title().trim() !== baseline.title) result.title = title().trim();
		if (source().trim() !== baseline.source) result.source = source().trim();
		return result;
	};
	return (
		<form
			class={s.form}
			aria-label={t("stories.planEdit.open", "Edit plan")}
			onSubmit={(event) => {
				event.preventDefault();
				props.onSave(patch());
			}}
		>
			<label>
				{t("stories.planEdit.title", "Plan title")}
				<Input
					required
					maxlength={200}
					value={title()}
					disabled={props.busy}
					onInput={(event) => setTitle(event.currentTarget.value)}
				/>
			</label>
			<label>
				{t("stories.planSource", "Plan document or link")}
				<Input
					required
					maxlength={4096}
					value={source()}
					disabled={props.busy}
					onInput={(event) => setSource(event.currentTarget.value)}
				/>
			</label>
			<Show when={props.error}>
				<p role="alert" class={s.editError}>
					{props.error}
				</p>
			</Show>
			<div class={s.inline}>
				<Button
					variant="primary"
					type="submit"
					disabled={props.busy || !title().trim() || !source().trim() || !Object.keys(patch()).length}
				>
					{t("stories.edit.save", "Save")}
				</Button>
				<Button type="button" disabled={props.busy} onClick={props.onCancel}>
					{t("stories.edit.cancel", "Cancel")}
				</Button>
			</div>
		</form>
	);
}
