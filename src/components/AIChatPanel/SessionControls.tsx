import { Button } from "../ui/Button";
import { Select } from "../ui/Select";
/**
 * The knobs a session publishes, and the three verbs ego adds to a turn.
 *
 * Every option drawn here comes from the session itself — model, reasoning
 * effort, mode, whatever else it chose to publish. There is no list of models
 * in this file on purpose: a hardcoded one would be a second, wrong answer to a
 * question the session already answers, and it would go stale the first time
 * ego learned a new model.
 *
 * Pause, resume and compact are ego's own extensions. Each is drawn only when
 * the agent advertised it, so a build of ego without them shows no button
 * rather than a button that fails.
 */

import { type Component, createSignal, For, Show } from "solid-js";
import { detachPanel, reattachPanel } from "../../panelRouter";
import type { AcpListedSession } from "../../services/acpClient";
import { acpTranscript } from "../../stores/acpTranscript";
import { registerModal } from "../../stores/modalStack";
import { isTauri } from "../../transport";
import type {
	AcpSessionConfigOption,
	AcpSessionConfigSelectGroup,
	AcpSessionConfigSelectOption,
} from "../../types/acp";
import { cx } from "../../utils";
import { ConfirmDialog } from "../ConfirmDialog";
import d from "../shared/dialog.module.css";
import s from "./AIChatPanel.module.css";
import type { AcpChat } from "./useAcpChat";

type SelectOption = Extract<AcpSessionConfigOption, { type: "select" }>;

function isGrouped(options: SelectOption["options"]): options is AcpSessionConfigSelectGroup[] {
	return options.length > 0 && "options" in options[0];
}

function groups(option: SelectOption): AcpSessionConfigSelectGroup[] {
	return isGrouped(option.options) ? option.options : [];
}

function flat(option: SelectOption): AcpSessionConfigSelectOption[] {
	return isGrouped(option.options) ? [] : option.options;
}

function choices(option: SelectOption): AcpSessionConfigSelectOption[] {
	return [...flat(option), ...groups(option).flatMap((group) => group.options)];
}

function choiceName(option: SelectOption): string {
	return choices(option).find((choice) => choice.value === option.currentValue)?.name ?? option.currentValue;
}

export function conversationLabel(session: Pick<AcpListedSession, "sessionId" | "title" | "updatedAt">): string {
	const title = acpTranscript.title(session.sessionId) || session.title;
	if (title?.trim() && !/^[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}$/i.test(title.trim())) return title.trim();
	const firstPrompt = acpTranscript.entries(session.sessionId).find((entry) => entry.kind === "user");
	if (firstPrompt?.kind === "user" && firstPrompt.text.trim()) return firstPrompt.text.trim().slice(0, 48);
	const updated = session.updatedAt ? new Date(session.updatedAt) : null;
	return updated && !Number.isNaN(updated.getTime())
		? `Conversation · ${updated.toLocaleString()}`
		: "Untitled conversation";
}

const ConfigSelect: Component<{ option: SelectOption; chat: AcpChat }> = (props) => (
	<Select
		class={s.sessionSettingSelect}
		aria-label={props.option.name}
		disabled={props.chat.busy()}
		onChange={(event) => {
			const select = event.currentTarget;
			void props.chat.setOption(props.option.id, { value: select.value }).then(() => {
				const latest = props.chat.configOptions().find((option) => option.id === props.option.id);
				select.value = latest?.type === "select" ? latest.currentValue : props.option.currentValue;
			});
		}}
	>
		<For each={flat(props.option)}>
			{(choice) => (
				<option value={choice.value} selected={choice.value === props.option.currentValue}>
					{choice.name}
				</option>
			)}
		</For>
		<For each={groups(props.option)}>
			{(group) => (
				<optgroup label={group.name}>
					<For each={group.options}>
						{(choice) => (
							<option value={choice.value} selected={choice.value === props.option.currentValue}>
								{choice.name}
							</option>
						)}
					</For>
				</optgroup>
			)}
		</For>
	</Select>
);

const SessionSettingsDialog: Component<{ chat: AcpChat; options: SelectOption[]; onClose: () => void }> = (props) => {
	registerModal(props.onClose);
	props.chat.clearError();
	return (
		<div class={d.overlay} onClick={props.onClose}>
			<div
				class={cx(d.popover, s.sessionSettingsDialog)}
				role="dialog"
				aria-modal="true"
				aria-labelledby="session-settings-title"
				onClick={(event) => event.stopPropagation()}
			>
				<div class={d.header}>
					<h4 id="session-settings-title">Session settings</h4>
				</div>
				<div class={d.body}>
					<For each={props.options}>
						{(option) => (
							<div class={s.sessionSettingRow}>
								<label class={s.sessionSettingLabel}>
									{option.name}
									<ConfigSelect option={option} chat={props.chat} />
								</label>
								<Show when={option.description}>
									<p class={s.sessionSettingDescription}>{option.description}</p>
								</Show>
							</div>
						)}
					</For>
					<Show when={props.chat.error()}>
						<p class={d.error} role="alert">
							{props.chat.error()}
						</p>
					</Show>
				</div>
				<div class={d.actions}>
					<Button onClick={props.onClose}>Done</Button>
				</div>
			</div>
		</div>
	);
};

export const SessionControls: Component<{ chat: AcpChat; onNewOptions: () => void; detached?: boolean }> = (props) => {
	const [settingsOpen, setSettingsOpen] = createSignal(false);
	const [deleteTarget, setDeleteTarget] = createSignal<AcpListedSession | null>(null);
	const [deleting, setDeleting] = createSignal(false);
	const holdOffered = () => props.chat.capabilities()?.egoHoldVersion != null;
	const compactOffered = () => props.chat.capabilities()?.egoCompactVersion != null;
	const forkOffered = () => props.chat.capabilities()?.fork === true;
	const sessions = () => props.chat.sessions();
	const selectOptions = () =>
		props.chat.configOptions().filter((option): option is SelectOption => option.type === "select");

	return (
		<div class={s.sessionHeader}>
			<div class={s.conversationSwitcher}>
				<span class={s.conversationLabel} aria-hidden="true">
					<svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
						<path d="M2 2h12v9H6l-4 3V2Zm1 1v9l2.7-2H13V3H3Z" />
					</svg>
					<span>{props.chat.title() || "AI Chat"}</span>
					<svg width="10" height="10" viewBox="0 0 16 16" fill="currentColor">
						<path d="m3 5 5 5 5-5H3Z" />
					</svg>
				</span>
				{/* Keep the native picker over the visible frame; both share its full hit region. */}
				<Select
					data-control-layout="overlay-picker"
					class={s.modelPicker}
					title="Conversation"
					aria-label="Conversation"
					value={props.chat.sessionId() ?? ""}
					onChange={(event) => {
						const select = event.currentTarget;
						void props.chat.selectSession(select.value).then(() => {
							select.value = props.chat.sessionId() ?? "";
						});
					}}
				>
					<Show when={!sessions().some((session) => session.sessionId === props.chat.sessionId())}>
						<option value={props.chat.sessionId() ?? ""}>{props.chat.title() || "AI Chat"}</option>
					</Show>
					<For each={sessions()}>
						{(session) => (
							<option
								value={session.sessionId}
								title={session.sessionId}
								disabled={session._meta?.tuicommander?.deleted === true}
							>
								{"  ".repeat(session._meta?.tuicommander?.lineageDepth ?? 0)}
								{session._meta?.tuicommander?.lineageDepth ? "↳ " : ""}
								{conversationLabel(session)}
							</option>
						)}
					</For>
				</Select>
			</div>

			<Button
				variant="ghost"
				type="button"
				class={s.headerBtn}
				aria-label="New chat tab"
				title="New chat tab (⌘T)"
				disabled={props.chat.phase() === "unconfigured"}
				onClick={() => void props.chat.startSession()}
			>
				<svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
					<path d="M7 2h2v5h5v2H9v5H7V9H2V7h5V2Z" />
				</svg>
				New
			</Button>
			<details
				class={s.actionsMenu}
				onKeyDown={(event) => {
					if (event.key === "Escape" && event.currentTarget.open) {
						event.currentTarget.open = false;
						event.preventDefault();
						event.stopPropagation();
						event.currentTarget.querySelector("summary")?.focus();
					}
				}}
			>
				<summary
					data-control-layout="disclosure"
					class={s.headerBtn}
					aria-label="Conversation actions"
					title="Conversation actions"
				>
					<svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
						<circle cx="3" cy="8" r="1" />
						<circle cx="8" cy="8" r="1" />
						<circle cx="13" cy="8" r="1" />
					</svg>
				</summary>
				<div class={s.actionsMenuItems}>
					<Button
						variant="ghost"
						type="button"
						class={s.headerBtn}
						disabled={props.chat.phase() === "unconfigured"}
						onClick={() => void props.chat.startSession()}
					>
						New conversation<span class={s.menuHint}>⌘T</span>
					</Button>
					<Button
						variant="ghost"
						type="button"
						class={s.headerBtn}
						aria-label="New conversation with options"
						title="New conversation with options"
						onClick={props.onNewOptions}
					>
						New conversation with options…
					</Button>
					<hr class={s.menuSeparator} />
					<Show when={holdOffered()}>
						<Show
							when={props.chat.held()}
							fallback={
								<Button
									variant="ghost"
									type="button"
									class={s.headerBtn}
									aria-label="Pause the turn"
									title="Pause the turn"
									disabled={!props.chat.busy()}
									onClick={() => void props.chat.pause()}
								>
									Pause the turn
									<Show when={!props.chat.busy()}>
										<span class={s.menuHint}>only while running</span>
									</Show>
								</Button>
							}
						>
							<Button
								variant="ghost"
								type="button"
								class={s.headerBtn}
								aria-pressed={true}
								aria-label="Resume the turn"
								title="Resume the turn"
								onClick={() => void props.chat.resume()}
							>
								Resume the turn
							</Button>
						</Show>
					</Show>

					<Show when={compactOffered()}>
						<Button
							variant="ghost"
							type="button"
							class={s.headerBtn}
							aria-label="Compact the conversation"
							title="Compact the conversation"
							disabled={props.chat.busy()}
							onClick={() => void props.chat.compact()}
						>
							Compact context
						</Button>
					</Show>

					<Show when={forkOffered()}>
						<Button
							variant="ghost"
							type="button"
							class={s.headerBtn}
							aria-label="Fork the conversation"
							title="Fork the conversation"
							disabled={props.chat.busy()}
							onClick={() => void props.chat.fork()}
						>
							Fork conversation
						</Button>
					</Show>
					<hr class={s.menuSeparator} />
					<Show when={selectOptions().length > 0}>
						<Button
							variant="ghost"
							type="button"
							class={s.headerBtn}
							aria-label="Session settings"
							title="Session settings"
							onClick={() => setSettingsOpen(true)}
						>
							Session settings — model, reasoning
						</Button>
					</Show>

					<Button
						variant="ghost"
						type="button"
						class={s.headerBtn}
						disabled={!props.detached && !isTauri()}
						title={!props.detached && !isTauri() ? "Desktop only" : undefined}
						onClick={() => (props.detached ? reattachPanel("ai-chat") : detachPanel("ai-chat"))}
					>
						{props.detached ? "Bring back to main window" : "Open in separate window"}
					</Button>
					<Show when={props.chat.launch()}>
						{(launch) => (
							<p class={s.sessionSettingDescription}>
								{launch().profile || "Default profile"} · {launch().workspace} · {launch().executable}
							</p>
						)}
					</Show>
					<hr class={s.menuSeparator} />
					<Show when={props.chat.capabilities()?.delete && sessions().length > 0}>
						<details class={s.conversationList}>
							<summary data-control-layout="disclosure" class={s.headerBtn}>
								Delete conversation…
							</summary>
							<div class={s.conversationRows}>
								<For each={sessions()}>
									{(session) => (
										<div class={s.conversationRow}>
											<Button
												variant="ghost"
												type="button"
												class={s.conversationName}
												disabled={session._meta?.tuicommander?.deleted === true}
												onClick={() => void props.chat.selectSession(session.sessionId)}
											>
												{conversationLabel(session)}
											</Button>
											<Show when={!session._meta?.tuicommander?.deleted}>
												<Button
													variant="ghost"
													type="button"
													class={s.headerBtn}
													disabled={deleting()}
													shape="icon"
													aria-label={`Delete ${conversationLabel(session)}`}
													title="Delete conversation"
													onClick={() => setDeleteTarget(session)}
												>
													<svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
														<path d="M6 1h4l1 2h3v1H2V3h3l1-2ZM3 5h10l-1 10H4L3 5Zm3 2v6h1V7H6Zm3 0v6h1V7H9Z" />
													</svg>
												</Button>
											</Show>
										</div>
									)}
								</For>
							</div>
						</details>
					</Show>
					<ConfirmDialog
						visible={deleteTarget() !== null}
						title="Delete conversation"
						message={`Delete “${deleteTarget() ? conversationLabel(deleteTarget()!) : ""}”? This cannot be undone.`}
						confirmLabel="Delete"
						defaultButton="cancel"
						kind="warning"
						onClose={() => setDeleteTarget(null)}
						onConfirm={() => {
							const target = deleteTarget();
							setDeleteTarget(null);
							if (!target || deleting()) return;
							setDeleting(true);
							void props.chat.deleteSession(target.sessionId).finally(() => setDeleting(false));
						}}
					/>
				</div>
			</details>

			<Show when={settingsOpen()}>
				<SessionSettingsDialog chat={props.chat} options={selectOptions()} onClose={() => setSettingsOpen(false)} />
			</Show>
		</div>
	);
};

export function sessionSummary(chat: AcpChat): string {
	return (
		chat
			.configOptions()
			.filter((option): option is SelectOption => option.type === "select")
			.filter((option) => option.id === "model" || option.id === "effort" || option.id === "mode")
			.map((option) =>
				option.id === "model"
					? choiceName(option).split("/").at(-1)
					: option.id === "effort"
						? option.currentValue
						: choiceName(option),
			)
			.join(" · ") || "Model: pending · Mode: pending"
	);
}
