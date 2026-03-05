import { Component, For, Show, createEffect, createSignal, onCleanup } from "solid-js";
import d from "../shared/dialog.module.css";
import s from "./PostMergeCleanupDialog.module.css";

export type StepId = "switch" | "pull" | "delete-local" | "delete-remote";
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
  onExecute: (steps: CleanupStep[]) => void;
  onSkip: () => void;
  /** When true, checkboxes and buttons are disabled */
  executing?: boolean;
  /** Per-step status overrides (driven by the execution hook) */
  stepStatuses?: Partial<Record<StepId, StepStatus>>;
  /** Per-step error messages */
  stepErrors?: Partial<Record<StepId, string>>;
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
): CleanupStep[] {
  return [
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
  ];
}

export const PostMergeCleanupDialog: Component<PostMergeCleanupDialogProps> = (props) => {
  const [steps, setSteps] = createSignal<CleanupStep[]>(
    buildInitialSteps(props.baseBranch, props.isOnBaseBranch, props.isDefaultBranch),
  );

  const toggleStep = (id: StepId) => {
    if (props.executing) return;
    setSteps((prev) =>
      prev.map((step) =>
        step.id === id && !step.disabled ? { ...step, checked: !step.checked } : step,
      ),
    );
  };

  const handleExecute = () => {
    props.onExecute(steps());
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
                        <span data-testid={`step-label-${step.id}`}>{step.label}</span>
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
