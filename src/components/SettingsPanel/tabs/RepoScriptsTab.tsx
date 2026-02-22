import { Component } from "solid-js";
import type { RepoTabProps } from "./RepoWorktreeTab";
import { t } from "../../../i18n";
import s from "../Settings.module.css";

export const RepoScriptsTab: Component<RepoTabProps> = (props) => {
  const setupPlaceholder = () =>
    props.settings.setupScript === null && props.defaults.setupScript
      ? `${t("repoScripts.placeholder.inheriting", "Inheriting:")} ${props.defaults.setupScript}`
      : "#!/bin/bash\nnpm install";

  const runPlaceholder = () =>
    props.settings.runScript === null && props.defaults.runScript
      ? `${t("repoScripts.placeholder.inheriting", "Inheriting:")} ${props.defaults.runScript}`
      : "#!/bin/bash\nnpm run dev";

  return (
    <div class={s.section}>
      <h3>{t("repoScripts.heading.automationScripts", "Automation Scripts")}</h3>

      <div class={s.group}>
        <label>{t("repoScripts.label.setupScript", "Setup Script")}</label>
        <textarea
          value={props.settings.setupScript ?? ""}
          onInput={(e) => {
            const val = e.currentTarget.value;
            props.onUpdate("setupScript", val === "" ? null : val);
          }}
          placeholder={setupPlaceholder()}
          rows={6}
        />
        <p class={s.hint}>
          {t("repoScripts.hint.setupScript", "Shell script run when creating a new worktree.")}
          {props.settings.setupScript === null ? ` ${t("repoScripts.hint.useGlobalDefault", "Using global default.")}` : ""}
        </p>
      </div>

      <div class={s.group}>
        <label>{t("repoScripts.label.runScript", "Run Script")}</label>
        <textarea
          value={props.settings.runScript ?? ""}
          onInput={(e) => {
            const val = e.currentTarget.value;
            props.onUpdate("runScript", val === "" ? null : val);
          }}
          placeholder={runPlaceholder()}
          rows={6}
        />
        <p class={s.hint}>
          {t("repoScripts.hint.runScript", "Shell script run when launching the worktree.")}
          {props.settings.runScript === null ? ` ${t("repoScripts.hint.useGlobalDefault", "Using global default.")}` : ""}
        </p>
      </div>
    </div>
  );
};
