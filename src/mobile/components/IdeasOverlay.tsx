import { createSignal, For, Show } from "solid-js";
import { Button, Textarea } from "../../components/ui";
import { appLogger } from "../../stores/appLogger";
import { ideasStore } from "../../stores/ideas";
import { rpc } from "../../transport";
import { sendCommand } from "../../utils/sendCommand";
import { formatRelativeTime } from "../../utils/time";
import { useBrowserDictation } from "../useBrowserDictation";
import { retryWrite } from "../utils/retryWrite";
import styles from "./IdeasOverlay.module.css";
import { MobileDictationButton } from "./MobileDictationButton";

interface IdeasOverlayProps {
	sessionId: string;
	repoPath: string | null;
	microphoneBlocked?: boolean;
	onDismiss: () => void;
}

export function IdeasOverlay(props: IdeasOverlayProps) {
	const [inputText, setInputText] = createSignal("");
	let inputEl: HTMLTextAreaElement | undefined;
	const browserDictation = useBrowserDictation({
		sessionId: () => props.sessionId,
		blocked: () => !!props.microphoneBlocked,
		getDraft: inputText,
		getSelection: () => ({ start: inputEl?.selectionStart ?? 0, end: inputEl?.selectionEnd ?? 0 }),
		setDraft: setInputText,
	});

	const notes = () => ideasStore.getFilteredIdeas(props.repoPath);

	function handleBackdrop(e: MouseEvent) {
		if (e.target === e.currentTarget) props.onDismiss();
	}

	function handleSubmit() {
		const text = inputText().trim();
		if (!text) return;
		browserDictation.cancel();
		ideasStore.addIdea(text, props.repoPath, deriveDisplayName(props.repoPath));
		setInputText("");
	}

	function handleKeyDown(e: KeyboardEvent) {
		if (e.key === "Enter" && !e.shiftKey) {
			e.preventDefault();
			handleSubmit();
		}
		if (e.key === "Escape") {
			props.onDismiss();
		}
	}

	async function handleSend(note: { id: string; text: string }) {
		props.onDismiss();
		ideasStore.markUsed(note.id);
		try {
			// Route through the canonical sendCommand helper (split Enter for Ink
			// raw mode, bracketed-paste for multi-line, Windows-native Ctrl-U skip).
			await sendCommand(
				(data) => retryWrite(() => rpc("write_pty", { sessionId: props.sessionId, data })),
				note.text,
				null,
				undefined,
				true,
				props.sessionId,
			);
		} catch (err) {
			appLogger.error("network", `Ideas send failed: ${err instanceof Error ? err.message : String(err)}`);
		}
	}

	return (
		<div class={styles.backdrop} onClick={handleBackdrop}>
			<div class={styles.sheet}>
				<div class={styles.header}>
					<span class={styles.title}>
						Ideas
						<Show when={notes().length > 0}>
							<span class={styles.badge}>{notes().length}</span>
						</Show>
					</span>
					<Button shape="icon" aria-label="Close ideas" variant="ghost" onClick={props.onDismiss}>
						<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
							<line x1="18" y1="6" x2="6" y2="18" />
							<line x1="6" y1="6" x2="18" y2="18" />
						</svg>
					</Button>
				</div>

				<div class={styles.list}>
					<Show when={notes().length === 0}>
						<div class={styles.empty}>No ideas yet. Add one below.</div>
					</Show>
					<For each={notes()}>
						{(note) => (
							<div class={styles.item} classList={{ [styles.itemUsed]: !!note.usedAt }}>
								<div class={styles.itemBody}>
									<span class={styles.itemText}>
										{note.usedAt ? "\u2713 " : ""}
										{note.text}
									</span>
									<div class={styles.itemMeta}>
										<span class={styles.itemDate}>
											{formatRelativeTime(note.createdAt, { showDateFallback: true })}
										</span>
										<Show when={note.repoDisplayName}>
											<span class={styles.itemProject}>{note.repoDisplayName}</span>
										</Show>
									</div>
								</div>
								<div class={styles.itemActions}>
									<Button
										variant="ghost"
										class={styles.sendBtn}
										onClick={() => handleSend(note)}
										shape="icon"
										aria-label="Send to terminal"
										title="Send to terminal"
									>
										<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
											<line x1="22" y1="2" x2="11" y2="13" />
											<polygon points="22 2 15 22 11 13 2 9 22 2" />
										</svg>
									</Button>
									<Button
										variant="ghost"
										class={styles.deleteBtn}
										onClick={() => ideasStore.removeIdea(note.id)}
										shape="icon"
										aria-label="Delete"
										title="Delete"
									>
										<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
											<line x1="18" y1="6" x2="6" y2="18" />
											<line x1="6" y1="6" x2="18" y2="18" />
										</svg>
									</Button>
								</div>
							</div>
						)}
					</For>
				</div>

				<div class={styles.inputArea}>
					<Textarea
						autoGrow
						ref={inputEl}
						class={styles.input}
						rows={1}
						placeholder="Type an idea..."
						value={inputText()}
						onInput={(e) => {
							browserDictation.cancel();
							setInputText(e.currentTarget.value);
						}}
						onKeyDown={handleKeyDown}
					/>
					<MobileDictationButton dictation={browserDictation} disabled={props.microphoneBlocked} />
					<Button
						shape="icon"
						aria-label="Add idea"
						variant="primary"
						onClick={handleSubmit}
						disabled={!inputText().trim()}
					>
						+
					</Button>
				</div>
			</div>
		</div>
	);
}

function deriveDisplayName(repoPath: string | null): string | null {
	if (!repoPath) return null;
	return repoPath.split("/").filter(Boolean).pop() ?? repoPath;
}
