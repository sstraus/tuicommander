import { Component, For, Show, type JSX } from "solid-js";

const NAV_MIN_WIDTH = 140;
const NAV_MAX_WIDTH = 280;
const NAV_DEFAULT_WIDTH = 180;

export interface SettingsShellTab {
  key: string;
  label: string;
  color?: string;
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
  /** Width of the nav sidebar in px (persisted externally) */
  navWidth?: number;
  /** Called on every pixel during nav resize drag (update state only) */
  onNavWidthChange?: (width: number) => void;
  /** Called once on mouseup after nav resize drag (persist to disk) */
  onNavWidthPersist?: () => void;
  footer?: JSX.Element;
  children: JSX.Element;
}

/** Shared shell for settings panels: overlay → panel → header → (nav | content) */
export const SettingsShell: Component<SettingsShellProps> = (props) => {
  const hasRepoHeader = () => !!props.icon || !!props.subtitle;
  const navWidth = () => props.navWidth ?? NAV_DEFAULT_WIDTH;

  const handleNavResizeStart = (e: MouseEvent) => {
    e.preventDefault();
    const startX = e.clientX;
    const startWidth = navWidth();

    const onMove = (ev: MouseEvent) => {
      const delta = ev.clientX - startX;
      const clamped = Math.min(NAV_MAX_WIDTH, Math.max(NAV_MIN_WIDTH, startWidth + delta));
      props.onNavWidthChange?.(clamped);
    };

    const onUp = () => {
      document.removeEventListener("mousemove", onMove);
      document.removeEventListener("mouseup", onUp);
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
      props.onNavWidthPersist?.();
    };

    document.addEventListener("mousemove", onMove);
    document.addEventListener("mouseup", onUp);
    document.body.style.cursor = "col-resize";
    document.body.style.userSelect = "none";
  };

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

          {/* Body: nav sidebar + scrollable content */}
          <div class="settings-body">
            <nav class="settings-nav" style={{ width: `${navWidth()}px` }}>
              <For each={props.tabs}>
                {(tab) =>
                  tab.key === "__sep__" ? (
                    <div class="settings-nav-separator" />
                  ) : tab.key.startsWith("__label__:") ? (
                    <div class="settings-nav-label">{tab.label}</div>
                  ) : (
                    <button
                      class={`settings-nav-item${tab.key.startsWith("repo:") ? " settings-nav-item--repo" : ""}${props.activeTab === tab.key ? " active" : ""}`}
                      style={tab.color ? { color: tab.color } : undefined}
                      onClick={() => props.onTabChange(tab.key)}
                    >
                      {tab.label}
                    </button>
                  )
                }
              </For>
              <div class="settings-nav-resize-handle" onMouseDown={handleNavResizeStart} />
            </nav>

            {/* Content */}
            <div class="settings-content">
              {props.children}
            </div>
          </div>

          {/* Footer (optional) */}
          {props.footer}
        </div>
      </div>
    </Show>
  );
};
