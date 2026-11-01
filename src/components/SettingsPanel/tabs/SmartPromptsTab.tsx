import { type Accessor, type Component, createMemo, createSignal, For, onMount, Show } from "solid-js";
import { AGENTS, type AgentType, toHeadlessAgentChoice } from "../../../agents";
import { SMART_PROMPTS_BUILTIN } from "../../../data/smartPromptsBuiltIn";
import { useAgentDetection } from "../../../hooks/useAgentDetection";
import { useConfirmDialog } from "../../../hooks/useConfirmDialog";
import { agentConfigsStore } from "../../../stores/agentConfigs";
import { promptLibraryStore, type SavedPrompt } from "../../../stores/promptLibrary";
import { onClickKeyDown } from "../../../utils/a11y";
import { ConfirmDialog } from "../../ConfirmDialog";
import { SmartPromptEditor } from "../../shared/SmartPromptEditor";

export { repoControlledVarsInContent } from "../../shared/SmartPromptEditor";

import { Button } from "../../ui/Button";
import { Select } from "../../ui/Select";
import { StatusBadge } from "../../ui/StatusBadge";
import { Switch } from "../../ui/Switch";
import s from "../Settings.module.css";
import { DangerZone, ListRow, SectionCard, SettingPageHeader, SettingRow } from "../SettingsPrimitives";
import sp from "./SmartPromptsTab.module.css";

// Categories extracted from smart prompt tags
const CATEGORIES = ["git", "review", "pr", "merge", "ci", "investigation", "code", "release"] as const;

type Category = (typeof CATEGORIES)[number];

/** Built-in defaults indexed by ID for quick lookup */
const BUILTIN_BY_ID = new Map(SMART_PROMPTS_BUILTIN.map((p) => [p.id, p]));

/** Derive the category from a prompt's tags */
function promptCategory(prompt: SavedPrompt): Category | "other" {
	const tags = prompt.tags ?? [];
	for (const cat of CATEGORIES) {
		if (tags.includes(cat)) return cat;
	}
	return "other";
}

/** Collect all smart prompts (enabled + disabled), merging store state with built-in defaults */
function getAllSmartPrompts(): SavedPrompt[] {
	const all = promptLibraryStore.getAllPrompts().filter((p) => p.tags?.includes("smart"));
	// Include built-in defaults that aren't in the store yet
	const storeIds = new Set(all.map((p) => p.id));
	for (const builtin of SMART_PROMPTS_BUILTIN) {
		if (!storeIds.has(builtin.id)) {
			all.push(builtin);
		}
	}
	return all.sort((a, b) => a.name.localeCompare(b.name));
}

/** Group prompts by category */
function groupByCategory(prompts: SavedPrompt[]): Map<string, SavedPrompt[]> {
	return Map.groupBy(prompts, promptCategory);
}

// ---------------------------------------------------------------------------
// Sub-components
// ---------------------------------------------------------------------------

/** Immediate-save Settings adapter for the shared editor. */
const PromptEditor: Component<{ prompt: SavedPrompt; onClose: () => void; headlessAgents: AgentType[] }> = (props) => {
	const isBuiltIn = () => !!props.prompt.builtIn;
	const builtInDefault = () => BUILTIN_BY_ID.get(props.prompt.id);
	const dialogs = useConfirmDialog();
	const handleReset = () => {
		const def = builtInDefault();
		if (def) {
			promptLibraryStore.resetToDefault(props.prompt.id, def);
		}
	};

	const handleDelete = async () => {
		const confirmed = await dialogs.confirm({
			title: "Delete smart prompt?",
			message: `Delete "${props.prompt.name}"? This cannot be undone.`,
			okLabel: "Delete",
			kind: "warning",
		});
		if (confirmed) {
			promptLibraryStore.deletePrompt(props.prompt.id);
			props.onClose();
		}
	};

	return (
		<>
			<SmartPromptEditor
				prompt={props.prompt}
				onChange={(update) => promptLibraryStore.updatePrompt(props.prompt.id, update)}
				headlessAgents={props.headlessAgents}
				onReset={handleReset}
			/>
			<Show when={!isBuiltIn()}>
				<DangerZone
					title="Delete prompt"
					scope={props.prompt.name}
					consequence="This permanently removes this custom prompt."
				>
					<Button
						variant="danger"
						data-non-setting="Reset, remove or create a stable Smart Prompt through its owner"
						onClick={handleDelete}
					>
						Delete
					</Button>
				</DangerZone>
			</Show>

			<ConfirmDialog
				visible={dialogs.dialogState() !== null}
				title={dialogs.dialogState()?.title ?? ""}
				message={dialogs.dialogState()?.message ?? ""}
				confirmLabel={dialogs.dialogState()?.confirmLabel}
				cancelLabel={dialogs.dialogState()?.cancelLabel}
				kind={dialogs.dialogState()?.kind}
				onClose={dialogs.handleClose}
				onConfirm={dialogs.handleConfirm}
			/>
		</>
	);
};

/** Single prompt row with toggle, badges, and click-to-expand */
const PromptRow: Component<{ prompt: SavedPrompt; onEdit: (id: string) => void; selected: Accessor<string | null> }> = (
	props,
) => {
	const isEnabled = () => props.prompt.enabled !== false;
	const mode = () => props.prompt.executionMode ?? "inject";
	const builtInDefault = () => BUILTIN_BY_ID.get(props.prompt.id);
	const hasUpdate = () => {
		const def = builtInDefault();
		return def ? promptLibraryStore.hasUpdate(props.prompt.id, def.builtInVersion ?? 1) : false;
	};

	return (
		<div class={sp.promptRow}>
			<ListRow
				name={props.prompt.name}
				description={props.prompt.description}
				badges={
					<>
						<StatusBadge label={mode()} />
						<For each={props.prompt.placement ?? []}>{(placement) => <StatusBadge label={placement} />}</For>
						<StatusBadge label={props.prompt.builtIn ? "Built-in" : "Custom"} />
						<Show when={hasUpdate()}>
							<StatusBadge label="Updated" />
						</Show>
					</>
				}
				actions={
					<>
						<Button
							data-non-setting="Open or close a structured Smart Prompt editor"
							class={s.testBtn}
							aria-label={`Edit ${props.prompt.name}`}
							title={`Edit ${props.prompt.name}`}
							aria-expanded={props.selected() === props.prompt.id}
							onClick={() => props.onEdit(props.prompt.id)}
						>
							Edit
						</Button>
						<Switch
							data-setting-id="prompts.prompts.{id}.text"
							role="switch"
							aria-label={`Enable ${props.prompt.name}`}
							checked={isEnabled()}
							onChange={() => promptLibraryStore.updatePrompt(props.prompt.id, { enabled: !isEnabled() })}
						/>
					</>
				}
			/>
		</div>
	);
};

/** Collapsible category group */
const CategoryGroup: Component<{
	category: string;
	prompts: SavedPrompt[];
	onEdit: (id: string) => void;
	selected: Accessor<string | null>;
}> = (props) => {
	const [open, setOpen] = createSignal(true);

	return (
		<>
			<div
				class={sp.categoryHeader}
				role="button"
				tabIndex={0}
				onClick={() => setOpen(!open())}
				onKeyDown={onClickKeyDown(() => setOpen(!open()))}
			>
				<span class={sp.categoryChevron} classList={{ [sp.open]: open() }}>
					&#9654;
				</span>
				<span>{props.category === "pr" ? "PR" : props.category === "ci" ? "CI" : props.category}</span>
				<span class={sp.categoryCount}>{props.prompts.length}</span>
			</div>
			<Show when={open()}>
				<For each={props.prompts}>
					{(prompt) => <PromptRow prompt={prompt} onEdit={props.onEdit} selected={props.selected} />}
				</For>
			</Show>
		</>
	);
};

// ---------------------------------------------------------------------------
// Main tab
// ---------------------------------------------------------------------------

// The prompts owner stores the structured SavedPrompt envelope in each record's text.
// These controls edit that envelope; they do not introduce parallel persisted fields.
export const SmartPromptsTab: Component = () => {
	const [selected, setSelected] = createSignal<string | null>(null);
	const selectedPrompt = () => getAllSmartPrompts().find((prompt) => prompt.id === selected());
	const detection = useAgentDetection();
	const groups = createMemo(() => groupByCategory(getAllSmartPrompts()));

	onMount(() => {
		detection.detectAll();
	});

	/** Agents that have a headless template and are installed on this machine */
	const headlessAgents = createMemo(() =>
		detection
			.getAvailable()
			.filter((a) => a.type !== "git" && AGENTS[a.type]?.defaultHeadlessTemplate)
			.map((a) => a.type),
	);

	// Ordered categories, with "other" at the end if present
	const orderedCategories = () => {
		const g = groups();
		const ordered: string[] = [];
		for (const cat of CATEGORIES) {
			if (g.has(cat)) ordered.push(cat);
		}
		if (g.has("other")) ordered.push("other");
		return ordered;
	};

	const handleNewPrompt = () => {
		promptLibraryStore.createPrompt({
			name: "New Smart Prompt",
			content: "",
			category: "custom",
			isFavorite: false,
			tags: ["smart"],
			enabled: true,
			placement: ["command-palette"],
			executionMode: "inject",
			autoExecute: false,
		});
	};

	return (
		<div class={s.section}>
			<SettingPageHeader title="Smart Prompts" scope="Application defaults" breadcrumb="AI / Smart Prompts" />
			<SectionCard title="Background agent">
				<SettingRow
					label="Headless Agent"
					hint="Default agent for headless prompts. Individual prompts can override this in their settings."
				>
					{(id, descriptionId) => (
						<Select
							data-setting-id="agents.headless_agent"
							id={id}
							aria-describedby={descriptionId()}
							value={agentConfigsStore.getHeadlessAgent() ?? ""}
							onChange={(e) => agentConfigsStore.setHeadlessAgent(toHeadlessAgentChoice(e.currentTarget.value))}
						>
							<option value="">— Not configured —</option>
							<For each={headlessAgents()}>
								{(type) => {
									const configs = () => agentConfigsStore.getRunConfigs(type);
									return (
										<Show
											when={configs().length > 0}
											fallback={<option value={type}>{AGENTS[type]?.name ?? type}</option>}
										>
											<optgroup label={AGENTS[type]?.name ?? type}>
												<option value={type}>{AGENTS[type]?.name ?? type} (default)</option>
												<For each={configs()}>
													{(cfg) => (
														<option value={`${type}:${cfg.name}`}>
															{cfg.name}
															{cfg.is_default ? " (default)" : ""}
														</option>
													)}
												</For>
											</optgroup>
										</Show>
									);
								}}
							</For>
							<option value="api">API (via ego)</option>
						</Select>
					)}
				</SettingRow>
				<Show when={detection.loading()}>
					<p class={s.hint}>Detecting...</p>
				</Show>
			</SectionCard>

			<SectionCard
				title="Prompt library"
				actions={
					<Button
						data-non-setting="Reset, remove or create a stable Smart Prompt through its owner"
						class={s.testBtn}
						onClick={handleNewPrompt}
					>
						+ New Smart Prompt
					</Button>
				}
			>
				<div class={sp.promptList}>
					<For each={orderedCategories()}>
						{(cat) => (
							<CategoryGroup
								category={cat}
								prompts={groups().get(cat)!}
								selected={selected}
								onEdit={(id) => setSelected(selected() === id ? null : id)}
							/>
						)}
					</For>
				</div>
			</SectionCard>
			<Show when={selectedPrompt()}>
				{(prompt) => (
					<>
						<PromptEditor prompt={prompt()} onClose={() => setSelected(null)} headlessAgents={headlessAgents()} />
					</>
				)}
			</Show>
		</div>
	);
};
