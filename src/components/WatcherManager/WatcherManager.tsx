import { type Component, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { invoke, listen } from "../../invoke";
import { appLogger } from "../../stores/appLogger";
import { promptLibraryStore } from "../../stores/promptLibrary";
import { repositoriesStore } from "../../stores/repositories";
import { terminalsStore } from "../../stores/terminals";
import s from "./WatcherManager.module.css";

type WatcherTrigger =
	| { type: "idle" }
	| { type: "busy" }
	| { type: "command_done"; on_failure_only: boolean }
	| { type: "question"; confident_only: boolean }
	| { type: "error" }
	| { type: "unseen" }
	| { type: "pattern"; regex: string }
	| { type: "pr_pushed"; authored_by_others: boolean }
	| { type: "pr_opened"; authored_by_others: boolean };

type WatcherTriggerKey =
	| "idle"
	| "busy"
	| "command_done"
	| "command_done_fail"
	| "question"
	| "error"
	| "unseen"
	| "pr_pushed"
	| "pr_opened";

/** Static triggers (the dynamic PR-trigger authored_by_others is set at submit). */
const TRIGGER_MAP: Record<Exclude<WatcherTriggerKey, "pr_pushed" | "pr_opened">, WatcherTrigger> = {
	idle: { type: "idle" },
	busy: { type: "busy" },
	command_done: { type: "command_done", on_failure_only: false },
	command_done_fail: { type: "command_done", on_failure_only: true },
	question: { type: "question", confident_only: true },
	error: { type: "error" },
	unseen: { type: "unseen" },
};

/** PR triggers are git-scoped, not terminal triggers. Drive the repo/author UI. */
export const isGitTrigger = (key: WatcherTriggerKey): boolean => key === "pr_pushed" || key === "pr_opened";

export function buildTrigger(key: WatcherTriggerKey, authoredByOthers: boolean): WatcherTrigger {
	if (key === "pr_pushed" || key === "pr_opened") return { type: key, authored_by_others: authoredByOthers };
	return TRIGGER_MAP[key];
}

/** A watcher form is submittable when it has an action (prompt or instructions)
 * and — for the git-scoped PR trigger — a selected repo. */
export function watcherFormReady(opts: {
	promptId: string;
	instructions: string;
	triggerKey: WatcherTriggerKey;
	repoPath: string;
}): boolean {
	const hasAction = !!opts.promptId || !!opts.instructions.trim();
	const repoOk = !isGitTrigger(opts.triggerKey) || !!opts.repoPath;
	return hasAction && repoOk;
}

function triggerToKey(trigger: WatcherTrigger): WatcherTriggerKey {
	if (trigger.type === "command_done") return trigger.on_failure_only ? "command_done_fail" : "command_done";
	return trigger.type as WatcherTriggerKey;
}

const TRIGGER_LABELS: Record<string, string> = {
	idle: "Idle",
	busy: "Busy",
	command_done: "Done",
	question: "Question",
	error: "Error",
	unseen: "Unseen",
	pattern: "Pattern",
	pr_pushed: "PR pushed",
	pr_opened: "PR opened",
};

interface WatcherRule {
	id: string;
	name: string;
	session_id: string | null;
	template_id: string | null;
	prompt_id?: string | null;
	repo_path?: string | null;
	trigger: WatcherTrigger;
	instructions?: string | null;
	max_fires: number;
	fire_count: number;
	cooldown_secs: number;
	status: "active" | "paused" | "stopped" | "exhausted";
}

function triggerLabel(trigger: WatcherTrigger): string {
	if (trigger.type === "command_done" && trigger.on_failure_only) return "Fail";
	if (trigger.type === "question" && trigger.confident_only) return "Question*";
	return TRIGGER_LABELS[trigger.type] ?? trigger.type;
}

function statusClass(status: string): string {
	switch (status) {
		case "active":
			return s.statusActive;
		case "paused":
			return s.statusPaused;
		case "exhausted":
			return s.statusExhausted;
		default:
			return s.statusStopped;
	}
}

export const WatcherManager: Component = () => {
	const [rules, setRules] = createSignal<WatcherRule[]>([]);
	const [showForm, setShowForm] = createSignal(false);
	const [editingId, setEditingId] = createSignal<string | null>(null);
	const [attachingId, setAttachingId] = createSignal<string | null>(null);

	const [formName, setFormName] = createSignal("");
	const [formTrigger, setFormTrigger] = createSignal<WatcherTriggerKey>("idle");
	const [formInstructions, setFormInstructions] = createSignal("");
	const [formMaxFires, setFormMaxFires] = createSignal(50);
	const [formCooldown, setFormCooldown] = createSignal(10);
	const [formPromptId, setFormPromptId] = createSignal("");
	const [formRepoPath, setFormRepoPath] = createSignal("");
	const [formAuthoredByOthers, setFormAuthoredByOthers] = createSignal(true);

	const smartPrompts = () => promptLibraryStore.getAllPrompts();
	const repos = () => repositoriesStore.getAllReposOrdered();

	// A rule is valid to submit when it references a prompt or has instructions,
	// and (for PR-pushed) a repo is selected.
	const canSubmit = () =>
		watcherFormReady({
			promptId: formPromptId(),
			instructions: formInstructions(),
			triggerKey: formTrigger(),
			repoPath: formRepoPath(),
		});

	const templates = () => rules().filter((r) => !r.session_id);
	const instances = () => rules().filter((r) => r.session_id);

	const refresh = () => {
		invoke<WatcherRule[]>("watcher_list")
			.then(setRules)
			.catch((err: unknown) => appLogger.error("ai-agent", "Failed to refresh watchers", err));
	};

	onMount(refresh);

	const unlisten = listen("watcher-status", () => refresh());
	onCleanup(() => {
		unlisten.then((fn) => fn());
	});

	const resetForm = () => {
		setFormName("");
		setFormTrigger("idle");
		setFormInstructions("");
		setFormMaxFires(50);
		setFormCooldown(10);
		setFormPromptId("");
		setFormRepoPath("");
		setFormAuthoredByOthers(true);
		setEditingId(null);
		setShowForm(false);
	};

	const openCreate = () => {
		resetForm();
		setShowForm(true);
	};

	const openEdit = (rule: WatcherRule) => {
		setFormName(rule.name);
		setFormTrigger(triggerToKey(rule.trigger));
		setFormInstructions(rule.instructions ?? "");
		setFormMaxFires(rule.max_fires);
		setFormCooldown(rule.cooldown_secs);
		setFormPromptId(rule.prompt_id ?? "");
		setFormRepoPath(rule.repo_path ?? "");
		setFormAuthoredByOthers("authored_by_others" in rule.trigger ? rule.trigger.authored_by_others : true);
		setEditingId(rule.id);
		setShowForm(true);
	};

	const handleSubmit = async () => {
		const id = editingId();
		const trigger = buildTrigger(formTrigger(), formAuthoredByOthers());
		const instructions = formInstructions().trim() || null;
		const promptId = formPromptId() || null;
		const repoPath = isGitTrigger(formTrigger()) ? formRepoPath() || null : null;
		if (id) {
			try {
				await invoke("watcher_update", {
					id,
					name: formName().trim() || null,
					trigger,
					instructions,
					promptId,
					repoPath,
					maxFires: formMaxFires(),
					cooldownSecs: formCooldown(),
				});
				resetForm();
				refresh();
			} catch (e) {
				appLogger.error("ai-agent", `Update failed: ${e}`);
			}
		} else {
			const name = formName().trim() || `Watcher (${formTrigger()})`;
			try {
				await invoke("watcher_create", {
					name,
					sessionId: null,
					trigger,
					instructions,
					promptId,
					repoPath,
					maxFires: formMaxFires(),
					cooldownSecs: formCooldown(),
				});
				resetForm();
				refresh();
			} catch (e) {
				appLogger.error("ai-agent", `Create failed: ${e}`);
			}
		}
	};

	const handleAttach = async (templateId: string, sessionId: string) => {
		try {
			await invoke("watcher_attach", { templateId, sessionId });
			setAttachingId(null);
			refresh();
		} catch (e) {
			appLogger.error("ai-agent", `Attach failed: ${e}`);
		}
	};

	const handleDetach = async (id: string) => {
		try {
			await invoke("watcher_detach", { id });
			refresh();
		} catch (e) {
			appLogger.error("ai-agent", `Detach failed: ${e}`);
		}
	};

	const handleToggle = async (id: string, enabled: boolean) => {
		try {
			await invoke("watcher_toggle", { id, enabled });
			refresh();
		} catch (e) {
			appLogger.error("ai-agent", `Toggle failed: ${e}`);
		}
	};

	const handleDelete = async (id: string) => {
		try {
			await invoke("watcher_delete", { id });
			refresh();
		} catch (e) {
			appLogger.error("ai-agent", `Delete failed: ${e}`);
		}
	};

	const terminalList = () => {
		const store = terminalsStore.state;
		const result: { id: string; sessionId: string; label: string }[] = [];
		for (const [id, t] of Object.entries(store.terminals)) {
			if (t.sessionId) {
				result.push({ id, sessionId: t.sessionId, label: t.name || id.slice(0, 8) });
			}
		}
		return result;
	};

	return (
		<div class={s.popover} onClick={(e) => e.stopPropagation()}>
			<div class={s.header}>
				<span>Watchers</span>
				<button class={s.addBtn} onClick={() => (showForm() ? resetForm() : openCreate())}>
					{showForm() ? "Cancel" : "+ New"}
				</button>
			</div>

			<Show when={showForm()}>
				<div class={s.createForm}>
					<div class={s.formRow}>
						<input
							class={s.formInput}
							placeholder="Name (optional)"
							value={formName()}
							onInput={(e) => setFormName(e.currentTarget.value)}
							autocomplete="off"
							autocorrect="off"
						/>
						<select
							class={s.formSelect}
							value={formTrigger()}
							onChange={(e) => setFormTrigger(e.currentTarget.value as WatcherTriggerKey)}
						>
							<option value="idle">Idle</option>
							<option value="busy">Busy</option>
							<option value="command_done">Done</option>
							<option value="command_done_fail">Fail</option>
							<option value="question">Question</option>
							<option value="error">Error</option>
							<option value="unseen">Unseen</option>
							<option value="pr_pushed">PR pushed</option>
							<option value="pr_opened">PR opened</option>
						</select>
					</div>

					{/* PR-pushed: git-scoped fields (repo + authored-by-others). Hidden for terminal triggers. */}
					<Show when={isGitTrigger(formTrigger())}>
						<div class={s.formRow}>
							<select
								class={s.formSelect}
								value={formRepoPath()}
								onChange={(e) => setFormRepoPath(e.currentTarget.value)}
								style={{ flex: "1" }}
							>
								<option value="">Select repo…</option>
								<For each={repos()}>{(r) => <option value={r.path}>{r.displayName || r.path}</option>}</For>
							</select>
						</div>
						<label
							class={s.formRow}
							style={{ "font-size": "var(--font-2xs)", color: "var(--fg-muted)", gap: "var(--space-1)" }}
						>
							<input
								type="checkbox"
								checked={formAuthoredByOthers()}
								onChange={(e) => setFormAuthoredByOthers(e.currentTarget.checked)}
							/>
							Only PRs authored by others
						</label>
					</Show>

					{/* Smart prompt picker — the primary action; instructions is the fallback. */}
					<div class={s.formRow}>
						<select
							class={s.formSelect}
							value={formPromptId()}
							onChange={(e) => setFormPromptId(e.currentTarget.value)}
							style={{ flex: "1" }}
						>
							<option value="">No smart prompt (use instructions)</option>
							<For each={smartPrompts()}>{(p) => <option value={p.id}>{p.name}</option>}</For>
						</select>
					</div>
					<Show when={!formPromptId() && !isGitTrigger(formTrigger())}>
						<textarea
							class={s.formTextarea}
							placeholder="Instructions for the AI agent (fallback when no smart prompt is set)…"
							value={formInstructions()}
							onInput={(e) => setFormInstructions(e.currentTarget.value)}
						/>
					</Show>
					<div class={s.formRow}>
						<label style={{ "font-size": "var(--font-2xs)", color: "var(--fg-muted)" }}>Max fires:</label>
						<input
							class={s.formInput}
							type="number"
							min="1"
							max="1000"
							value={formMaxFires()}
							onInput={(e) => setFormMaxFires(Number.parseInt(e.currentTarget.value, 10) || 50)}
							style={{ width: "60px", flex: "none" }}
						/>
						<label style={{ "font-size": "var(--font-2xs)", color: "var(--fg-muted)" }}>Cooldown:</label>
						<input
							class={s.formInput}
							type="number"
							min="5"
							max="3600"
							value={formCooldown()}
							onInput={(e) => setFormCooldown(Math.max(5, Number.parseInt(e.currentTarget.value, 10) || 10))}
							style={{ width: "60px", flex: "none" }}
						/>
						<span class={s.unit}>s</span>
						<span
							class={s.helpTip}
							data-tooltip="Minimum seconds between consecutive fires. Prevents the watcher from triggering too frequently."
							data-tooltip-align="right"
						>
							?
						</span>
					</div>
					<div class={s.formActions}>
						<button class={s.cancelBtn} onClick={resetForm}>
							Cancel
						</button>
						<button class={s.submitBtn} disabled={!canSubmit()} onClick={handleSubmit}>
							{editingId() ? "Save" : "Create Template"}
						</button>
					</div>
				</div>
			</Show>

			<Show when={templates().length > 0}>
				<div class={s.section}>
					<div class={s.sectionTitle}>Templates</div>
					<For each={templates()}>
						{(rule) => (
							<>
								<div class={s.ruleItem}>
									<span class={`${s.statusDot} ${statusClass(rule.status)}`} />
									<span class={s.triggerBadge}>{triggerLabel(rule.trigger)}</span>
									<span class={s.ruleName} title={rule.name}>
										{rule.name}
									</span>
									<button
										class={s.actionBtn}
										onClick={() => setAttachingId(attachingId() === rule.id ? null : rule.id)}
										title="Attach to terminal"
									>
										{attachingId() === rule.id ? "Cancel" : "Attach"}
									</button>
									<button class={s.actionBtn} onClick={() => openEdit(rule)} title="Edit">
										<svg viewBox="0 0 16 16" width="10" height="10" fill="currentColor">
											<path d="M11.013 1.427a1.75 1.75 0 0 1 2.474 0l1.086 1.086a1.75 1.75 0 0 1 0 2.474l-8.61 8.61c-.21.21-.47.364-.756.445l-3.251.93a.75.75 0 0 1-.927-.928l.929-3.25c.081-.286.235-.547.445-.758l8.61-8.61Zm.176 4.823L9.75 4.81l-6.286 6.287a.253.253 0 0 0-.064.108l-.558 1.953 1.953-.558a.253.253 0 0 0 .108-.064Zm1.238-3.763a.25.25 0 0 0-.354 0L10.811 3.75l1.439 1.44 1.263-1.263a.25.25 0 0 0 0-.354Z" />
										</svg>
									</button>
									<button class={s.deleteBtn} onClick={() => handleDelete(rule.id)} title="Delete">
										<svg viewBox="0 0 16 16" width="10" height="10" fill="currentColor">
											<path d="M11 1.75V3h2.25a.75.75 0 0 1 0 1.5H2.75a.75.75 0 0 1 0-1.5H5V1.75C5 .784 5.784 0 6.75 0h2.5C10.216 0 11 .784 11 1.75ZM6.5 1.75v1.25h3V1.75a.25.25 0 0 0-.25-.25h-2.5a.25.25 0 0 0-.25.25ZM3.613 5.5l.806 8.873A1.75 1.75 0 0 0 6.16 16h3.68a1.75 1.75 0 0 0 1.741-1.627L12.387 5.5Z" />
										</svg>
									</button>
								</div>
								<Show when={attachingId() === rule.id}>
									<div class={s.sessionPicker}>
										<For each={terminalList()}>
											{(term) => (
												<button class={s.sessionOption} onClick={() => handleAttach(rule.id, term.sessionId)}>
													<svg viewBox="0 0 16 16" width="12" height="12" fill="currentColor">
														<path d="M0 2.75C0 1.784.784 1 1.75 1h12.5c.966 0 1.75.784 1.75 1.75v10.5A1.75 1.75 0 0 1 14.25 15H1.75A1.75 1.75 0 0 1 0 13.25Zm1.75-.25a.25.25 0 0 0-.25.25v10.5c0 .138.112.25.25.25h12.5a.25.25 0 0 0 .25-.25V2.75a.25.25 0 0 0-.25-.25ZM7 4.5a.75.75 0 0 1 .75-.75h4.5a.75.75 0 0 1 0 1.5h-4.5A.75.75 0 0 1 7 4.5ZM3.25 7h.5a.75.75 0 0 1 0 1.5h-.5a.75.75 0 0 1 0-1.5Z" />
													</svg>
													{term.label}
												</button>
											)}
										</For>
										<Show when={terminalList().length === 0}>
											<span class={s.emptyState}>No terminals open</span>
										</Show>
									</div>
								</Show>
							</>
						)}
					</For>
				</div>
			</Show>

			<Show when={instances().length > 0}>
				<div class={s.section}>
					<div class={s.sectionTitle}>Active</div>
					<For each={instances()}>
						{(rule) => (
							<div class={s.ruleItem}>
								<span class={`${s.statusDot} ${statusClass(rule.status)}`} />
								<span class={s.triggerBadge}>{triggerLabel(rule.trigger)}</span>
								<span class={s.ruleName} title={rule.name}>
									{rule.name}
								</span>
								<span class={s.fireCount}>
									{rule.fire_count}/{rule.max_fires}
								</span>
								<Show when={rule.status === "paused"}>
									<button class={s.actionBtn} onClick={() => handleToggle(rule.id, true)} title="Resume">
										Resume
									</button>
								</Show>
								<button class={s.actionBtn} onClick={() => handleDetach(rule.id)} title="Detach from terminal">
									Detach
								</button>
							</div>
						)}
					</For>
				</div>
			</Show>

			<Show when={rules().length === 0}>
				<div class={s.emptyState}>No watchers yet. Create a template to get started.</div>
			</Show>
		</div>
	);
};
