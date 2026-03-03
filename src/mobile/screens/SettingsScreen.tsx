import { createSignal, onMount } from "solid-js";
import styles from "./SettingsScreen.module.css";

const SOUND_KEY = "tuic-mobile-sounds";

interface SettingsScreenProps {
  isConnected: boolean;
}

export function SettingsScreen(props: SettingsScreenProps) {
  const [soundEnabled, setSoundEnabled] = createSignal(
    localStorage.getItem(SOUND_KEY) !== "false",
  );
  const [serverUrl, setServerUrl] = createSignal("");

  onMount(() => {
    setServerUrl(window.location.origin);
  });

  function toggleSound() {
    const next = !soundEnabled();
    setSoundEnabled(next);
    localStorage.setItem(SOUND_KEY, String(next));
  }

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
        <h3 class={styles.sectionTitle}>PREFERENCES</h3>
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
