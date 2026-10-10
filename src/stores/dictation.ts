import { createStore } from "solid-js/store";
import { invoke, listen } from "../invoke";
import { isTauri } from "../transport";
import { type BrowserVoiceSession, connectBrowserVoice } from "../utils/browserVoice";
import { browserVoiceMediaSession } from "../utils/browserVoiceMediaSession";
import { type Earcon, playEarcon, primeEarcons } from "../utils/earcon";
import { appLogger } from "./appLogger";
import { toastsStore } from "./toasts";

/** Dictation config persisted to ~/.tuicommander/dictation-config.json */
interface DictationConfig {
	enabled: boolean;
	hotkey: string;
	language: string;
	model: string;
	device: string | null;
	long_press_ms: number;
	auto_send: boolean;
	rms_threshold: number;
	no_speech_threshold: number;
	/** Hands-free hold-back before a transcript is enqueued. */
	hands_free_hold_back_ms: number;
	/** Hands-free activation phrase; empty means ungated. */
	hands_free_activation_phrase: string;
	/** Tell the bound model when hands-free starts and stops. On by default. */
	hands_free_notify_model: boolean;
	/** Start notice sent to the model; empty means the built-in text. */
	hands_free_start_notice: string;
	/** Play the hands-free earcons on the owning client. On by default. */
	hands_free_earcons: boolean;
	hands_free_spoken_replies: boolean;
	/** A user-supplied speech engine as argv, used when `speech_engine` is `external`. Edited in Expert. */
	speech_command: string[];
	/** Which engine speaks replies. Rust answers an unset value from what is installed, so this is never empty on read. */
	speech_engine: SpeechEngineId;
	/** The Edge voice (`it-IT-IsabellaNeural`). Empty means the language's default. */
	speech_edge_voice: string;
	/** Which of the language's voices speaks. Empty means the first one it ships. */
	speech_voice: string;
	/** Speech level of every reply in dBFS (-30..=-12). */
	speech_volume_db: number;
	/** Levelling strength within a reply: 0 is off, 1 is 4:1. */
	speech_levelling: number;
	/** The backend kept valid fields while replacing malformed config fields. */
	recovered_from_corruption?: boolean;
}

/**
 * A downloadable speech asset: the ONNX runtime, one language bundle, or one
 * catalogue voice of a language.
 *
 * Snake_case because `SpeechAssetInfo` carries no serde rename, unlike the
 * hands-free and speech status structs below it.
 */
export interface SpeechAsset {
	id: string;
	display_name: string;
	/** `"language"`, `"runtime"` or `"voice"`. */
	kind: string;
	/**
	 * The Whisper language code this speaks (`"it"`); null for the runtime
	 * library. The same alphabet as `DictationConfig.language`, so the two can
	 * be compared directly — which is the whole reason it is a code. For a
	 * voice, the code of the language it belongs to.
	 */
	language: string | null;
	voices: string[];
	/** The voice a `"voice"` asset downloads (`"jean"`); absent otherwise. */
	voice?: string | null;
	download_bytes: number;
	/** `"absent"`, `"downloading"`, `"incomplete"` or `"ready"`. */
	state: string;
	/** Which files an incomplete asset is missing. Empty otherwise. */
	missing: string[];
}

/** The engines that can speak a reply. Edge is the default; the other two are Expert. */
export type SpeechEngineId = "edge" | "pocket" | "external";

/** One Microsoft Edge voice, as the service lists it. Mirrors Rust's `EdgeVoice`. */
export interface EdgeVoice {
	id: string;
	locale: string;
	gender: string;
	label: string;
}

/** One voice a language can speak with. Mirrors Rust's `VoiceChoice`. */
export interface SpeechVoice {
	id: string;
	/** Shipped with the language, downloaded from the catalogue, or imported by the user. */
	source: "default" | "downloaded" | "user";
}

/** What the hands-free conversation is doing. Mirrors `Phase::as_wire`. */
export type HandsFreePhase =
	| "disarmed"
	| "waiting"
	| "capturing"
	| "transcribing"
	| "holding_back"
	| "delivered"
	| "error";

/** Live hands-free state, mirroring Rust's `HandsFreeStatus`. */
export interface HandsFreeStatus {
	armed: boolean;
	phase: HandsFreePhase;
	/** The bound terminal. Unchanged by focus for as long as it is set. */
	sessionId: string | null;
	/** The bound audio endpoint. */
	owner: string | null;
	generation: number;
	/**
	 * The transcript waiting out its hold-back, or held by a dialog or a draft
	 * in the composer, while there is still time to stop it.
	 */
	pendingText: string | null;
	holdBackMs: number;
	error: string | null;
	/** Spoken turns handed to the agent. Monotonic for the backend's life. */
	deliveredTurns: number;
	/** Turns the activation-phrase gate dropped. Monotonic like the above. */
	droppedTurns: number;
}

/** Whether a reply can be spoken, and what the speaker is doing. */
export interface SpeechStatus {
	available: boolean;
	unavailableReason: string;
	sessionId: string | null;
	language: string;
	turn: number;
	voice: string;
	queued: number;
	/** Synthesis is running. */
	rendering: boolean;
	/** Audio is playing. */
	speaking: boolean;
	/** A reply is held mid-playback and `resume_speech` continues it; `speaking` is false meanwhile. */
	paused: boolean;
	lastError: string | null;
}

/** What the pill's playback controls show: pause + stop, play + stop, or nothing. */
export type PlaybackState = "idle" | "speaking" | "paused";

export function playbackState(speech: SpeechStatus | null): PlaybackState {
	if (speech?.paused) return "paused";
	return speech?.speaking ? "speaking" : "idle";
}

/**
 * One reply, and what became of it.
 *
 * Pushed on `speech-utterance` as the speaker moves it, on every transport.
 * `finished` and `interrupted` are decided on Rust's render thread long after
 * the call that queued the reply returned, so a client that waited for a return
 * value would have to poll for them instead.
 */
export interface SpokenReply {
	utteranceId: string;
	/** `queued`, `rendering`, `speaking`, `finished`, `interrupted` or `failed`. */
	state: string;
	/** Set only for `failed`. */
	error: string | null;
	/** The turn this reply belongs to. */
	turn: number;
}

/**
 * This machine's own microphone and speakers.
 *
 * Reserved: Rust opens the local devices for this name and looks up a
 * connected socket for every other one — see `DESKTOP_OWNER` in
 * `dictation/commands.rs`. A browser tab must therefore arm under a name of
 * its own, never this one.
 */
export const DESKTOP_AUDIO_OWNER = "desktop";

/**
 * This tab's audio owner name, for a conversation held in a browser.
 *
 * Per tab rather than per browser or per user: two tabs of the same app are
 * two microphones and two speakers, and a shared name would let one tab's
 * reply come out of the other's. Generated once per page load, because that is
 * exactly the lifetime of the socket it names.
 */
export const browserAudioOwner = `browser-${Math.random().toString(36).slice(2, 10)}`;

/**
 * Which earcon, if any, the move from `previous` to `next` calls for.
 *
 * Keyed on the turn counters rather than the phase: the status is polled, a
 * `delivered` phase can be overwritten by the next utterance between two polls,
 * and a gate drop goes straight back to `waiting` with no phase of its own.
 * Only the client whose hardware the conversation uses plays it — a phone
 * holding the conversation must not make the desktop beep, nor the reverse.
 * No baseline, no sound: the first status a client reads is not news.
 */
export function turnEarcon(previous: HandsFreeStatus | null, next: HandsFreeStatus, owner: string): Earcon | null {
	if (!previous || !next.armed || next.owner !== owner) return null;
	if (next.deliveredTurns > previous.deliveredTurns) return "delivered";
	if (next.droppedTurns > previous.droppedTurns) return "dropped";
	return null;
}

/** Whisper's own no_speech_thold default, mirrored from `transcribe.rs`. */
export const DEFAULT_NO_SPEECH_THRESHOLD = 0.6;

/** The historical hardcoded RMS floor, mirrored from `transcribe.rs`. */
export const DEFAULT_RMS_THRESHOLD = 0.001;

/**
 * The hands-free hold-back default, mirrored from `default_hold_back_ms`.
 *
 * Long enough to read a transcript and stop it, short enough not to feel like
 * a delay. Only where the slider starts — Rust owns the number that is used.
 */
export const DEFAULT_HOLD_BACK_MS = 1500;

/**
 * Loudness defaults, mirrored from `DictationConfig::default()`. Only where the
 * sliders start before the config has loaded — Rust owns the numbers used.
 */
export const DEFAULT_SPEECH_VOLUME_DB = -18;
export const DEFAULT_SPEECH_LEVELLING = 0.67;

/** What "Listen" says in each bundled speech language. */
const VOICE_PREVIEW_TEXT: Record<string, string> = {
	en: "This is how replies will sound.",
	fr: "Voici comment les réponses sonneront.",
	de: "So werden Antworten klingen.",
	it: "Questa è la voce delle risposte.",
	pt: "É assim que as respostas vão soar.",
	es: "Así sonarán las respuestas.",
};

function voicePreviewText(language: string): string {
	return VOICE_PREVIEW_TEXT[language] ?? VOICE_PREVIEW_TEXT.en;
}

/** A file's bytes as base64, the form a voice file travels in over IPC and HTTP. */
async function fileToBase64(file: Blob): Promise<string> {
	const bytes = new Uint8Array(await file.arrayBuffer());
	let binary = "";
	for (let i = 0; i < bytes.length; i += 0x8000) {
		binary += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
	}
	return btoa(binary);
}

/** GPU/CPU backend reported by whisper after model load. */
export type DictationBackend = "cpu" | "gpu";

/** Model info from Rust backend */
export interface ModelInfo {
	name: string;
	display_name: string;
	size_hint_mb: number;
	downloaded: boolean;
	actual_size_mb: number;
}

/** Marker of the backend refusal when another instance owns dictation (`dictation::ownership`). */
const OWNED_ELSEWHERE_MARKER = "owned by another TUICommander instance";

/** Model status values from Rust backend */
type ModelStatus = "not_downloaded" | "downloaded" | "ready";

/** Model status from Rust backend */
interface DictationStatus {
	model_status: ModelStatus;
	model_name: string;
	model_size_mb: number;
	recording: boolean;
	processing: boolean;
	audio_level?: number;
	/** Another instance holds dictation for this config directory. */
	owned_elsewhere?: boolean;
}

/** Transcription response from Rust backend */
interface TranscribeResponse {
	text: string;
	skip_reason: string | null;
	duration_s: number;
	/** Seconds of speech the recording cap dropped — 0 for an ordinary recording. */
	truncated_s: number;
}

/** Audio device from Rust backend */
interface AudioDevice {
	name: string;
	is_default: boolean;
}

/** Download progress event payload */
interface DownloadProgress {
	downloaded: number;
	total: number;
	percent: number;
}

function normalizeAudioLevel(value: number | undefined): number {
	return Number.isFinite(value) ? Math.max(0, Math.min(1, value as number)) : 0;
}

// The hands-free meter shows the voice, not the room. Below the floor sits
// fan and keyboard noise (the reported level is sqrt(rms * 20), so 0.35 is an
// rms of ~0.006, under the 0.01 the segmenter treats as speech). Above it the
// needle rises at once and falls back slowly, so a sentence reads as one
// steady swell instead of a flicker per poll.
const METER_NOISE_FLOOR = 0.35;
const METER_RELEASE = 0.8;

/** Next hands-free meter value from the previous one and a raw level. */
export function easeMeterLevel(previous: number, raw: number | undefined): number {
	const voice = Math.max(0, (normalizeAudioLevel(raw) - METER_NOISE_FLOOR) / (1 - METER_NOISE_FLOOR));
	const released = previous * METER_RELEASE;
	return voice > released ? voice : released < 0.01 ? 0 : released;
}

/** Supported languages for Whisper */
export const WHISPER_LANGUAGES: Record<string, string> = {
	auto: "Auto-detect",
	en: "English",
	es: "Spanish",
	fr: "French",
	de: "German",
	it: "Italian",
	pt: "Portuguese",
	nl: "Dutch",
	ja: "Japanese",
	zh: "Chinese",
	ko: "Korean",
	ru: "Russian",
};

/** Store state */
interface DictationStoreState {
	enabled: boolean;
	hotkey: string;
	language: string;
	selectedModel: string;
	selectedDevice: string | null;
	models: ModelInfo[];
	modelStatus: ModelStatus;
	modelName: string;
	modelSizeMb: number;
	recording: boolean;
	processing: boolean;
	/** Another TUICommander instance owns dictation; this one cannot record. */
	ownedElsewhere: boolean;
	loading: boolean; // Model is being loaded into memory on first use
	downloading: boolean;
	downloadPercent: number;
	corrections: Record<string, string>;
	devices: AudioDevice[];
	longPressMs: number;
	autoSend: boolean;
	/**
	 * Whether arming and disarming hands-free tell the bound model so.
	 *
	 * Defaults to true, mirroring the Rust default: the voice tool is offered
	 * whether or not this is set, and a model with no reason to speak answers
	 * in text.
	 */
	notifyModelOnHandsFree: boolean;
	/**
	 * The hands-free start notice as saved. Empty means the built-in text,
	 * which `getDefaultHandsFreeStartNotice` returns.
	 */
	handsFreeStartNotice: string;
	/** Hold-back between a hands-free transcript and its enqueue, in ms. */
	handsFreeHoldBackMs: number;
	/** Phrase that must open each hands-free turn; empty means ungated. */
	handsFreeActivationPhrase: string;
	/** Whether the delivered/dropped earcons play. Defaults to true, as in Rust. */
	handsFreeEarcons: boolean;
	spokenReplies: boolean;
	/** Which engine speaks replies. */
	speechEngine: SpeechEngineId;
	/** The external speech command as argv; empty when none is set. */
	speechCommand: string[];
	/** The Edge voice. Empty means the language's default, decided in Rust. */
	speechEdgeVoice: string;
	/** The service's voices for the language last asked for (`refreshEdgeVoices`). */
	edgeVoices: EdgeVoice[];
	/** Why the Edge voice list could not be read (offline, service down), or null. */
	edgeVoicesError: string | null;
	/** Which Pocket voice speaks. Empty means the language's first, decided in Rust. */
	speechVoice: string;
	/** Speech level of every reply in dBFS (-30..=-12). */
	speechVolumeDb: number;
	/** Levelling strength within a reply: 0 is off, 1 is strongest. */
	speechLevelling: number;
	/** The speech catalogue and what state each entry is in. */
	speechAssets: SpeechAsset[];
	/** The voices of the language last asked for (`refreshSpeechVoices`). */
	speechVoices: SpeechVoice[];
	/** Download percent per asset id, present only while one is downloading. */
	speechDownloads: Record<string, number | undefined>;
	/**
	 * Live hands-free state, or null before anything has polled for it.
	 *
	 * Null is not "disarmed": nothing here ever arms by itself, and the
	 * difference between "not asked yet" and "asked, and nothing is armed"
	 * decides whether the panel may show a phase at all.
	 */
	handsFree: HandsFreeStatus | null;
	/** What the last arm or disarm refused to do, cleared by the next one. */
	handsFreeError: string | null;
	/** Live speaker state, or null before anything has polled for it. */
	speech: SpeechStatus | null;
	/**
	 * The most recent reply the backend pushed, or null before any.
	 *
	 * Beside `speech` rather than merged into it: `speech` is a snapshot of the
	 * whole queue that only a poll can produce, while this is one reply moving.
	 * Merging them would have a single utterance overwrite a queue depth it
	 * knows nothing about.
	 */
	utterance: SpokenReply | null;
	rmsThreshold: number;
	noSpeechThreshold: number;
	capturingHotkey: boolean;
	partialText: string;
	/** Normalized live microphone level used by the dictation preview meter. */
	audioLevel: number;
	backendInfo: DictationBackend | null;
	/**
	 * Why the last recording produced no text, or null when it produced some.
	 *
	 * Tuning the two thresholds is guesswork without it: a gate that rejects
	 * speech and a microphone that captured nothing look identical from the
	 * outside. Settings > Voice renders this verbatim.
	 */
	lastSkipReason: string | null;
}

function createDictationStore() {
	/**
	 * This tab's audio socket while it owns a conversation, `null` otherwise.
	 *
	 * Outside the store because it is not rendered and not serialisable: it is
	 * an open microphone and an open WebSocket, and putting it in reactive
	 * state would make every subscriber re-run when a socket opened.
	 */
	let browserVoice: BrowserVoiceSession | null = null;
	let nowPlaying: ReturnType<typeof browserVoiceMediaSession> = null;
	const stopBrowserVoice = () => {
		nowPlaying?.stop();
		nowPlaying = null;
		browserVoice?.stop();
		browserVoice = null;
	};
	// A language switch may start a second request before the first answers.
	// Only the most recently requested voice list may update the picker.
	let speechVoicesRequest = 0;
	let edgeVoicesRequest = 0;

	const [state, setState] = createStore<DictationStoreState>({
		enabled: false,
		hotkey: "F5",
		language: "auto",
		selectedModel: "large-v3-turbo",
		selectedDevice: null,
		models: [],
		modelStatus: "not_downloaded",
		modelName: "",
		modelSizeMb: 0,
		recording: false,
		processing: false,
		ownedElsewhere: false,
		loading: false,
		downloading: false,
		downloadPercent: 0,
		corrections: {},
		devices: [],
		longPressMs: 400,
		autoSend: true,
		notifyModelOnHandsFree: true,
		handsFreeStartNotice: "",
		handsFreeHoldBackMs: DEFAULT_HOLD_BACK_MS,
		handsFreeActivationPhrase: "",
		handsFreeEarcons: true,
		spokenReplies: true,
		speechEngine: "edge",
		speechCommand: [],
		speechEdgeVoice: "",
		edgeVoices: [],
		edgeVoicesError: null,
		speechVoice: "",
		speechVolumeDb: DEFAULT_SPEECH_VOLUME_DB,
		speechLevelling: DEFAULT_SPEECH_LEVELLING,
		speechAssets: [],
		speechVoices: [],
		speechDownloads: {},
		handsFree: null,
		handsFreeError: null,
		speech: null,
		utterance: null,
		rmsThreshold: DEFAULT_RMS_THRESHOLD,
		noSpeechThreshold: DEFAULT_NO_SPEECH_THRESHOLD,
		capturingHotkey: false,
		partialText: "",
		audioLevel: 0,
		backendInfo: null,
		lastSkipReason: null,
	});

	// Listen for download progress events from Rust
	listen<DownloadProgress>("dictation-download-progress", (event) => {
		setState("downloadPercent", event.payload.percent);
	});

	// Listen for streaming partial transcription results
	listen<string>("dictation-partial", (event) => {
		setState("partialText", event.payload);
	});

	// Per-asset speech download progress. Keyed by asset id because the runtime
	// library and a language are separate downloads a user can start together,
	// and one shared percent would show each of them the other's.
	// `done` ends the bar for every client, including one that did not start
	// the download and so has no `downloadSpeechAsset` to clear it.
	listen<{ asset: string; percent: number; done?: boolean }>("speech-download-progress", (event) => {
		if (event.payload.done) {
			setState("speechDownloads", event.payload.asset, undefined);
			void actions.refreshSpeechAssets();
			return;
		}
		setState("speechDownloads", event.payload.asset, event.payload.percent);
	});

	// What the speaker is doing right now, pushed rather than polled. The states
	// that matter — finished, interrupted — are decided on Rust's render thread
	// after the call that queued the reply returned, so there is nothing to
	// await and nothing would refresh the panel without this.
	listen<SpokenReply>("speech-utterance", (event) => {
		setState("utterance", event.payload);
		// The queue snapshot beside it stays whatever the last poll said, except
		// for the two booleans this event settles for certain.
		setState("speech", (current) =>
			current
				? {
						...current,
						rendering: event.payload.state === "rendering",
						speaking: event.payload.state === "speaking",
						// Only a reply starting or ending settles this; a push for a
						// queued or rendering one says nothing about a held reply.
						paused: ["speaking", "finished", "interrupted", "failed"].includes(event.payload.state)
							? false
							: current.paused,
						lastError: event.payload.error ?? current.lastError,
					}
				: current,
		);
		nowPlaying?.update(state.speech?.paused === true);
	});

	let audioLevelTimer: ReturnType<typeof setInterval> | null = null;
	const stopAudioLevelPolling = () => {
		if (audioLevelTimer) clearInterval(audioLevelTimer);
		audioLevelTimer = null;
	};
	const startAudioLevelPolling = () => {
		stopAudioLevelPolling();
		audioLevelTimer = setInterval(() => {
			void invoke<DictationStatus>("get_dictation_status")
				.then((status) => setState("audioLevel", normalizeAudioLevel(status.audio_level)))
				.catch(() => stopAudioLevelPolling());
		}, 75);
	};

	// While a hands-free conversation is armed, the toast shows its meter and
	// phase wherever the user is, not only inside Settings. The level is
	// polled at the push-to-talk rate; phase and speaker state change on the
	// scale of an utterance, so every fifth tick is enough. Started and
	// stopped from `applyHandsFree`, the one place that stores the status.
	const HANDS_FREE_STATUS_EVERY = 5;
	let handsFreeTimer: ReturnType<typeof setInterval> | null = null;
	const stopHandsFreeMonitor = () => {
		if (handsFreeTimer) clearInterval(handsFreeTimer);
		handsFreeTimer = null;
	};
	const startHandsFreeMonitor = () => {
		if (handsFreeTimer) return;
		let tick = 0;
		handsFreeTimer = setInterval(() => {
			void invoke<DictationStatus>("get_dictation_status")
				.then((status) => setState("audioLevel", (previous) => easeMeterLevel(previous, status.audio_level)))
				.catch(() => {});
			tick += 1;
			if (tick % HANDS_FREE_STATUS_EVERY === 0) {
				void actions.refreshHandsFree();
				void actions.refreshSpeechStatus();
			}
		}, 75);
	};
	const speechControl = async (command: "pause_speech" | "resume_speech" | "stop_speech"): Promise<void> => {
		try {
			setState("speech", await invoke<SpeechStatus>(command));
			nowPlaying?.update(state.speech?.paused === true);
		} catch (err) {
			appLogger.warn("dictation", `${command} refused`, err);
			await actions.refreshSpeechStatus();
		}
	};

	const applyHandsFree = (status: HandsFreeStatus) => {
		const earcon = turnEarcon(state.handsFree, status, isTauri() ? DESKTOP_AUDIO_OWNER : browserAudioOwner);
		if (earcon && state.handsFreeEarcons) playEarcon(earcon);
		setState("handsFree", status);
		if (status.armed) {
			if (browserVoice && !nowPlaying) {
				nowPlaying = browserVoiceMediaSession({
					play: () => actions.resumeSpeech(),
					pause: () => actions.pauseSpeech(),
					stop: () => actions.disarmHandsFree(),
				});
			}
			startHandsFreeMonitor();
		} else {
			stopBrowserVoice();
			stopHandsFreeMonitor();
			setState("audioLevel", 0);
		}
	};

	// Listen for backend info (gpu/cpu) after model load
	listen<{ backend: DictationBackend }>("dictation-backend-info", (event) => {
		setState("backendInfo", event.payload.backend);
	});

	const actions = {
		/** Load config from Rust backend (file-based) */
		async refreshConfig(): Promise<void> {
			try {
				const config = await invoke<DictationConfig>("get_dictation_config");
				setState({
					enabled: config.enabled,
					hotkey: config.hotkey,
					language: config.language,
					selectedModel: config.model ?? "large-v3-turbo",
					selectedDevice: config.device ?? null,
					longPressMs: config.long_press_ms ?? 400,
					autoSend: config.auto_send ?? true,
					notifyModelOnHandsFree: config.hands_free_notify_model ?? true,
					handsFreeStartNotice: config.hands_free_start_notice ?? "",
					handsFreeHoldBackMs: config.hands_free_hold_back_ms ?? DEFAULT_HOLD_BACK_MS,
					handsFreeActivationPhrase: config.hands_free_activation_phrase ?? "",
					handsFreeEarcons: config.hands_free_earcons ?? true,
					spokenReplies: config.hands_free_spoken_replies ?? true,
					speechEngine: config.speech_engine || "edge",
					speechCommand: config.speech_command ?? [],
					speechEdgeVoice: config.speech_edge_voice ?? "",
					speechVoice: config.speech_voice ?? "",
					speechVolumeDb: config.speech_volume_db ?? DEFAULT_SPEECH_VOLUME_DB,
					speechLevelling: config.speech_levelling ?? DEFAULT_SPEECH_LEVELLING,
					rmsThreshold: config.rms_threshold ?? DEFAULT_RMS_THRESHOLD,
					noSpeechThreshold: config.no_speech_threshold ?? DEFAULT_NO_SPEECH_THRESHOLD,
				});
				if (config.recovered_from_corruption) {
					appLogger.warn("dictation", "Recovered valid dictation settings from malformed config fields");
					toastsStore.add(
						"Dictation settings recovered",
						"Some invalid settings were reset; valid settings were kept.",
						"warn",
					);
				}
			} catch (err) {
				appLogger.error("dictation", "Failed to get dictation config", err);
			}
		},

		/**
		 * Save a single config field to disk via Rust.
		 *
		 * Load a base snapshot and override only the explicitly changed fields.
		 * Rust applies that base-to-desired delta to the latest locked file.
		 */
		async saveConfig(partial: Partial<DictationConfig>): Promise<void> {
			try {
				// A save that cannot read first is abandoned: writing a config
				// assembled from defaults is how the fields below got lost.
				// "Could not read" includes an answer that is not a config —
				// the fields below are read off it by name, and a save built on
				// nothing is the very thing this guard exists to stop.
				const stored = await invoke<DictationConfig>("get_dictation_config");
				if (!stored || typeof stored !== "object") {
					appLogger.error("dictation", "Refusing to save: the stored config could not be read");
					return;
				}
				const config: DictationConfig = { ...stored, ...partial };
				await invoke("set_dictation_config", { base: stored, config });
				// Map DictationConfig fields to DictationStoreState fields
				const storeUpdate: Partial<DictationStoreState> = {};
				if (partial.enabled !== undefined) storeUpdate.enabled = partial.enabled;
				if (partial.hotkey !== undefined) storeUpdate.hotkey = partial.hotkey;
				if (partial.language !== undefined) storeUpdate.language = partial.language;
				if (partial.model !== undefined) storeUpdate.selectedModel = partial.model;
				if (partial.device !== undefined) storeUpdate.selectedDevice = partial.device;
				if (partial.long_press_ms !== undefined) storeUpdate.longPressMs = partial.long_press_ms;
				if (partial.auto_send !== undefined) storeUpdate.autoSend = partial.auto_send;
				if (partial.hands_free_notify_model !== undefined)
					storeUpdate.notifyModelOnHandsFree = partial.hands_free_notify_model;
				if (partial.hands_free_hold_back_ms !== undefined)
					storeUpdate.handsFreeHoldBackMs = partial.hands_free_hold_back_ms;
				if (partial.hands_free_activation_phrase !== undefined)
					storeUpdate.handsFreeActivationPhrase = partial.hands_free_activation_phrase;
				if (partial.hands_free_start_notice !== undefined)
					storeUpdate.handsFreeStartNotice = partial.hands_free_start_notice;
				if (partial.speech_voice !== undefined) storeUpdate.speechVoice = partial.speech_voice;
				if (partial.speech_engine !== undefined) storeUpdate.speechEngine = partial.speech_engine;
				if (partial.speech_command !== undefined) storeUpdate.speechCommand = partial.speech_command;
				if (partial.speech_edge_voice !== undefined) storeUpdate.speechEdgeVoice = partial.speech_edge_voice;
				if (partial.speech_volume_db !== undefined) storeUpdate.speechVolumeDb = partial.speech_volume_db;
				if (partial.speech_levelling !== undefined) storeUpdate.speechLevelling = partial.speech_levelling;
				if (partial.hands_free_spoken_replies !== undefined)
					storeUpdate.spokenReplies = partial.hands_free_spoken_replies;
				if (partial.hands_free_earcons !== undefined) storeUpdate.handsFreeEarcons = partial.hands_free_earcons;
				if (partial.rms_threshold !== undefined) storeUpdate.rmsThreshold = partial.rms_threshold;
				if (partial.no_speech_threshold !== undefined) storeUpdate.noSpeechThreshold = partial.no_speech_threshold;
				setState(storeUpdate);
			} catch (err) {
				appLogger.error("dictation", "Failed to save dictation config", err);
			}
		},

		setEnabled(value: boolean): void {
			actions.saveConfig({ enabled: value });
		},

		setHotkey(value: string): void {
			actions.saveConfig({ hotkey: value });
		},

		setCapturingHotkey(value: boolean): void {
			setState("capturingHotkey", value);
		},

		setLongPressMs(value: number): void {
			actions.saveConfig({ long_press_ms: value });
		},

		setNotifyModelOnHandsFree(value: boolean): void {
			actions.saveConfig({ hands_free_notify_model: value });
		},

		setSpokenReplies(value: boolean): void {
			void actions.saveConfig({ hands_free_spoken_replies: value });
		},

		setHandsFreeEarcons(value: boolean): void {
			actions.saveConfig({ hands_free_earcons: value });
		},

		setHandsFreeHoldBackMs(value: number): void {
			actions.saveConfig({ hands_free_hold_back_ms: value });
		},

		/**
		 * Set the phrase that must open each hands-free turn.
		 *
		 * Trimmed here because the matching is Rust's and it compares words:
		 * a phrase saved with a trailing space would be a phrase no utterance
		 * ever opens with, and nothing on screen would say why.
		 */
		setHandsFreeActivationPhrase(value: string): void {
			actions.saveConfig({ hands_free_activation_phrase: value.trim() });
		},

		/**
		 * Set the start notice the model reads when hands-free arms.
		 *
		 * Saved as typed; Rust folds it to one line when it sends it. An empty
		 * or blank value means the built-in text — that is the reset.
		 */
		setHandsFreeStartNotice(value: string): void {
			actions.saveConfig({ hands_free_start_notice: value.trim() });
		},

		/** Clear the custom start notice so the built-in text is sent again. */
		resetHandsFreeStartNotice(): void {
			actions.saveConfig({ hands_free_start_notice: "" });
		},

		/**
		 * The built-in start notice, for a placeholder or a "reset" preview.
		 * Rust owns the text; the frontend never keeps a copy of it.
		 */
		async getDefaultHandsFreeStartNotice(): Promise<string> {
			return invoke<string>("get_hands_free_default_notice");
		},

		setSpeechVoice(value: string): void {
			actions.saveConfig({ speech_voice: value });
		},

		setSpeechEngine(value: SpeechEngineId): Promise<void> {
			return actions.saveConfig({ speech_engine: value });
		},

		setSpeechCommand(argv: string[]): Promise<void> {
			return actions.saveConfig({ speech_command: argv });
		},

		setSpeechEdgeVoice(value: string): void {
			actions.saveConfig({ speech_edge_voice: value });
		},

		/**
		 * Read the service's voices for a language. A failure (offline, service
		 * down) is kept as text for the picker to show: an empty list with no
		 * reason would look like "this language has no voices".
		 */
		async refreshEdgeVoices(language: string): Promise<void> {
			const request = ++edgeVoicesRequest;
			try {
				const voices = await invoke<EdgeVoice[]>("get_edge_voices", { language });
				if (request === edgeVoicesRequest) setState({ edgeVoices: voices, edgeVoicesError: null });
			} catch (err) {
				appLogger.warn("dictation", `Failed to list Edge voices for ${language}`, err);
				if (request === edgeVoicesRequest) setState({ edgeVoices: [], edgeVoicesError: String(err) });
			}
		},

		setSpeechVolumeDb(value: number): Promise<void> {
			return actions.saveConfig({ speech_volume_db: value });
		},

		setSpeechLevelling(value: number): Promise<void> {
			return actions.saveConfig({ speech_levelling: value });
		},

		setAutoSend(value: boolean): void {
			actions.saveConfig({ auto_send: value });
		},

		setRmsThreshold(value: number): void {
			actions.saveConfig({ rms_threshold: value });
		},

		setNoSpeechThreshold(value: number): void {
			actions.saveConfig({ no_speech_threshold: value });
		},

		setLanguage(value: string): void {
			actions.saveConfig({ language: value });
		},

		setDevice(value: string | null): void {
			actions.saveConfig({ device: value });
		},

		/** Refresh status from Rust backend */
		async refreshStatus(): Promise<void> {
			try {
				const status = await invoke<DictationStatus>("get_dictation_status");
				setState({
					modelStatus: status.model_status,
					modelName: status.model_name,
					modelSizeMb: status.model_size_mb,
					recording: status.recording,
					processing: status.processing,
					ownedElsewhere: status.owned_elsewhere === true,
					audioLevel: normalizeAudioLevel(status.audio_level),
				});
			} catch (err) {
				appLogger.error("dictation", "Failed to get dictation status", err);
			}
		},

		/** Refresh correction map from Rust backend */
		async refreshCorrections(): Promise<void> {
			try {
				const map = await invoke<Record<string, string>>("get_correction_map");
				setState("corrections", map);
			} catch (err) {
				appLogger.error("dictation", "Failed to get correction map", err);
			}
		},

		/** Save correction map to Rust backend */
		async saveCorrections(map: Record<string, string>): Promise<void> {
			try {
				await invoke("set_correction_map", { map });
				setState("corrections", map);
			} catch (err) {
				appLogger.error("dictation", "Failed to save corrections", err);
			}
		},

		/** List available audio devices */
		async refreshDevices(): Promise<void> {
			// The first enumeration can wait on the macOS microphone prompt until the
			// backend gives up. A failure keeps the previous list, and one more try
			// picks up the answer that arrived late.
			for (let attempt = 0; attempt < 2; attempt++) {
				try {
					setState("devices", await invoke<AudioDevice[]>("list_audio_devices"));
					return;
				} catch (err) {
					appLogger.error("dictation", "Failed to list audio devices", err);
				}
			}
		},

		/** Fetch available model info from Rust backend */
		async refreshModels(): Promise<void> {
			try {
				const models = await invoke<ModelInfo[]>("get_model_info");
				setState("models", models);
			} catch (err) {
				appLogger.error("dictation", "Failed to get model info", err);
			}
		},

		/** Set the selected model and persist to config */
		async setModel(name: string): Promise<void> {
			await actions.saveConfig({ model: name });
			setState("selectedModel", name);
		},

		/** Delete a downloaded model and refresh the model list */
		async deleteModel(name: string): Promise<void> {
			try {
				await invoke("delete_whisper_model", { modelName: name });
				await actions.refreshModels();
			} catch (err) {
				appLogger.error("dictation", "Failed to delete model", err);
			}
		},

		/** Download a Whisper model (defaults to selectedModel) */
		async downloadModel(modelName?: string): Promise<void> {
			setState("downloading", true);
			setState("downloadPercent", 0);
			try {
				await invoke<string>("download_whisper_model", { modelName: modelName ?? state.selectedModel });
				await actions.refreshStatus();
				await actions.refreshModels();
			} catch (err) {
				appLogger.error("dictation", "Model download failed", err);
			} finally {
				setState("downloading", false);
			}
		},

		/** Start recording (sets loading=true while model initializes on first use) */
		async startRecording(source = "ui"): Promise<void> {
			setState("loading", true);
			try {
				await invoke("start_dictation", { source });
				setState("recording", true);
				setState("audioLevel", 0);
				startAudioLevelPolling();
			} catch (err) {
				const errStr = String(err);
				if (errStr.includes(OWNED_ELSEWHERE_MARKER)) {
					setState("ownedElsewhere", true);
					toastsStore.add("Dictation unavailable", errStr, "warn");
				} else if (errStr.includes("microphone_denied")) {
					appLogger.error(
						"dictation",
						"Microphone access denied. Open System Settings > Privacy > Microphone to allow access.",
					);
					invoke("open_microphone_settings").catch(() => {});
				} else if (errStr.includes("microphone_restricted")) {
					appLogger.error("dictation", "Microphone access restricted by system policy");
				} else {
					appLogger.error("dictation", "Failed to start recording", err);
				}
				throw err;
			} finally {
				setState("loading", false);
			}
		},

		/** Stop recording and get transcription result */
		async stopRecording(): Promise<TranscribeResponse | null> {
			// Guard against concurrent stop calls — the Rust side rejects "Not recording"
			// but we avoid the noise by checking frontend state first.
			if (!state.recording) return null;
			// Optimistically clear recording so concurrent callers bail out above.
			setState("recording", false);
			setState("audioLevel", 0);
			stopAudioLevelPolling();
			try {
				const response = await invoke<TranscribeResponse>("stop_dictation_and_transcribe");
				setState("processing", false);
				setState("partialText", "");
				setState("audioLevel", 0);
				setState("lastSkipReason", response.skip_reason);
				return response;
			} catch (err) {
				appLogger.error("dictation", "Failed to stop recording", err);
				setState("processing", false);
				setState("partialText", "");
				setState("audioLevel", 0);
				setState("lastSkipReason", "transcription failed");
				return null;
			}
		},

		// --- Speech assets (818-2a29) ---------------------------------------

		/** Load the speech catalogue and what state each entry is in. */
		async refreshSpeechAssets(): Promise<void> {
			try {
				setState("speechAssets", await invoke<SpeechAsset[]>("get_speech_assets"));
			} catch (err) {
				appLogger.error("dictation", "Failed to list speech assets", err);
			}
		},

		/**
		 * Download one asset, then re-read the catalogue.
		 *
		 * The percent comes from the `speech-download-progress` event rather
		 * than from here; this only marks the asset as started so the bar
		 * appears before the first event, and clears it either way — a failed
		 * download that left its last percent behind would read as one still
		 * running.
		 */
		async downloadSpeechAsset(id: string): Promise<void> {
			setState("speechDownloads", id, 0);
			try {
				await invoke<string>("download_speech_asset", { asset: id });
			} catch (err) {
				appLogger.error("dictation", `Speech asset download failed: ${id}`, err);
			} finally {
				// By key, not by returning a smaller object: a store update at
				// a path merges, so a rest-spread that drops the key leaves it
				// exactly where it was.
				setState("speechDownloads", id, undefined);
				await actions.refreshSpeechAssets();
			}
		},

		/**
		 * Ask Rust to stop a download.
		 *
		 * The bar is left alone: the download is still running until the
		 * in-flight `downloadSpeechAsset` returns, and clearing it here would
		 * show a finished download that is still writing to disk.
		 */
		async cancelSpeechDownload(id: string): Promise<void> {
			try {
				await invoke<string>("cancel_speech_download", { asset: id });
			} catch (err) {
				appLogger.error("dictation", `Failed to cancel download: ${id}`, err);
			}
		},

		async deleteSpeechAsset(id: string): Promise<void> {
			try {
				await invoke<string>("delete_speech_asset", { asset: id });
			} catch (err) {
				appLogger.error("dictation", `Failed to delete speech asset: ${id}`, err);
			}
			await actions.refreshSpeechAssets();
		},

		/**
		 * Import a voice file the user picked into a language (its Whisper code).
		 *
		 * The name is the file name without its extension; Rust validates it,
		 * the size and whether the file fits that language's model. Returns the
		 * reason a file was refused, or null when it was stored.
		 */
		async importSpeechVoice(language: string, file: File): Promise<string | null> {
			const name = file.name.replace(/\.[^.]*$/, "");
			try {
				const dataBase64 = await fileToBase64(file);
				await invoke<string>("import_speech_voice", { language, name, dataBase64 });
				return null;
			} catch (err) {
				appLogger.error("dictation", `Failed to import voice file: ${file.name}`, err);
				return String(err);
			} finally {
				await actions.refreshSpeechAssets();
			}
		},

		/** Delete a voice file the user imported into a language. */
		async deleteSpeechVoice(language: string, name: string): Promise<void> {
			try {
				await invoke<string>("delete_speech_voice", { language, name });
			} catch (err) {
				appLogger.error("dictation", `Failed to delete voice file: ${name}`, err);
			}
			await actions.refreshSpeechAssets();
		},

		/** Read which voices a language can speak with, and where each comes from. */
		async refreshSpeechVoices(language: string): Promise<void> {
			const request = ++speechVoicesRequest;
			try {
				const voices = await invoke<SpeechVoice[]>("get_speech_voices", { language });
				if (request === speechVoicesRequest) setState("speechVoices", voices);
			} catch (err) {
				appLogger.error("dictation", `Failed to list voices for ${language}`, err);
			}
		},

		/**
		 * Let the user hear a voice of a language. Needs no hands-free
		 * conversation and does not change the saved voice. Returns why Rust
		 * refused (for example, a reply is being spoken), or null.
		 */
		async previewSpeechVoice(language: string, voice: string): Promise<string | null> {
			try {
				await invoke("preview_speech_voice", { language, voice, text: voicePreviewText(language) });
				return null;
			} catch (err) {
				appLogger.warn("dictation", `Voice preview refused: ${voice}`, err);
				return String(err);
			}
		},

		// --- Hands-free conversation (818-2a29) -----------------------------

		/**
		 * Read the live hands-free state.
		 *
		 * Deliberately one call. The hotkey asks this on every press to find
		 * out whether it is starting a recording or ending a conversation, and
		 * the speaker state below is not part of that answer.
		 */
		async refreshHandsFree(): Promise<void> {
			try {
				applyHandsFree(await invoke<HandsFreeStatus>("get_hands_free_status"));
			} catch (err) {
				appLogger.error("dictation", "Failed to get hands-free status", err);
			}
		},

		/** Read what the speaker is doing: queued, synthesising, playing. */
		async refreshSpeechStatus(): Promise<void> {
			try {
				setState("speech", await invoke<SpeechStatus>("get_speech_status"));
				nowPlaying?.update(state.speech?.paused === true);
			} catch (err) {
				appLogger.error("dictation", "Failed to get speech status", err);
			}
		},

		/**
		 * Pause, resume or stop the spoken reply. The backend answers with the
		 * speaker's new state, which is stored as is; on a refusal the state is
		 * re-read so the pill never keeps showing a button that no longer applies.
		 */
		pauseSpeech: () => speechControl("pause_speech"),
		resumeSpeech: () => speechControl("resume_speech"),
		stopSpeech: () => speechControl("stop_speech"),

		/**
		 * Bind hands-free to a terminal and open the microphone.
		 *
		 * Only ever from a user action — nothing here runs on mount, so a
		 * restart never re-opens the microphone by itself.
		 *
		 * The owner is the desktop endpoint on the desktop and this tab's own
		 * name in a browser, and the difference decides whose hardware is used:
		 * Rust opens the local devices for `desktop` and the named client's
		 * socket for anything else, with no fallback between them. A browser
		 * therefore opens its socket *first* — arming under a name no socket
		 * has claimed is refused, which is what stops a laptop from listening
		 * through the microphone of the machine running TUICommander.
		 */
		async armHandsFree(sessionId: string): Promise<boolean> {
			setState("handsFreeError", null);
			try {
				const owner = isTauri() ? DESKTOP_AUDIO_OWNER : browserAudioOwner;
				// Inside the arming gesture, so the first earcon is not lost
				// to a context that started suspended.
				primeEarcons();
				// Refresh earcons at each arm, including after another client saved.
				// Never a reason to refuse capture.
				void invoke<DictationConfig>("get_dictation_config")
					.then((config) => {
						if (typeof config?.hands_free_earcons === "boolean")
							setState("handsFreeEarcons", config.hands_free_earcons);
					})
					.catch(() => {});
				if (!isTauri()) {
					stopBrowserVoice();
					browserVoice = await connectBrowserVoice(owner);
				}
				applyHandsFree(
					await invoke<HandsFreeStatus>("arm_hands_free_dictation", {
						sessionId,
						owner,
					}),
				);
				await actions.refreshSpeechStatus();
				return true;
			} catch (err) {
				// The socket is this tab's microphone: leaving it open after a
				// refused arm would keep the device light on for a
				// conversation that does not exist.
				stopBrowserVoice();
				if (String(err).includes(OWNED_ELSEWHERE_MARKER)) setState("ownedElsewhere", true);
				setState("handsFreeError", String(err));
				appLogger.error("dictation", "Failed to arm hands-free", err);
				return false;
			}
		},

		/**
		 * Stop the conversation: the microphone, the pending transcript and
		 * anything being spoken. Turns already typed into the terminal stay
		 * there; nothing else is parked anywhere to take back.
		 */
		async disarmHandsFree(): Promise<void> {
			setState("handsFreeError", null);
			try {
				const result = await invoke<{ status: HandsFreeStatus }>("disarm_hands_free_dictation");
				applyHandsFree(result.status);
				await actions.refreshSpeechStatus();
			} catch (err) {
				setState("handsFreeError", String(err));
				appLogger.error("dictation", "Failed to disarm hands-free", err);
			} finally {
				// Whether or not the backend answered: the conversation is over
				// as far as this tab is concerned, and the microphone closes.
				stopBrowserVoice();
			}
		},

		/** Inject text (apply corrections) without recording */
		async injectText(text: string): Promise<string | null> {
			try {
				return await invoke<string>("inject_text", { text });
			} catch (err) {
				appLogger.error("dictation", "Failed to inject text", err);
				return null;
			}
		},
	};

	return { state, ...actions };
}

export const dictationStore = createDictationStore();
