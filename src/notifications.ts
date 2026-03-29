import { appLogger } from "./stores/appLogger";
import { invoke } from "./invoke";
import { isTauri } from "./transport";

/** Notification sound types */
export type NotificationSound = "question" | "error" | "completion" | "warning" | "info";

/** Notification configuration */
export interface NotificationConfig {
  enabled: boolean;
  volume: number; // 0.0 to 1.0
  sounds: Record<NotificationSound, boolean>;
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
  },
};

/** Notification manager — delegates audio playback to Rust via Tauri IPC.
 *  Handles config, per-sound enable/disable, and rate limiting in JS.
 *  Actual tone generation happens natively (rodio), bypassing WebKit
 *  AudioContext restrictions entirely. */
export class NotificationManager {
  private config: NotificationConfig;
  private lastPlayTime: Map<NotificationSound, number> = new Map();
  private readonly minInterval = 500; // Minimum ms between same sound
  private consecutiveFailures = 0;
  private backoffUntil = 0;

  constructor(config: Partial<NotificationConfig> = {}) {
    this.config = { ...DEFAULT_NOTIFICATION_CONFIG, ...config };
  }

  /** Play a notification sound */
  async play(sound: NotificationSound): Promise<void> {
    if (!this.config.enabled) return;
    if (!this.config.sounds[sound]) return;

    const now = Date.now();

    // Back off after repeated failures (exponential: 5s, 30s, 5min cap)
    if (now < this.backoffUntil) return;

    // Rate limit: prevent spam
    const lastPlay = this.lastPlayTime.get(sound) || 0;
    if (now - lastPlay < this.minInterval) return;
    this.lastPlayTime.set(sound, now);

    try {
      if (isTauri()) {
        await invoke("play_notification_sound", {
          sound,
          volume: this.config.volume,
        });
      } else {
        playWebAudioTone(sound, this.config.volume);
      }
      this.consecutiveFailures = 0;
    } catch (err) {
      this.consecutiveFailures++;
      if (this.consecutiveFailures >= 3) {
        const delay = Math.min(300_000, 5_000 * Math.pow(2, this.consecutiveFailures - 3));
        this.backoffUntil = now + delay;
        appLogger.debug("app", `Notification sound failing, backing off ${Math.round(delay / 1000)}s`, err);
      }
    }
  }

  async playQuestion(): Promise<void> { return this.play("question"); }
  async playError(): Promise<void> { return this.play("error"); }
  async playCompletion(): Promise<void> { return this.play("completion"); }
  async playWarning(): Promise<void> { return this.play("warning"); }
  async playInfo(): Promise<void> { return this.play("info"); }

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
  question: [880, 1100],     // ascending two-tone
  error: [440, 330],         // descending two-tone
  completion: [660, 880],    // ascending two-tone
  warning: [550, 440],       // descending two-tone
  info: [660],               // single tone
};

let webAudioCtx: AudioContext | null = null;

function playWebAudioTone(sound: NotificationSound, volume: number): void {
  if (!webAudioCtx) {
    webAudioCtx = new AudioContext();
  }
  const ctx = webAudioCtx;
  if (ctx.state === "suspended") ctx.resume();

  const freqs = TONE_FREQS[sound] ?? [660];
  const gain = ctx.createGain();
  gain.gain.value = volume * 0.3; // Gentle volume
  gain.connect(ctx.destination);

  freqs.forEach((freq, i) => {
    const osc = ctx.createOscillator();
    osc.type = "sine";
    osc.frequency.value = freq;
    osc.connect(gain);
    osc.start(ctx.currentTime + i * 0.15);
    osc.stop(ctx.currentTime + i * 0.15 + 0.12);
  });
}

/** Global notification manager instance */
export const notificationManager = new NotificationManager();
