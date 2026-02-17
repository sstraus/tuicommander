import { createStore } from "solid-js/store";
import { invoke } from "../invoke";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  notificationManager,
  type NotificationConfig,
  type NotificationSound,
  DEFAULT_NOTIFICATION_CONFIG,
} from "../notifications";

const LEGACY_STORAGE_KEY = "tui-commander-notifications";

/** Create a fresh copy of the default config */
function copyDefaults(): NotificationConfig {
  return {
    ...DEFAULT_NOTIFICATION_CONFIG,
    sounds: { ...DEFAULT_NOTIFICATION_CONFIG.sounds },
  };
}

/** Persist config to Rust backend (fire-and-forget) */
function saveConfig(config: NotificationConfig): void {
  invoke("save_notification_config", { config }).catch((err) =>
    console.debug("Failed to save notification config:", err),
  );
}

/** Notifications store state */
interface NotificationsState {
  config: NotificationConfig;
  isAvailable: boolean;
  badgeCount: number;
}

/** Create notifications store */
function createNotificationsStore() {
  const defaults = copyDefaults();
  notificationManager.updateConfig(defaults);

  const [state, setState] = createStore<NotificationsState>({
    config: defaults,
    isAvailable: notificationManager.isAvailable(),
    badgeCount: 0,
  });

  const actions = {
    /** Load config from Rust backend; migrate from localStorage on first run */
    async hydrate(): Promise<void> {
      try {
        // One-time migration from localStorage
        const legacy = localStorage.getItem(LEGACY_STORAGE_KEY);
        if (legacy) {
          try {
            const parsed = { ...copyDefaults(), ...JSON.parse(legacy) };
            await invoke("save_notification_config", { config: parsed });
          } catch { /* ignore corrupt legacy data */ }
          localStorage.removeItem(LEGACY_STORAGE_KEY);
        }

        const loaded = await invoke<NotificationConfig>("load_notification_config");
        const config = { ...copyDefaults(), ...loaded };
        setState("config", config);
        notificationManager.updateConfig(config);
      } catch (err) {
        console.debug("Failed to hydrate notification config:", err);
      }
    },

    /** Enable or disable notifications */
    setEnabled(enabled: boolean): void {
      setState("config", "enabled", enabled);
      notificationManager.setEnabled(enabled);
      saveConfig(state.config);
    },

    /** Set volume (0.0 to 1.0) */
    setVolume(volume: number): void {
      const clampedVolume = Math.max(0, Math.min(1, volume));
      setState("config", "volume", clampedVolume);
      notificationManager.setVolume(clampedVolume);
      saveConfig(state.config);
    },

    /** Enable/disable a specific sound */
    setSoundEnabled(sound: NotificationSound, enabled: boolean): void {
      setState("config", "sounds", sound, enabled);
      notificationManager.setSoundEnabled(sound, enabled);
      saveConfig(state.config);
    },

    /** Play a notification sound; also increments dock badge when window is not focused */
    async play(sound: NotificationSound): Promise<void> {
      await notificationManager.play(sound);
      if (!document.hasFocus()) {
        actions.incrementBadge();
      }
    },

    /** Play question notification */
    async playQuestion(): Promise<void> {
      await actions.play("question");
    },

    /** Play error notification */
    async playError(): Promise<void> {
      await actions.play("error");
    },

    /** Play completion notification */
    async playCompletion(): Promise<void> {
      await actions.play("completion");
    },

    /** Play warning notification */
    async playWarning(): Promise<void> {
      await actions.play("warning");
    },

    /** Test a notification sound (bypasses enabled check) */
    async testSound(sound: NotificationSound): Promise<void> {
      const wasEnabled = state.config.enabled;
      const wasSoundEnabled = state.config.sounds[sound];

      // Temporarily enable
      notificationManager.setEnabled(true);
      notificationManager.setSoundEnabled(sound, true);

      await notificationManager.play(sound);

      // Restore
      notificationManager.setEnabled(wasEnabled);
      notificationManager.setSoundEnabled(sound, wasSoundEnabled);
    },

    /** Increment badge count on the app dock icon */
    async incrementBadge(): Promise<void> {
      const newCount = state.badgeCount + 1;
      setState("badgeCount", newCount);
      try {
        await getCurrentWindow().setBadgeCount(newCount);
      } catch {
        // setBadgeCount may not be supported on all platforms
      }
    },

    /** Clear badge count from the app dock icon */
    async clearBadge(): Promise<void> {
      if (state.badgeCount === 0) return;
      setState("badgeCount", 0);
      try {
        await getCurrentWindow().setBadgeCount(0);
      } catch {
        // setBadgeCount may not be supported on all platforms
      }
    },

    /** Reset to defaults */
    reset(): void {
      const defaults = copyDefaults();
      setState("config", defaults);
      notificationManager.updateConfig(defaults);
      saveConfig(defaults);
    },

    /** Check if notifications are enabled */
    isEnabled(): boolean {
      return state.config.enabled;
    },

    /** Check if a specific sound is enabled */
    isSoundEnabled(sound: NotificationSound): boolean {
      return state.config.enabled && state.config.sounds[sound];
    },
  };

  return { state, ...actions };
}

export const notificationsStore = createNotificationsStore();
