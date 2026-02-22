import { Component, Show, createEffect, onCleanup } from "solid-js";
import { openUrl } from "@tauri-apps/plugin-opener";
import { t } from "../../i18n";
import { isTauri } from "../../transport";
import s from "./HelpPanel.module.css";

const GITHUB_URL = "https://github.com/sstraus/tui-commander";
const DOCS_URL = "https://github.com/sstraus/tui-commander/wiki";

function handleOpenUrl(url: string) {
  if (isTauri()) {
    openUrl(url).catch((err) => console.error("Failed to open URL:", err));
  } else {
    window.open(url, "_blank");
  }
}

export interface HelpPanelProps {
  visible: boolean;
  onClose: () => void;
  onOpenShortcuts: () => void;
}

export const HelpPanel: Component<HelpPanelProps> = (props) => {
  // Close on Escape
  createEffect(() => {
    if (!props.visible) return;

    const handleKeydown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        props.onClose();
      }
    };

    document.addEventListener("keydown", handleKeydown);
    onCleanup(() => document.removeEventListener("keydown", handleKeydown));
  });

  return (
    <Show when={props.visible}>
      <div class={s.overlay} onClick={props.onClose}>
        <div class={s.panel} onClick={(e) => e.stopPropagation()}>
          <div class={s.header}>
            <h2>{t("helpPanel.title", "Help")}</h2>
            <button class={s.close} onClick={props.onClose}>
              &times;
            </button>
          </div>

          <div class={s.content}>
            <div class={s.section}>
              <h3 class={s.sectionTitle}>{t("helpPanel.aboutApp", "TUI Commander")}</h3>
              <p class={s.desc}>
                {t("helpPanel.appDescription", "A modern terminal multiplexer and Git worktree manager built with Tauri, SolidJS, and xterm.js.")}
              </p>
            </div>

            <div class={s.section}>
              <h3 class={s.sectionTitle}>{t("helpPanel.quickActions", "Quick Actions")}</h3>
              <div class={s.linkList}>
                <button class={s.linkButton} onClick={() => { props.onClose(); props.onOpenShortcuts(); }}>
                  <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor">
                    <path d="M3 1a2 2 0 0 0-2 2v10a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V3a2 2 0 0 0-2-2H3zm2.5 4a.5.5 0 0 1 .5.5v1a.5.5 0 0 1-.5.5h-1a.5.5 0 0 1-.5-.5v-1a.5.5 0 0 1 .5-.5h1zm3 0a.5.5 0 0 1 .5.5v1a.5.5 0 0 1-.5.5h-1a.5.5 0 0 1-.5-.5v-1a.5.5 0 0 1 .5-.5h1zm3 0a.5.5 0 0 1 .5.5v1a.5.5 0 0 1-.5.5h-1a.5.5 0 0 1-.5-.5v-1a.5.5 0 0 1 .5-.5h1zM5 9a.5.5 0 0 1 .5.5v1a.5.5 0 0 1-.5.5H4.5a.5.5 0 0 1-.5-.5v-1a.5.5 0 0 1 .5-.5H5zm6.5 0a.5.5 0 0 1 .5.5v1a.5.5 0 0 1-.5.5h-5a.5.5 0 0 1-.5-.5v-1a.5.5 0 0 1 .5-.5h5z"/>
                  </svg>
                  {t("helpPanel.keyboardShortcuts", "Keyboard Shortcuts")}
                </button>
              </div>
            </div>

            <div class={s.section}>
              <h3 class={s.sectionTitle}>{t("helpPanel.resources", "Resources")}</h3>
              <div class={s.linkList}>
                <button class={s.linkButton} onClick={() => handleOpenUrl(GITHUB_URL)}>
                  <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor">
                    <path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0016 8c0-4.42-3.58-8-8-8z"/>
                  </svg>
                  {t("helpPanel.githubProject", "GitHub Project")}
                </button>
                <button class={s.linkButton} onClick={() => handleOpenUrl(DOCS_URL)}>
                  <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor">
                    <path d="M1 2.828c.885-.37 2.154-.769 3.388-.893 1.33-.134 2.458.063 3.112.752v9.746c-.935-.53-2.12-.603-3.213-.493-1.18.12-2.37.461-3.287.811V2.828zm7.5-.141c.654-.689 1.782-.886 3.112-.752 1.234.124 2.503.523 3.388.893v9.923c-.918-.35-2.107-.692-3.287-.81-1.094-.111-2.278-.039-3.213.492V2.687zM8 1.783C7.015.936 5.587.81 4.287.94c-1.514.153-3.042.672-3.994 1.105A.5.5 0 000 2.5v10a.5.5 0 00.707.455c.882-.4 2.303-.881 3.68-1.02 1.409-.142 2.59.087 3.223.877a.5.5 0 00.78 0c.633-.79 1.814-1.019 3.222-.877 1.378.139 2.8.62 3.681 1.02A.5.5 0 0016 12.5v-10a.5.5 0 00-.293-.455c-.952-.433-2.48-.952-3.994-1.105C10.413.81 8.985.936 8 1.783z"/>
                  </svg>
                  {t("helpPanel.documentation", "Documentation")}
                </button>
                <button class={s.linkButton} onClick={() => handleOpenUrl(`${GITHUB_URL}/issues`)}>
                  <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor">
                    <path d="M8 9.5a1.5 1.5 0 100-3 1.5 1.5 0 000 3z"/>
                    <path d="M8 0a8 8 0 100 16A8 8 0 008 0zM1.5 8a6.5 6.5 0 1113 0 6.5 6.5 0 01-13 0z"/>
                  </svg>
                  {t("helpPanel.reportIssue", "Report an Issue")}
                </button>
              </div>
            </div>

            <div class={s.section}>
              <p class={s.menuNote}>
                {t("helpPanel.version", "Version")} {__APP_VERSION__}
              </p>
            </div>
          </div>
        </div>
      </div>
    </Show>
  );
};

export default HelpPanel;
