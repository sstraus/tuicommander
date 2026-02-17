import { Component, For, Show, type JSX } from "solid-js";

export interface SettingsShellTab {
  key: string;
  label: string;
}

export interface SettingsShellProps {
  visible: boolean;
  onClose: () => void;
  title: string;
  subtitle?: string;
  icon?: string;
  tabs: SettingsShellTab[];
  activeTab: string;
  onTabChange: (key: string) => void;
  footer?: JSX.Element;
  children: JSX.Element;
}

/** Shared shell for settings panels: overlay → panel → header → tabs → content */
export const SettingsShell: Component<SettingsShellProps> = (props) => {
  const hasRepoHeader = () => !!props.icon || !!props.subtitle;

  return (
    <Show when={props.visible}>
      <div class="settings-overlay" onClick={props.onClose}>
        <div class="settings-panel" onClick={(e) => e.stopPropagation()}>
          {/* Header */}
          <div class={`settings-header${hasRepoHeader() ? " settings-header--repo" : ""}`}>
            <Show
              when={hasRepoHeader()}
              fallback={<h2>{props.title}</h2>}
            >
              <div class="settings-title--repo">
                <Show when={props.icon}>
                  <span class="settings-icon--repo">{props.icon}</span>
                </Show>
                <div>
                  <h2>{props.title}</h2>
                  <Show when={props.subtitle}>
                    <p class="settings-path--repo">{props.subtitle}</p>
                  </Show>
                </div>
              </div>
            </Show>
            <button class="settings-close" onClick={props.onClose}>
              &times;
            </button>
          </div>

          {/* Tabs */}
          <div class="settings-tabs">
            <For each={props.tabs}>
              {(tab) =>
                tab.key === "__sep__" ? (
                  <span class="settings-tab-separator" />
                ) : (
                  <button
                    class={`settings-tab ${props.activeTab === tab.key ? "active" : ""}`}
                    onClick={() => props.onTabChange(tab.key)}
                  >
                    {tab.label}
                  </button>
                )
              }
            </For>
          </div>

          {/* Content */}
          <div class="settings-content">
            {props.children}
          </div>

          {/* Footer (optional) */}
          {props.footer}
        </div>
      </div>
    </Show>
  );
};
