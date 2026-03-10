import { Component, Show, For, createSignal, createEffect, onCleanup } from "solid-js";
import { invoke } from "../../invoke";
import { terminalsStore } from "../../stores/terminals";
import { repositoriesStore } from "../../stores/repositories";
import { appLogger } from "../../stores/appLogger";
import { escapeShellArg, isValidBranchName, isValidPath } from "../../utils";
import { t } from "../../i18n";
import { cx } from "../../utils";
import { BranchCombobox } from "../shared/BranchCombobox";
import {
  PullIcon, PushIcon, FetchIcon, MergeIcon, BranchIcon,
  StashIcon, NewBranchIcon, WarningIcon, CheckIcon, ErrorIcon, CloseIcon,
} from "./icons";
import s from "./GitOperationsPanel.module.css";

/** GitPanelContext returned by the Rust backend */
interface GitPanelContext {
  branch: string;
  is_detached: boolean;
  status: string;
  ahead: number | null;
  behind: number | null;
  staged_count: number;
  changed_count: number;
  stash_count: number;
  last_commit: { hash: string; short_hash: string; subject: string } | null;
  in_rebase: boolean;
  in_cherry_pick: boolean;
}

/** Result from run_git_command */
interface GitCommandResult {
  success: boolean;
  stdout: string;
  stderr: string;
  exit_code: number;
}

const STATUS_CLASSES: Record<string, string> = {
  clean: s.stateClean,
  dirty: s.stateDirty,
  conflict: s.stateConflict,
  merge: s.stateMerge,
  unknown: s.stateUnknown,
};

export interface GitOperationsPanelProps {
  visible: boolean;
  repoPath: string | null;
  onClose: () => void;
  onBranchChange?: () => void;
}

interface GitOperation {
  id: string;
  label: string;
  Icon: Component;
  args: (branch?: string) => string[];
  requiresBranch?: boolean;
  description: string;
}

const SYNC_OPERATIONS: GitOperation[] = [
  {
    id: "pull",
    label: "Pull",
    Icon: PullIcon,
    args: () => ["pull"],
    description: "Fetch and merge changes from remote",
  },
  {
    id: "push",
    label: "Push",
    Icon: PushIcon,
    args: () => ["push"],
    description: "Push local commits to remote",
  },
  {
    id: "fetch",
    label: "Fetch",
    Icon: FetchIcon,
    args: () => ["fetch", "--all"],
    description: "Download remote changes without merging",
  },
];

const BRANCH_OPERATIONS: GitOperation[] = [
  {
    id: "switch",
    label: "Switch",
    Icon: BranchIcon,
    args: (branch) => ["checkout", branch || ""],
    requiresBranch: true,
    description: "Switch to selected branch",
  },
  {
    id: "merge",
    label: "Merge",
    Icon: MergeIcon,
    args: (branch) => ["merge", branch || ""],
    requiresBranch: true,
    description: "Merge selected branch into current",
  },
];

const STASH_OPERATIONS: GitOperation[] = [
  {
    id: "stash",
    label: "Stash",
    Icon: StashIcon,
    args: () => ["stash"],
    description: "Stash current changes",
  },
  {
    id: "pop",
    label: "Pop",
    Icon: StashIcon,
    args: () => ["stash", "pop"],
    description: "Apply and remove latest stash",
  },
];

/** Terminal-injected actions for conflict resolution (need editor interaction) */
const MERGE_ACTIONS = [
  { id: "abort", label: "Abort", command: (repo: string) => `cd ${escapeShellArg(repo)} && git merge --abort` },
  { id: "continue", label: "Continue", command: (repo: string) => `cd ${escapeShellArg(repo)} && git merge --continue` },
];

const REBASE_ACTIONS = [
  { id: "abort", label: "Abort", command: (repo: string) => `cd ${escapeShellArg(repo)} && git rebase --abort` },
  { id: "continue", label: "Continue", command: (repo: string) => `cd ${escapeShellArg(repo)} && git rebase --continue` },
  { id: "skip", label: "Skip", command: (repo: string) => `cd ${escapeShellArg(repo)} && git rebase --skip` },
];

const CHERRY_PICK_ACTIONS = [
  { id: "abort", label: "Abort", command: (repo: string) => `cd ${escapeShellArg(repo)} && git cherry-pick --abort` },
  { id: "continue", label: "Continue", command: (repo: string) => `cd ${escapeShellArg(repo)} && git cherry-pick --continue` },
];

const FEEDBACK_DISMISS_MS = 5000;

interface OperationFeedback {
  success: boolean;
  message: string;
}

export const GitOperationsPanel: Component<GitOperationsPanelProps> = (props) => {
  const [ctx, setCtx] = createSignal<GitPanelContext | null>(null);
  const [selectedBranch, setSelectedBranch] = createSignal("");
  const [branches, setBranches] = createSignal<string[]>([]);
  const [loading, setLoading] = createSignal(false);
  const [isRunning, setIsRunning] = createSignal(false);
  const [runningOp, setRunningOp] = createSignal<string | null>(null);
  const [feedback, setFeedback] = createSignal<OperationFeedback | null>(null);
  const [showNewBranch, setShowNewBranch] = createSignal(false);
  const [newBranchName, setNewBranchName] = createSignal("");
  const [newBranchError, setNewBranchError] = createSignal<string | null>(null);
  const [creatingBranch, setCreatingBranch] = createSignal(false);

  let dismissTimer: ReturnType<typeof setTimeout> | undefined;

  onCleanup(() => {
    if (dismissTimer) clearTimeout(dismissTimer);
  });

  /** Run a git operation via run_git_command (background, no terminal) */
  const runOperation = async (op: GitOperation) => {
    if (!props.repoPath || !isValidPath(props.repoPath)) return;
    if (op.requiresBranch) {
      const branch = selectedBranch();
      if (!branch || !isValidBranchName(branch)) return;
    }

    setIsRunning(true);
    setRunningOp(op.id);
    setFeedback(null);
    if (dismissTimer) clearTimeout(dismissTimer);

    try {
      const result = await invoke<GitCommandResult>("run_git_command", {
        path: props.repoPath,
        args: op.args(selectedBranch()),
      });

      if (result.success) {
        const msg = result.stdout.trim().split("\n")[0] || "Done";
        setFeedback({ success: true, message: msg.slice(0, 120) });
        repositoriesStore.bumpRevision(props.repoPath!);
        props.onBranchChange?.();
        dismissTimer = setTimeout(() => setFeedback(null), FEEDBACK_DISMISS_MS);
      } else {
        const msg = result.stderr.trim().split("\n")[0] || `Exit code ${result.exit_code}`;
        setFeedback({ success: false, message: msg.slice(0, 200) });
      }
    } catch (err) {
      setFeedback({ success: false, message: String(err) });
      appLogger.error("git", `Failed to run git ${op.id}`, err);
    } finally {
      setIsRunning(false);
      setRunningOp(null);
    }
  };

  /** Terminal injection for conflict resolution actions (need editor/interactive) */
  const executeInTerminal = (command: string) => {
    const active = terminalsStore.getActive();
    if (active?.ref) {
      active.ref.write(`${command}\r`);
      props.onBranchChange?.();
    }
  };

  /** Create a new branch (optionally switching to it) */
  const createBranch = async (andSwitch: boolean) => {
    const name = newBranchName().trim();
    if (!name) {
      setNewBranchError("Branch name is required");
      return;
    }
    if (!isValidBranchName(name)) {
      setNewBranchError("Invalid branch name");
      return;
    }
    if (branches().includes(name)) {
      setNewBranchError("Branch already exists");
      return;
    }
    if (!props.repoPath) return;

    setNewBranchError(null);
    setCreatingBranch(true);

    try {
      const args = andSwitch ? ["checkout", "-b", name] : ["branch", name];
      const result = await invoke<GitCommandResult>("run_git_command", {
        path: props.repoPath,
        args,
      });

      if (result.success) {
        setShowNewBranch(false);
        setNewBranchName("");
        setNewBranchError(null);
        repositoriesStore.bumpRevision(props.repoPath!);
        props.onBranchChange?.();
        setFeedback({ success: true, message: `Branch '${name}' created${andSwitch ? " and checked out" : ""}` });
        if (dismissTimer) clearTimeout(dismissTimer);
        dismissTimer = setTimeout(() => setFeedback(null), FEEDBACK_DISMISS_MS);
        void fetchContext();
      } else {
        const msg = result.stderr.trim().split("\n")[0] || `Exit code ${result.exit_code}`;
        setNewBranchError(msg);
      }
    } catch (err) {
      setNewBranchError(String(err));
    } finally {
      setCreatingBranch(false);
    }
  };

  // Fetch context when panel opens or repo changes
  createEffect(() => {
    if (!props.visible || !props.repoPath) return;
    void repositoriesStore.getRevision(props.repoPath);
    void fetchContext();
  });

  const fetchContext = async () => {
    if (!props.repoPath) return;
    setLoading(true);
    try {
      const [context, branchList] = await Promise.all([
        invoke<GitPanelContext>("get_git_panel_context", { path: props.repoPath }),
        invoke<Array<{ name: string; is_current: boolean; is_remote: boolean }>>(
          "get_git_branches", { path: props.repoPath }
        ),
      ]);
      setCtx(context);
      setBranches(branchList.filter((b) => !b.is_remote).map((b) => b.name));
    } catch (err) {
      appLogger.error("git", "Failed to fetch git panel context", err);
      setCtx(null);
      setBranches([]);
    } finally {
      setLoading(false);
    }
  };

  const context = () => ctx();
  const isMergeInProgress = () => {
    const c = context();
    return c?.status === "conflict" || c?.status === "merge";
  };
  const isDetached = () => context()?.is_detached ?? false;

  let panelRef: HTMLDivElement | undefined;

  // Auto-focus panel when opened
  createEffect(() => {
    if (props.visible) {
      requestAnimationFrame(() => panelRef?.focus());
    }
  });

  const handlePanelKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Escape" && !showNewBranch()) {
      e.preventDefault();
      e.stopPropagation();
      props.onClose();
    }
  };

  return (
    <Show when={props.visible}>
      <div
        class={s.panel}
        data-testid="git-operations-panel"
        ref={panelRef}
        tabIndex={-1}
        onKeyDown={handlePanelKeyDown}
      >
        {/* Header */}
        <div class={s.header}>
          <span class={s.headerTitle}>{t("gitOps.title", "Git Operations")}</span>
          <button class={s.closeBtn} onClick={props.onClose} title="Close">
            <CloseIcon />
          </button>
        </div>

        <div class={s.content}>
          {/* Status Card */}
          <div class={s.statusCard}>
            <div class={s.statusRow}>
              <span class={s.branchName}>
                {context()?.branch || t("gitOps.noBranch", "No branch")}
              </span>
              <Show when={isDetached()}>
                <span class={s.detachedBadge}>DETACHED</span>
              </Show>
              <span class={cx(s.statusBadge, STATUS_CLASSES[context()?.status ?? "unknown"])}>
                {context()?.status ?? "unknown"}
              </span>
            </div>

            {/* Counts row */}
            <Show when={context()}>
              <div class={s.countsRow}>
                <Show when={context()!.ahead != null}>
                  <span class={cx(s.countItem, s.countAhead)} title="Commits ahead">
                    {"\u2191"}{context()!.ahead}
                  </span>
                </Show>
                <Show when={context()!.behind != null}>
                  <span class={cx(s.countItem, s.countBehind)} title="Commits behind">
                    {"\u2193"}{context()!.behind}
                  </span>
                </Show>
                <Show when={context()!.staged_count > 0}>
                  <span class={cx(s.countItem, s.countStaged)} title="Staged files">
                    {context()!.staged_count} staged
                  </span>
                </Show>
                <Show when={context()!.changed_count > 0}>
                  <span class={cx(s.countItem, s.countChanged)} title="Changed files">
                    {context()!.changed_count} changed
                  </span>
                </Show>
                <Show when={context()!.stash_count > 0}>
                  <span class={cx(s.countItem, s.countStash)} title="Stash entries">
                    {context()!.stash_count} stash
                  </span>
                </Show>
              </div>
            </Show>

            {/* Last commit */}
            <Show when={context()?.last_commit}>
              <div class={s.lastCommit}>
                <span class={s.commitHash}>{context()!.last_commit!.short_hash}</span>
                {" "}{context()!.last_commit!.subject}
              </div>
            </Show>
          </div>

          {/* Merge in progress — terminal injection */}
          <Show when={isMergeInProgress()}>
            <div class={s.alertSection}>
              <div class={s.alertTitle}>
                <WarningIcon />
                {t("gitOps.mergeInProgress", "Merge in Progress")}
              </div>
              <div class={s.warning}>
                {t("gitOps.resolveConflicts", "Resolve conflicts before continuing")}
              </div>
              <div class={s.buttons}>
                <For each={MERGE_ACTIONS}>
                  {(action) => (
                    <button
                      class={s.btn}
                      onClick={() => props.repoPath && executeInTerminal(action.command(props.repoPath))}
                      disabled={isRunning()}
                      title={`${action.label} merge`}
                    >
                      {action.label}
                    </button>
                  )}
                </For>
              </div>
            </div>
          </Show>

          {/* Rebase in progress — terminal injection */}
          <Show when={context()?.in_rebase}>
            <div class={s.alertSection}>
              <div class={s.alertTitle}>
                <WarningIcon />
                Rebase in Progress
              </div>
              <div class={s.buttons}>
                <For each={REBASE_ACTIONS}>
                  {(action) => (
                    <button
                      class={s.btn}
                      onClick={() => props.repoPath && executeInTerminal(action.command(props.repoPath))}
                      disabled={isRunning()}
                      title={`${action.label} rebase`}
                    >
                      {action.label}
                    </button>
                  )}
                </For>
              </div>
            </div>
          </Show>

          {/* Cherry-pick in progress — terminal injection */}
          <Show when={context()?.in_cherry_pick}>
            <div class={s.alertSection}>
              <div class={s.alertTitle}>
                <WarningIcon />
                Cherry-pick in Progress
              </div>
              <div class={s.buttons}>
                <For each={CHERRY_PICK_ACTIONS}>
                  {(action) => (
                    <button
                      class={s.btn}
                      onClick={() => props.repoPath && executeInTerminal(action.command(props.repoPath))}
                      disabled={isRunning()}
                      title={`${action.label} cherry-pick`}
                    >
                      {action.label}
                    </button>
                  )}
                </For>
              </div>
            </div>
          </Show>

          {/* Sync section — background execution */}
          <div class={s.section}>
            <div class={s.sectionTitle}>{t("gitOps.sync", "Sync")}</div>
            <div class={s.buttons}>
              <For each={SYNC_OPERATIONS}>
                {(op) => (
                  <button
                    class={cx(s.btn, runningOp() === op.id && s.btnRunning)}
                    onClick={() => runOperation(op)}
                    disabled={!props.repoPath || isMergeInProgress() || isRunning()}
                    title={op.description}
                  >
                    <span class={s.btnIcon}>
                      <Show when={runningOp() === op.id} fallback={<op.Icon />}>
                        <span class={s.spinner} />
                      </Show>
                    </span>
                    {op.label}
                  </button>
                )}
              </For>
            </div>
          </div>

          {/* Branch section — background execution */}
          <div class={s.section}>
            <div class={s.sectionTitle}>{t("gitOps.branch", "Branch")}</div>
            <div class={s.branchSelect}>
              <BranchCombobox
                branches={branches().filter((b) => b !== context()?.branch)}
                currentBranch={context()?.branch ?? null}
                value={selectedBranch()}
                onChange={setSelectedBranch}
                placeholder={t("gitOps.selectBranch", "Select a branch...")}
                loading={loading()}
                disabled={isMergeInProgress() || isDetached() || isRunning()}
              />
            </div>
            <div class={s.buttons}>
              <For each={BRANCH_OPERATIONS}>
                {(op) => (
                  <button
                    class={cx(s.btn, runningOp() === op.id && s.btnRunning)}
                    onClick={() => runOperation(op)}
                    disabled={!props.repoPath || !selectedBranch() || isMergeInProgress() || isRunning()}
                    title={op.description}
                  >
                    <span class={s.btnIcon}>
                      <Show when={runningOp() === op.id} fallback={<op.Icon />}>
                        <span class={s.spinner} />
                      </Show>
                    </span>
                    {op.label}
                  </button>
                )}
              </For>
              <button
                class={s.btn}
                onClick={() => { setShowNewBranch(!showNewBranch()); setNewBranchError(null); setNewBranchName(""); }}
                disabled={!props.repoPath || isRunning()}
                title="Create a new branch"
                data-testid="new-branch-toggle"
              >
                <span class={s.btnIcon}><NewBranchIcon /></span>
                New
              </button>
            </div>

            {/* New Branch inline form */}
            <Show when={showNewBranch()}>
              <div class={s.newBranchForm} data-testid="new-branch-form">
                <div class={s.newBranchInputRow}>
                  <input
                    class={cx(s.newBranchInput, newBranchError() && s.newBranchInputError)}
                    type="text"
                    placeholder="new-branch-name"
                    value={newBranchName()}
                    onInput={(e) => { setNewBranchName(e.currentTarget.value); setNewBranchError(null); }}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") createBranch(false);
                      if (e.key === "Escape") { setShowNewBranch(false); setNewBranchError(null); }
                    }}
                    disabled={creatingBranch()}
                    spellcheck={false}
                    autocomplete="off"
                  />
                </div>
                <Show when={newBranchError()}>
                  <div class={s.newBranchErrorMsg} data-testid="new-branch-error">{newBranchError()}</div>
                </Show>
                <div class={s.buttons}>
                  <button
                    class={s.btn}
                    onClick={() => createBranch(false)}
                    disabled={creatingBranch() || !newBranchName().trim()}
                    data-testid="create-branch-btn"
                  >
                    {creatingBranch() ? <span class={s.spinner} /> : null}
                    Create
                  </button>
                  <button
                    class={s.btn}
                    onClick={() => createBranch(true)}
                    disabled={creatingBranch() || !newBranchName().trim()}
                    data-testid="create-switch-btn"
                  >
                    {creatingBranch() ? <span class={s.spinner} /> : null}
                    Create & Switch
                  </button>
                  <button
                    class={s.btn}
                    onClick={() => { setShowNewBranch(false); setNewBranchError(null); }}
                    data-testid="cancel-branch-btn"
                  >
                    Cancel
                  </button>
                </div>
              </div>
            </Show>
          </div>

          {/* Stash section — background execution */}
          <div class={s.section}>
            <div class={s.sectionTitle}>
              {t("gitOps.stash", "Stash")}
              <Show when={(context()?.stash_count ?? 0) > 0}>
                {" "}({context()!.stash_count})
              </Show>
            </div>
            <div class={s.buttons}>
              <For each={STASH_OPERATIONS}>
                {(op) => (
                  <button
                    class={cx(s.btn, runningOp() === op.id && s.btnRunning)}
                    onClick={() => runOperation(op)}
                    disabled={!props.repoPath || isRunning()}
                    title={op.description}
                  >
                    <span class={s.btnIcon}>
                      <Show when={runningOp() === op.id} fallback={<op.Icon />}>
                        <span class={s.spinner} />
                      </Show>
                    </span>
                    {op.label}
                  </button>
                )}
              </For>
            </div>
          </div>

          {/* Feedback bar */}
          <Show when={feedback()}>
            {(fb) => (
              <div
                class={cx(s.feedbackBar, fb().success ? s.feedbackSuccess : s.feedbackError)}
                data-testid="feedback-bar"
              >
                <span class={s.feedbackIcon}>
                  <Show when={fb().success} fallback={<ErrorIcon />}>
                    <CheckIcon />
                  </Show>
                </span>
                <span class={s.feedbackMessage}>{fb().message}</span>
                <button
                  class={s.feedbackDismiss}
                  onClick={() => setFeedback(null)}
                  title="Dismiss"
                >
                  <CloseIcon />
                </button>
              </div>
            )}
          </Show>
        </div>
      </div>
    </Show>
  );
};

export default GitOperationsPanel;
