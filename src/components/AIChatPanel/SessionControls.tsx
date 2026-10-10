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
import type { AcpListedSession } from "../../services/acpClient";
import { acpTranscript } from "../../stores/acpTranscript";
import { registerModal } from "../../stores/modalStack";
import type {
	AcpSessionConfigOption,
	AcpSessionConfigSelectGroup,
	AcpSessionConfigSelectOption,
} from "../../types/acp";
import { cx } from "../../utils";
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
	<select
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
	</select>
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
					<button type="button" class={d.cancelBtn} onClick={props.onClose}>
						Done
					</button>
				</div>
			</div>
		</div>
	);
};

export const SessionControls: Component<{ chat: AcpChat }> = (props) => {
	const [settingsOpen, setSettingsOpen] = createSignal(false);
	const holdOffered = () => props.chat.capabilities()?.egoHoldVersion != null;
	const compactOffered = () => props.chat.capabilities()?.egoCompactVersion != null;
	const forkOffered = () => props.chat.capabilities()?.fork === true;
	const sessions = () => props.chat.sessions();
	const selectOptions = () =>
		props.chat.configOptions().filter((option): option is SelectOption => option.type === "select");
	const summary = () =>
		selectOptions()
			.filter((option) => option.id === "model" || option.id === "mode")
			.map(
				(option) =>
					`${option.name}: ${option.id === "model" ? choiceName(option).split("/").at(-1) : choiceName(option)}`,
			)
			.join(" · ");

	return (
		<div class={s.controlBar}>
			{/* A boolean option is deliberately absent: the client does not advertise
			    `clientBooleanConfig`, so one cannot arrive, and drawing a control for
			    it would offer a switch the agent never said it would read. */}
			<Show
				when={summary()}
				fallback={
					<span class={s.sessionSettingsSummary} role="status">
						Model: pending · Mode: pending
					</span>
				}
			>
				<span class={s.sessionSettingsSummary}>{summary()}</span>
			</Show>
			<Show when={selectOptions().length > 0}>
				<button
					type="button"
					class={s.headerBtn}
					aria-label="Session settings"
					title="Session settings"
					onClick={() => setSettingsOpen(true)}
				>
					<svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
						<path d="M6.8 1h2.4l.4 1.6 1.3.6 1.4-.8L14 4l-.8 1.4.6 1.3 1.6.4v2.4l-1.6.4-.6 1.3.8 1.4-1.7 1.7-1.4-.8-1.3.6-.4 1.6H6.8l-.4-1.6-1.3-.6-1.4.8L2 12.3l.8-1.4-.6-1.3L.6 9.2V6.8l1.6-.4.6-1.3L2 3.7 3.7 2l1.4.8 1.3-.6L6.8 1Zm1.2 4.4a2.6 2.6 0 1 0 0 5.2 2.6 2.6 0 0 0 0-5.2Z" />
					</svg>
				</button>
			</Show>

			<Show when={sessions().length > 1}>
				<select
					class={s.modelPicker}
					title="Conversation"
					value={props.chat.sessionId() ?? ""}
					onChange={(event) => {
						const select = event.currentTarget;
						void props.chat.selectSession(select.value).then(() => {
							select.value = props.chat.sessionId() ?? "";
						});
					}}
				>
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
				</select>
			</Show>

			<span class={s.controlSpacer} />

			<Show when={holdOffered()}>
				<Show
					when={props.chat.held()}
					fallback={
						<button
							type="button"
							class={s.headerBtn}
							aria-label="Pause the turn"
							title="Pause the turn"
							disabled={!props.chat.busy()}
							onClick={() => void props.chat.pause()}
						>
							<svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
								<path d="M3 2h3v12H3zm7 0h3v12h-3z" />
							</svg>
						</button>
					}
				>
					<button
						type="button"
						class={cx(s.headerBtn, s.headerBtnActive)}
						aria-label="Resume the turn"
						title="Resume the turn"
						onClick={() => void props.chat.resume()}
					>
						<svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
							<path d="M4 2.5v11l9-5.5z" />
						</svg>
					</button>
				</Show>
			</Show>

			<Show when={compactOffered()}>
				<button
					type="button"
					class={s.headerBtn}
					aria-label="Compact the conversation"
					title="Compact the conversation"
					disabled={props.chat.busy()}
					onClick={() => void props.chat.compact()}
				>
					<svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
						<path d="M2 6h5V1H5v2.6L1.7.3.3 1.7 3.6 5H2zm12 4H9v5h2v-2.6l3.3 3.3 1.4-1.4L12.4 11H14z" />
					</svg>
				</button>
			</Show>

			<Show when={forkOffered()}>
				<button
					type="button"
					class={s.headerBtn}
					aria-label="Fork the conversation"
					title="Fork the conversation"
					disabled={props.chat.busy()}
					onClick={() => void props.chat.fork()}
				>
					<svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
						<path d="M3 1.5a1.75 1.75 0 0 1 .75 3.33V6.5h4.5V4.83a1.75 1.75 0 1 1 1.5 0V6.5A1.5 1.5 0 0 1 8.25 8H8v3.17a1.75 1.75 0 1 1-1.5 0V8H3.75A1.5 1.5 0 0 1 2.25 6.5V4.83A1.75 1.75 0 0 1 3 1.5zm0 1.5a.25.25 0 1 0 0 .5.25.25 0 0 0 0-.5z" />
					</svg>
				</button>
			</Show>

			<button
				type="button"
				class={s.headerBtn}
				aria-label="Start another conversation"
				title="Start another conversation"
				onClick={() => void props.chat.startSession()}
			>
				<svg width="14" height="14" viewBox="0 0 14 14" fill="currentColor" aria-hidden="true">
					<path d="M6.4 1h1.2v5.4H13v1.2H7.6V13H6.4V7.6H1V6.4h5.4z" />
				</svg>
			</button>
			<Show when={settingsOpen()}>
				<SessionSettingsDialog chat={props.chat} options={selectOptions()} onClose={() => setSettingsOpen(false)} />
			</Show>
		</div>
	);
};
