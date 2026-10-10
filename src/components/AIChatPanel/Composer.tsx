/**
 * Where a turn is written and stopped.
 *
 * Enter sends and Shift+Enter breaks the line, which is what every chat in this
 * app does. While a turn is running the same corner holds Stop instead of Send:
 * the next thing a person wants during a turn they regret is not a second turn.
 */

import { type Component, createEffect, createSignal, For, Show } from "solid-js";
import mobileInput from "../../mobile/components/CommandInput.module.css";
import { uploadAttachment } from "../../services/uploadAttachment";
import { pastedImageFiles } from "../../utils/pastedImage";
import { ComposePinIcon, ComposeSendIcon } from "../shared/ComposeActionIcons";
import actions from "../shared/ComposeActions.module.css";
import s from "./AIChatPanel.module.css";
import { aiChatDraft } from "./draft";
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
	const [pasteError, setPasteError] = createSignal<string | null>(null);
	const [uploading, setUploading] = createSignal(false);
	let textarea: HTMLTextAreaElement | undefined;
	let inputArea: HTMLDivElement | undefined;
	let fileInput: HTMLInputElement | undefined;
	const hasContent = () =>
		!!aiChatDraft.text().trim() || aiChatDraft.images().length > 0 || aiChatDraft.files().length > 0;
	const parkLabel = () =>
		!aiChatDraft.parked() ? "Park draft" : hasContent() ? "Swap parked draft" : "Restore parked draft";
	createEffect(() => aiChatDraft.activate(props.chat.sessionId() ?? ""));
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
		if (file.type.startsWith("image/")) {
			setPasteError(await stageImages([file]));
			return;
		}
		setUploading(true);
		try {
			await props.chat.ensureStarted();
			const connectionId = props.chat.connectionId();
			if (!connectionId) throw new Error("AI Chat is not connected");
			const receipt = await uploadAttachment(file, { kind: "acp", id: connectionId });
			aiChatDraft.stageFile({ name: file.name, path: receipt.path });
		} catch (error) {
			setPasteError(error instanceof Error ? error.message : String(error));
		} finally {
			setUploading(false);
			if (fileInput) fileInput.value = "";
		}
	};
	createEffect(() => {
		const shared = props.sharedFile;
		if (!shared) return;
		props.onSharedFileConsumed?.();
		void attachFile(shared);
	});

	const onPaste = (event: ClipboardEvent) => {
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
			aiChatDraft.parkOrSwap();
			queueMicrotask(resize);
			return;
		}
		if (event.key !== "Enter" || event.shiftKey) return;
		event.preventDefault();
		send();
	};

	const attachmentButton = () => (
		<button
			type="button"
			class={props.mobileAttachments ? mobileInput.attach : s.attachBtn}
			aria-label="Attach file"
			disabled={uploading()}
			onClick={() => fileInput?.click()}
		>
			<svg
				width="18"
				height="18"
				viewBox="0 0 24 24"
				fill="none"
				stroke="currentColor"
				stroke-width="2"
				aria-hidden="true"
			>
				<path d="M20 11.5l-8.6 8.6a5 5 0 0 1-7.1-7.1L13 4.3a3.5 3.5 0 0 1 5 5l-8.7 8.7a2 2 0 0 1-2.8-2.8l8-8" />
			</svg>
		</button>
	);

	return (
		<div
			ref={inputArea}
			class={props.mobileAttachments ? `${s.inputArea} ${s.mobileComposer} ${mobileInput.form}` : s.inputArea}
		>
			<Show when={props.mobileAttachments}>
				<input
					ref={fileInput}
					type="file"
					accept="image/*,application/pdf,text/*"
					aria-label="Choose attachment"
					hidden
					onChange={(event) => {
						const file = event.currentTarget.files?.[0];
						if (file) void attachFile(file);
					}}
				/>
			</Show>
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
									<button
										type="button"
										class={s.queueCancel}
										aria-label={`Cancel queued prompt ${label}`}
										onClick={() => void props.chat.cancelQueued(queued.turnId)}
									>
										<svg width="12" height="12" viewBox="0 0 12 12" fill="currentColor" aria-hidden="true">
											<path d="M2.8 2l3.2 3.2L9.2 2l.8.8L6.8 6l3.2 3.2-.8.8L6 6.8 2.8 10l-.8-.8L5.2 6 2 2.8z" />
										</svg>
									</button>
								</div>
							);
						}}
					</For>
				</div>
			</Show>
			<div class={s.inputBody}>
				<Show when={aiChatDraft.images().length > 0}>
					<div class={s.imagePreviews}>
						<For each={aiChatDraft.images()}>
							{(image) => (
								<div class={s.imagePreview}>
									<img src={image.src} alt="Pasted image" />
									<button type="button" aria-label="Remove pasted image" onClick={() => aiChatDraft.removeImage(image)}>
										<svg width="12" height="12" viewBox="0 0 12 12" fill="currentColor" aria-hidden="true">
											<path d="M2.8 2l3.2 3.2L9.2 2l.8.8L6.8 6l3.2 3.2-.8.8L6 6.8 2.8 10l-.8-.8L5.2 6 2 2.8z" />
										</svg>
									</button>
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
									<button type="button" aria-label={`Remove ${file.name}`} onClick={() => aiChatDraft.removeFile(file)}>
										×
									</button>
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
				<textarea
					ref={textarea}
					class={props.mobileAttachments ? mobileInput.input : s.textarea}
					placeholder={props.mobileAttachments ? "Message ego..." : "Ask ego about this repository"}
					value={aiChatDraft.text()}
					onInput={(event) => {
						aiChatDraft.set(event.currentTarget.value);
						resize();
					}}
					onKeyDown={onKeyDown}
					onPaste={onPaste}
					rows={1}
				/>
			</div>
			<Show when={props.mobileAttachments}>{attachmentButton()}</Show>
			<div class={props.mobileAttachments ? s.mobileComposerActions : s.composerActions}>
				<Show when={props.chat.busy()}>
					<button
						type="button"
						class={s.stopBtn}
						aria-label={props.mobileAttachments ? "Stop" : undefined}
						title="Stop"
						onClick={() => void props.chat.cancel()}
					>
						{props.mobileAttachments ? (
							<svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
								<path d="M6 6h12v12H6z" />
							</svg>
						) : (
							"Stop"
						)}
					</button>
				</Show>
				<button
					type="button"
					class={props.mobileAttachments ? s.parkBtn : actions.pinButton}
					classList={{
						[s.parkedDraft]: !!props.mobileAttachments && !!aiChatDraft.parked(),
						[actions.pinButtonActive]: !props.mobileAttachments && !!aiChatDraft.parked(),
					}}
					aria-label={parkLabel()}
					title={parkLabel()}
					aria-pressed={!!aiChatDraft.parked()}
					disabled={!aiChatDraft.parked() && !hasContent()}
					onClick={() => aiChatDraft.parkOrSwap()}
				>
					{props.mobileAttachments ? (
						<svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
							<path d="M3 3h18v5H3zm2 7h14v11H5zm4 2v2h6v-2z" />
						</svg>
					) : (
						<ComposePinIcon />
					)}
				</button>
			</div>
			<button
				type="button"
				aria-label={props.chat.busy() ? "Queue" : "Send"}
				title={props.chat.busy() ? "Queue" : "Send"}
				class={props.mobileAttachments ? mobileInput.send : actions.sendButton}
				disabled={!aiChatDraft.text().trim() && aiChatDraft.images().length === 0 && aiChatDraft.files().length === 0}
				onClick={send}
			>
				{props.mobileAttachments ? (
					<svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor">
						<path d="M2.01 21L23 12 2.01 3 2 10l15 2-15 2z" />
					</svg>
				) : (
					<ComposeSendIcon />
				)}
			</button>
		</div>
	);
};
