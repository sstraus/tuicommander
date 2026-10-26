import { createEffect, createSignal, onCleanup, Show } from "solid-js";
import { t } from "../../i18n";
import { dictationStore, playbackState } from "../../stores/dictation";
import { Button } from "../ui/Button";
import styles from "./DictationToast.module.css";
import { handsFreePhaseLabel } from "./handsFreePhaseLabel";

// Centered spectrum-style meter: an odd number of bars so one sits dead
// center. Heights taper toward the edges, and outer bars only rise once the
// level clears their distance — so the silhouette visibly widens outward from
// the center as the mic gets louder. It is cosmetic (driven by a single RMS
// level, not a real FFT), so all bars share one source; the shape carries it.
const BAR_COUNT = 15;
const CENTER = (BAR_COUNT - 1) / 2;
const MIN_BAR_PX = 2;
const MAX_BAR_PX = 16;

/** Height fraction (0..1) for a bar `d` (normalized 0..1) from the center. */
function barFraction(d: number, level: number): number {
	const denom = 1 - d * 0.6;
	return Math.max(0, Math.min(1, (level - d * 0.6) / denom));
}

/**
 * Floating toast that shows partial transcription results during streaming
 * dictation, and the meter plus phase for as long as a hands-free
 * conversation is armed. Positioned above the status bar; hides when neither
 * is active.
 */
export function DictationToast() {
	const [visible, setVisible] = createSignal(false);
	const [exiting, setExiting] = createSignal(false);
	const handsFreeArmed = () => dictationStore.state.handsFree?.armed === true;
	/** A replies-only conversation holds no microphone: its meter would be a flat line. */
	const listeningConversation = () => handsFreeArmed() && dictationStore.state.handsFree?.listening !== false;
	const playback = () => playbackState(dictationStore.state.speech);
	const active = () =>
		dictationStore.state.recording || listeningConversation() || (handsFreeArmed() && playback() !== "idle");
	const spokenReplies = () => dictationStore.state.spokenReplies;

	/** What the toast says when no partial transcript is showing. */
	const statusText = () => {
		if (!listeningConversation() && playback() === "idle") return "Listening";
		if (playback() === "paused") return t("dictation.phasePaused", "Paused");
		if (playback() === "speaking") return t("dictation.phaseSpeaking", "Speaking");
		return handsFreePhaseLabel(dictationStore.state.handsFree?.phase);
	};

	// Show the preview as soon as capture starts, so the meter confirms input
	// before Whisper has produced its first partial transcription.
	createEffect(() => {
		if (active() || dictationStore.state.partialText) {
			setExiting(false);
			setVisible(true);
		}
	});

	// Auto-hide when recording stops and no conversation is armed
	createEffect(() => {
		if (!active() && visible()) {
			setExiting(true);
			const timer = setTimeout(() => {
				setVisible(false);
				setExiting(false);
			}, 150); // match fadeOut duration
			onCleanup(() => clearTimeout(timer));
		}
	});

	return (
		<Show when={visible()}>
			<div class={styles.toast} data-exiting={exiting()}>
				<Show
					when={listeningConversation()}
					fallback={
						<>
							<span class={styles.indicator} />
							<span
								class={styles.meter}
								role="meter"
								aria-label={`Microphone level ${Math.round(dictationStore.state.audioLevel * 100)}%`}
								aria-valuemin="0"
								aria-valuemax="100"
								aria-valuenow={Math.round(dictationStore.state.audioLevel * 100)}
							>
								{Array.from({ length: BAR_COUNT }, (_, index) => {
									const d = Math.abs(index - CENTER) / CENTER;
									return (
										<span
											class={styles.bar}
											classList={{ [styles.barActive]: barFraction(d, dictationStore.state.audioLevel) > 0.05 }}
											style={{
												height: `${MIN_BAR_PX + barFraction(d, dictationStore.state.audioLevel) * (MAX_BAR_PX - MIN_BAR_PX)}px`,
											}}
										/>
									);
								})}
							</span>
						</>
					}
				>
					{/* A conversation stays open for minutes, so its meter is one calm
					    level line: no pulsing dot, no animated ellipsis. The store
					    already gates the room noise and eases the fall. */}
					<span
						class={styles.voiceMeter}
						role="meter"
						aria-label={`Voice level ${Math.round(dictationStore.state.audioLevel * 100)}%`}
						aria-valuemin="0"
						aria-valuemax="100"
						aria-valuenow={Math.round(dictationStore.state.audioLevel * 100)}
					>
						<span class={styles.voiceFill} style={{ transform: `scaleX(${dictationStore.state.audioLevel})` }} />
					</span>
				</Show>
				<span class={styles.text}>
					{dictationStore.state.partialText || statusText()}
					<Show when={!listeningConversation()}>
						<span class={styles.dots} />
					</Show>
				</span>
				<Show when={handsFreeArmed() && playback() !== "idle"}>
					<span class={styles.controls}>
						<Show
							when={playback() === "paused"}
							fallback={
								<Button
									shape="icon"
									variant="ghost"
									type="button"
									class={styles.control}
									aria-label={t("dictation.pauseReply", "Pause reply")}
									onClick={() => void dictationStore.pauseSpeech()}
								>
									<svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor" aria-hidden="true">
										<rect x="3" y="2" width="3.5" height="12" rx="1" />
										<rect x="9.5" y="2" width="3.5" height="12" rx="1" />
									</svg>
								</Button>
							}
						>
							<Button
								shape="icon"
								variant="ghost"
								type="button"
								class={styles.control}
								aria-label={t("dictation.resumeReply", "Resume reply")}
								onClick={() => void dictationStore.resumeSpeech()}
							>
								<svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor" aria-hidden="true">
									<path d="M4 2.5v11a.5.5 0 0 0 .76.43l9-5.5a.5.5 0 0 0 0-.86l-9-5.5A.5.5 0 0 0 4 2.5z" />
								</svg>
							</Button>
						</Show>
						<Button
							shape="icon"
							variant="ghost"
							type="button"
							class={styles.control}
							aria-label={t("dictation.stopReply", "Stop reply")}
							onClick={() => void dictationStore.stopSpeech()}
						>
							<svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor" aria-hidden="true">
								<rect x="3" y="3" width="10" height="10" rx="1.5" />
							</svg>
						</Button>
					</span>
				</Show>
				<Show when={handsFreeArmed()}>
					<Button
						shape="icon"
						variant="ghost"
						type="button"
						class={styles.control}
						classList={{ [styles.controlOff]: !spokenReplies() }}
						aria-label={t("dictation.spokenRepliesToggle", "Spoken replies")}
						aria-pressed={spokenReplies()}
						title={
							spokenReplies()
								? t("dictation.spokenRepliesOnTitle", "Spoken replies on: click to turn off")
								: t("dictation.spokenRepliesOffTitle", "Spoken replies off: click to turn on")
						}
						onClick={() => dictationStore.setSpokenReplies(!spokenReplies())}
					>
						<svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor" aria-hidden="true">
							<path d="M2 6h2.5L8 3v10L4.5 10H2z" />
							<Show
								when={spokenReplies()}
								fallback={
									<path d="M10.5 6.2l3 3.6m0-3.6l-3 3.6" stroke="currentColor" stroke-width="1.4" fill="none" />
								}
							>
								<path d="M10.2 5.8a3.2 3.2 0 0 1 0 4.4" stroke="currentColor" stroke-width="1.4" fill="none" />
								<path d="M11.9 4.2a5.5 5.5 0 0 1 0 7.6" stroke="currentColor" stroke-width="1.4" fill="none" />
							</Show>
						</svg>
					</Button>
				</Show>
			</div>
		</Show>
	);
}
