import { createEffect, createMemo, createSignal, For, Show } from "solid-js";
import type { WorkItemProvider } from "../../../plugins/workItemProvider";
import { workItemLinksStore } from "../../../stores/workItemLinks";
import type {
	WorkItemFieldSchema,
	WorkItemFieldValue,
	WorkItemOption,
	WorkItemRef,
} from "../../../types/work-items.gen";
import { Button } from "../../ui/Button";
import { Input } from "../../ui/Input";
import { Select } from "../../ui/Select";
import { Switch } from "../../ui/Switch";
import { filterOptions, loadFilterChoices } from "./filterChoices";
import { TaskRows } from "./TaskRows";
import s from "./tasksList.module.css";
import { initialTaskQuery, useTasksListQuery } from "./useTasksListQuery";

export interface TasksListProps {
	providers: readonly WorkItemProvider[];
	onOpen: (reference: WorkItemRef) => void;
	selected?: WorkItemRef | null;
	visible?: boolean;
	repoPath?: string;
}
const sourceKey = (provider: WorkItemProvider) => JSON.stringify([provider.providerId, provider.accountId]);

function filterValue(
	field: WorkItemFieldSchema,
	value: string,
	options: readonly WorkItemOption[],
): WorkItemFieldValue | undefined {
	switch (field.kind) {
		case "text":
		case "markdown":
		case "date":
		case "link":
		case "enum":
			return { kind: field.kind, value };
		case "number":
			return Number.isFinite(Number(value)) ? { kind: "number", value: Number(value) } : undefined;
		case "labels":
			return {
				kind: "labels",
				value: [{ id: value, name: options.find((option) => option.id === value)?.name ?? value }],
			};
		case "user":
			return {
				kind: "user",
				value: [
					{
						id: value,
						name: options.find((option) => option.id === value)?.name ?? value,
						avatarUrl: null,
						bot: false,
					},
				],
			};
	}
}

/** Generic host-owned list; provider descriptors and query results supply all item data. */
export function TasksList(props: TasksListProps) {
	const [selectedSource, setSelectedSource] = createSignal("");
	const provider = createMemo(
		() => props.providers.find((entry) => sourceKey(entry) === selectedSource()) ?? props.providers[0],
	);
	const selectedKey = () => {
		const current = provider();
		return current ? sourceKey(current) : "";
	};
	const { model, query, setQuery, search, setSearch, linkedOnly, setLinkedOnly, cancelSearch, updateSearch } =
		useTasksListQuery(props, provider);
	const [filtersOpen, setFiltersOpen] = createSignal(false);
	const activeFilters = () =>
		query().filters.length + (query().sort ? 1 : 0) + (query().groupBy ? 1 : 0) + (linkedOnly() ? 1 : 0);
	const linkedBranch = (reference: WorkItemRef) => workItemLinksStore.findByItem(reference)?.branch;
	// Links live in the host, not in the provider, so this filter applies to the loaded rows.
	const shownItems = createMemo(() =>
		linkedOnly() ? model.state().items.filter((item) => linkedBranch(item.reference)) : model.state().items,
	);
	const shownGroups = createMemo(() =>
		linkedOnly()
			? model
					.state()
					.groups.map((group) => ({ ...group, items: group.items.filter((item) => linkedBranch(item.reference)) }))
					.filter((group) => group.items.length)
			: model.state().groups,
	);
	// The provider total counts every match; the linked filter narrows only the loaded rows, so it no longer applies.
	const itemsLabel = () => {
		const total = model.state().total;
		const shown = shownItems().length;
		return !linkedOnly() && total !== null && total > shown
			? `${shown} of ${total.toLocaleString()} items`
			: `${shown} items`;
	};
	const [choices, setChoices] = createSignal<readonly WorkItemFieldSchema[]>([]);
	createEffect(() => {
		const current = provider();
		if (!filtersOpen() || !current) return;
		void loadFilterChoices(current).then((fields) => {
			if (provider() === current) setChoices(fields);
		});
	});
	function applyPreset(id: string) {
		cancelSearch();
		const preset = provider()?.savedFilters.find((entry) => entry.id === id);
		const next = preset ? structuredClone({ ...preset.query, savedFilterId: id, cursor: null }) : initialTaskQuery();
		setSearch(next.search);
		setQuery(next);
	}
	function applyFilter(field: WorkItemFieldSchema, raw: string) {
		const value = raw ? filterValue(field, raw, filterOptions(field, choices())) : undefined;
		setQuery((previous) => ({
			...previous,
			savedFilterId: null,
			filters: [
				...previous.filters.filter((entry) => entry.fieldId !== field.id),
				...(value ? [{ fieldId: field.id, values: [value] }] : []),
			],
		}));
	}
	function selectedFilter(id: string) {
		const value = query().filters.find((entry) => entry.fieldId === id)?.values[0];
		if (!value) return "";
		if (value.kind === "user" || value.kind === "labels") return value.value[0]?.id ?? "";
		return String(value.value);
	}

	return (
		<section class={s.list} aria-label="Tasks">
			<div class={s.toolbar}>
				<label>
					Source
					<Select
						aria-label="Task source"
						value={selectedKey()}
						onChange={(event) => setSelectedSource(event.currentTarget.value)}
					>
						<For each={props.providers}>
							{(entry) => (
								<option value={sourceKey(entry)}>
									{entry.name} · {entry.accountId}
								</option>
							)}
						</For>
					</Select>
				</label>
				<label>
					Preset
					<Select
						aria-label="Task preset"
						value={query().savedFilterId ?? ""}
						onChange={(event) => applyPreset(event.currentTarget.value)}
					>
						<option value="">All items</option>
						<For each={provider()?.savedFilters}>{(entry) => <option value={entry.id}>{entry.name}</option>}</For>
					</Select>
				</label>
				<label class={s.search}>
					Search
					<Input
						type="search"
						aria-label="Search tasks"
						value={search()}
						onInput={(event) => updateSearch(event.currentTarget.value)}
					/>
				</label>
				<Button type="button" aria-expanded={filtersOpen()} onClick={() => setFiltersOpen((open) => !open)}>
					{activeFilters() ? `Filters (${activeFilters()})` : "Filters"}
				</Button>
				<Button
					type="button"
					disabled={!provider() || model.state().loading || props.visible === false}
					onClick={() => void model.refresh()}
				>
					Refresh
				</Button>
			</div>
			<Show when={filtersOpen() ? provider() : undefined}>
				{(current) => (
					<div class={s.filters}>
						<For each={current().fields.filter((field) => model.state().local || field.filterable)}>
							{(field) => (
								<label>
									{field.name}
									<Show
										when={filterOptions(field, choices()).length > 0}
										fallback={
											<Input
												aria-label={`Filter ${field.name}`}
												type={field.kind === "date" ? "date" : field.kind === "number" ? "number" : "text"}
												value={selectedFilter(field.id)}
												onChange={(event) => applyFilter(field, event.currentTarget.value)}
											/>
										}
									>
										<Select
											aria-label={`Filter ${field.name}`}
											value={selectedFilter(field.id)}
											onChange={(event) => applyFilter(field, event.currentTarget.value)}
										>
											<option value="">Any</option>
											<For each={filterOptions(field, choices())}>
												{(option) => <option value={option.id}>{option.name}</option>}
											</For>
										</Select>
									</Show>
								</label>
							)}
						</For>
						<label>
							Sort
							<Select
								aria-label="Sort tasks"
								value={query().sort?.fieldId ?? ""}
								onChange={(event) => {
									const fieldId = event.currentTarget.value;
									setQuery((previous) => ({
										...previous,
										savedFilterId: null,
										sort: fieldId ? { fieldId, descending: previous.sort?.descending ?? false } : null,
									}));
								}}
							>
								<option value="">Provider default</option>
								<For each={current().fields.filter((field) => model.state().local || field.sortable)}>
									{(field) => <option value={field.id}>{field.name}</option>}
								</For>
							</Select>
						</label>
						<Button
							type="button"
							disabled={!query().sort}
							aria-label="Reverse task sort"
							onClick={() =>
								setQuery((previous) => ({
									...previous,
									savedFilterId: null,
									sort: previous.sort ? { ...previous.sort, descending: !previous.sort.descending } : null,
								}))
							}
						>
							{query().sort?.descending ? "Descending" : "Ascending"}
						</Button>
						<label>
							Group
							<Select
								aria-label="Group tasks"
								value={query().groupBy ?? ""}
								onChange={(event) => {
									const groupBy = event.currentTarget.value || null;
									setQuery((previous) => ({ ...previous, savedFilterId: null, groupBy }));
								}}
							>
								<option value="">No grouping</option>
								<For each={current().fields.filter((field) => model.state().local || field.groupable)}>
									{(field) => <option value={field.id}>{field.name}</option>}
								</For>
							</Select>
						</label>
						<label class={s.check}>
							<Switch
								role="checkbox"
								checked={linkedOnly()}
								onChange={(event) => setLinkedOnly(event.currentTarget.checked)}
							/>
							Linked to a worktree
						</label>
					</div>
				)}
			</Show>
			<div class={s.status} role="status" aria-live="polite">
				{model.state().loading ? "Loading tasks…" : itemsLabel()}
				<Show when={!model.state().loading && provider() && model.state().local}>
					<span>All items loaded</span>
				</Show>
				<Show when={model.state().stale}>
					<span class={s.stale}>Stale results</span>
				</Show>
				<Show when={model.state().error}>
					<span class={s.error}>{model.state().error}</span>
					<Button type="button" onClick={() => void model.refresh()}>
						Retry
					</Button>
				</Show>
			</div>
			<Show when={!provider()}>
				<p class={s.empty}>Install a work-item provider to show tasks.</p>
			</Show>
			<Show when={provider() && !model.state().loading && !shownItems().length && !model.state().error}>
				<p class={s.empty}>No tasks match this query.</p>
			</Show>
			<TaskRows
				items={shownItems()}
				groups={shownGroups()}
				loading={model.state().loading}
				resetKey={[query(), linkedOnly()]}
				linkedBranch={linkedBranch}
				selected={props.selected}
				onOpen={props.onOpen}
			/>
			<Show when={model.state().nextCursor}>
				<Button
					class={s.more}
					type="button"
					disabled={model.state().loading || props.visible === false}
					onClick={() => void model.loadMore()}
				>
					Load more
				</Button>
			</Show>
		</section>
	);
}
