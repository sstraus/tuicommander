import { Component } from "solid-js";
import { openUrl } from "@tauri-apps/plugin-opener";
import { t } from "../../../i18n";
import { isTauri } from "../../../transport";
import s from "../Settings.module.css";

const APP_VERSION = __APP_VERSION__;
const GITHUB_URL = "https://github.com/sstraus/tui-commander";
const DOCS_URL = "https://github.com/sstraus/tui-commander/wiki";

function handleOpenUrl(url: string) {
  if (isTauri()) {
    openUrl(url).catch((err) => console.error("Failed to open URL:", err));
  } else {
    window.open(url, "_blank");
  }
}

export const AboutTab: Component = () => {
  return (
    <div class={s.section}>
      <h3>{t("about.title", "About TUI Commander")}</h3>

      <div class={s.group}>
        <label>{t("about.version", "Version")}</label>
        <p class={s.hint}>{APP_VERSION}</p>
      </div>

      <div class={s.group}>
        <label>{t("about.description", "Description")}</label>
        <p class={s.hint}>
          {t("about.descriptionText", "A modern terminal multiplexer and Git worktree manager built with Tauri, SolidJS, and xterm.js.")}
        </p>
      </div>

      <div class={s.group}>
        <label>{t("about.links", "Links")}</label>
        <div style={{ display: "flex", gap: "8px", "flex-wrap": "wrap" }}>
          <button class={s.testBtn} onClick={() => handleOpenUrl(GITHUB_URL)}>
            {t("about.github", "GitHub Repository")}
          </button>
          <button class={s.testBtn} onClick={() => handleOpenUrl(DOCS_URL)}>
            {t("about.documentation", "Documentation")}
          </button>
          <button class={s.testBtn} onClick={() => handleOpenUrl(`${GITHUB_URL}/issues`)}>
            {t("about.reportIssue", "Report an Issue")}
          </button>
        </div>
      </div>

      <div class={s.group}>
        <label>{t("about.license", "License")}</label>
        <p class={s.hint}>MIT License</p>
      </div>

      <div class={s.group}>
        <label>{t("about.credits", "Built With")}</label>
        <p class={s.hint}>
          Tauri 2 &middot; SolidJS &middot; xterm.js &middot; Rust
        </p>
      </div>

      <div class={s.group}>
        <p class={s.hint}>&copy; 2026 Stefano Straus &middot; stefano@straus.it</p>
      </div>
    </div>
  );
};
