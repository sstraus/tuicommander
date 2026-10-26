import { type Component, createEffect, createMemo, createSignal, onCleanup, Show } from "solid-js";
import { invoke } from "../../invoke";
import { ChoicePromptOverlay } from "../../mobile/components/ChoicePromptOverlay";
import { appLogger } from "../../stores/appLogger";
import { chatViewAvailability, chatViewStore } from "../../stores/chatView";
import { terminalsStore } from "../../stores/terminals";
import { openFileLinkHere, resolveFileLink } from "../../utils/openFileLink";
import { sendNativeChoice } from "../../utils/sendNativeChoice";
import type { TranscriptSearchRef } from "../AIChatPanel/Transcript";
import { Button, SegmentedControl } from "../ui";
import { TerminalChatTranscript } from "./TerminalChatTranscript";
import s from "./TerminalChatView.module.css";

/**
 * PTY transcript and native question controls, shown in place of the grid.
 *
 * Mounted only while the terminal is in chat mode; the grid stays mounted and
 * hidden by the caller. Falls back to the grid on its own when the agent goes
 * away or the backend reports the terminal as not bound, and says why.
 */
export const TerminalChatView: Component<{
	terminalId: string;
	sessionId: string;
	onSearchRef?: (ref: TranscriptSearchRef | undefined) => void;
}> = (props) => {
	let disposed = false;
	onCleanup(() => {
		disposed = true;
	});
	const term = () => terminalsStore.get(props.terminalId);
	const bound = () =>
		term()?.sessionId === props.sessionId &&
		chatViewAvailability(term()).available &&
		chatViewStore.unavailableReason(props.sessionId) === null;
	const prompt = () => (bound() && term()?.awaitingInput === "question" ? term()?.choicePrompt : null);
	const signature = createMemo(() =>
		JSON.stringify([
			props.sessionId,
			prompt()?.title,
			prompt()?.selection_mode,
			prompt()?.dismiss_key,
			prompt()?.options.map((o) => [
				o.key,
				o.label,
				...(prompt()?.selection_mode === "toggle-enter" ? [o.highlighted] : []),
			]),
		]),
	);
	const [sending, setSending] = createSignal(false);
	const [error, setError] = createSignal<string | null>(null);
	createEffect(() => {
		signature();
		setSending(false);
		setError(null);
	});
	async function select(key: string) {
		const question = prompt();
		if (!question || sending()) return;
		const sessionId = props.sessionId;
		const owner = signature();
		const agent = term()?.agentType;
		const agentSession = term()?.agentSessionId;
		const exactSession = term()?.exactAgentSessionId;
		setSending(true);
		try {
			const write = async (data: string) => {
				// Every continuation remains on its original live PTY and dialog. Never
				// retarget an answer after terminal replacement or transcript fallback.
				if (
					disposed ||
					!bound() ||
					props.sessionId !== sessionId ||
					signature() !== owner ||
					term()?.agentType !== agent ||
					term()?.agentSessionId !== agentSession ||
					term()?.exactAgentSessionId !== exactSession
				)
					throw new Error("The native question changed; answer the current prompt instead.");
				await invoke<void>("write_pty", { sessionId, data });
			};
			await sendNativeChoice(write, question, agent, key, () => terminalsStore.setViewMode(props.terminalId, "cli"));
			if (signature() === owner && question.selection_mode === "navigate-only" && key !== "↵") setSending(false);
		} catch (err) {
			if (signature() === owner) {
				setSending(false);
				setError(err instanceof Error ? err.message : String(err));
			}
			appLogger.warn("terminal", "Native Chat answer failed", err);
		}
	}
	createEffect(() => {
		const availability = chatViewAvailability(terminalsStore.get(props.terminalId));
		const reason = availability.available ? chatViewStore.unavailableReason(props.sessionId) : availability.reason;
		if (reason === null) return;
		chatViewStore.markUnavailable(props.sessionId, reason);
		terminalsStore.setViewMode(props.terminalId, "cli");
	});

	/** False when the path resolves to nothing, so the caller can drop the dead link. */
	const openFile = async (href: string) => {
		const cwd = terminalsStore.get(props.terminalId)?.cwd;
		if (!cwd) return;
		try {
			const target = await resolveFileLink(cwd, href);
			if (!target) return false;
			openFileLinkHere(target);
		} catch (error) {
			appLogger.error("terminal", "Failed to resolve file link", error);
		}
	};

	return (
		<>
			<TerminalChatTranscript
				sessionId={props.sessionId}
				fontSize={term()?.fontSize}
				terminalAgent={terminalsStore.get(props.terminalId)?.agentType ?? undefined}
				onSearchRef={props.onSearchRef}
				onOpenFile={openFile}
				onUnavailable={() => terminalsStore.setViewMode(props.terminalId, "cli")}
			/>
			<Show when={bound() && term()?.awaitingInput === "question"}>
				<section class={s.nativeQuestion} aria-label="Native terminal question">
					<Show
						when={prompt()}
						fallback={
							<div class={s.fallbackBanner}>
								{term()?.questionText}
								<Button
									variant="ghost"
									type="button"
									onClick={() => terminalsStore.setViewMode(props.terminalId, "cli")}
								>
									Answer in CLI
								</Button>
							</div>
						}
					>
						{(question) => <ChoicePromptOverlay prompt={question()} embedded disabled={sending()} onSelect={select} />}
					</Show>
					<Show when={error()}>
						<div role="alert" class={s.fallbackBanner}>
							{error()}
						</div>
					</Show>
				</section>
			</Show>
		</>
	);
};

/** The CLI | Chat switch of a terminal that runs an agent. */
export const ViewModeToggle: Component<{ terminalId: string; onWheel?: (e: WheelEvent) => void }> = (props) => {
	const term = () => terminalsStore.get(props.terminalId);
	const availability = () => chatViewAvailability(term());
	const mode = () => term()?.viewMode ?? "cli";
	const reason = () => {
		const a = availability();
		return a.available ? undefined : a.reason;
	};
	return (
		<Show when={term()?.agentType}>
			<div class={s.toggle} data-terminal-view-toggle data-mode={mode()} title={reason()} onWheel={props.onWheel}>
				<SegmentedControl
					aria-label="Terminal view"
					options={[
						{ id: "cli", label: "CLI" },
						{ id: "chat", label: "Chat", disabled: !availability().available },
					]}
					value={mode()}
					onChange={(value) => {
						if (value === "chat") {
							const sessionId = term()?.sessionId;
							if (sessionId) chatViewStore.clearUnavailable(sessionId);
						}
						terminalsStore.setViewMode(props.terminalId, value);
					}}
				/>
			</div>
		</Show>
	);
};

/** One line left behind when the chat view closed itself. */
export const ChatViewFallbackBanner: Component<{ terminalId: string }> = (props) => {
	const term = () => terminalsStore.get(props.terminalId);
	const reason = () => {
		const sessionId = term()?.sessionId;
		return sessionId && term()?.viewMode === "cli" ? chatViewStore.unavailableReason(sessionId) : null;
	};
	return <Show when={reason()}>{(text) => <div class={s.fallbackBanner}>Chat view closed: {text()}</div>}</Show>;
};
