import { Component, For, Show, createEffect, createSignal, onCleanup } from "solid-js";
import d from "../shared/dialog.module.css";
import s from "./PostMergeCleanupDialog.module.css";

export type StepId = "worktree" | "switch" | "pull" | "delete-local" | "delete-remote";
export type StepStatus = "pending" | "running" | "success" | "error" | "skipped";

export interface CleanupStep {
  id: StepId;
  label: string;
  checked: boolean;
  disabled?: boolean;
}

export interface PostMergeCleanupDialogProps {
  branchName: string;
  baseBranch: string;
  repoPath: string;
  isOnBaseBranch: boolean;
  isDefaultBranch: boolean;
  hasTerminals: boolean;
  /** Working directory has uncommitted changes */
  hasDirtyFiles?: boolean;
  onExecute: (steps: CleanupStep[], options?: { unstash?: boolean }) => void;
  onSkip: () => void;
  /** When true, checkboxes and buttons are disabled */
  executing?: boolean;
  /** Per-step status overrides (driven by the execution hook) */
  stepStatuses?: Partial<Record<StepId, StepStatus>>;
  /** Per-step error messages */
  stepErrors?: Partial<Record<StepId, string>>;
  /** When set, adds a worktree archive/delete step as the first step */
  worktreeAction?: "archive" | "delete";
  /** Called when the user toggles between archive/delete for the worktree step */
  onWorktreeActionChange?: (action: "archive" | "delete") => void;
}

const STATUS_ICONS: Record<StepStatus, string> = {
  pending: "",
  running: "\u25CF", // filled circle
  success: "\u2713", // checkmark
  error: "\u2717",   // cross
  skipped: "\u2013",  // en dash
};

function buildInitialSteps(
  baseBranch: string,
  isOnBaseBranch: boolean,
  isDefaultBranch: boolean,
  worktreeAction?: "archive" | "delete",
): CleanupStep[] {
  const steps: CleanupStep[] = [];

  if (worktreeAction) {
    steps.push({
      id: "worktree",
      label: worktreeAction === "archive" ? "Archive worktree" : "Delete worktree",
      checked: true,
    });
  }

  steps.push(
    {
      id: "switch",
      label: `Switch to ${baseBranch}`,
      checked: !isOnBaseBranch,
    },
    {
      id: "pull",
      label: `Pull ${baseBranch} (ff-only)`,
      checked: !isOnBaseBranch,
    },
    {
      id: "delete-local",
      label: "Delete local branch",
      checked: !isDefaultBranch,
      disabled: isDefaultBranch,
    },
    {
      id: "delete-remote",
      label: "Delete remote branch",
      checked: true,
    },
  );

  return steps;
}

export const PostMergeCleanupDialog: Component<PostMergeCleanupDialogProps> = (props) => {
  const [steps, setSteps] = createSignal<CleanupStep[]>(
    buildInitialSteps(props.baseBranch, props.isOnBaseBranch, props.isDefaultBranch, props.worktreeAction),
  );
  const [unstash, setUnstash] = createSignal(false);

  const toggleStep = (id: StepId) => {
    if (props.executing) return;
    setSteps((prev) =>
      prev.map((step) =>
        step.id === id && !step.disabled ? { ...step, checked: !step.checked } : step,
      ),
    );
  };

  const handleExecute = () => {
    props.onExecute(steps(), { unstash: unstash() });
  };

  createEffect(() => {
    const handleKeydown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        props.onSkip();
      }
    };
    document.addEventListener("keydown", handleKeydown);
    onCleanup(() => document.removeEventListener("keydown", handleKeydown));
  });

  return (
    <div class={d.overlay} onClick={props.onSkip}>
      <div class={d.popover} onClick={(e) => e.stopPropagation()}>
        <div class={d.header}>
          <h4>Post-merge cleanup</h4>
        </div>
        <div class={d.body}>
          <p class={s.subtitle}>
            <code>{props.branchName}</code> merged into <code>{props.baseBranch}</code>
          </p>
          <ul class={s.stepList}>
            <For each={steps()}>
              {(step) => {
                const status = () => props.stepStatuses?.[step.id] ?? "pending";
                const error = () => props.stepErrors?.[step.id];
                return (
                  <li>
                    <div class={s.step}>
                      <label class={step.disabled ? s.disabled : undefined}>
                        <input
                          type="checkbox"
                          data-testid={`step-check-${step.id}`}
                          checked={step.checked}
                          disabled={step.disabled || !!props.executing}
                          onChange={() => toggleStep(step.id)}
                        />
                        <Show when={step.id === "worktree" && props.onWorktreeActionChange}
                          fallback={<span data-testid={`step-label-${step.id}`}>{step.label}</span>}
                        >
                          <select
                            data-testid={`step-label-${step.id}`}
                            class={s.worktreeSelect}
                            value={props.worktreeAction}
                            disabled={!!props.executing}
                            onChange={(e) => {
                              const action = e.currentTarget.value as "archive" | "delete";
                              props.onWorktreeActionChange?.(action);
                              setSteps((prev) =>
                                prev.map((st) =>
                                  st.id === "worktree"
                                    ? { ...st, label: action === "archive" ? "Archive worktree" : "Delete worktree" }
                                    : st,
                                ),
                              );
                            }}
                            onClick={(e) => e.stopPropagation()}
                          >
                            <option value="archive">Archive worktree</option>
                            <option value="delete">Delete worktree</option>
                          </select>
                        </Show>
                      </label>
                      <Show when={status() !== "pending"}>
                        <span
                          class={`${s.stepStatus} ${s[status()]}`}
                          data-testid={`step-status-${step.id}`}
                        >
                          {STATUS_ICONS[status()]}
                        </span>
                      </Show>
                    </div>
                    <Show when={step.id === "switch" && step.checked && props.hasDirtyFiles}>
                      <div class={s.dirtyWarning} data-testid="dirty-warning">
                        Uncommitted changes will be stashed before switching.
                        <label class={s.unstashOption}>
                          <input
                            type="checkbox"
                            data-testid="unstash-check"
                            checked={unstash()}
                            disabled={!!props.executing}
                            onChange={() => setUnstash((v) => !v)}
                          />
                          <span>Unstash after switch</span>
                        </label>
                      </div>
                    </Show>
                    <Show when={error()}>
                      <div class={s.stepError} data-testid={`step-error-${step.id}`}>
                        {error()}
                      </div>
                    </Show>
                  </li>
                );
              }}
            </For>
          </ul>
        </div>
        <div class={d.actions}>
          <button
            class={d.cancelBtn}
            data-testid="skip-btn"
            onClick={props.onSkip}
            disabled={!!props.executing}
          >
            Skip
          </button>
          <button
            class={d.primaryBtn}
            data-testid="execute-btn"
            onClick={handleExecute}
            disabled={!!props.executing}
          >
            Execute
          </button>
        </div>
      </div>
    </div>
  );
};
