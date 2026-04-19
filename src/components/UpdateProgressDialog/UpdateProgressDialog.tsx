import { Component, Show } from "solid-js";
import { updaterStore } from "../../stores/updater";
import d from "../shared/dialog.module.css";
import s from "./UpdateProgressDialog.module.css";

/**
 * Modal dialog showing update download progress.
 * Appears automatically when updaterStore.state.downloading is true.
 * No dismiss — download completes then app relaunches.
 */
export const UpdateProgressDialog: Component = () => {
  const pct = () => updaterStore.state.progress;
  const version = () => updaterStore.state.version ?? "";
  const error = () => updaterStore.state.error;

  return (
    <Show when={updaterStore.state.downloading || error()}>
      <div class={d.overlay}>
        <div class={d.popover} onClick={(e) => e.stopPropagation()}>
          <div class={d.header}>
            <span class={d.headerIcon}>
              <svg viewBox="0 0 16 16" width="16" height="16" fill="currentColor">
                <path d="M8 16A8 8 0 1 1 8 0a8 8 0 0 1 0 16zm1-11H7v4H5l3 3 3-3H9V5z"/>
              </svg>
            </span>
            <h4>{error() ? "Update failed" : `Updating to v${version()}`}</h4>
          </div>
          <div class={d.body}>
            <Show when={!error()} fallback={
              <p class={d.error}>{error()}</p>
            }>
              <div class={s.progressContainer}>
                <div class={s.progressBar}>
                  <div class={s.progressFill} style={{ width: `${pct()}%` }} />
                </div>
                <span class={s.progressText}>{pct()}%</span>
              </div>
              <p class={s.hint}>
                {pct() < 100
                  ? "Downloading update..."
                  : "Installing — app will restart shortly"}
              </p>
            </Show>
          </div>
          <Show when={error()}>
            <div class={d.actions}>
              <button class={d.cancelBtn} onClick={() => updaterStore.dismiss()}>
                Close
              </button>
            </div>
          </Show>
        </div>
      </div>
    </Show>
  );
};

export default UpdateProgressDialog;
