import { appLogger } from "../stores/appLogger";
import { dictationStore } from "../stores/dictation";
import { toastsStore } from "../stores/toasts";
import { captureDictationTarget, type DictationTarget } from "./dictationTarget";

/** Transcription result from the Rust backend */
interface TranscribeResponse {
	text: string;
	skip_reason: string | null;
	duration_s: number;
	/** Seconds of speech the recording cap dropped — 0 for an ordinary recording. */
	truncated_s: number;
}

/** Dependencies injected into useDictation */
export interface DictationDeps {
	/** A detached chat has no active PTY: transcript focus still targets its composer. */
	fallbackInput?: () => HTMLTextAreaElement | null;
	pty: {
		write: (sessionId: string, data: string) => Promise<void>;
	};
	dictation: {
		state: {
			enabled: boolean;
			recording: boolean;
			processing: boolean;
			loading: boolean;
			modelStatus: "not_downloaded" | "downloaded" | "ready";
			/** Null before anything polled; see the store's own doc comment. */
			handsFree: { armed: boolean } | null;
		};
		refreshStatus: () => Promise<void>;
		startRecording: (source?: string) => Promise<void>;
		stopRecording: () => Promise<TranscribeResponse | null>;
		refreshHandsFree: () => Promise<void>;
		disarmHandsFree: () => Promise<void>;
	};
	setStatusInfo: (msg: string) => void;
	openSettings: (tab?: string) => void;
}

/** Dictation: start/stop recording, transcription, and terminal injection */
export function useDictation(deps: DictationDeps) {
	// Track the in-flight start promise so stop can await it.
	// Without this, a fast key release can fire handleDictationStop before
	// startRecording resolves, causing it to see recording=false and bail.
	let startPromise: Promise<boolean> | null = null;

	let recordingTarget: { id: string; target: DictationTarget } | null = null;
	let starting = false;

	const handleDictationStart = async (source = "hotkey") => {
		if (!deps.dictation.state.enabled || starting) return;
		const captured = { id: crypto.randomUUID(), target: captureDictationTarget(deps.fallbackInput?.()) };
		starting = true;
		startPromise = (async () => {
			// The hotkey also stops hands-free; refresh before trusting its cached flag.
			await deps.dictation.refreshHandsFree();
			if (deps.dictation.state.handsFree?.armed) {
				await deps.dictation.disarmHandsFree();
				deps.setStatusInfo("Hands-free: stopped");
				return false;
			}
			if (deps.dictation.state.recording || deps.dictation.state.processing || deps.dictation.state.loading)
				return false;
			recordingTarget = captured;
			appLogger.info("dictation", "Dictation target captured", {
				recordingId: captured.id,
				targetKind: captured.target.kind,
				terminalId: captured.target.terminalId,
				sessionId: captured.target.sessionId,
			});

			// Sync state from Rust before proceeding — the store may be stale
			// (e.g. Rust still processing previous transcription)
			await deps.dictation.refreshStatus();

			// Re-check guards after refresh — Rust state is now authoritative
			if (deps.dictation.state.recording || deps.dictation.state.processing) {
				deps.setStatusInfo("Dictation: previous session still active");
				return false;
			}

			if (deps.dictation.state.modelStatus === "not_downloaded") {
				deps.setStatusInfo("Dictation: model not downloaded — open Settings > Voice");
				deps.openSettings("dictation");
				return false;
			}

			if (deps.dictation.state.modelStatus !== "ready") {
				deps.setStatusInfo("Dictation: loading model into memory (first use takes a moment)…");
			}

			try {
				await deps.dictation.startRecording(source);
				deps.setStatusInfo("Dictation: recording…");
				return true;
			} catch (err) {
				appLogger.error("dictation", "Dictation start failed", err);
				deps.setStatusInfo("Dictation: failed to start recording");
				return false;
			}
		})().finally(() => {
			starting = false;
		});

		await startPromise;
	};

	const handleDictationStop = async () => {
		// Wait for any in-flight start to finish before checking state
		if (startPromise) {
			// A refused overlapping start resolves false while the first recording is
			// still active, so the recording flag below decides, not the start result.
			await startPromise;
			startPromise = null;
		}

		if (!deps.dictation.state.recording) return;
		const captured = recordingTarget;
		recordingTarget = null;
		deps.setStatusInfo("Dictation: transcribing…");
		const response = await deps.dictation.stopRecording();

		if (!response) {
			deps.setStatusInfo("Dictation: transcription failed");
			return;
		}

		if (response.skip_reason) {
			deps.setStatusInfo(`Dictation: ${response.skip_reason}`);
			return;
		}

		const text = response.text.trim();
		if (!text) {
			deps.setStatusInfo("Dictation: no text recognized");
			return;
		}

		// A recording past the cap lost its beginning. Report it wherever the text
		// lands — otherwise a truncated transcription reads as a complete one.
		const doneMessage =
			response.truncated_s > 0
				? `Dictation: ${Math.round(response.truncated_s)}s of audio were lost before transcription`
				: "Ready";

		const metadata = {
			recordingId: captured?.id ?? null,
			targetKind: captured?.target.kind ?? "none",
			terminalId: captured?.target.terminalId ?? null,
			sessionId: captured?.target.sessionId ?? null,
			characterCount: text.length,
		};
		try {
			if (!captured) throw new Error("No captured dictation target");
			await captured.target.deliver(text, deps.pty.write, dictationStore.state.autoSend);
			appLogger.info("dictation", "Dictation delivered", { ...metadata, outcome: "delivered" });
			deps.setStatusInfo(doneMessage);
		} catch {
			// Never log the transcript or an exception that might contain it. The
			// sticky message and Messages bell retain it for manual recovery.
			appLogger.warn("dictation", "Dictation delivery failed; transcript retained", {
				...metadata,
				outcome: "retained",
			});
			toastsStore.add(
				"Dictation was not delivered",
				text,
				"error",
				false,
				undefined,
				0,
				undefined,
				captured?.target.sessionId ?? undefined,
				false,
			);
			toastsStore.addToBell(
				"Dictation was not delivered",
				text,
				"error",
				undefined,
				undefined,
				captured?.target.sessionId ?? undefined,
			);
			deps.setStatusInfo(
				captured?.target.kind === "none"
					? "Dictation: no active terminal"
					: "Dictation: target unavailable — transcript kept in Messages",
			);
		}
	};

	return {
		handleDictationStart,
		handleDictationStop,
	};
}
