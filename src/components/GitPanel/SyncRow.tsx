import { Component, Show, createSignal, onCleanup } from "solid-js";
import { invoke } from "../../invoke";
import { repositoriesStore } from "../../stores/repositories";
import { appLogger } from "../../stores/appLogger";
import { cx } from "../../utils";
import s from "./SyncRow.module.css";

/** Mirrors the Rust WorkingTreeStatus struct returned by get_working_tree_status */
export interface WorkingTreeStatus {
  branch: string | null;
  upstream: string | null;
  ahead: number;
  behind: number;
  stash_count: number;
  staged: StatusEntry[];
  unstaged: StatusEntry[];
  untracked: string[];
}

interface StatusEntry {
  path: string;
  status: string;
}

/** Result from run_git_command */
interface GitCommandResult {
  success: boolean;
  stdout: string;
  stderr: string;
  exit_code: number;
}

export interface SyncRowProps {
  repoPath: string;
  status: WorkingTreeStatus;
}

/* Inline SVG icons — monochrome, fill="currentColor" per STYLE_GUIDE */

const PullIcon = () => (
  <svg viewBox="0 0 16 16" fill="currentColor">
    <path d="M8 12l-4-4h2.5V3h3v5H12L8 12z" />
  </svg>
);

const PushIcon = () => (
  <svg viewBox="0 0 16 16" fill="currentColor">
    <path d="M8 3l4 4H9.5v5h-3V7H4l4-4z" />
  </svg>
);

const FetchIcon = () => (
  <svg viewBox="0 0 16 16" fill="currentColor">
    <path d="M8 2a6 6 0 100 12A6 6 0 008 2zm0 1a5 5 0 014.9 4H11l-3 3.5L5 7H3.1A5 5 0 018 3z" />
  </svg>
);

const StashIcon = () => (
  <svg viewBox="0 0 16 16" fill="currentColor">
    <path d="M3 3h10v2H3V3zm1 3h8v2H4V6zm1 3h6v2H5V9zm1 3h4v2H6v-2z" />
  </svg>
);

const FEEDBACK_DISMISS_MS = 5000;

interface SyncOperation {
  id: string;
  label: string;
  icon: () => ReturnType<Component>;
  args: string[];
  title: string;
}

const OPERATIONS: SyncOperation[] = [
  { id: "pull", label: "Pull", icon: PullIcon, args: ["pull"], title: "Fetch and merge changes from remote" },
  { id: "push", label: "Push", icon: PushIcon, args: ["push"], title: "Push local commits to remote" },
  { id: "fetch", label: "Fetch", icon: FetchIcon, args: ["fetch", "--all"], title: "Download remote changes without merging" },
  { id: "stash", label: "Stash", icon: StashIcon, args: ["stash", "push"], title: "Stash current changes" },
];

export const SyncRow: Component<SyncRowProps> = (props) => {
  const [runningOp, setRunningOp] = createSignal<string | null>(null);
  const [feedback, setFeedback] = createSignal<{ success: boolean; message: string } | null>(null);

  let dismissTimer: ReturnType<typeof setTimeout> | undefined;
  onCleanup(() => { if (dismissTimer) clearTimeout(dismissTimer); });

  const changedCount = () =>
    props.status.staged.length + props.status.unstaged.length + props.status.untracked.length;

  const runOperation = async (op: SyncOperation) => {
    setRunningOp(op.id);
    setFeedback(null);
    if (dismissTimer) clearTimeout(dismissTimer);

    try {
      const result = await invoke<GitCommandResult>("run_git_command", {
        path: props.repoPath,
        args: op.args,
      });

      if (result.success) {
        const msg = result.stdout.trim().split("\n")[0] || "Done";
        setFeedback({ success: true, message: msg.slice(0, 120) });
        repositoriesStore.bumpRevision(props.repoPath);
      } else {
        const msg = result.stderr.trim().split("\n")[0] || `Exit code ${result.exit_code}`;
        setFeedback({ success: false, message: msg.slice(0, 200) });
      }
      dismissTimer = setTimeout(() => setFeedback(null), FEEDBACK_DISMISS_MS);
    } catch (err) {
      setFeedback({ success: false, message: String(err) });
      appLogger.error("git", `SyncRow: failed to run git ${op.id}`, err);
      dismissTimer = setTimeout(() => setFeedback(null), FEEDBACK_DISMISS_MS);
    } finally {
      setRunningOp(null);
    }
  };

  const isRunning = () => runningOp() !== null;

  return (
    <div class={s.syncRow}>
      {/* Status line */}
      <div class={s.statusLine}>
        <Show
          when={props.status.branch}
          fallback={<span class={cx(s.branch, s.detached)}>(detached)</span>}
        >
          <span class={s.branch} title={props.status.branch!}>
            {props.status.branch}
          </span>
        </Show>

        <Show when={props.status.ahead > 0 || props.status.behind > 0}>
          <Show when={props.status.ahead > 0}>
            <span class={s.ahead} title="Commits ahead of upstream">
              {"\u2191"}{props.status.ahead}
            </span>
          </Show>
          <Show when={props.status.behind > 0}>
            <span class={s.behind} title="Commits behind upstream">
              {"\u2193"}{props.status.behind}
            </span>
          </Show>
        </Show>

        <Show when={changedCount() > 0}>
          <span class={s.changed} title="Changed files">
            {changedCount()} changed
          </span>
        </Show>

        <Show when={props.status.stash_count > 0}>
          <span class={s.stash} title="Stash entries">
            {props.status.stash_count} stash
          </span>
        </Show>
      </div>

      {/* Action buttons */}
      <div class={s.actions}>
        {OPERATIONS.map((op) => (
          <button
            class={cx(s.btn, runningOp() === op.id && s.btnRunning)}
            onClick={() => runOperation(op)}
            disabled={isRunning()}
            title={op.title}
          >
            <span class={s.btnIcon}>
              <Show when={runningOp() === op.id} fallback={op.icon()}>
                <span class={s.spinner} />
              </Show>
            </span>
            {op.label}
          </button>
        ))}
      </div>

      {/* Feedback message */}
      <Show when={feedback()}>
        {(fb) => (
          <div class={cx(s.feedback, fb().success ? s.feedbackSuccess : s.feedbackError)}>
            {fb().message}
          </div>
        )}
      </Show>
    </div>
  );
};

export default SyncRow;
