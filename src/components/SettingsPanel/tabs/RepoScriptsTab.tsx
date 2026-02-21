import { Component } from "solid-js";
import type { RepoTabProps } from "./RepoWorktreeTab";

export const RepoScriptsTab: Component<RepoTabProps> = (props) => {
  const setupPlaceholder = () =>
    props.settings.setupScript === null && props.defaults.setupScript
      ? `Inheriting from global: ${props.defaults.setupScript}`
      : "#!/bin/bash\nnpm install";

  const runPlaceholder = () =>
    props.settings.runScript === null && props.defaults.runScript
      ? `Inheriting from global: ${props.defaults.runScript}`
      : "#!/bin/bash\nnpm run dev";

  return (
    <div class="settings-section">
      <h3>Automation Scripts</h3>

      <div class="settings-group">
        <label>Setup Script</label>
        <textarea
          value={props.settings.setupScript ?? ""}
          onInput={(e) => {
            const val = e.currentTarget.value;
            props.onUpdate("setupScript", val === "" ? null : val);
          }}
          placeholder={setupPlaceholder()}
          rows={6}
        />
        <p class="settings-hint">
          Executed once after a new worktree is created (e.g., npm install).
          {props.settings.setupScript === null ? " Leave empty to use global default." : ""}
        </p>
      </div>

      <div class="settings-group">
        <label>Run Script</label>
        <textarea
          value={props.settings.runScript ?? ""}
          onInput={(e) => {
            const val = e.currentTarget.value;
            props.onUpdate("runScript", val === "" ? null : val);
          }}
          placeholder={runPlaceholder()}
          rows={6}
        />
        <p class="settings-hint">
          On-demand script launchable from the toolbar (e.g., npm run dev).
          {props.settings.runScript === null ? " Leave empty to use global default." : ""}
        </p>
      </div>
    </div>
  );
};
