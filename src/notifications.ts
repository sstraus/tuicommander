import { appLogger } from "./stores/appLogger";

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

/** A single note in a sound sequence */
interface Note {
  frequency: number;
  duration: number;
  wave: OscillatorType;
}

/** A sequence of notes with a gap between them */
interface SoundSequence {
  notes: Note[];
  gap: number;
}

/** Sound definitions as note sequences */
const SOUNDS: Record<NotificationSound, SoundSequence> = {
  // Gentle two-note ascending chime: C5 → E5
  question: {
    notes: [
      { frequency: 523, duration: 0.12, wave: "sine" },
      { frequency: 659, duration: 0.12, wave: "sine" },
    ],
    gap: 0.03,
  },
  // Satisfying major triad arpeggio: C5 → E5 → G5
  completion: {
    notes: [
      { frequency: 523, duration: 0.1, wave: "sine" },
      { frequency: 659, duration: 0.1, wave: "sine" },
      { frequency: 784, duration: 0.1, wave: "sine" },
    ],
    gap: 0.03,
  },
  // Low descending minor interval: E4 → C4
  error: {
    notes: [
      { frequency: 330, duration: 0.15, wave: "triangle" },
      { frequency: 262, duration: 0.15, wave: "triangle" },
    ],
    gap: 0.04,
  },
  // Quick double-tap: A4 × 2
  warning: {
    notes: [
      { frequency: 440, duration: 0.08, wave: "triangle" },
      { frequency: 440, duration: 0.08, wave: "triangle" },
    ],
    gap: 0.06,
  },
  // Soft single pluck: G5 (subtle, non-intrusive)
  info: {
    notes: [
      { frequency: 784, duration: 0.08, wave: "sine" },
    ],
    gap: 0,
  },
};

/** Audio context for sound generation */
let audioContext: AudioContext | null = null;

/**
 * Warm up the AudioContext by creating it inside a real user gesture.
 *
 * WebKit (used by Tauri on macOS) requires `new AudioContext()` or
 * `audioContext.resume()` to happen in the **synchronous** call chain
 * of a user gesture (click, keydown, pointerdown).  Parsed terminal
 * events arrive via Tauri's async `listen()` — that's NOT a gesture,
 * so resume() is silently ignored.
 *
 * Call this once at app startup.  It attaches listeners that fire on
 * the very first user interaction and move the context to "running".
 * After that, all subsequent `playSoundSequence` calls just work.
 */
export function warmUpAudioContext(): void {
  const unlock = () => {
    if (!audioContext) {
      audioContext = new AudioContext();
    }
    if (audioContext.state === "suspended") {
      audioContext.resume();
    }
    // Once running, remove ourselves — no need to keep firing
    if (audioContext.state === "running") {
      document.removeEventListener("click", unlock, true);
      document.removeEventListener("keydown", unlock, true);
      document.removeEventListener("pointerdown", unlock, true);
    }
  };
  // Use capture phase so we fire even if the event is stopped
  document.addEventListener("click", unlock, true);
  document.addEventListener("keydown", unlock, true);
  document.addEventListener("pointerdown", unlock, true);
}

/** Get or create audio context */
function getAudioContext(): AudioContext {
  if (!audioContext) {
    audioContext = new AudioContext();
  }
  // Belt-and-suspenders: still try resume in case warmUp hasn't fired yet
  if (audioContext.state === "suspended") {
    audioContext.resume();
  }
  return audioContext;
}

/** Play a sound sequence using Web Audio API */
async function playSoundSequence(
  sequence: SoundSequence,
  volume: number
): Promise<void> {
  const ctx = getAudioContext();

  const attackTime = 0.01;
  const releaseTime = 0.03;
  let offset = 0;

  for (const note of sequence.notes) {
    const oscillator = ctx.createOscillator();
    const gainNode = ctx.createGain();

    oscillator.type = note.wave;
    oscillator.frequency.setValueAtTime(note.frequency, ctx.currentTime + offset);

    // ADSR envelope: quick attack, sustain, smooth release to avoid clicks
    const noteStart = ctx.currentTime + offset;
    const noteEnd = noteStart + note.duration;
    gainNode.gain.setValueAtTime(0, noteStart);
    gainNode.gain.linearRampToValueAtTime(volume, noteStart + attackTime);
    gainNode.gain.setValueAtTime(volume, noteEnd - releaseTime);
    gainNode.gain.linearRampToValueAtTime(0, noteEnd);

    oscillator.connect(gainNode);
    gainNode.connect(ctx.destination);

    oscillator.start(noteStart);
    oscillator.stop(noteEnd);

    offset += note.duration + sequence.gap;
  }

  // Wait for the full sequence to finish
  return new Promise((resolve) => {
    setTimeout(resolve, offset * 1000);
  });
}

/** Notification manager class */
export class NotificationManager {
  private config: NotificationConfig;
  private lastPlayTime: Map<NotificationSound, number> = new Map();
  private readonly minInterval = 500; // Minimum ms between same sound

  constructor(config: Partial<NotificationConfig> = {}) {
    this.config = { ...DEFAULT_NOTIFICATION_CONFIG, ...config };
  }

  /** Play a notification sound */
  async play(sound: NotificationSound): Promise<void> {
    // Check if notifications are enabled
    if (!this.config.enabled) return;

    // Check if this specific sound is enabled
    if (!this.config.sounds[sound]) return;

    // Rate limit: prevent spam
    const now = Date.now();
    const lastPlay = this.lastPlayTime.get(sound) || 0;
    if (now - lastPlay < this.minInterval) return;
    this.lastPlayTime.set(sound, now);

    try {
      await playSoundSequence(SOUNDS[sound], this.config.volume);
    } catch (err) {
      appLogger.warn("app", "Failed to play notification sound", err);
    }
  }

  /** Play question notification */
  async playQuestion(): Promise<void> {
    return this.play("question");
  }

  /** Play error notification */
  async playError(): Promise<void> {
    return this.play("error");
  }

  /** Play completion notification */
  async playCompletion(): Promise<void> {
    return this.play("completion");
  }

  /** Play warning notification */
  async playWarning(): Promise<void> {
    return this.play("warning");
  }

  /** Play info notification */
  async playInfo(): Promise<void> {
    return this.play("info");
  }

  /** Update configuration */
  updateConfig(config: Partial<NotificationConfig>): void {
    this.config = { ...this.config, ...config };
  }

  /** Set enabled state */
  setEnabled(enabled: boolean): void {
    this.config.enabled = enabled;
  }

  /** Set volume (0.0 to 1.0) */
  setVolume(volume: number): void {
    this.config.volume = Math.max(0, Math.min(1, volume));
  }

  /** Enable/disable specific sound */
  setSoundEnabled(sound: NotificationSound, enabled: boolean): void {
    this.config.sounds[sound] = enabled;
  }

  /** Get current configuration */
  getConfig(): NotificationConfig {
    return { ...this.config };
  }

  /** Check if notifications are available */
  isAvailable(): boolean {
    return typeof AudioContext !== "undefined" || typeof (window as unknown as { webkitAudioContext: unknown }).webkitAudioContext !== "undefined";
  }
}

/** Global notification manager instance */
export const notificationManager = new NotificationManager();
