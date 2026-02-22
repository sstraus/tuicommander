import { Component, For, Show, type JSX } from "solid-js";
import { cx } from "../../utils";
import s from "./Settings.module.css";

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
      <div class={s.overlay} onClick={props.onClose}>
        <div class={s.panel} onClick={(e) => e.stopPropagation()}>
          {/* Header */}
          <div class={cx(s.header, hasRepoHeader() && s.headerRepo)}>
            <Show
              when={hasRepoHeader()}
              fallback={<h2>{props.title}</h2>}
            >
              <div class={s.titleRepo}>
                <Show when={props.icon}>
                  <span class={s.iconRepo}>{props.icon}</span>
                </Show>
                <div>
                  <h2>{props.title}</h2>
                  <Show when={props.subtitle}>
                    <p class={s.pathRepo}>{props.subtitle}</p>
                  </Show>
                </div>
              </div>
            </Show>
            <button class={s.close} onClick={props.onClose}>
              &times;
            </button>
          </div>

          {/* Body: nav sidebar + scrollable content */}
          <div class={s.body}>
            <nav class={s.nav} style={{ width: `${navWidth()}px` }}>
              <For each={props.tabs}>
                {(tab) =>
                  tab.key === "__sep__" ? (
                    <div class={s.navSeparator} />
                  ) : tab.key.startsWith("__label__:") ? (
                    <div class={s.navLabel}>{tab.label}</div>
                  ) : (
                    <button
                      class={cx(s.navItem, tab.key.startsWith("repo:") && s.navItemRepo, props.activeTab === tab.key && s.active)}
                      style={tab.color ? { color: tab.color } : undefined}
                      onClick={() => props.onTabChange(tab.key)}
                    >
                      {tab.label}
                    </button>
                  )
                }
              </For>
              <div class={s.navResizeHandle} onMouseDown={handleNavResizeStart} />
            </nav>

            {/* Content */}
            <div class={s.content}>
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
