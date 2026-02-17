import { Component, For, Show } from "solid-js";
import { notificationsStore } from "../../../stores/notifications";
import type { NotificationSound } from "../../../notifications";

export const NotificationsTab: Component = () => {
  const sounds: { key: NotificationSound; label: string }[] = [
    { key: "question", label: "Agent asks question" },
    { key: "error", label: "Error occurred" },
    { key: "completion", label: "Task completed" },
    { key: "warning", label: "Warning" },
  ];

  return (
    <div class="settings-section">
      <h3>Notification Settings</h3>

      <Show
        when={notificationsStore.state.isAvailable}
        fallback={
          <p class="settings-warning">
            Audio notifications are not available in this browser or environment.
          </p>
        }
      >
        <div class="settings-group">
          <label>Enable Audio Notifications</label>
          <div class="settings-toggle">
            <input
              type="checkbox"
              checked={notificationsStore.state.config.enabled}
              onChange={(e) => notificationsStore.setEnabled(e.currentTarget.checked)}
            />
            <span>Play sounds for agent events and system notifications</span>
          </div>
        </div>

        <div class="settings-group">
          <label>Master Volume</label>
          <div class="settings-slider">
            <input
              type="range"
              min="0"
              max="100"
              value={Math.round(notificationsStore.state.config.volume * 100)}
              onInput={(e) => notificationsStore.setVolume(parseInt(e.currentTarget.value) / 100)}
            />
            <span>{Math.round(notificationsStore.state.config.volume * 100)}%</span>
          </div>
          <p class="settings-hint">Adjust the volume level for all notification sounds</p>
        </div>

        <div class="settings-group">
          <label>Notification Events</label>
          <p class="settings-hint" style={{ "margin-bottom": "12px" }}>
            Choose which events trigger sound notifications
          </p>
          <For each={sounds}>
            {(sound) => (
              <div class="settings-sound-row">
                <div class="settings-toggle">
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
                  class="settings-test-btn"
                  onClick={() => notificationsStore.testSound(sound.key)}
                >
                  Test
                </button>
              </div>
            )}
          </For>
        </div>

        <div class="settings-actions">
          <button onClick={() => notificationsStore.reset()}>Reset to Defaults</button>
        </div>
      </Show>
    </div>
  );
};
