import { createSignal, onMount, Show } from "solid-js";
import { TelegramTab } from "../../components/SettingsPanel/tabs/TelegramTab";
import { appLogger } from "../../stores/appLogger";
import { loadMobileTheme, mobileTheme, setMobileTheme } from "../mobileTheme";
import styles from "./SettingsScreen.module.css";

const SOUND_KEY = "tuic-mobile-sounds";

interface SettingsScreenProps {
	isConnected: boolean;
}

/** Convert base64url-encoded VAPID public key to Uint8Array for PushManager.subscribe. */
function urlBase64ToUint8Array(base64String: string): Uint8Array {
	const padding = "=".repeat((4 - (base64String.length % 4)) % 4);
	const base64 = (base64String + padding).replaceAll("-", "+").replaceAll("_", "/");
	const raw = atob(base64);
	return new Uint8Array([...raw].map((c) => c.charCodeAt(0)));
}

type PushState = "unsupported" | "requires-https" | "requires-install" | "denied" | "default" | "subscribed";

export function SettingsScreen(props: SettingsScreenProps) {
	const [telegramOpen, setTelegramOpen] = createSignal(false);
	const [soundEnabled, setSoundEnabled] = createSignal(localStorage.getItem(SOUND_KEY) !== "false");
	const [serverUrl, setServerUrl] = createSignal("");
	const [serverVersion, setServerVersion] = createSignal<string | null>(null);
	const [theme, setTheme] = createSignal(mobileTheme());
	const [themeReady, setThemeReady] = createSignal(false);
	const [themeError, setThemeError] = createSignal<string | null>(null);
	const [pushState, setPushState] = createSignal<PushState>("unsupported");
	const [pushLoading, setPushLoading] = createSignal(false);
	const [pushError, setPushError] = createSignal<string | null>(null);

	onMount(() => {
		setServerUrl(window.location.origin);
		void detectPushState().then(setPushState);
		void loadMobileTheme()
			.then(setTheme)
			.catch((error: unknown) => {
				appLogger.warn("app", "Could not load mobile theme", error);
				setThemeError("Could not load theme preference");
			})
			.finally(() => setThemeReady(true));
		void fetch("/api/version")
			.then(async (response) => {
				if (response.ok) {
					const data = (await response.json()) as { version?: string };
					setServerVersion(data.version ?? null);
				}
			})
			.catch((error: unknown) => appLogger.warn("network", "Could not load server version", error));
	});

	async function detectPushState(): Promise<PushState> {
		if (!("serviceWorker" in navigator) || !("PushManager" in window)) {
			return "unsupported";
		}
		// Service workers require HTTPS (except localhost)
		if (location.protocol !== "https:" && location.hostname !== "localhost" && location.hostname !== "127.0.0.1") {
			return "requires-https";
		}
		// iOS requires standalone mode (Add to Home Screen)
		const isIOS = /iPad|iPhone|iPod/.test(navigator.userAgent);
		const isStandalone =
			window.matchMedia("(display-mode: standalone)").matches ||
			("standalone" in navigator && (navigator as unknown as { standalone: boolean }).standalone === true);
		if (isIOS && !isStandalone) {
			return "requires-install";
		}
		// Check notification permission
		const perm = Notification.permission;
		if (perm === "denied") return "denied";
		// Check existing subscription
		try {
			const reg = await navigator.serviceWorker.ready;
			const sub = await reg.pushManager.getSubscription();
			if (sub) return "subscribed";
		} catch (e) {
			appLogger.warn("push", "Failed to read push subscription state", e);
		}
		return "default";
	}

	async function handleTogglePush() {
		const state = pushState();

		if (state === "subscribed") {
			// Unsubscribe
			try {
				setPushLoading(true);
				const reg = await navigator.serviceWorker.ready;
				const sub = await reg.pushManager.getSubscription();
				if (sub) {
					const resp = await fetch("/api/push/subscribe", {
						method: "DELETE",
						headers: { "Content-Type": "application/json" },
						body: JSON.stringify({ endpoint: sub.endpoint }),
					});
					if (!resp.ok) throw new Error(`Server unsubscribe failed: ${resp.status}`);
					await sub.unsubscribe();
				}
				setPushState("default");
			} catch (e) {
				appLogger.error("push", "Unsubscribe failed", e);
				setPushState(await detectPushState());
			} finally {
				setPushLoading(false);
			}
			return;
		}

		// Subscribe
		try {
			setPushLoading(true);
			setPushError(null);

			// Request permission (MUST be in click handler for iOS/Firefox)
			const perm = await Notification.requestPermission();
			if (perm !== "granted") {
				setPushState(perm === "denied" ? "denied" : "default");
				if (perm !== "denied") setPushError("Permission not granted");
				return;
			}

			// Register service worker if needed
			await navigator.serviceWorker.register("/sw.js");
			const reg = await navigator.serviceWorker.ready;

			// Fetch VAPID public key from server
			const keyResp = await fetch("/api/push/vapid-key");
			if (!keyResp.ok) {
				setPushError(`VAPID key fetch failed (${keyResp.status}) — push not configured on server`);
				setPushState("default");
				return;
			}
			const keyJson = await keyResp.json();
			const publicKey = keyJson?.publicKey;
			if (typeof publicKey !== "string" || publicKey.length === 0) {
				setPushError("Server returned empty VAPID key");
				setPushState("default");
				return;
			}

			// Subscribe with VAPID key
			const sub = await reg.pushManager.subscribe({
				userVisibleOnly: true,
				applicationServerKey: urlBase64ToUint8Array(publicKey).buffer as ArrayBuffer,
			});

			// Send subscription to backend
			const postResp = await fetch("/api/push/subscribe", {
				method: "POST",
				headers: { "Content-Type": "application/json" },
				body: JSON.stringify(sub.toJSON()),
			});
			if (!postResp.ok) throw new Error(`Server subscribe failed: ${postResp.status}`);

			setPushError(null);
			setPushState("subscribed");
		} catch (e) {
			const msg = e instanceof Error ? e.message : String(e);
			appLogger.error("push", "Subscribe failed", e);
			setPushError(msg);
			setPushState(await detectPushState());
		} finally {
			setPushLoading(false);
		}
	}

	function toggleSound() {
		const next = !soundEnabled();
		setSoundEnabled(next);
		localStorage.setItem(SOUND_KEY, String(next));
	}

	const pushStatusText = () => {
		switch (pushState()) {
			case "unsupported":
				return "Not supported in this browser";
			case "requires-https":
				return "Requires HTTPS (enable Tailscale)";
			case "requires-install":
				return "Add to Home Screen first";
			case "denied":
				return "Blocked in browser settings";
			case "subscribed":
				return "Enabled";
			case "default":
				return "Disabled";
		}
	};

	const canTogglePush = () => pushState() === "default" || pushState() === "subscribed";

	return (
		<div class={styles.screen}>
			<section class={styles.section}>
				<h3 class={styles.sectionTitle}>CONNECTION</h3>
				<div class={styles.row}>
					<span class={styles.label}>Server</span>
					<span class={styles.value}>{serverUrl()}</span>
				</div>
				<div class={styles.row}>
					<span class={styles.label}>Status</span>
					<span
						classList={{
							[styles.connected]: props.isConnected,
							[styles.disconnected]: !props.isConnected,
						}}
					>
						{props.isConnected ? "Connected" : "Disconnected"}
					</span>
				</div>
				<div class={styles.row}>
					<span class={styles.label}>App version</span>
					<span class={styles.value}>{__APP_VERSION__}</span>
				</div>
				<div class={styles.row}>
					<span class={styles.label}>Server version</span>
					<span class={styles.value}>{serverVersion() ?? "Unavailable"}</span>
				</div>
			</section>

			<section class={styles.section}>
				<h3 class={styles.sectionTitle}>APPEARANCE</h3>
				<label class={styles.row}>
					<span class={styles.label}>Theme</span>
					<select
						class={styles.select}
						aria-label="Theme"
						value={theme()}
						disabled={!themeReady()}
						onChange={(event) => {
							const next = event.currentTarget.value === "vscode-light" ? "vscode-light" : "commander";
							const previous = theme();
							setTheme(next);
							setThemeReady(false);
							setThemeError(null);
							void setMobileTheme(next)
								.catch((error: unknown) => {
									setTheme(previous);
									appLogger.warn("app", "Could not save mobile theme", error);
									setThemeError("Could not save theme preference");
								})
								.finally(() => setThemeReady(true));
						}}
					>
						<option value="commander">Dark</option>
						<option value="vscode-light">Light</option>
					</select>
				</label>
				<Show when={themeError()}>
					<div class={styles.hint} role="alert" style={{ color: "var(--error)" }}>
						{themeError()}
					</div>
				</Show>
			</section>

			<section class={styles.section}>
				<h3 class={styles.sectionTitle}>NOTIFICATIONS</h3>
				<div class={styles.row}>
					<span class={styles.label}>Push notifications</span>
					<Show when={canTogglePush()} fallback={<span class={styles.value}>{pushStatusText()}</span>}>
						<button
							class={styles.toggle}
							classList={{ [styles.toggleOn]: pushState() === "subscribed" }}
							onClick={handleTogglePush}
							disabled={pushLoading()}
							aria-pressed={pushState() === "subscribed"}
						>
							<span class={styles.toggleThumb} />
						</button>
					</Show>
				</div>
				<Show when={pushState() === "requires-install"}>
					<div class={styles.hint}>Tap Share, then "Add to Home Screen" to enable push notifications</div>
				</Show>
				<Show when={pushError()}>
					<div class={styles.hint} style={{ color: "var(--error)" }}>
						{pushError()}
					</div>
				</Show>
				<div class={styles.row}>
					<span class={styles.label}>Notification sounds</span>
					<button
						class={styles.toggle}
						classList={{ [styles.toggleOn]: soundEnabled() }}
						onClick={toggleSound}
						aria-pressed={soundEnabled()}
					>
						<span class={styles.toggleThumb} />
					</button>
				</div>
			</section>

			<section class={styles.section}>
				<button
					class={styles.select}
					disabled={!props.isConnected}
					onClick={() => setTelegramOpen(!telegramOpen())}
					aria-expanded={telegramOpen()}
				>
					Telegram setup
				</button>
				<Show when={telegramOpen()}>
					<TelegramTab />
				</Show>
			</section>

			<section class={styles.section}>
				<h3 class={styles.sectionTitle}>ACTIONS</h3>
				<a href="/" class={styles.link}>
					Open Desktop UI
				</a>
				<a href="/process/monitor" class={styles.link} target="_blank" rel="noopener">
					Process Monitor
				</a>
			</section>

			<div class={styles.footer}>TUICommander Mobile</div>
		</div>
	);
}
