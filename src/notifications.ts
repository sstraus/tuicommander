import { invoke } from "./invoke";
import { appLogger } from "./stores/appLogger";
import { isTauri } from "./transport";

/** Notification sound types */
export type NotificationSound = "question" | "error" | "completion" | "warning" | "info" | "attention";

/** Every sound name, for runtime validation of values coming off the wire. */
export const NOTIFICATION_SOUNDS: readonly NotificationSound[] = [
	"question",
	"error",
	"completion",
	"warning",
	"info",
	"attention",
] as const;

export function isNotificationSound(value: unknown): value is NotificationSound {
	return typeof value === "string" && (NOTIFICATION_SOUNDS as readonly string[]).includes(value);
}

/** Notification configuration */
export interface NotificationConfig {
	enabled: boolean;
	volume: number; // 0.0 to 1.0
	sounds: Record<NotificationSound, boolean>;
	audio_device: string | null;
	/** Mute sounds and native notices for parented sessions. Also retains the
	 *  legacy completion-chime mute for unparented MCP/HTTP-created sessions.
	 *  Activity, bell entries and urgent cards are unaffected. */
	silence_remote_completions: boolean;
	/** Mirror every toast into the toolbar bell, so a message that auto-dismissed
	 *  while the user looked elsewhere is still readable afterwards. */
	toasts_in_bell: boolean;
	/** Send an OS notification when a PR becomes ready, fails CI, gets changes
	 *  requested or is merged — the toolbar bell is invisible while another app is focused. */
	pr_native_notifications: boolean;
}

/** Default notification configuration */
export const DEFAULT_NOTIFICATION_CONFIG: NotificationConfig = {
	enabled: true,
	volume: 0.5,
	sounds: {
		question: true,
		error: true,
		completion: true,
		warning: true,
		info: true,
		attention: true,
	},
	audio_device: null,
	silence_remote_completions: true,
	toasts_in_bell: true,
	pr_native_notifications: true,
};

/** Notification manager — delegates audio playback to Rust via Tauri IPC.
 *  Handles config, per-sound enable/disable, and rate limiting in JS.
 *  Actual tone generation happens natively (rodio), bypassing WebKit
 *  AudioContext restrictions entirely. */
export class NotificationManager {
	private config: NotificationConfig;
	private lastPlayTime: number | null = null;
	private readonly minInterval = 500; // Minimum ms between notification sounds
	private consecutiveFailures = 0;
	private backoffUntil = 0;

	constructor(config: Partial<NotificationConfig> = {}) {
		this.config = { ...DEFAULT_NOTIFICATION_CONFIG, ...config };
	}

	/** Play a notification sound.
	 *  `force` bypasses the enabled / per-sound / backoff / rate-limit gates — used
	 *  by the Settings "Test" buttons, where the user explicitly asked to hear the
	 *  sound right now and throttling would silently swallow rapid A/B comparisons. */
	async play(sound: NotificationSound, opts?: { force?: boolean }): Promise<void> {
		const now = Date.now();

		if (!opts?.force) {
			if (!this.config.enabled) return;
			if (!this.config.sounds[sound]) return;

			// Back off after repeated failures (exponential: 5s, 30s, 5min cap)
			if (now < this.backoffUntil) return;

			// One audio output serves every notification type. A per-sound limit let
			// completion, question, and attention tones start on separate native
			// threads and overlap during a burst, so throttle the shared output.
			if (this.lastPlayTime !== null && now - this.lastPlayTime < this.minInterval) return;
			this.lastPlayTime = now;
		}

		try {
			if (isTauri()) {
				await invoke("play_notification_sound", {
					sound,
					volume: this.config.volume,
					device: this.config.audio_device,
				});
			} else {
				playWebAudioTone(sound, this.config.volume);
			}
			this.consecutiveFailures = 0;
		} catch (err) {
			this.consecutiveFailures++;
			appLogger.debug("app", `Notification sound failed (attempt ${this.consecutiveFailures})`, err);
			if (this.consecutiveFailures >= 3) {
				const delay = Math.min(300_000, 5_000 * 2 ** (this.consecutiveFailures - 3));
				this.backoffUntil = now + delay;
				appLogger.warn("app", `Notification sound backing off ${Math.round(delay / 1000)}s`, err);
			}
		}
	}

	async playQuestion(): Promise<void> {
		return this.play("question");
	}
	async playError(): Promise<void> {
		return this.play("error");
	}
	async playCompletion(): Promise<void> {
		return this.play("completion");
	}
	async playWarning(): Promise<void> {
		return this.play("warning");
	}
	async playInfo(): Promise<void> {
		return this.play("info");
	}

	updateConfig(config: Partial<NotificationConfig>): void {
		this.config = { ...this.config, ...config };
	}

	setEnabled(enabled: boolean): void {
		this.config.enabled = enabled;
	}

	setVolume(volume: number): void {
		this.config.volume = Math.max(0, Math.min(1, volume));
	}

	setSoundEnabled(sound: NotificationSound, enabled: boolean): void {
		this.config.sounds[sound] = enabled;
	}

	getConfig(): NotificationConfig {
		return { ...this.config };
	}

	/** Notifications are always available when running in Tauri (native audio) */
	isAvailable(): boolean {
		return true;
	}
}

// Web Audio API fallback for browser mode — simple tones per sound type
const TONE_FREQS: Record<NotificationSound, number[]> = {
	question: [880, 1100], // ascending two-tone
	error: [440, 330], // descending two-tone
	completion: [660, 880], // ascending two-tone
	warning: [550, 440], // descending two-tone
	info: [660], // single tone
	attention: [392, 392, 659], // two quick G4 knocks, then a longer E5 call
};

/** Timbre per sound. The attention call uses a softer triangle wave while its
 *  double-knock motif keeps it distinct from the chimes. Mirrors Rust. */
const TONE_WAVEFORMS: Partial<Record<NotificationSound, OscillatorType>> = {
	attention: "triangle",
};

/** Extra attenuation per sound, mirroring the Rust engine. */
const TONE_GAINS: Partial<Record<NotificationSound, number>> = {
	attention: 0.8,
};

const ATTENTION_DURATIONS = [0.075, 0.075, 0.14];

let webAudioCtx: AudioContext | null = null;

function playWebAudioTone(sound: NotificationSound, volume: number): void {
	if (!webAudioCtx) {
		webAudioCtx = new AudioContext();
	}
	const ctx = webAudioCtx;
	if (ctx.state === "suspended") ctx.resume();

	const freqs = TONE_FREQS[sound] ?? [660];
	const gain = ctx.createGain();
	gain.gain.value = volume * 0.3 * (TONE_GAINS[sound] ?? 1); // Gentle volume
	gain.connect(ctx.destination);
	const interval = sound === "attention" ? 0.125 : 0.15;

	freqs.forEach((freq, i) => {
		const duration = sound === "attention" ? ATTENTION_DURATIONS[i] : 0.12;
		const osc = ctx.createOscillator();
		osc.type = TONE_WAVEFORMS[sound] ?? "sine";
		osc.frequency.value = freq;
		osc.connect(gain);
		osc.start(ctx.currentTime + i * interval);
		osc.stop(ctx.currentTime + i * interval + duration);
		osc.onended = () => osc.disconnect();
	});
}

/** Global notification manager instance */
export const notificationManager = new NotificationManager();
