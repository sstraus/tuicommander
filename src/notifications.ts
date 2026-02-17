/** Notification sound types */
export type NotificationSound = "question" | "error" | "completion" | "warning";

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
  },
};

/** Base64 encoded notification sounds (short beeps) */
const SOUNDS: Record<NotificationSound, string> = {
  // Short ascending tone for questions
  question: generateToneDataUrl(880, 0.15, "sine"),
  // Short descending tone for errors
  error: generateToneDataUrl(330, 0.2, "square"),
  // Pleasant completion chime
  completion: generateToneDataUrl(660, 0.15, "sine"),
  // Warning beep
  warning: generateToneDataUrl(440, 0.1, "triangle"),
};

/** Audio context for sound generation */
let audioContext: AudioContext | null = null;

/** Get or create audio context */
function getAudioContext(): AudioContext {
  if (!audioContext) {
    audioContext = new AudioContext();
    // Resume on first user interaction (autoplay policy workaround)
    const resume = () => {
      if (audioContext?.state === "suspended") {
        audioContext.resume();
      }
    };
    document.addEventListener("click", resume, { once: true });
    document.addEventListener("keydown", resume, { once: true });
  }
  return audioContext;
}

/** Generate a simple tone as a data URL */
function generateToneDataUrl(
  frequency: number,
  duration: number,
  waveType: OscillatorType
): string {
  // We'll use Web Audio API directly instead of data URLs for better quality
  return `tone:${frequency}:${duration}:${waveType}`;
}

/** Play a tone using Web Audio API */
async function playTone(
  frequency: number,
  duration: number,
  waveType: OscillatorType,
  volume: number
): Promise<void> {
  const ctx = getAudioContext();

  // Resume context if suspended (autoplay policy)
  if (ctx.state === "suspended") {
    await ctx.resume();
  }

  const oscillator = ctx.createOscillator();
  const gainNode = ctx.createGain();

  oscillator.type = waveType;
  oscillator.frequency.setValueAtTime(frequency, ctx.currentTime);

  // Envelope: quick attack, sustain, quick release
  gainNode.gain.setValueAtTime(0, ctx.currentTime);
  gainNode.gain.linearRampToValueAtTime(volume, ctx.currentTime + 0.01);
  gainNode.gain.setValueAtTime(volume, ctx.currentTime + duration - 0.05);
  gainNode.gain.linearRampToValueAtTime(0, ctx.currentTime + duration);

  oscillator.connect(gainNode);
  gainNode.connect(ctx.destination);

  oscillator.start(ctx.currentTime);
  oscillator.stop(ctx.currentTime + duration);

  return new Promise((resolve) => {
    setTimeout(resolve, duration * 1000);
  });
}

/** Parse tone data URL */
function parseToneUrl(url: string): { frequency: number; duration: number; waveType: OscillatorType } | null {
  if (!url.startsWith("tone:")) return null;
  const parts = url.split(":");
  if (parts.length !== 4) return null;
  return {
    frequency: parseFloat(parts[1]),
    duration: parseFloat(parts[2]),
    waveType: parts[3] as OscillatorType,
  };
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

    // Parse and play the tone
    const toneData = parseToneUrl(SOUNDS[sound]);
    if (toneData) {
      try {
        await playTone(
          toneData.frequency,
          toneData.duration,
          toneData.waveType,
          this.config.volume
        );
      } catch (err) {
        console.warn("Failed to play notification sound:", err);
      }
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
