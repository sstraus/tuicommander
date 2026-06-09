import { createStore } from "solid-js/store";
import { invoke } from "../invoke";
import {
	DEFAULT_NOTIFICATION_CONFIG,
	type NotificationConfig,
	type NotificationSound,
	notificationManager,
} from "../notifications";
import { isTauri } from "../transport";
import { appLogger } from "./appLogger";

interface PlayOptions {
	terminalId?: string;
}

const OS_NOTIFICATION_TITLES: Record<NotificationSound, string> = {
	question: "Agent needs input",
	error: "Error detected",
	completion: "Task completed",
	warning: "Warning",
	info: "Info",
};

let osNotificationPermission: NotificationPermission | null = null;

async function ensureNotificationPermission(): Promise<boolean> {
	if (!("Notification" in window)) return false;
	if (osNotificationPermission === null) {
		osNotificationPermission = Notification.permission;
	}
	if (osNotificationPermission === "granted") return true;
	if (osNotificationPermission === "denied") return false;
	osNotificationPermission = await Notification.requestPermission();
	return osNotificationPermission === "granted";
}

function sendOsNotification(sound: NotificationSound, terminalId: string, tabName: string): void {
	const n = new Notification(OS_NOTIFICATION_TITLES[sound], {
		body: tabName,
		silent: true,
	});
	n.onclick = () => {
		n.close();
		if (isTauri()) {
			import("@tauri-apps/api/window").then(({ getCurrentWindow }) => {
				getCurrentWindow().setFocus();
			});
		} else {
			window.focus();
		}
		import("../utils/navigateToTerminal").then(({ navigateToTerminal }) => {
			navigateToTerminal(terminalId);
		});
	};
}

const LEGACY_STORAGE_KEY = "tui-commander-notifications";

/** Create a fresh copy of the default config */
function copyDefaults(): NotificationConfig {
	return {
		...DEFAULT_NOTIFICATION_CONFIG,
		sounds: { ...DEFAULT_NOTIFICATION_CONFIG.sounds },
		audio_device: DEFAULT_NOTIFICATION_CONFIG.audio_device,
	};
}

/** Persist config to Rust backend (fire-and-forget) */
function saveConfig(config: NotificationConfig): void {
	invoke("save_notification_config", { config }).catch((err) =>
		appLogger.debug("config", "Failed to save notification config", err),
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
					} catch {
						/* ignore corrupt legacy data */
					}
					localStorage.removeItem(LEGACY_STORAGE_KEY);
				}

				const loaded = await invoke<NotificationConfig>("load_notification_config");
				const config = { ...copyDefaults(), ...loaded };
				setState("config", config);
				notificationManager.updateConfig(config);
			} catch (err) {
				appLogger.debug("config", "Failed to hydrate notification config", err);
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

		/** Set the audio output device (null = system default) */
		setAudioDevice(device: string | null): void {
			setState("config", "audio_device", device);
			notificationManager.updateConfig({ audio_device: device });
			saveConfig(state.config);
		},

		/** Enable/disable a specific sound */
		setSoundEnabled(sound: NotificationSound, enabled: boolean): void {
			setState("config", "sounds", sound, enabled);
			notificationManager.setSoundEnabled(sound, enabled);
			saveConfig(state.config);
		},

		/** Play a notification sound; also increments dock badge and sends OS notification when window is not focused */
		async play(sound: NotificationSound, opts?: PlayOptions): Promise<void> {
			const caller =
				new Error().stack
					?.split("\n")
					.slice(1, 4)
					.map((l) => l.trim())
					.join(" <- ") ?? "unknown";
			appLogger.debug("app", `[Notification.Play] sound=${sound} focused=${document.hasFocus()} caller=${caller}`);
			await notificationManager.play(sound);
			if (!document.hasFocus()) {
				actions.incrementBadge();
				if (opts?.terminalId) {
					ensureNotificationPermission().then((ok) => {
						if (!ok) return;
						import("./terminals").then(({ terminalsStore }) => {
							const term = terminalsStore.get(opts.terminalId!);
							const tabName = term?.name ?? opts.terminalId!;
							sendOsNotification(sound, opts.terminalId!, tabName);
						});
					});
				}
			}
		},

		/** Play question notification */
		async playQuestion(terminalId?: string): Promise<void> {
			await actions.play("question", { terminalId });
		},

		/** Play error notification */
		async playError(terminalId?: string): Promise<void> {
			await actions.play("error", { terminalId });
		},

		/** Play completion notification */
		async playCompletion(terminalId?: string): Promise<void> {
			await actions.play("completion", { terminalId });
		},

		/** Play warning notification */
		async playWarning(terminalId?: string): Promise<void> {
			await actions.play("warning", { terminalId });
		},

		/** Play info notification */
		async playInfo(terminalId?: string): Promise<void> {
			await actions.play("info", { terminalId });
		},

		/** Test a notification sound — explicit user action, so it bypasses the
		 *  enabled / per-sound / rate-limit gates and always plays at the current volume */
		async testSound(sound: NotificationSound): Promise<void> {
			await notificationManager.play(sound, { force: true });
		},

		/** Increment badge count on the app dock icon */
		async incrementBadge(): Promise<void> {
			const newCount = state.badgeCount + 1;
			setState("badgeCount", newCount);
			try {
				if (isTauri()) {
					const { getCurrentWindow } = await import("@tauri-apps/api/window");
					await getCurrentWindow().setBadgeCount(newCount);
				} else if ("setAppBadge" in navigator) {
					await (navigator as Navigator & { setAppBadge: (n: number) => Promise<void> }).setAppBadge(newCount);
				}
			} catch (err) {
				appLogger.debug("app", "Badge API unavailable or failed", err);
			}
		},

		/** Clear badge count from the app dock icon */
		async clearBadge(): Promise<void> {
			if (state.badgeCount === 0) return;
			setState("badgeCount", 0);
			try {
				if (isTauri()) {
					const { getCurrentWindow } = await import("@tauri-apps/api/window");
					await getCurrentWindow().setBadgeCount();
				} else if ("clearAppBadge" in navigator) {
					await (navigator as Navigator & { clearAppBadge: () => Promise<void> }).clearAppBadge();
				}
			} catch (err) {
				appLogger.debug("app", "Badge API unavailable or failed", err);
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
