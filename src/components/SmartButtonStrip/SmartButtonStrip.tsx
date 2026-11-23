import { type Component, createMemo, createSignal, For, onCleanup, Show } from "solid-js";
import { useSmartPrompts } from "../../hooks/useSmartPrompts";
import { t } from "../../i18n";
import { appLogger } from "../../stores/appLogger";
import { promptLibraryStore, type SavedPrompt, type SmartPlacement } from "../../stores/promptLibrary";
import { cx } from "../../utils";
import { promptError } from "../../utils/promptError";
import { Button } from "../ui/Button";
import { ButtonGroup } from "../ui/ButtonGroup";
import type { JsonValue } from "../WorkflowDesigner/catalogTypes";
import s from "./SmartButtonStrip.module.css";
import { ManageWorkflowsItem, WorkflowMenuGroup } from "./WorkflowMenuGroup";

const SparkleIcon = () => (
	<svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor">
		<path d="M8 0l1.5 4.5L14 6l-4.5 1.5L8 12 6.5 7.5 2 6l4.5-1.5L8 0zm4 9l.75 2.25L15 12l-2.25.75L12 15l-.75-2.25L9 12l2.25-.75L12 9z" />
	</svg>
);

const ChevronIcon = () => (
	<svg width="10" height="10" viewBox="0 0 16 16" fill="currentColor">
		<path d="M4 6l4 4 4-4" stroke="currentColor" stroke-width="2" fill="none" />
	</svg>
);

const CheckIcon = () => (
	<svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor" class={s.checkMark}>
		<path d="M13.78 4.22a.75.75 0 0 1 0 1.06l-7.25 7.25a.75.75 0 0 1-1.06 0L2.22 9.28a.75.75 0 0 1 1.06-1.06L6 10.94l6.72-6.72a.75.75 0 0 1 1.06 0z" />
	</svg>
);

export interface SmartButtonStripProps {
	placement: SmartPlacement;
	repoPath: string;
	/** Default prompt ID to show in the main button (before user picks one) */
	defaultPromptId?: string;
	extraFilter?: (p: SavedPrompt) => boolean;
	/** Called with error message when a prompt fails */
	onError?: (msg: string) => void;
	/** Called when busy state changes (for parent spinner/state) */
	onBusyChange?: (busy: boolean) => void;
	/** Extra context variables to pass as manualVariables to executeSmartPrompt */
	contextVariables?: () => Record<string, string>;
	/** "ghost" (default) for toolbar strips; "primary"/"secondary" draw the filled UI v3 split button. */
	variant?: "ghost" | "primary" | "secondary";
	/** Fixed main label ("Review", "Work on it"); the menu marks what a click runs. */
	label?: string;
	/** Non-prompt actions listed first under "Start" (e.g. Auto-fix in worktree). */
	starts?: SmartStart[];
	/** Lists the project's published workflows under "Workflow", started with this subject. */
	workflowSubject?: () => Record<string, JsonValue>;
}

export interface SmartStart {
	id: string;
	label: string;
	onSelect: () => void;
}

type Choice = { kind: "start" | "prompt"; id: string };
/** Last pick per placement for this session, so a click repeats it (a start is not a prompt,
 *  so the prompt library's recent list cannot remember it). */
const [lastChoice, setLastChoice] = createSignal<Record<string, Choice>>({});

/** Translate internal error codes into user-friendly messages */
function friendlyError(result: { reason?: string; output?: string }, promptName: string): string {
	if (result.reason === "unresolved_variables" && result.output) {
		try {
			const vars = JSON.parse(result.output) as string[];
			if (vars.includes("staged_diff")) return "Stage some files first — no staged changes to analyze";
			return `Missing context: ${vars.join(", ")}`;
		} catch {
			/* fall through */
		}
	}
	return result.reason ?? `"${promptName}" failed`;
}

export const SmartButtonStrip: Component<SmartButtonStripProps> = (props) => {
	const { canExecute, executeSmartPrompt } = useSmartPrompts();
	const [menuOpen, setMenuOpen] = createSignal(false);
	const [busy, setBusy] = createSignal(false);
	let rootRef: HTMLDivElement | undefined;

	// Close dropdown on click outside
	const handleClickOutside = (e: MouseEvent) => {
		if (menuOpen() && rootRef && !rootRef.contains(e.target as Node)) {
			setMenuOpen(false);
		}
	};
	document.addEventListener("mousedown", handleClickOutside);
	onCleanup(() => document.removeEventListener("mousedown", handleClickOutside));

	const prompts = createMemo(() => {
		const all = promptLibraryStore.getSmartByPlacement(props.placement);
		return props.extraFilter ? all.filter(props.extraFilter) : all;
	});

	/** Most recently used prompt for this placement, or the configured default */
	const activePrompt = createMemo((): SavedPrompt | undefined => {
		const all = prompts();
		if (all.length === 0) return undefined;
		const recent = promptLibraryStore.state.recentIds;
		const allIds = new Set(all.map((p) => p.id));
		for (const id of recent) {
			if (allIds.has(id)) return all.find((p) => p.id === id);
		}
		if (props.defaultPromptId) {
			const def = all.find((p) => p.id === props.defaultPromptId);
			if (def) return def;
		}
		return all[0];
	});

	const starts = () => props.starts ?? [];
	/** What a click on the main part runs: the last pick, else the first start, else the active prompt. */
	const chosenStart = (): SmartStart | undefined => {
		const last = lastChoice()[props.placement];
		if (last?.kind === "prompt" && activePrompt()) return undefined;
		return starts().find((start) => start.id === last?.id) ?? (last ? undefined : starts()[0]);
	};
	const remember = (choice: Choice) => setLastChoice((all) => ({ ...all, [props.placement]: choice }));
	const runStart = (start: SmartStart) => {
		setMenuOpen(false);
		remember({ kind: "start", id: start.id });
		start.onSelect();
	};
	const entryCount = () => prompts().length + starts().length + (props.workflowSubject ? 2 : 0);
	const boxed = () => (props.variant ?? "ghost") !== "ghost";
	const mainBlocked = () => {
		if (chosenStart()) return undefined;
		const prompt = activePrompt();
		const check = prompt ? canExecute(prompt) : undefined;
		return check && !check.ok ? check.reason : undefined;
	};

	async function runPrompt(prompt: SavedPrompt) {
		setMenuOpen(false);
		remember({ kind: "prompt", id: prompt.id });
		promptLibraryStore.markAsUsed(prompt.id);
		const check = canExecute(prompt);
		if (!check.ok) {
			props.onError?.(friendlyError(check, prompt.name));
			return;
		}
		props.onError?.(""); // clear previous error
		setBusy(true);
		props.onBusyChange?.(true);
		try {
			const extraVars = props.contextVariables?.();
			const result = await executeSmartPrompt(prompt, extraVars);
			if (!result.ok) props.onError?.(friendlyError(result, prompt.name));
		} catch (err) {
			props.onError?.(promptError(err));
			appLogger.error("prompts", `SmartButtonStrip "${prompt.name}" failed`, err);
		} finally {
			setBusy(false);
			props.onBusyChange?.(false);
		}
	}

	return (
		<Show when={activePrompt() || starts().length > 0}>
			<div class={s.split} ref={rootRef}>
				<ButtonGroup aria-label="Smart prompt actions">
					<Button
						size="compact"
						variant={props.variant ?? "ghost"}
						class={cx(
							boxed() ? s.boxedMain : s.splitMain,
							busy() && s.splitBusy,
							mainBlocked() !== undefined && s.splitDisabled,
						)}
						disabled={busy() || mainBlocked() !== undefined}
						onClick={() => {
							const start = chosenStart();
							if (start) runStart(start);
							else if (activePrompt()) runPrompt(activePrompt()!);
						}}
						title={mainBlocked() ?? chosenStart()?.label ?? activePrompt()?.description}
					>
						<Show when={busy()} fallback={<SparkleIcon />}>
							<span class={s.spinner} />
						</Show>
						<span class={s.promptName}>{props.label ?? chosenStart()?.label ?? activePrompt()?.name}</span>
					</Button>
					<Show when={entryCount() > 1}>
						<Button
							size="compact"
							variant={props.variant ?? "ghost"}
							class={cx(boxed() ? s.boxedArrow : s.splitArrow, menuOpen() && !boxed() && s.splitArrowOpen)}
							onClick={() => setMenuOpen(!menuOpen())}
							title="More agent actions"
							aria-label="More agent actions"
							aria-expanded={menuOpen()}
						>
							<ChevronIcon />
						</Button>
					</Show>
				</ButtonGroup>
				<Show when={entryCount() > 1}>
					<Show when={menuOpen()}>
						<div class={cx(s.menu, boxed() && s.menuStart)}>
							<Show when={starts().length > 0}>
								<div class={s.menuGroup}>{t("smartStrip.start", "Start")}</div>
								<For each={starts()}>
									{(start) => (
										<button
											type="button"
											data-control-layout="row"
											class={cx(s.menuItem, chosenStart()?.id === start.id && s.menuItemActive)}
											onClick={(e) => {
												e.stopPropagation();
												runStart(start);
											}}
										>
											{start.label}
											<Show when={chosenStart()?.id === start.id}>
												<CheckIcon />
											</Show>
										</button>
									)}
								</For>
							</Show>
							<Show when={props.workflowSubject}>
								{(subject) => (
									<WorkflowMenuGroup
										project={props.repoPath}
										subject={subject()}
										onDone={() => setMenuOpen(false)}
										onError={props.onError}
									/>
								)}
							</Show>
							<Show when={(boxed() || starts().length > 0) && prompts().length > 0}>
								<div class={s.menuGroup}>{t("smartStrip.agentPrompt", "Agent prompt")}</div>
							</Show>
							<For each={prompts()}>
								{(prompt) => {
									const check = () => canExecute(prompt);
									const isActive = () => !chosenStart() && prompt.id === activePrompt()?.id;
									// DEFERRED (2026-09-06) — this hint stays a `title=` tooltip, so its
									// "Settings → …" text is not clickable here. Making it clickable
									// needs an onOpenSettings prop threaded through all six mount points
									// (GitHubPanel, PrSection, PrDetailPopover, ChangesTab, BranchesTab, and
									// each of their parents), because `openSettings` is local App.tsx state
									// with no store. The real fix is to promote it to a store; until then the
									// threading costs more than it buys. Clickable version lives in
									// SmartPromptsDropdown (#706-8d98).
									return (
										<button
											type="button"
											data-control-layout="row"
											class={cx(s.menuItem, isActive() && s.menuItemActive, !check().ok && s.menuItemDisabled)}
											disabled={!check().ok}
											title={!check().ok ? check().reason : prompt.description}
											onClick={(e) => {
												e.stopPropagation();
												runPrompt(prompt);
											}}
										>
											{prompt.name}
											<Show when={isActive()}>
												<CheckIcon />
											</Show>
										</button>
									);
								}}
							</For>
							<Show when={props.workflowSubject}>
								<ManageWorkflowsItem project={props.repoPath} onDone={() => setMenuOpen(false)} />
							</Show>
						</div>
					</Show>
				</Show>
			</div>
		</Show>
	);
};
