/**
 * Where a turn is written and stopped.
 *
 * Enter sends and Shift+Enter breaks the line, which is what every chat in this
 * app does. While a turn is running the same corner holds Stop instead of Send:
 * the next thing a person wants during a turn they regret is not a second turn.
 */

import { type Component, createEffect, createMemo, createSignal, createUniqueId, For, on, Show } from "solid-js";
import { t } from "../../i18n";
import mobileInput from "../../mobile/components/CommandInput.module.css";
import { MobileComposerMenu } from "../../mobile/components/MobileComposerMenu";
import { MobileDictationButton } from "../../mobile/components/MobileDictationButton";
import { useBrowserDictation } from "../../mobile/useBrowserDictation";
import { useKeyboardCorrection } from "../../mobile/useKeyboardCorrection";
import { uploadAttachment } from "../../services/uploadAttachment";
import type { AcpAvailableCommand } from "../../types/acp";
import { pastedImageFiles } from "../../utils/pastedImage";
import { spacedUrlPaste } from "../../utils/pastedUrl";
import { pathBasename } from "../../utils/pathUtils";
import { getRepoTextColor } from "../../utils/repoColor";
import { ComposeSendIcon } from "../shared/ComposeActionIcons";
import { Button } from "../ui/Button";
import { Textarea } from "../ui/Textarea";
import s from "./AIChatPanel.module.css";
import { aiChatDraft } from "./draft";
import { SESSION_SETTINGS_UNAVAILABLE, sessionSelectOptions, sessionSummary } from "./SessionControls";
import type { AcpChat } from "./useAcpChat";

const MIN_HEIGHT_PX = 36;
const MAX_PANEL_FRACTION = 0.4;
/** Used while the panel has no layout yet (hidden or not mounted). */
const FALLBACK_CAP_PX = 150;

export const Composer: Component<{
	chat: AcpChat;
	mobileAttachments?: boolean;
	onSessionSettings?: () => void;
	sharedFile?: File | null;
	onSharedFileConsumed?: () => void;
}> = (props) => {
	const keyboardCorrection = useKeyboardCorrection();
	const [pasteError, setPasteError] = createSignal<string | null>(null);
	const [uploading, setUploading] = createSignal(false);
	const [commandNotice, setCommandNotice] = createSignal<string | null>(null);
	const commandMenuId = createUniqueId();
	const [commandIndex, setCommandIndex] = createSignal(0);
	const [commandsDismissed, setCommandsDismissed] = createSignal(false);
	type Command = AcpAvailableCommand & { run?: () => void; unavailable?: string };
	const commands = createMemo<Command[]>(() => {
		const chat = props.chat;
		const live = !!chat.sessionId() && !!chat.connectionId?.();
		const busy = chat.busy();
		const host: Command[] = [
			{
				name: "new",
				description: "Start a new conversation",
				run: () => {
					void chat.startSession();
				},
			},
			{ name: "clear", description: "Clear the composer and its attachments", run: () => aiChatDraft.clear() },
		];
		if (live) {
			host.push({
				name: "quit",
				description: "End the AI Chat connection (alias /q)",
				run: () => {
					void chat.stop();
				},
			});
			host.push({
				name: "usage",
				description: "Show current context usage and cost",
				run: () => {
					const usage = chat.usage();
					setCommandNotice(
						usage
							? `Context ${usage.used} / ${usage.size} tokens (${Math.round((usage.used / usage.size) * 100)}%)${usage.cost ? ` · ${usage.cost.currency} ${usage.cost.amount}` : ""}`
							: "No usage has been reported for this conversation.",
					);
				},
			});
		}
		if (live && chat.capabilities?.()?.egoCompactVersion != null)
			host.push({
				name: "compact",
				description: "Compact context and continue in the successor conversation",
				unavailable: busy ? "Wait for the current turn to finish before compacting." : undefined,
				run: () => {
					void chat.compact();
				},
			});
		if (live && chat.capabilities?.()?.fork)
			host.push({
				name: "fork",
				description: "Fork the conversation into a new tab",
				unavailable: busy ? "Wait for the current turn to finish before forking." : undefined,
				run: () => {
					void chat.fork();
				},
			});
		if (live && chat.capabilities?.()?.egoHoldVersion != null) {
			host.push({
				name: "pause",
				description: "Pause the current turn",
				unavailable: !busy || chat.held() ? "Pause is available while a turn is running." : undefined,
				run: () => {
					void chat.pause();
				},
			});
			host.push({
				name: "resume",
				description: "Resume the paused turn",
				unavailable: !chat.held() ? "Resume is available while a turn is paused." : undefined,
				run: () => {
					void chat.resume();
				},
			});
		}
		const hostNames = new Set([
			"new",
			"clear",
			"quit",
			"q",
			"usage",
			"compact",
			"fork",
			"pause",
			"resume",
			"name",
			"retry",
		]);
		return [...host, ...(chat.availableCommands?.() ?? []).filter((command) => !hostNames.has(command.name))];
	});
	const commandText = () => aiChatDraft.text().match(/^\/(\S*)(?:\s([\s\S]*))?$/);
	const matchingCommands = createMemo(() => {
		const typed = commandText();
		if (!typed || typed[2] !== undefined) return [];
		return commands().filter((command) => command.name.toLowerCase().startsWith(typed[1].toLowerCase()));
	});
	const commandMenuOpen = () => !commandsDismissed() && matchingCommands().length > 0;
	createEffect(on(matchingCommands, () => setCommandIndex(0)));
	createEffect(
		on(
			() => aiChatDraft.text(),
			() => {
				setCommandIndex(0);
				setCommandsDismissed(false);
			},
		),
	);
	const chooseCommand = (command: Command) => {
		setCommandNotice(null);
		aiChatDraft.set(`/${command.name} `);
		queueMicrotask(() => textarea?.focus());
	};
	let textarea: HTMLTextAreaElement | undefined;
	let inputArea: HTMLDivElement | undefined;
	const browserDictation = useBrowserDictation({
		sessionId: () => props.chat.sessionId() || null,
		enabled: !!props.mobileAttachments,
		blocked: () => !props.mobileAttachments || uploading(),
		getDraft: () => aiChatDraft.text(),
		setDraft: (text) => aiChatDraft.set(text),
	});
	const hasContent = () =>
		!!aiChatDraft.text().trim() || aiChatDraft.images().length > 0 || aiChatDraft.files().length > 0;
	const parkLabel = () =>
		!aiChatDraft.parked() ? "Park draft" : hasContent() ? "Swap parked draft" : "Restore parked draft";
	createEffect(
		on(
			() => props.chat.sessionId() ?? "",
			(session) => {
				browserDictation.cancel();
				aiChatDraft.activate(session);
				setCommandNotice(null);
			},
		),
	);
	createEffect(() => {
		if (props.chat.phase() !== "live" || aiChatDraft.focusRequested() !== props.chat.sessionId()) return;
		queueMicrotask(() => {
			if (
				!textarea?.isConnected ||
				props.chat.phase() !== "live" ||
				aiChatDraft.focusRequested() !== props.chat.sessionId()
			)
				return;
			textarea.focus();
			textarea.setSelectionRange(textarea.value.length, textarea.value.length);
			aiChatDraft.consumeFocus();
		});
	});
	const resize = () => {
		if (!textarea || props.mobileAttachments) return;
		const previous = textarea.offsetHeight;
		textarea.style.height = "auto";
		const natural = textarea.scrollHeight;
		const panelHeight = inputArea?.parentElement?.parentElement?.clientHeight ?? 0;
		const cap = panelHeight > 0 ? Math.max(MIN_HEIGHT_PX, panelHeight * MAX_PANEL_FRACTION) : FALLBACK_CAP_PX;
		const target = Math.min(Math.max(natural, MIN_HEIGHT_PX), cap);
		// Re-assert the old height and force a reflow so the CSS transition animates to the new one.
		if (previous > 0) {
			textarea.style.height = `${previous}px`;
			void textarea.offsetHeight;
		}
		textarea.style.height = `${target}px`;
		textarea.style.overflowY = natural > cap ? "auto" : "hidden";
	};
	createEffect(() => {
		aiChatDraft.text();
		queueMicrotask(resize);
	});
	const send = () => {
		// Compaction may refuse or lose selection. Keep the draft until it can be accepted.
		if (props.chat.compacting?.()) return;
		if (props.mobileAttachments && uploading()) return;
		browserDictation.cancel();
		setCommandNotice(null);
		const text = aiChatDraft.expandedText();
		const images = aiChatDraft.images().map((image) => image.block);
		const files = aiChatDraft.files();
		if (!text.trim() && images.length === 0 && files.length === 0) return;
		if (text.startsWith("/")) {
			const typed = text.match(/^\/(\S+)(?:\s([\s\S]*))?$/);
			const name = typed?.[1] === "q" ? "quit" : typed?.[1];
			const command = commands().find((candidate) => candidate.name === name);
			// A second slash identifies path text; advertised command names still take precedence.
			if (!command && !typed?.[1].includes("/")) {
				setCommandNotice(
					`${typed ? `/${typed[1]}` : "This command"} is not available in AI Chat. Available commands: ${commands()
						.map((candidate) => `/${candidate.name}`)
						.join(", ")}`,
				);
				return;
			}
			if (command?.run) {
				if (command.unavailable || typed?.[2]?.trim() || ((images.length || files.length) && name !== "clear")) {
					setCommandNotice(command.unavailable ?? `/${name} does not accept arguments or attachments.`);
					return;
				}
				aiChatDraft.clear();
				command.run();
				queueMicrotask(resize);
				return;
			}
		}
		aiChatDraft.restoreAfterSend();
		queueMicrotask(resize);
		setPasteError(null);
		void props.chat.send(text, images, files);
	};

	const stageImages = async (files: File[]): Promise<string | null> => {
		const ownsDraft = aiChatDraft.captureOwnership();
		try {
			// An unstarted conversation has no capabilities yet, not an image refusal.
			const startedSession = await props.chat.ensureStarted();
			for (const file of files) {
				if (!ownsDraft(startedSession ?? "")) return null;
				const error = await aiChatDraft.stageImage(file, props.chat.capabilities()?.promptImage === true);
				if (!ownsDraft(startedSession ?? "")) return null;
				if (error) return error;
			}
			return null;
		} catch (error) {
			return error instanceof Error ? error.message : String(error);
		}
	};

	const attachFile = async (file: File) => {
		setPasteError(null);

		setUploading(true);
		try {
			if (file.type.startsWith("image/")) {
				setPasteError(await stageImages([file]));
				return;
			}
			await props.chat.ensureStarted();
			const connectionId = props.chat.connectionId();
			const session = props.chat.sessionId();
			if (!connectionId || !session) throw new Error("AI Chat is not connected");
			const receipt = await uploadAttachment(file, { kind: "acp", id: connectionId, session });
			aiChatDraft.stageFile({ name: file.name, path: receipt.path });
			if (receipt.notice) setPasteError(receipt.notice);
		} catch (error) {
			setPasteError(error instanceof Error ? error.message : String(error));
		} finally {
			setUploading(false);
		}
	};
	createEffect(() => {
		const shared = props.sharedFile;
		if (!shared) return;
		props.onSharedFileConsumed?.();
		void attachFile(shared);
	});

	const onPaste = (event: ClipboardEvent) => {
		browserDictation.cancel();
		// DEFERRED (2026-09-28) — image drop needs Boss's explicit approval for drag/drop handlers.
		const files = pastedImageFiles(event);
		if (files.length === 0) {
			const value = event.clipboardData?.getData("text/plain") ?? "";
			if (!textarea) return;
			const spaced = props.mobileAttachments
				? spacedUrlPaste(value, textarea.value.charAt(textarea.selectionEnd))
				: null;
			let cursor = aiChatDraft.stageTextPaste(spaced ?? value, textarea.selectionStart, textarea.selectionEnd);
			if (cursor === null) {
				if (spaced === null) return;
				textarea.setRangeText(spaced, textarea.selectionStart, textarea.selectionEnd, "end");
				cursor = textarea.selectionStart;
				aiChatDraft.set(textarea.value);
			}
			event.preventDefault();
			queueMicrotask(() => {
				textarea?.setSelectionRange(cursor, cursor);
				resize();
			});
			return;
		}
		event.preventDefault();
		setPasteError(null);
		void stageImages(files).then((error) => {
			if (error) setPasteError(error);
		});
	};

	const onKeyDown = (event: KeyboardEvent) => {
		if (event.isComposing) return;
		if (commandMenuOpen()) {
			if (event.key === "Escape") {
				event.preventDefault();
				event.stopPropagation();
				setCommandsDismissed(true);
				return;
			}
			if (event.key === "ArrowDown" || event.key === "ArrowUp") {
				event.preventDefault();
				const count = matchingCommands().length;
				setCommandIndex((index) => (index + (event.key === "ArrowDown" ? 1 : count - 1)) % count);
				queueMicrotask(() =>
					document.getElementById(`${commandMenuId}-${commandIndex()}`)?.scrollIntoView?.({ block: "nearest" }),
				);
				return;
			}
			const selected = matchingCommands()[commandIndex()];
			if (
				selected &&
				(event.key === "Tab" || (event.key === "Enter" && !event.shiftKey && commandText()?.[1] !== selected.name))
			) {
				event.preventDefault();
				chooseCommand(selected);
				return;
			}
		}
		if (event.key.toLowerCase() === "s" && event.ctrlKey && !event.metaKey && !event.altKey) {
			event.preventDefault();
			if (event.repeat) return;
			browserDictation.cancel();
			aiChatDraft.parkOrSwap();
			queueMicrotask(resize);
			return;
		}
		if (event.key !== "Enter" || event.shiftKey) return;
		event.preventDefault();
		send();
	};

	return (
		<div class={props.mobileAttachments ? s.mobileComposerContainer : s.composerCard}>
			<Show when={!props.mobileAttachments}>
				<div class={s.composerHeader}>
					<Show when={props.chat.operatingRepo?.()}>
						{(repo) => (
							<div class={s.repoContext}>
								<span>
									{t("aiChat.repository", "Repository")} ·{" "}
									<strong title={repo()} style={{ color: getRepoTextColor(repo()) }}>
										{pathBasename(repo())}
									</strong>
								</span>
								<Button
									type="button"
									variant="ghost"
									shape="icon"
									aria-label={props.chat.pinnedRepo?.() ? "Unpin repository" : "Pin repository"}
									aria-pressed={!!props.chat.pinnedRepo?.()}
									disabled={!props.chat.sessionId()}
									onClick={() => props.chat.toggleRepoPin()}
								>
									<svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
										<path d="m10 1 5 5-2 1-2 3v3l-1 1-4-4-4 4-1-1 4-4-3-3 1-1h3l3-2 1-2Zm.5 2.5-.7 1.4L7 7H4l5 5V9l2.1-2.8 1.4-.7-2-2Z" />
									</svg>
								</Button>
							</div>
						)}
					</Show>
					<Button
						variant="ghost"
						shape="icon"
						aria-label={parkLabel()}
						title={parkLabel()}
						aria-pressed={!!aiChatDraft.parked()}
						disabled={!aiChatDraft.parked() && !hasContent()}
						onClick={() => aiChatDraft.parkOrSwap()}
					>
						<svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
							<path d="M3 1h6l4 4v10H3V1Zm1 1v12h8V6H8V2H4Zm5 .7V5h2.3L9 2.7ZM5 8h6v1H5V8Zm0 3h6v1H5v-1Z" />
						</svg>
					</Button>
				</div>
			</Show>
			<div
				ref={inputArea}
				class={props.mobileAttachments ? `${s.inputArea} ${s.mobileComposer} ${mobileInput.form}` : s.inputArea}
			>
				<Show when={props.chat.queuedPrompts().length > 0}>
					<div class={s.queueList} aria-label="Queued prompts">
						<div class={s.queueHeader}>
							Queued <span class={s.queueBadge}>{props.chat.queuedPrompts().length}</span>
						</div>
						<For each={props.chat.queuedPrompts()}>
							{(queued) => {
								const label = queued.summary;
								return (
									<div class={s.queueRow}>
										<span class={s.queueText}>{label}</span>
										<Button
											type="button"
											shape="icon"
											variant="ghost"
											class={s.queueCancel}
											aria-label={`Cancel queued prompt ${label}`}
											onClick={() => void props.chat.cancelQueued(queued.turnId)}
										>
											<svg width="12" height="12" viewBox="0 0 12 12" fill="currentColor" aria-hidden="true">
												<path d="M2.8 2l3.2 3.2L9.2 2l.8.8L6.8 6l3.2 3.2-.8.8L6 6.8 2.8 10l-.8-.8L5.2 6 2 2.8z" />
											</svg>
										</Button>
									</div>
								);
							}}
						</For>
					</div>
				</Show>
				<Show when={props.mobileAttachments}>
					<MobileComposerMenu
						uploading={uploading()}
						onAttach={(file) => void attachFile(file)}
						onPark={() => {
							browserDictation.cancel();
							aiChatDraft.parkOrSwap();
						}}
						parkLabel={parkLabel()}
						parkDisabled={!aiChatDraft.parked() && !hasContent()}
						onQueue={props.chat.busy() ? send : undefined}
						onStop={props.chat.busy() ? () => void props.chat.cancel() : undefined}
						queueDisabled={!hasContent()}
					/>
				</Show>
				<div class={s.inputBody}>
					<Show when={commandMenuOpen()}>
						<div id={commandMenuId} class={s.slashCommands} role="listbox" aria-label="Slash commands">
							<For each={matchingCommands()}>
								{(command, index) => (
									<button
										id={`${commandMenuId}-${index()}`}
										type="button"
										data-control-layout="row"
										role="option"
										aria-selected={index() === commandIndex()}
										class={s.slashCommand}
										onClick={() => chooseCommand(command)}
									>
										<strong>/{command.name}</strong>
										<span>{command.description}</span>
										<Show when={command.input?.hint}>
											<code>{command.input?.hint}</code>
										</Show>
										<Show when={command.unavailable}>
											<span>{command.unavailable}</span>
										</Show>
									</button>
								)}
							</For>
						</div>
					</Show>
					<Show when={commandNotice()}>
						<span class={s.commandNotice} role="status">
							{commandNotice()}
						</span>
					</Show>
					<Show when={aiChatDraft.images().length > 0}>
						<div class={s.imagePreviews}>
							<For each={aiChatDraft.images()}>
								{(image) => (
									<div class={s.imagePreview}>
										<img src={image.src} alt="Pasted image" />
										<Button
											type="button"
											shape="icon"
											variant="ghost"
											aria-label="Remove pasted image"
											onClick={() => aiChatDraft.removeImage(image)}
										>
											<svg width="12" height="12" viewBox="0 0 12 12" fill="currentColor" aria-hidden="true">
												<path d="M2.8 2l3.2 3.2L9.2 2l.8.8L6.8 6l3.2 3.2-.8.8L6 6.8 2.8 10l-.8-.8L5.2 6 2 2.8z" />
											</svg>
										</Button>
									</div>
								)}
							</For>
						</div>
					</Show>
					<Show when={aiChatDraft.files().length > 0 || uploading()}>
						<div class={s.filePreviews}>
							<For each={aiChatDraft.files()}>
								{(file) => (
									<span class={s.filePreview}>
										{file.name}
										<Button
											type="button"
											shape="icon"
											variant="ghost"
											aria-label={`Remove ${file.name}`}
											onClick={() => aiChatDraft.removeFile(file)}
										>
											×
										</Button>
									</span>
								)}
							</For>
							<Show when={uploading()}>
								<span class={s.filePreview} role="status">
									Uploading…
								</span>
							</Show>
						</div>
					</Show>
					<Show when={pasteError()}>
						<span class={s.pasteError} role="alert">
							{pasteError()}
						</span>
					</Show>
					<Show when={aiChatDraft.storageError()}>
						<span class={s.pasteError} role="alert">
							Browser storage is unavailable; reload may lose it.
						</span>
					</Show>
					<div class={props.mobileAttachments ? mobileInput.composer : s.composerActions}>
						<Textarea
							compact={!props.mobileAttachments}
							ref={textarea}
							data-control-layout="composer-editor"
							class={props.mobileAttachments ? `${s.textarea} ${mobileInput.input}` : s.textarea}
							autocorrect={props.mobileAttachments ? (keyboardCorrection.enabled() ? "on" : "off") : undefined}
							spellcheck={props.mobileAttachments ? keyboardCorrection.enabled() : undefined}
							autocapitalize={
								props.mobileAttachments ? (keyboardCorrection.enabled() ? "sentences" : "off") : undefined
							}
							aria-controls={commandMenuOpen() ? commandMenuId : undefined}
							aria-autocomplete="list"
							aria-activedescendant={commandMenuOpen() ? `${commandMenuId}-${commandIndex()}` : undefined}
							placeholder={
								props.mobileAttachments ? "Message ego..." : t("aiChat.messagePlaceholder", "Write a message…")
							}
							value={aiChatDraft.text()}
							onInput={(event) => {
								browserDictation.cancel();
								setCommandNotice(null);
								aiChatDraft.set(event.currentTarget.value);
								resize();
							}}
							onKeyDown={onKeyDown}
							onPaste={onPaste}
							rows={1}
						/>
						<Show when={!props.mobileAttachments}>
							<Show when={props.chat.busy()}>
								<Button
									type="button"
									variant="danger"
									geometry="stretch"
									title="Stop"
									onClick={() => void props.chat.cancel()}
								>
									Stop
								</Button>
							</Show>
							<Button
								variant="primary"
								geometry="stretch"
								aria-label={props.chat.busy() ? "Queue" : "Send"}
								title={props.chat.busy() ? "Queue" : "Send"}
								disabled={!hasContent()}
								onClick={send}
							>
								<ComposeSendIcon />
							</Button>
						</Show>
						<Show when={props.mobileAttachments}>
							<MobileDictationButton dictation={browserDictation} disabled={uploading()} />
						</Show>
					</div>
				</div>
				<Show when={props.mobileAttachments}>
					<Button
						type="button"
						shape="icon"
						variant="primary"
						aria-label="Send"
						title={props.chat.busy() ? "Queue" : "Send"}
						disabled={uploading() || !hasContent()}
						onClick={send}
					>
						<svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor">
							<path d="M2.01 21L23 12 2.01 3 2 10l15 2-15 2z" />
						</svg>
					</Button>
				</Show>
			</div>
			<div class={s.composerMeta}>
				<Show when={props.mobileAttachments && props.chat.operatingRepo?.()}>
					{(repo) => (
						<div class={s.repoContext}>
							<span style={{ color: getRepoTextColor(repo()) }}>{pathBasename(repo())}</span>
							<Button
								variant="ghost"
								aria-label={props.chat.pinnedRepo?.() ? "Unpin repository" : "Pin repository"}
								aria-pressed={!!props.chat.pinnedRepo?.()}
								disabled={!props.chat.sessionId()}
								onClick={() => props.chat.toggleRepoPin()}
							>
								· {props.chat.pinnedRepo?.() ? "pinned repository" : "follows the repository on screen"}
							</Button>
						</div>
					)}
				</Show>
				<Show when={!props.mobileAttachments ? props.chat.usage?.() : null}>
					{(usage) => (
						<span class={s.contextUsage} aria-label="Context usage">
							{t("aiChat.contextUsage", "Ctx {percent}% · {used}/{size} · ~{free} free", {
								percent: String(Math.round((usage().used / usage().size) * 100)),
								used: Intl.NumberFormat(undefined, { notation: "compact", maximumFractionDigits: 1 }).format(
									usage().used,
								),
								size: Intl.NumberFormat(undefined, { notation: "compact", maximumFractionDigits: 1 }).format(
									usage().size,
								),
								free: Intl.NumberFormat(undefined, { notation: "compact", maximumFractionDigits: 1 }).format(
									Math.max(0, usage().size - usage().used),
								),
							})}
							<Show when={usage().cost}>
								{(cost) => (
									<span>
										{" "}
										· {cost().currency} {cost().amount}
									</span>
								)}
							</Show>
						</span>
					)}
				</Show>
				<Show
					when={props.onSessionSettings}
					fallback={<span class={s.sessionSettingsSummary}>{sessionSummary(props.chat)}</span>}
				>
					<Button
						variant="ghost"
						size="compact"
						class={s.sessionSettingsSummary}
						aria-label="Change model, reasoning and mode"
						title={
							sessionSelectOptions(props.chat).length > 0
								? "Change model, reasoning and mode"
								: SESSION_SETTINGS_UNAVAILABLE
						}
						disabled={sessionSelectOptions(props.chat).length === 0}
						onClick={props.onSessionSettings}
					>
						{sessionSummary(props.chat)}
					</Button>
				</Show>
			</div>
		</div>
	);
};
