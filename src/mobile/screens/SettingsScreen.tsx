import { createSignal, onMount, Show } from "solid-js";
import { appLogger } from "../../stores/appLogger";
import styles from "./SettingsScreen.module.css";

const SOUND_KEY = "tuic-mobile-sounds";

interface SettingsScreenProps {
  isConnected: boolean;
}

/** Convert base64url-encoded VAPID public key to Uint8Array for PushManager.subscribe. */
function urlBase64ToUint8Array(base64String: string): Uint8Array {
  const padding = "=".repeat((4 - (base64String.length % 4)) % 4);
  const base64 = (base64String + padding).replace(/-/g, "+").replace(/_/g, "/");
  const raw = atob(base64);
  return new Uint8Array([...raw].map((c) => c.charCodeAt(0)));
}

type PushState = "unsupported" | "requires-https" | "requires-install" | "denied" | "default" | "subscribed";

export function SettingsScreen(props: SettingsScreenProps) {
  const [soundEnabled, setSoundEnabled] = createSignal(
    localStorage.getItem(SOUND_KEY) !== "false",
  );
  const [serverUrl, setServerUrl] = createSignal("");
  const [pushState, setPushState] = createSignal<PushState>("unsupported");
  const [pushLoading, setPushLoading] = createSignal(false);

  onMount(async () => {
    setServerUrl(window.location.origin);
    setPushState(await detectPushState());
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
    const isStandalone = window.matchMedia("(display-mode: standalone)").matches
      || (navigator as any).standalone === true;
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
    } catch { /* ignore */ }
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
          await fetch("/api/push/subscribe", {
            method: "DELETE",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify({ endpoint: sub.endpoint }),
          });
          await sub.unsubscribe();
        }
        setPushState("default");
      } catch (e) {
        appLogger.error("push", "Unsubscribe failed", e);
      } finally {
        setPushLoading(false);
      }
      return;
    }

    // Subscribe
    try {
      setPushLoading(true);

      // Request permission (MUST be in click handler for iOS/Firefox)
      const perm = await Notification.requestPermission();
      if (perm !== "granted") {
        setPushState(perm === "denied" ? "denied" : "default");
        return;
      }

      // Register service worker if needed
      await navigator.serviceWorker.register("/sw.js");
      const reg = await navigator.serviceWorker.ready;

      // Fetch VAPID public key from server
      const keyResp = await fetch("/api/push/vapid-key");
      if (!keyResp.ok) {
        appLogger.warn("push", "VAPID key not available — push not enabled on server");
        setPushState("default");
        return;
      }
      const { publicKey } = await keyResp.json();

      // Subscribe with VAPID key
      const sub = await reg.pushManager.subscribe({
        userVisibleOnly: true,
        applicationServerKey: urlBase64ToUint8Array(publicKey).buffer as ArrayBuffer,
      });

      // Send subscription to backend
      await fetch("/api/push/subscribe", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(sub.toJSON()),
      });

      setPushState("subscribed");
    } catch (e) {
      appLogger.error("push", "Subscribe failed", e);
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
      case "unsupported": return "Not supported in this browser";
      case "requires-https": return "Requires HTTPS (enable Tailscale)";
      case "requires-install": return "Add to Home Screen first";
      case "denied": return "Blocked in browser settings";
      case "subscribed": return "Enabled";
      case "default": return "Disabled";
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
          <span classList={{
            [styles.connected]: props.isConnected,
            [styles.disconnected]: !props.isConnected,
          }}>
            {props.isConnected ? "Connected" : "Disconnected"}
          </span>
        </div>
      </section>

      <section class={styles.section}>
        <h3 class={styles.sectionTitle}>NOTIFICATIONS</h3>
        <div class={styles.row}>
          <span class={styles.label}>Push notifications</span>
          <Show
            when={canTogglePush()}
            fallback={<span class={styles.value}>{pushStatusText()}</span>}
          >
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
          <div class={styles.hint}>
            Tap Share, then "Add to Home Screen" to enable push notifications
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
        <h3 class={styles.sectionTitle}>ACTIONS</h3>
        <a href="/" class={styles.link}>
          Open Desktop UI
        </a>
      </section>

      <div class={styles.footer}>
        TUICommander Mobile
      </div>
    </div>
  );
}
