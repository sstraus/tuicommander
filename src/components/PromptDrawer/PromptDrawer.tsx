import { type Component, createEffect, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { createStore, reconcile } from "solid-js/store";
import { AGENTS } from "../../agents";
import { SMART_PROMPTS_BUILTIN, VARIABLE_DESCRIPTIONS } from "../../data/smartPromptsBuiltIn";
import { useAgentDetection } from "../../hooks/useAgentDetection";
import { useConfirmDialog } from "../../hooks/useConfirmDialog";
import { usePty } from "../../hooks/usePty";
import { shouldSubmitInjectPrompt } from "../../hooks/useSmartPrompts";
import { t } from "../../i18n";
import { appLogger } from "../../stores/appLogger";
import { type PromptCategory, promptLibraryStore, type SavedPrompt } from "../../stores/promptLibrary";
import { terminalsStore } from "../../stores/terminals";
import { cx } from "../../utils";
import { ConfirmDialog } from "../ConfirmDialog";
import { SmartPromptEditor } from "../shared/SmartPromptEditor";
import { Button } from "../ui/Button";
import { Input } from "../ui/Input";
import s from "./PromptDrawer.module.css";

export interface PromptDrawerProps {
	onClose?: () => void;
}

/** Category labels */
const CATEGORY_LABELS: Record<PromptCategory | "all", string> = {
	all: "All",
	custom: "Custom",
	recent: "Recent",
	favorite: "Favorites",
};

/** Built-in defaults indexed by ID for quick lookup */
const BUILTIN_BY_ID = new Map(SMART_PROMPTS_BUILTIN.map((p) => [p.id, p]));

export const PromptDrawer: Component<PromptDrawerProps> = (props) => {
	const [selectedIndex, setSelectedIndex] = createSignal(0);
	const [showEditor, setShowEditor] = createSignal(false);
	const [editingPrompt, setEditingPrompt] = createSignal<SavedPrompt | null>(null);
	const [variableValues, setVariableValues] = createSignal<Record<string, string>>({});
	const [showVariableDialog, setShowVariableDialog] = createSignal(false);
	const [pendingPrompt, setPendingPrompt] = createSignal<SavedPrompt | null>(null);
	const dialogs = useConfirmDialog();
	let pendingPromptClick: ReturnType<typeof setTimeout> | null = null;

	const pty = usePty();

	const filteredPrompts = () => promptLibraryStore.getFilteredPrompts();
	const isOpen = () => promptLibraryStore.state.drawerOpen;
	const clearPendingPromptClick = () => {
		if (pendingPromptClick !== null) {
			clearTimeout(pendingPromptClick);
			pendingPromptClick = null;
		}
	};
	onCleanup(clearPendingPromptClick);

	// Reset selection when prompts change
	createEffect(() => {
		filteredPrompts();
		setSelectedIndex(0);
	});

	createEffect(() => {
		if (!isOpen()) clearPendingPromptClick();
	});

	// Keyboard navigation
	createEffect(() => {
		if (!isOpen()) return;

		const handleKeydown = (e: KeyboardEvent) => {
			if (showVariableDialog()) {
				if (e.key === "Escape") {
					setShowVariableDialog(false);
					setPendingPrompt(null);
				}
				return;
			}

			if (showEditor()) {
				if (e.key === "Escape") {
					setShowEditor(false);
					setEditingPrompt(null);
				}
				return;
			}

			const prompts = filteredPrompts();

			switch (e.key) {
				case "ArrowDown":
					e.preventDefault();
					setSelectedIndex((i) => Math.min(i + 1, prompts.length - 1));
					break;
				case "ArrowUp":
					e.preventDefault();
					setSelectedIndex((i) => Math.max(i - 1, 0));
					break;
				case "Enter":
					e.preventDefault();
					if (prompts[selectedIndex()]) {
						injectPrompt(prompts[selectedIndex()]);
					}
					break;
				case "Escape":
					e.preventDefault();
					promptLibraryStore.closeDrawer();
					props.onClose?.();
					break;
				case "n":
					if (e.ctrlKey || e.metaKey) {
						e.preventDefault();
						createNewPrompt();
					}
					break;
				case "e":
					if (e.ctrlKey || e.metaKey) {
						e.preventDefault();
						if (prompts[selectedIndex()]) {
							editPrompt(prompts[selectedIndex()]);
						}
					}
					break;
				case "f":
					if (e.ctrlKey || e.metaKey) {
						e.preventDefault();
						if (prompts[selectedIndex()]) {
							promptLibraryStore.toggleFavorite(prompts[selectedIndex()].id);
						}
					}
					break;
			}
		};

		document.addEventListener("keydown", handleKeydown);
		onCleanup(() => document.removeEventListener("keydown", handleKeydown));
	});

	/** Inject prompt into active terminal */
	const injectPrompt = async (prompt: SavedPrompt, submitOverride?: boolean) => {
		if (prompt.enabled === false) return;

		const variables = await promptLibraryStore.extractVariables(prompt.content);

		if (variables.length > 0) {
			setPendingPrompt(prompt);
			// Pre-populate with auto-resolved values where possible
			const autoVars: Record<string, string> = {};
			for (const v of variables) {
				const promptVar = prompt.variables?.find((pv) => pv.name === v);
				autoVars[v] = promptVar?.defaultValue || "";
			}
			setVariableValues(autoVars);
			setShowVariableDialog(true);
			return;
		}

		await doInject(prompt, {}, submitOverride);
	};

	const doInject = async (prompt: SavedPrompt, variables: Record<string, string>, submitOverride?: boolean) => {
		const activeTerminal = terminalsStore.getActive();
		if (!activeTerminal?.sessionId) return;

		const content = await promptLibraryStore.processContent(prompt, variables);

		try {
			const target = prompt.injectTarget ?? "compose";
			const submit = shouldSubmitInjectPrompt(prompt, submitOverride);
			if (!submit && target === "compose" && activeTerminal.ref?.openComposeWithText) {
				activeTerminal.ref.openComposeWithText(content);
			} else {
				await pty.sendCommand(activeTerminal.sessionId, content, activeTerminal.agentType, submit);
			}
			promptLibraryStore.markAsUsed(prompt.id);
			promptLibraryStore.closeDrawer();
			props.onClose?.();
		} catch (err) {
			appLogger.error("app", "Failed to inject prompt", err);
		}
	};

	const handlePromptClick = (event: MouseEvent, prompt: SavedPrompt) => {
		// A browser emits click, click, then dblclick. Delay the first click so a
		// double-click can replace it instead of inserting once and submitting again.
		if (event.detail > 1) return;
		clearPendingPromptClick();
		pendingPromptClick = setTimeout(() => {
			pendingPromptClick = null;
			void injectPrompt(prompt);
		}, 200);
	};

	const handlePromptDoubleClick = (event: MouseEvent, prompt: SavedPrompt) => {
		event.preventDefault();
		clearPendingPromptClick();
		void injectPrompt(prompt, true);
	};

	const handleVariableSubmit = (executeImmediately: boolean) => {
		const prompt = pendingPrompt();
		if (!prompt) return;

		doInject(prompt, variableValues(), executeImmediately);
		setShowVariableDialog(false);
		setPendingPrompt(null);
	};

	const createNewPrompt = () => {
		setEditingPrompt(null);
		setShowEditor(true);
	};

	const editPrompt = (prompt: SavedPrompt) => {
		setEditingPrompt(prompt);
		setShowEditor(true);
	};

	const deletePrompt = async (prompt: SavedPrompt) => {
		if (prompt.builtIn) return;
		const confirmed = await dialogs.confirm({
			title: "Delete prompt?",
			message: `Delete "${prompt.name}"?`,
			okLabel: "Delete",
			kind: "warning",
		});
		if (confirmed) {
			promptLibraryStore.deletePrompt(prompt.id);
		}
	};

	const toggleEnabled = (e: MouseEvent, prompt: SavedPrompt) => {
		e.stopPropagation();
		promptLibraryStore.updatePrompt(prompt.id, { enabled: prompt.enabled === false });
	};

	return (
		<>
			<Show when={isOpen()}>
				<div class={s.overlay} onClick={() => promptLibraryStore.closeDrawer()}>
					<div class={s.drawer} onClick={(e) => e.stopPropagation()}>
						{/* Header */}
						<div class={s.header}>
							<h3>{t("promptDrawer.title", "Smart Prompts Library")}</h3>
							<Button variant="ghost" aria-label="Close" onClick={() => promptLibraryStore.closeDrawer()}>
								&times;
							</Button>
						</div>

						{/* Search */}
						<div class={s.search}>
							<Input
								type="text"
								placeholder={t("promptDrawer.searchPlaceholder", "Search prompts... (type to filter)")}
								value={promptLibraryStore.state.searchQuery}
								onInput={(e) => promptLibraryStore.setSearchQuery(e.currentTarget.value)}
								autocomplete="off"
								autocorrect="off"
								spellcheck={false}
								autofocus
							/>
						</div>

						{/* Categories */}
						<div class={s.categories}>
							<For each={Object.entries(CATEGORY_LABELS)}>
								{([category, label]) => (
									<Button
										class={cx(promptLibraryStore.state.selectedCategory === category && s.categoryBtnActive)}
										aria-pressed={promptLibraryStore.state.selectedCategory === category}
										onClick={() => promptLibraryStore.setSelectedCategory(category as PromptCategory | "all")}
									>
										{label}
									</Button>
								)}
							</For>
						</div>

						{/* Prompt list */}
						<div class={s.list}>
							<Show
								when={filteredPrompts().length > 0}
								fallback={
									<div class={s.empty}>
										<p>{t("promptDrawer.noPrompts", "No prompts found")}</p>
										<Button variant="primary" onClick={createNewPrompt}>
											{t("promptDrawer.createFirst", "Create your first prompt")}
										</Button>
									</div>
								}
							>
								<For each={filteredPrompts()}>
									{(prompt, index) => {
										const isDisabled = () => prompt.enabled === false;
										return (
											<div
												class={cx(
													s.promptItem,
													index() === selectedIndex() && s.selected,
													isDisabled() && s.itemDisabled,
												)}
												onClick={(event) => handlePromptClick(event, prompt)}
												onDblClick={(event) => handlePromptDoubleClick(event, prompt)}
											>
												<div class={s.itemContent}>
													<div class={s.itemName}>
														{prompt.isFavorite && (
															<span class={s.favoriteStar}>
																<svg viewBox="0 0 16 16" width="10" height="10" fill="currentColor">
																	<path d="M8 .25a.75.75 0 0 1 .673.418l1.882 3.815 4.21.612a.75.75 0 0 1 .416 1.279l-3.046 2.97.719 4.192a.75.75 0 0 1-1.088.791L8 12.347l-3.766 1.98a.75.75 0 0 1-1.088-.79l.72-4.194L.818 6.374a.75.75 0 0 1 .416-1.28l4.21-.611L7.327.668A.75.75 0 0 1 8 .25z" />
																</svg>
															</span>
														)}
														{prompt.name}
														<span class={s.itemBadges}>
															<span class={s.tagBadge} data-type={prompt.executionMode ?? "inject"}>
																{prompt.executionMode ?? "inject"}
															</span>
															<Show when={prompt.builtIn}>
																<span class={s.tagBadge} data-type="builtin">
																	built-in
																</span>
															</Show>
															<For each={prompt.placement ?? []}>
																{(p) => (
																	<span class={s.tagBadge} data-type="placement">
																		{p}
																	</span>
																)}
															</For>
														</span>
														{prompt.shortcut && <span class={s.shortcut}>{prompt.shortcut}</span>}
													</div>
													<Show when={prompt.description}>
														<div class={s.itemDescription}>{prompt.description}</div>
													</Show>
												</div>
												<div class={s.itemActions} onDblClick={(event) => event.stopPropagation()}>
													<Button
														variant="ghost"
														title={isDisabled() ? "Enable" : "Disable"}
														onClick={(e) => toggleEnabled(e, prompt)}
														class={cx(isDisabled() && s.toggleOff)}
													>
														<svg viewBox="0 0 16 16" width="12" height="12" fill="currentColor">
															{isDisabled() ? (
																<circle cx="8" cy="8" r="6" fill="none" stroke="currentColor" stroke-width="1.5" />
															) : (
																<circle cx="8" cy="8" r="6" />
															)}
														</svg>
													</Button>
													<Button
														variant="ghost"
														title="Edit"
														onClick={(e) => {
															e.stopPropagation();
															editPrompt(prompt);
														}}
													>
														<svg viewBox="0 0 16 16" width="12" height="12" fill="currentColor">
															<path d="M11.013 1.427a1.75 1.75 0 0 1 2.474 0l1.086 1.086a1.75 1.75 0 0 1 0 2.474l-8.61 8.61c-.21.21-.47.364-.756.445l-3.251.93a.75.75 0 0 1-.927-.928l.929-3.25c.081-.286.235-.547.445-.758l8.61-8.61z" />
														</svg>
													</Button>
													<Button
														variant="ghost"
														title={prompt.isFavorite ? "Unfavorite" : "Favorite"}
														onClick={(e) => {
															e.stopPropagation();
															promptLibraryStore.toggleFavorite(prompt.id);
														}}
													>
														<svg viewBox="0 0 16 16" width="12" height="12" fill="currentColor">
															{prompt.isFavorite ? (
																<path d="M8 .25a.75.75 0 0 1 .673.418l1.882 3.815 4.21.612a.75.75 0 0 1 .416 1.279l-3.046 2.97.719 4.192a.75.75 0 0 1-1.088.791L8 12.347l-3.766 1.98a.75.75 0 0 1-1.088-.79l.72-4.194L.818 6.374a.75.75 0 0 1 .416-1.28l4.21-.611L7.327.668A.75.75 0 0 1 8 .25z" />
															) : (
																<path d="M8 .25a.75.75 0 0 1 .673.418l1.882 3.815 4.21.612a.75.75 0 0 1 .416 1.279l-3.046 2.97.719 4.192a.75.75 0 0 1-1.088.791L8 12.347l-3.766 1.98a.75.75 0 0 1-1.088-.79l.72-4.194L.818 6.374a.75.75 0 0 1 .416-1.28l4.21-.611L7.327.668A.75.75 0 0 1 8 .25zm0 2.445L6.615 5.5a.75.75 0 0 1-.564.41l-3.097.45 2.24 2.184a.75.75 0 0 1 .216.664l-.528 3.084 2.769-1.456a.75.75 0 0 1 .698 0l2.77 1.456-.53-3.084a.75.75 0 0 1 .216-.664l2.24-2.183-3.096-.45a.75.75 0 0 1-.564-.41L8 2.694z" />
															)}
														</svg>
													</Button>
													<Show when={!prompt.builtIn}>
														<Button
															variant="ghost"
															title="Delete"
															onClick={(e) => {
																e.stopPropagation();
																deletePrompt(prompt);
															}}
														>
															<svg viewBox="0 0 16 16" width="12" height="12" fill="currentColor">
																<path d="M11 1.75V3h2.25a.75.75 0 0 1 0 1.5H2.75a.75.75 0 0 1 0-1.5H5V1.75C5 .784 5.784 0 6.75 0h2.5C10.216 0 11 .784 11 1.75zM6.5 1.75V3h3V1.75a.25.25 0 0 0-.25-.25h-2.5a.25.25 0 0 0-.25.25zM3.613 5.5l.806 8.87A1.75 1.75 0 0 0 6.163 16h3.674a1.75 1.75 0 0 0 1.744-1.63l.806-8.87H3.613z" />
															</svg>
														</Button>
													</Show>
												</div>
											</div>
										);
									}}
								</For>
							</Show>
						</div>

						{/* Footer */}
						<div class={s.footer}>
							<Button variant="primary" onClick={createNewPrompt}>
								{t("promptDrawer.newPrompt", "+ New Prompt")}
							</Button>
							<span class={s.hint}>
								{t("promptDrawer.hint", "↑↓ Navigate • Enter Insert • Ctrl+N New • Esc Close")}
							</span>
						</div>
					</div>
				</div>

				{/* Variable Dialog */}
				<Show when={showVariableDialog() && pendingPrompt()}>
					<div class={s.variableOverlay} onClick={() => setShowVariableDialog(false)}>
						<div class={s.variableDialog} onClick={(e) => e.stopPropagation()}>
							<h4>{pendingPrompt()?.name ?? "Fill in variables"}</h4>
							<For each={Object.keys(variableValues())}>
								{(varName) => (
									<div class={s.variableInput}>
										<label class={s.varLabel}>{`{${varName}}`}</label>
										<Show when={VARIABLE_DESCRIPTIONS[varName]}>
											<span class={s.varDesc}>{VARIABLE_DESCRIPTIONS[varName]}</span>
										</Show>
										<Input
											type="text"
											value={variableValues()[varName] || ""}
											onInput={(e) => setVariableValues((v) => ({ ...v, [varName]: e.currentTarget.value }))}
											placeholder={VARIABLE_DESCRIPTIONS[varName] ?? varName}
										/>
									</div>
								)}
							</For>
							<div class={s.variableActions}>
								<Button variant="primary" onClick={() => handleVariableSubmit(false)}>
									{t("promptDrawer.insert", "Insert")}
								</Button>
								<Button variant="primary" onClick={() => handleVariableSubmit(true)}>
									{t("promptDrawer.insertAndRun", "Insert & Run")}
								</Button>
								<Button onClick={() => setShowVariableDialog(false)}>{t("promptDrawer.cancel", "Cancel")}</Button>
							</div>
						</div>
					</div>
				</Show>

				{/* Editor Dialog */}
				<Show when={showEditor()}>
					<PromptEditor
						prompt={editingPrompt()}
						onSave={(promptData) => {
							if (editingPrompt()) {
								promptLibraryStore.updatePrompt(editingPrompt()!.id, promptData);
							} else {
								if (promptData.name && promptData.content) {
									promptLibraryStore.createPrompt({
										name: promptData.name,
										content: promptData.content,
										description: promptData.description,
										shortcut: promptData.shortcut,
										category: "custom",
										isFavorite: false,
										tags: promptData.tags,
										placement: promptData.placement,
										autoExecute: promptData.autoExecute,
										executionMode: promptData.executionMode,
										outputTarget: promptData.outputTarget,
										injectTarget: promptData.injectTarget,
										preferredAgent: promptData.preferredAgent,
										systemPrompt: promptData.systemPrompt,
										enabled: true,
									});
								}
							}
							setShowEditor(false);
							setEditingPrompt(null);
						}}
						onCancel={() => {
							setShowEditor(false);
							setEditingPrompt(null);
						}}
					/>
				</Show>
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

/** Explicit-save drawer adapter. The shared fields never write the store directly. */
interface PromptEditorProps {
	prompt: SavedPrompt | null;
	onSave: (data: Partial<SavedPrompt>) => void;
	onCancel: () => void;
}

const PromptEditor: Component<PromptEditorProps> = (props) => {
	const [draft, setDraft] = createStore<SavedPrompt>(
		props.prompt
			? JSON.parse(JSON.stringify(props.prompt))
			: {
					id: "",
					name: "",
					content: "",
					category: "custom",
					isFavorite: false,
					createdAt: 0,
					updatedAt: 0,
					executionMode: "inject",
					injectTarget: "compose",
					autoExecute: false,
				},
	);
	const initialSnapshot = JSON.stringify(draft);
	const formDirty = () => JSON.stringify(draft) !== initialSnapshot;
	const [validationError, setValidationError] = createSignal<string | null>(null);
	const detection = useAgentDetection();
	onMount(() => {
		void detection.detectAll();
	});
	const headlessAgents = () =>
		detection
			.getAvailable()
			.filter((agent) => agent.type !== "git" && AGENTS[agent.type]?.defaultHeadlessTemplate)
			.map((agent) => agent.type);
	const handleSave = () => {
		if (!draft.name.trim() || !draft.content.trim()) {
			setValidationError("Name and content are required");
			return;
		}
		props.onSave({
			name: draft.name.trim(),
			description: draft.description?.trim() || undefined,
			content: draft.content,
			shortcut: draft.shortcut?.trim() || undefined,
			placement: draft.placement,
			autoExecute: draft.autoExecute,
			executionMode: draft.executionMode,
			injectTarget: draft.injectTarget,
			preferredAgent: draft.preferredAgent,
			outputTarget: draft.outputTarget,
			systemPrompt: draft.systemPrompt?.trim() || undefined,
		});
	};
	const handleReset = () => {
		const def = props.prompt && BUILTIN_BY_ID.get(props.prompt.id);
		if (def)
			setDraft(
				reconcile({
					...draft,
					name: def.name,
					description: def.description,
					content: def.content,
					placement: def.placement,
					autoExecute: def.autoExecute,
					executionMode: def.executionMode ?? "inject",
					outputTarget: def.outputTarget,
					injectTarget: def.injectTarget,
					preferredAgent: def.preferredAgent,
					systemPrompt: def.systemPrompt,
				}),
			);
	};
	return (
		<div class={s.editorOverlay} onClick={props.onCancel}>
			<div
				class={s.editor}
				role="dialog"
				aria-label={props.prompt ? "Edit Prompt" : "New Prompt"}
				aria-modal="true"
				onClick={(event) => event.stopPropagation()}
			>
				<Show when={validationError()}>
					<p class={s.validationError} role="alert">
						{validationError()}
					</p>
				</Show>
				<SmartPromptEditor
					prompt={draft}
					title={props.prompt ? "Edit Prompt" : "New Prompt"}
					headlessAgents={headlessAgents()}
					onChange={(update) => {
						setDraft(update);
						setValidationError(null);
					}}
					onReset={handleReset}
				/>
				<div class={s.editorActions}>
					<Button variant="primary" data-ready={formDirty() ? "true" : undefined} onClick={handleSave}>
						Save
					</Button>
					<Button onClick={props.onCancel}>Cancel</Button>
				</div>
			</div>
		</div>
	);
};

export default PromptDrawer;
