import { Component, For, Show } from "solid-js";
import { notificationsStore } from "../../../stores/notifications";
import type { NotificationSound } from "../../../notifications";
import { t } from "../../../i18n";
import s from "../Settings.module.css";

// ---------------------------------------------------------------------------
// Sound pattern visualizations (inline SVG showing pitch contour)
// ---------------------------------------------------------------------------

/** Mini musical staff showing the note pattern for each sound.
 *  5 staff lines, note heads positioned by pitch, stems going up. */
function SoundPatternSvg(props: { sound: NotificationSound }) {
  // Staff: 5 lines from y=4 to y=20, spaced 4px apart
  // Note positions: y maps to pitch (lower y = higher pitch)
  // Each note: x position, y (staff position), filled noteHead
  const patterns: Record<NotificationSound, { x: number; y: number }[]> = {
    question:   [{ x: 14, y: 16 }, { x: 30, y: 8 }],             // C5 → E5 ascending
    completion: [{ x: 10, y: 16 }, { x: 24, y: 10 }, { x: 38, y: 4 }], // C5 → E5 → G5
    error:      [{ x: 14, y: 8 }, { x: 30, y: 16 }],             // E4 → C4 descending
    warning:    [{ x: 14, y: 12 }, { x: 30, y: 12 }],            // A4 × 2 same pitch
    info:       [{ x: 22, y: 4 }],                                 // G5 single note
  };

  const colors: Record<NotificationSound, string> = {
    question: "var(--warning)",
    completion: "var(--success)",
    error: "var(--error)",
    warning: "var(--accent)",
    info: "var(--fg-muted)",
  };

  const notes = patterns[props.sound];
  const color = colors[props.sound];
  const w = props.sound === "completion" ? 36 : props.sound === "info" ? 24 : 32;

  return (
    <svg viewBox={`0 0 ${w} 18`} width={w} height="14" style={{ "vertical-align": "middle", "flex-shrink": "0" }}>
      {/* Staff lines */}
      <For each={[3, 6, 9, 12, 15]}>
        {(ly) => (
          <line x1="1" y1={ly} x2={w - 1} y2={ly} stroke="var(--border)" stroke-width="0.4" />
        )}
      </For>
      {/* Note heads + stems */}
      <For each={notes}>
        {(note) => {
          const sy = (note.y / 20) * 15;
          const sx = (note.x / 48) * w;
          return (
            <>
              <ellipse cx={sx} cy={sy} rx="2.5" ry="1.8" fill={color} transform={`rotate(-15 ${sx} ${sy})`} />
              <line x1={sx + 2.3} y1={sy - 0.5} x2={sx + 2.3} y2={sy - 7} stroke={color} stroke-width="0.7" />
            </>
          );
        }}
      </For>
    </svg>
  );
}


// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export const NotificationsTab: Component = () => {
  const sounds: { key: NotificationSound; label: string }[] = [
    { key: "question", label: t("notifications.sound.question", "Question") },
    { key: "error", label: t("notifications.sound.error", "Error") },
    { key: "completion", label: t("notifications.sound.completion", "Completion") },
    { key: "warning", label: t("notifications.sound.warning", "Warning") },
    { key: "info", label: t("notifications.sound.info", "Info") },
  ];

  return (
    <>
      <div class={s.section}>
        <h3>{t("notifications.heading.notificationSettings", "Notification Settings")}</h3>

        <Show
          when={notificationsStore.state.isAvailable}
          fallback={
            <p class={s.warning}>
              {t("notifications.warning.notAvailable", "Audio notifications are not available on this platform")}
            </p>
          }
        >
          <div class={s.group}>
            <label>{t("notifications.label.enableAudio", "Enable Audio")}</label>
            <div class={s.toggle}>
              <input
                type="checkbox"
                checked={notificationsStore.state.config.enabled}
                onChange={(e) => notificationsStore.setEnabled(e.currentTarget.checked)}
              />
              <span>{t("notifications.toggle.enableAudio", "Enable audio notifications")}</span>
            </div>
          </div>

          <div class={s.group}>
            <label>{t("notifications.label.masterVolume", "Master Volume")}</label>
            <div class={s.slider}>
              <input
                type="range"
                min="0"
                max="100"
                value={Math.round(notificationsStore.state.config.volume * 100)}
                onInput={(e) => notificationsStore.setVolume(parseInt(e.currentTarget.value) / 100)}
              />
              <span>{Math.round(notificationsStore.state.config.volume * 100)}%</span>
            </div>
            <p class={s.hint}>{t("notifications.hint.masterVolume", "Overall volume for all notification sounds")}</p>
          </div>

          <div class={s.group}>
            <label>{t("notifications.label.notificationEvents", "Notification Events")}</label>
            <p class={s.hint} style={{ "margin-bottom": "12px" }}>
              {t("notifications.hint.notificationEvents", "Choose which events play a sound")}
            </p>
            <For each={sounds}>
              {(sound) => (
                <div class={s.soundRow}>
                  <div class={s.toggle}>
                    <input
                      type="checkbox"
                      checked={notificationsStore.state.config.sounds[sound.key]}
                      onChange={(e) =>
                        notificationsStore.setSoundEnabled(sound.key, e.currentTarget.checked)
                      }
                    />
                    <span>{sound.label}</span>
                  </div>
                  <SoundPatternSvg sound={sound.key} />
                  <button
                    class={s.testBtn}
                    onClick={() => notificationsStore.testSound(sound.key)}
                  >
                    {t("notifications.btn.test", "Test")}
                  </button>
                </div>
              )}
            </For>
          </div>

          <div class={s.actions}>
            <button onClick={() => notificationsStore.reset()}>{t("notifications.btn.resetDefaults", "Reset Defaults")}</button>
          </div>
        </Show>
      </div>
    </>
  );
};
