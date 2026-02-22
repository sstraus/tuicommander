import { Component, For, Show } from "solid-js";
import { notificationsStore } from "../../../stores/notifications";
import type { NotificationSound } from "../../../notifications";
import { t } from "../../../i18n";
import { cx } from "../../../utils";
import s from "../Settings.module.css";

export const NotificationsTab: Component = () => {
  const sounds: { key: NotificationSound; label: string }[] = [
    { key: "question", label: t("notifications.sound.question", "Question") },
    { key: "error", label: t("notifications.sound.error", "Error") },
    { key: "completion", label: t("notifications.sound.completion", "Completion") },
    { key: "warning", label: t("notifications.sound.warning", "Warning") },
  ];

  return (
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
  );
};
