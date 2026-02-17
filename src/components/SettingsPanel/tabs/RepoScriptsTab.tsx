import { Component } from "solid-js";
import type { RepoTabProps } from "./RepoWorktreeTab";

export const RepoScriptsTab: Component<RepoTabProps> = (props) => {
  return (
    <div class="settings-section">
      <h3>Automation Scripts</h3>

      <div class="settings-group">
        <label>Setup Script</label>
        <textarea
          value={props.settings.setupScript ?? ""}
          onInput={(e) => props.onUpdate("setupScript", e.currentTarget.value)}
          placeholder="#!/bin/bash&#10;npm install"
          rows={6}
        />
        <p class="settings-hint">
          Executed once after a new worktree is created (e.g., npm install)
        </p>
      </div>

      <div class="settings-group">
        <label>Run Script</label>
        <textarea
          value={props.settings.runScript ?? ""}
          onInput={(e) => props.onUpdate("runScript", e.currentTarget.value)}
          placeholder="#!/bin/bash&#10;npm run dev"
          rows={6}
        />
        <p class="settings-hint">
          On-demand script launchable from the toolbar (e.g., npm run dev)
        </p>
      </div>
    </div>
  );
};
