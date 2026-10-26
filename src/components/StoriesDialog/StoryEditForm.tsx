import { createSignal, For, Index, Show } from "solid-js";
import { t } from "../../i18n";
import type { StoryRisk, StoryType } from "../../types/stories.gen";
import { Button } from "../ui/Button";
import { Input } from "../ui/Input";
import { Select } from "../ui/Select";
import { Textarea } from "../ui/Textarea";
import s from "./StoriesDialog.module.css";

type StoryOrigin = { type: "native" } | { type: "plan_step"; step: string } | { type: "git_hub_issue"; number: number };

export interface EditableStory {
	id: string;
	revision: number;
	title: string;
	priority: number;
	storyType?: StoryType | null;
	origin: StoryOrigin;
	problem?: string | null;
	risk?: StoryRisk | null;
	criteria: string[];
	fileScope: string[];
	dependencies: string[];
}

export type StoryEditPatch = Partial<
	Pick<
		EditableStory,
		"title" | "priority" | "storyType" | "origin" | "problem" | "risk" | "criteria" | "fileScope" | "dependencies"
	>
>;

/** The snapshot is fixed for the lifetime of the form, including after a failed save. */
export function StoryEditForm(props: {
	story: EditableStory;
	candidates: { id: string; title: string }[];
	busy: boolean;
	error: string;
	onSave: (patch: StoryEditPatch) => void;
	onCancel: () => void;
}) {
	const baseline = props.story;
	const [title, setTitle] = createSignal(baseline.title);
	const [priority, setPriority] = createSignal(baseline.priority);
	const [storyType, setStoryType] = createSignal<StoryType | "">(baseline.storyType ?? "");
	const [originType, setOriginType] = createSignal(baseline.origin.type);
	const [step, setStep] = createSignal(baseline.origin.type === "plan_step" ? baseline.origin.step : "");
	const [issue, setIssue] = createSignal(
		baseline.origin.type === "git_hub_issue" ? String(baseline.origin.number) : "",
	);
	const [problem, setProblem] = createSignal(baseline.problem ?? "");
	const [risk, setRisk] = createSignal<StoryRisk | "">(baseline.risk ?? "");
	const [criteria, setCriteria] = createSignal([...baseline.criteria]);
	const [scope, setScope] = createSignal(baseline.fileScope.join("\n"));
	const [dependencies, setDependencies] = createSignal([...baseline.dependencies]);
	const lines = (text: string) =>
		text
			.split("\n")
			.map((line) => line.trim())
			.filter(Boolean);
	const patch = (): StoryEditPatch => {
		const result: StoryEditPatch = {};
		const changed = <K extends keyof StoryEditPatch>(key: K, value: StoryEditPatch[K]) => {
			if (JSON.stringify(value ?? null) !== JSON.stringify(baseline[key] ?? null)) result[key] = value;
		};
		if (title() !== baseline.title) changed("title", title().trim());
		changed("priority", priority());
		changed("storyType", storyType() || null);
		if (problem() !== (baseline.problem ?? "")) changed("problem", problem().trim() || null);
		changed("risk", risk() || null);
		changed(
			"origin",
			originType() === "native"
				? { type: "native" }
				: originType() === "plan_step"
					? { type: "plan_step", step: step() }
					: { type: "git_hub_issue", number: Number(issue()) },
		);
		// Do not resend untouched criteria: duplicate requirement text has no stable identity.
		changed("criteria", criteria());
		if (scope() !== baseline.fileScope.join("\n")) changed("fileScope", lines(scope()));
		changed("dependencies", dependencies());
		return result;
	};
	function moveCriterion(index: number, direction: -1 | 1): void {
		setCriteria((items) => {
			const next = [...items];
			[next[index], next[index + direction]] = [next[index + direction], next[index]];
			return next;
		});
	}

	return (
		<form
			class={s.form}
			aria-label={t("stories.edit.form", "Edit story")}
			onSubmit={(event) => {
				event.preventDefault();
				props.onSave(patch());
			}}
		>
			<label>
				{t("stories.edit.title", "Title")}
				<Input
					required
					maxlength={500}
					value={title()}
					disabled={props.busy}
					onInput={(event) => setTitle(event.currentTarget.value)}
				/>
			</label>
			<div class={s.inline}>
				<label>
					{t("stories.priority", "Priority")}
					<Select
						value={priority()}
						disabled={props.busy}
						onChange={(event) => setPriority(Number(event.currentTarget.value))}
					>
						<For each={[1, 2, 3]}>{(value) => <option value={value}>P{value}</option>}</For>
					</Select>
				</label>
				<label>
					{t("stories.storyType", "Story type")}
					<Select
						value={storyType()}
						disabled={props.busy}
						onChange={(event) => setStoryType(event.currentTarget.value as StoryType | "")}
					>
						<option value="">{t("stories.type.untyped", "Untyped")}</option>
						<option value="feature">{t("stories.type.feature", "Feature")}</option>
						<option value="fix">{t("stories.type.fix", "Fix")}</option>
						<option value="refactor">{t("stories.type.refactor", "Refactor")}</option>
						<option value="chore">{t("stories.type.chore", "Chore")}</option>
						<option value="task">{t("stories.type.task", "Task")}</option>
					</Select>
				</label>
			</div>
			<label>
				{t("stories.edit.origin", "Origin")}
				<Select
					value={originType()}
					disabled={props.busy}
					onChange={(event) => setOriginType(event.currentTarget.value as StoryOrigin["type"])}
				>
					<option value="native">{t("stories.edit.native", "Native")}</option>
					<option value="plan_step">{t("stories.edit.planStep", "Plan step")}</option>
					<option value="git_hub_issue">{t("stories.edit.issue", "GitHub issue")}</option>
				</Select>
			</label>
			<Show when={originType() === "plan_step"}>
				<label>
					{t("stories.edit.planStep", "Plan step")}
					<Input
						required
						value={step()}
						disabled={props.busy}
						onInput={(event) => setStep(event.currentTarget.value)}
					/>
				</label>
			</Show>
			<Show when={originType() === "git_hub_issue"}>
				<label>
					{t("stories.edit.issue", "GitHub issue")}
					<Input
						required
						type="number"
						min={1}
						step={1}
						value={issue()}
						disabled={props.busy}
						onInput={(event) => setIssue(event.currentTarget.value)}
					/>
				</label>
			</Show>
			<label>
				{t("stories.edit.problem", "Problem (optional)")}
				<Textarea
					rows={3}
					maxlength={20000}
					value={problem()}
					disabled={props.busy}
					onInput={(event) => setProblem(event.currentTarget.value)}
				/>
			</label>
			<label>
				{t("stories.edit.risk", "Risk (optional)")}
				<Select
					value={risk()}
					disabled={props.busy}
					onChange={(event) => setRisk(event.currentTarget.value as StoryRisk | "")}
				>
					<option value="">{t("stories.unspecified", "Unspecified")}</option>
					<option value="low">{t("stories.edit.riskLow", "Low")}</option>
					<option value="medium">{t("stories.edit.riskMedium", "Medium")}</option>
					<option value="high">{t("stories.edit.riskHigh", "High")}</option>
				</Select>
			</label>
			<fieldset disabled={props.busy} class={s.editCriteria}>
				<legend>{t("stories.criteria", "Acceptance criteria")}</legend>
				<Index each={criteria()}>
					{(criterion, index) => (
						<div class={s.editCriterion}>
							<label>
								{t("stories.edit.criterion", "Acceptance criterion {number}", { number: String(index + 1) })}
								<Textarea
									required
									rows={3}
									value={criterion()}
									onInput={(event) => {
										const text = event.currentTarget.value;
										setCriteria((items) => items.map((item, position) => (position === index ? text : item)));
									}}
								/>
							</label>
							<div class={s.inline}>
								<Button
									type="button"
									disabled={index === 0}
									aria-label={t("stories.edit.moveUp", "Move criterion {number} up", { number: String(index + 1) })}
									onClick={() => moveCriterion(index, -1)}
								>
									{t("stories.edit.up", "Up")}
								</Button>
								<Button
									type="button"
									disabled={index === criteria().length - 1}
									aria-label={t("stories.edit.moveDown", "Move criterion {number} down", { number: String(index + 1) })}
									onClick={() => moveCriterion(index, 1)}
								>
									{t("stories.edit.down", "Down")}
								</Button>
								<Button
									type="button"
									disabled={criteria().length === 1}
									aria-label={t("stories.edit.removeCriterion", "Remove criterion {number}", {
										number: String(index + 1),
									})}
									onClick={() => setCriteria((items) => items.filter((_, position) => position !== index))}
								>
									{t("stories.remove", "Remove")}
								</Button>
							</div>
						</div>
					)}
				</Index>
				<Button type="button" onClick={() => setCriteria((items) => [...items, ""])}>
					{t("stories.edit.addCriterion", "Add criterion")}
				</Button>
			</fieldset>
			<label>
				{t("stories.edit.scope", "File scope (one path per line)")}
				<Textarea
					rows={3}
					value={scope()}
					disabled={props.busy}
					onInput={(event) => setScope(event.currentTarget.value)}
				/>
			</label>
			<fieldset disabled={props.busy} class={s.editDependencies}>
				<legend>{t("stories.dependencies", "Dependencies")}</legend>
				<For each={props.candidates}>
					{(candidate) => (
						<label>
							<input
								type="checkbox"
								checked={dependencies().includes(candidate.id)}
								onChange={(event) =>
									setDependencies(
										event.currentTarget.checked
											? [...dependencies(), candidate.id]
											: dependencies().filter((id) => id !== candidate.id),
									)
								}
							/>
							<span>{candidate.title}</span>
						</label>
					)}
				</For>
			</fieldset>
			<Show when={props.error}>
				<p role="alert" class={s.editError}>
					{props.error}
				</p>
			</Show>
			<div class={s.inline}>
				<Button variant="primary" type="submit" disabled={props.busy || !Object.keys(patch()).length}>
					{t("stories.edit.save", "Save")}
				</Button>
				<Button type="button" disabled={props.busy} onClick={props.onCancel}>
					{t("stories.edit.cancel", "Cancel")}
				</Button>
			</div>
		</form>
	);
}
