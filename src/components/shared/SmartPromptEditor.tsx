import { type Component, createMemo, createSignal, For, Show } from "solid-js";
import { AGENT_TYPES, AGENTS, type AgentType } from "../../agents";
import { SMART_PROMPTS_BUILTIN } from "../../data/smartPromptsBuiltIn";
import type { SavedPrompt, SmartPlacement } from "../../stores/promptLibrary";
import { SectionCard, SettingRow } from "../SettingsPanel/SettingsPrimitives";
import { Button } from "../ui/Button";
import { Input } from "../ui/Input";
import { Select } from "../ui/Select";
import { Switch } from "../ui/Switch";
import { Textarea } from "../ui/Textarea";
import { KeyComboCapture } from "./KeyComboCapture";
import sp from "./SmartPromptEditor.module.css";

const BUILTIN_BY_ID = new Map(SMART_PROMPTS_BUILTIN.map((prompt) => [prompt.id, prompt]));
// All placements for the checkbox grid
const ALL_PLACEMENTS: { value: SmartPlacement; label: string }[] = [
	{ value: "toolbar", label: "Toolbar" },
	{ value: "command-palette", label: "Command palette" },
	{ value: "git-changes", label: "Git · Changes" },
	{ value: "git-branches", label: "Git · Branches" },
	{ value: "pr-popover", label: "PR popover" },
	{ value: "issue-popover", label: "Issue popover" },
	{ value: "terminal-context", label: "Terminal menu" },
	{ value: "file-context", label: "File menu" },
];

/** Context variables available for smart prompt templates */
interface VarDef {
	name: string;
	description: string;
	group: string;
}

const CONTEXT_VARIABLES: VarDef[] = [
	// Git
	{ name: "branch", description: "Current branch name", group: "Git" },
	{ name: "base_branch", description: "Base branch (main/master/develop)", group: "Git" },
	{ name: "diff", description: "Full working tree diff", group: "Git" },
	{ name: "staged_diff", description: "Staged changes diff", group: "Git" },
	{ name: "changed_files", description: "git status --short", group: "Git" },
	{ name: "dirty_files_count", description: "Number of modified files", group: "Git" },
	{ name: "commit_log", description: "Last 20 commits (oneline)", group: "Git" },
	{ name: "last_commit", description: "Last commit hash + subject", group: "Git" },
	{ name: "conflict_files", description: "Files with merge conflicts", group: "Git" },
	{ name: "stash_list", description: "Stash entries", group: "Git" },
	{ name: "branch_status", description: "Ahead/behind remote tracking", group: "Git" },
	{ name: "remote_url", description: "Remote origin URL", group: "Git" },
	{ name: "current_user", description: "Git user.name", group: "Git" },
	{ name: "repo_name", description: "Repository directory name", group: "Git" },
	{ name: "repo_path", description: "Full repository path", group: "Git" },
	{ name: "repo_owner", description: "GitHub owner from remote URL", group: "Git" },
	{ name: "repo_slug", description: "Repository name from remote URL", group: "Git" },
	// GitHub
	{ name: "pr_number", description: "PR number for current branch", group: "GitHub" },
	{ name: "pr_title", description: "PR title", group: "GitHub" },
	{ name: "pr_url", description: "PR URL", group: "GitHub" },
	{ name: "pr_state", description: "open / closed / merged", group: "GitHub" },
	{ name: "pr_author", description: "PR author username", group: "GitHub" },
	{ name: "pr_labels", description: "PR labels (comma-separated)", group: "GitHub" },
	{ name: "pr_additions", description: "Lines added in PR", group: "GitHub" },
	{ name: "pr_deletions", description: "Lines deleted in PR", group: "GitHub" },
	{ name: "merge_status", description: "Mergeable status", group: "GitHub" },
	{ name: "review_decision", description: "Review decision", group: "GitHub" },
	{ name: "pr_checks", description: "CI check summary", group: "GitHub" },
	{ name: "pr_open_threads", description: "Open review threads (untrusted list)", group: "GitHub" },
	// Terminal
	{ name: "agent_type", description: "Detected agent (claude, codex...)", group: "Terminal" },
	{ name: "cwd", description: "Terminal working directory", group: "Terminal" },
	// File (populated for placement="file-context" hosts)
	{ name: "file_path", description: "Absolute path of selected file/folder", group: "File" },
	{ name: "file_rel_path", description: "Path relative to repo root", group: "File" },
	{ name: "file_name", description: "Basename (foo.ts)", group: "File" },
	{ name: "file_ext", description: "Extension including dot (.ts)", group: "File" },
	{ name: "file_dir", description: "Parent directory absolute path", group: "File" },
	{ name: "file_is_dir", description: "'true' if a folder, else 'false'", group: "File" },
];

/** Variables whose runtime value is controlled by repository contents
 * (branch names, commit messages, PR titles, remote URLs). When they are
 * substituted into a shell-execution template, quoting is mandatory —
 * otherwise a crafted branch like `main'; rm -rf ~ ;#` escapes `sh -c`.
 * The Rust backend quotes these for us via `process_prompt_content_shell_safe`;
 * this list drives a UI warning so the author knows the risk surface. */
const REPO_CONTROLLED_VARIABLES: ReadonlySet<string> = new Set([
	"branch",
	"base_branch",
	"diff",
	"staged_diff",
	"changed_files",
	"commit_log",
	"last_commit",
	"conflict_files",
	"stash_list",
	"remote_url",
	"current_user",
	"repo_name",
	"repo_owner",
	"repo_slug",
	"pr_title",
	"pr_author",
	"pr_labels",
	"pr_url",
	"pr_state",
	"pr_checks",
	"pr_open_threads",
	"merge_status",
	"review_decision",
	"agent_type",
	"cwd",
]);

/** Return the subset of repo-controlled variables referenced in content. */
export function repoControlledVarsInContent(content: string): string[] {
	const found = new Set<string>();
	const re = /\{([^{}]+)\}/g;
	let match: RegExpExecArray | null;
	while ((match = re.exec(content)) !== null) {
		if (REPO_CONTROLLED_VARIABLES.has(match[1])) {
			found.add(match[1]);
		}
	}
	return [...found].sort();
}

/** Variable insertion dropdown */
const VariableDropdown: Component<{
	onInsert: (varName: string) => void;
}> = (props) => {
	const [open, setOpen] = createSignal(false);

	// Group variables
	const groups = createMemo(() => Map.groupBy(CONTEXT_VARIABLES, (v) => v.group));

	return (
		<div class={sp.varDropdownWrap}>
			<Button
				data-non-setting="Open or close a structured Smart Prompt editor"
				onClick={() => setOpen(!open())}
				type="button"
			>
				Insert variable...
				<span class={sp.varChevron} classList={{ [sp.varChevronOpen]: open() }}>
					&#9660;
				</span>
			</Button>
			<Show when={open()}>
				<div class={sp.varDropdownList}>
					<For each={Array.from(groups().entries())}>
						{([group, vars]) => (
							<>
								<div class={sp.varGroupLabel}>{group}</div>
								<For each={vars}>
									{(v) => (
										<button
											data-setting-id="prompts.prompts.{id}.text"
											class={sp.varItem}
											data-control-layout="row"
											type="button"
											onClick={() => {
												props.onInsert(v.name);
												setOpen(false);
											}}
										>
											<span class={sp.varName}>{`{${v.name}}`}</span>
											<span class={sp.varDesc}>{v.description}</span>
										</button>
									)}
								</For>
							</>
						)}
					</For>
				</div>
			</Show>
		</div>
	);
};

/** Shared fields. The caller owns the draft or immediate persistence boundary. */
export const SmartPromptEditor: Component<{
	prompt: SavedPrompt;
	onChange: (update: Partial<SavedPrompt>) => void;
	headlessAgents: AgentType[];
	title?: string;
	onReset?: () => void;
}> = (props) => {
	const isBuiltIn = () => !!props.prompt.builtIn;
	const builtInDefault = () => BUILTIN_BY_ID.get(props.prompt.id);
	let textareaRef: HTMLTextAreaElement | undefined;

	const handleContentChange = (content: string) => {
		props.onChange({ content });
	};

	const handleNameChange = (name: string) => {
		if (!isBuiltIn()) {
			props.onChange({ name });
		}
	};

	const handleDescriptionChange = (description: string) => {
		props.onChange({ description });
	};

	const handlePlacementToggle = (placement: SmartPlacement) => {
		const current = props.prompt.placement ?? [];
		const next = current.includes(placement) ? current.filter((p) => p !== placement) : [...current, placement];
		props.onChange({ placement: next });
	};

	const handleAutoExecuteToggle = () => {
		props.onChange({ autoExecute: !props.prompt.autoExecute });
	};

	const handleShortcutChange = (combo: string) => {
		props.onChange({ shortcut: combo });
	};

	const handleInsertVariable = (varName: string) => {
		const el = textareaRef;
		if (!el) return;
		const insertion = `{${varName}}`;
		const start = el.selectionStart;
		const end = el.selectionEnd;
		const before = el.value.slice(0, start);
		const after = el.value.slice(end);
		const newContent = before + insertion + after;
		handleContentChange(newContent);
		// Restore cursor position after insertion
		requestAnimationFrame(() => {
			el.focus();
			const pos = start + insertion.length;
			el.setSelectionRange(pos, pos);
		});
	};

	const isOverridden = () => {
		const def = builtInDefault();
		return def ? props.prompt.content !== def.content : false;
	};

	return (
		<SectionCard title={props.title ?? `Edit ${props.prompt.name}`} layout="aligned">
			{/* Name */}
			<SettingRow label="Name *">
				{(id, descriptionId) => (
					<div>
						<Input
							data-setting-id="prompts.prompts.{id}.label"
							id={id}
							aria-describedby={descriptionId()}
							type="text"
							placeholder="My Prompt"
							value={props.prompt.name}
							disabled={isBuiltIn()}
							onInput={(e) => handleNameChange(e.currentTarget.value)}
						/>
					</div>
				)}
			</SettingRow>

			{/* Description */}
			<SettingRow label="Description">
				{(id, descriptionId) => (
					<div>
						<Input
							data-setting-id="prompts.prompts.{id}.text"
							id={id}
							aria-describedby={descriptionId()}
							type="text"
							value={props.prompt.description ?? ""}
							onInput={(e) => handleDescriptionChange(e.currentTarget.value)}
							placeholder="Short description of what this prompt does"
						/>
					</div>
				)}
			</SettingRow>

			{/* Content + Variable dropdown */}
			<SettingRow label="Content *">
				{(id, descriptionId) => (
					<div>
						<Textarea
							data-setting-id="prompts.prompts.{id}.text"
							id={id}
							aria-describedby={descriptionId()}
							value={props.prompt.content}
							onInput={(e) => handleContentChange(e.currentTarget.value)}
							ref={textareaRef}
							rows={6}
							placeholder="Enter your prompt text here..."
						/>
						<VariableDropdown onInsert={handleInsertVariable} />
					</div>
				)}
			</SettingRow>

			{/* Placement */}
			<SettingRow label="Placement">
				{() => (
					<>
						<div class={sp.placementGrid}>
							<For each={ALL_PLACEMENTS}>
								{(placement) => (
									<label class={sp.placementCheck}>
										<input
											data-setting-id="prompts.prompts.{id}.text"
											type="checkbox"
											checked={props.prompt.placement?.includes(placement.value) ?? false}
											onChange={() => handlePlacementToggle(placement.value)}
										/>
										<span>{placement.label}</span>
									</label>
								)}
							</For>
						</div>
					</>
				)}
			</SettingRow>

			{/* Execution and its conditional target each keep one shared row. */}
			<SettingRow label="Execution">
				{(id, descriptionId) => (
					<div>
						<Select
							data-setting-id="prompts.prompts.{id}.text"
							id={id}
							aria-describedby={descriptionId()}
							value={props.prompt.executionMode ?? "inject"}
							onChange={(e) => {
								const mode = e.currentTarget.value as "inject" | "headless" | "api" | "shell";
								const update: Partial<SavedPrompt> = { executionMode: mode };
								if (mode === "inject") {
									update.outputTarget = undefined;
									update.systemPrompt = undefined;
								}
								if (mode === "shell") {
									update.systemPrompt = undefined;
								}
								props.onChange(update);
							}}
						>
							<option value="inject">Inject into terminal</option>
							<option value="shell">Shell script (direct run)</option>
							<option value="headless">Headless (one-shot CLI)</option>
							<option value="api">API (via ego)</option>
						</Select>
					</div>
				)}
			</SettingRow>

			<Show when={(props.prompt.executionMode ?? "inject") === "inject"}>
				<SettingRow label="Target">
					{(id, descriptionId) => (
						<div>
							<Select
								data-setting-id="prompts.prompts.{id}.text"
								id={id}
								aria-describedby={descriptionId()}
								value={props.prompt.injectTarget ?? "compose"}
								onChange={(e) => {
									const val = e.currentTarget.value;
									if (val === "terminal" || val === "compose") {
										props.onChange({ injectTarget: val });
									}
								}}
							>
								<option value="compose">Compose box (review)</option>
								<option value="terminal">Terminal (send to agent)</option>
							</Select>
						</div>
					)}
				</SettingRow>
			</Show>

			<Show
				when={
					(props.prompt.executionMode ?? "inject") === "inject" &&
					(props.prompt.injectTarget ?? "compose") === "terminal"
				}
			>
				<SettingRow label="Send immediately">
					{(id, descriptionId) => (
						<Switch
							data-setting-id="prompts.prompts.{id}.text"
							id={id}
							aria-describedby={descriptionId()}
							role="switch"
							checked={props.prompt.autoExecute ?? false}
							onChange={handleAutoExecuteToggle}
						/>
					)}
				</SettingRow>
			</Show>

			{/* Preferred Agent (headless mode only) */}
			<Show when={props.prompt.executionMode === "headless"}>
				<SettingRow label="Preferred Agent">
					{(id, descriptionId) => (
						<div>
							<Select
								data-setting-id="prompts.prompts.{id}.text"
								id={id}
								aria-describedby={descriptionId()}
								value={props.prompt.preferredAgent ?? ""}
								onChange={(e) => {
									const val = e.currentTarget.value || undefined;
									props.onChange({
										preferredAgent: val && AGENT_TYPES.includes(val as AgentType) ? (val as AgentType) : undefined,
									});
								}}
							>
								<option value="">Use global default</option>
								<For each={props.headlessAgents}>
									{(type) => <option value={type}>{AGENTS[type]?.name ?? type}</option>}
								</For>
								<option value="api">API (via ego)</option>
							</Select>
						</div>
					)}
				</SettingRow>
			</Show>

			{/* Shell-mode warning for repo-controlled variable substitution.
          Even though values are shell-quoted in the backend, authors should
          know which values come from the repo so they can design the
          template accordingly. */}
			<Show
				when={props.prompt.executionMode === "shell" && repoControlledVarsInContent(props.prompt.content).length > 0}
			>
				<p class={sp.fieldHint} style={{ color: "var(--color-warning, #c07a00)" }}>
					⚠ Shell mode substitutes repo-controlled variables (
					{repoControlledVarsInContent(props.prompt.content).join(", ")}). Values are shell-quoted automatically, but a
					template that uses them outside argument position (e.g. `eval {"{branch}"}`) can still be abused by crafted
					branch names / PR titles.
				</p>
			</Show>

			{/* Output Target (headless and api modes) */}
			<Show when={(props.prompt.executionMode ?? "inject") !== "inject"}>
				<SettingRow label="Output Target">
					{(id, descriptionId) => (
						<div>
							<Select
								data-setting-id="prompts.prompts.{id}.text"
								id={id}
								aria-describedby={descriptionId()}
								value={props.prompt.outputTarget ?? ""}
								onChange={(e) => {
									const val = e.currentTarget.value || undefined;
									props.onChange({
										outputTarget: val as SavedPrompt["outputTarget"],
									});
								}}
							>
								<option value="">None (return in result)</option>
								<option value="commit-message">Commit message</option>
								<option value="clipboard">Clipboard</option>
								<option value="toast">Notification</option>
								<option value="panel">Panel</option>
							</Select>
						</div>
					)}
				</SettingRow>
			</Show>

			{/* System Prompt (api mode only) */}
			<Show when={props.prompt.executionMode === "api"}>
				<SettingRow label="System Prompt">
					{(id, descriptionId) => (
						<div>
							<Textarea
								data-setting-id="prompts.prompts.{id}.text"
								id={id}
								aria-describedby={descriptionId()}
								rows={3}
								value={props.prompt.systemPrompt ?? ""}
								placeholder="Instructions for the LLM (e.g. 'You are a Git expert. Output only the requested content.')"
								onInput={(e) => {
									props.onChange({
										systemPrompt: e.currentTarget.value || undefined,
									});
								}}
							/>
						</div>
					)}
				</SettingRow>
			</Show>

			{/* Shortcut */}
			<SettingRow
				label="Shortcut"
				actions={
					props.prompt.shortcut ? (
						<Button
							shape="icon"
							aria-label="Clear shortcut"
							data-setting-id="prompts.prompts.{id}.text"
							onClick={() => handleShortcutChange("")}
						>
							<svg viewBox="0 0 16 16" width="12" height="12" fill="currentColor" aria-hidden="true">
								<path d="m4.2 3.2 3.8 3.8 3.8-3.8 1 1L9 8l3.8 3.8-1 1L8 9l-3.8 3.8-1-1L7 8 3.2 4.2z" />
							</svg>
						</Button>
					) : undefined
				}
			>
				{(id) => (
					<div>
						<KeyComboCapture
							id={id}
							settingId="prompts.prompts.{id}.text"
							value={props.prompt.shortcut ?? ""}
							onChange={handleShortcutChange}
							placeholder="Click to set shortcut"
						/>
					</div>
				)}
			</SettingRow>

			{/* Actions */}
			<div class={sp.editorActions}>
				<Show when={props.onReset && isBuiltIn() && isOverridden()}>
					<Button
						data-non-setting="Reset, remove or create a stable Smart Prompt through its owner"
						onClick={props.onReset}
					>
						Reset to Default
					</Button>
				</Show>
			</div>
		</SectionCard>
	);
};
