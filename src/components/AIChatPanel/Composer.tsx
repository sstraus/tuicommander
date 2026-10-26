/**
 * Where a turn is written and stopped.
 *
 * Enter sends and Shift+Enter breaks the line, which is what every chat in this
 * app does. While a turn is running the same corner holds Stop instead of Send:
 * the next thing a person wants during a turn they regret is not a second turn.
 */

import { type Component, createEffect, createSignal, For, on, Show } from "solid-js";
import mobileInput from "../../mobile/components/CommandInput.module.css";
import { MobileComposerMenu } from "../../mobile/components/MobileComposerMenu";
import { MobileDictationButton } from "../../mobile/components/MobileDictationButton";
import { useBrowserDictation } from "../../mobile/useBrowserDictation";
import { useKeyboardCorrection } from "../../mobile/useKeyboardCorrection";
import { uploadAttachment } from "../../services/uploadAttachment";
import { pastedImageFiles } from "../../utils/pastedImage";
import { pathBasename } from "../../utils/pathUtils";
import { getRepoTextColor } from "../../utils/repoColor";
import { ComposePinIcon, ComposeSendIcon } from "../shared/ComposeActionIcons";
import { Button } from "../ui/Button";
import { Textarea } from "../ui/Textarea";
import s from "./AIChatPanel.module.css";
import { aiChatDraft } from "./draft";
import { sessionSummary } from "./SessionControls";
import type { AcpChat } from "./useAcpChat";

const MIN_HEIGHT_PX = 36;
const MAX_PANEL_FRACTION = 0.4;
/** Used while the panel has no layout yet (hidden or not mounted). */
const FALLBACK_CAP_PX = 150;

export const Composer: Component<{
	chat: AcpChat;
	mobileAttachments?: boolean;
	sharedFile?: File | null;
	onSharedFileConsumed?: () => void;
}> = (props) => {
	const keyboardCorrection = useKeyboardCorrection();
	const [pasteError, setPasteError] = createSignal<string | null>(null);
	const [uploading, setUploading] = createSignal(false);
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
			},
		),
	);
	const resize = () => {
		if (!textarea || props.mobileAttachments) return;
		const previous = textarea.offsetHeight;
		textarea.style.height = "auto";
		const natural = textarea.scrollHeight;
		const panelHeight = inputArea?.parentElement?.clientHeight ?? 0;
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
		if (props.mobileAttachments && uploading()) return;
		browserDictation.cancel();
		const text = aiChatDraft.expandedText();
		const images = aiChatDraft.images().map((image) => image.block);
		const files = aiChatDraft.files();
		if (!text.trim() && images.length === 0 && files.length === 0) return;
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
			const cursor = aiChatDraft.stageTextPaste(value, textarea.selectionStart, textarea.selectionEnd);
			if (cursor === null) return;
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
		<>
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
							ref={textarea}
							data-control-layout="composer-editor"
							class={props.mobileAttachments ? `${s.textarea} ${mobileInput.input}` : s.textarea}
							autocorrect={props.mobileAttachments ? (keyboardCorrection.enabled() ? "on" : "off") : undefined}
							spellcheck={props.mobileAttachments ? keyboardCorrection.enabled() : undefined}
							autocapitalize={
								props.mobileAttachments ? (keyboardCorrection.enabled() ? "sentences" : "off") : undefined
							}
							placeholder={props.mobileAttachments ? "Message ego..." : "Ask ego about this repository"}
							value={aiChatDraft.text()}
							onInput={(event) => {
								browserDictation.cancel();
								aiChatDraft.set(event.currentTarget.value);
								resize();
							}}
							onKeyDown={onKeyDown}
							onPaste={onPaste}
							rows={1}
						/>
						<Show when={props.mobileAttachments}>
							<MobileDictationButton dictation={browserDictation} disabled={uploading()} />
						</Show>
					</div>
				</div>
				<div class={props.mobileAttachments ? s.mobileComposerActions : s.composerActions}>
					<Show when={props.chat.busy() && !props.mobileAttachments}>
						<Button
							type="button"
							variant="danger"
							class={s.stopBtn}
							title="Stop"
							onClick={() => void props.chat.cancel()}
						>
							Stop
						</Button>
					</Show>
					<Show when={!props.mobileAttachments}>
						<Button
							variant="ghost"
							shape="icon"
							aria-label={parkLabel()}
							title={parkLabel()}
							aria-pressed={!!aiChatDraft.parked()}
							disabled={!aiChatDraft.parked() && !hasContent()}
							onClick={() => aiChatDraft.parkOrSwap()}
						>
							<ComposePinIcon />
						</Button>
					</Show>
				</div>
				<Show
					when={props.mobileAttachments}
					fallback={
						<Button
							variant="primary"
							shape="icon"
							aria-label={props.chat.busy() ? "Queue" : "Send"}
							title={props.chat.busy() ? "Queue" : "Send"}
							disabled={!hasContent()}
							onClick={send}
						>
							<ComposeSendIcon />
						</Button>
					}
				>
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
				<Show when={props.chat.operatingRepo?.()}>
					{(repo) => (
						<div class={s.repoContext}>
							<span style={{ color: getRepoTextColor(repo()) }}>{pathBasename(repo())}</span>
							<Button
								type="button"
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
				<span class={s.sessionSettingsSummary}>{sessionSummary(props.chat)}</span>
			</div>
		</>
	);
};
