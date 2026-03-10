import { Component, Show, For, createSignal, createEffect } from "solid-js";
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
  StashIcon, WarningIcon, CloseIcon,
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
  command: (repoPath: string, branch?: string) => string;
  requiresBranch?: boolean;
  description: string;
}

const SYNC_OPERATIONS: GitOperation[] = [
  {
    id: "pull",
    label: "Pull",
    Icon: PullIcon,
    command: (repo) => `cd ${escapeShellArg(repo)} && git pull`,
    description: "Fetch and merge changes from remote",
  },
  {
    id: "push",
    label: "Push",
    Icon: PushIcon,
    command: (repo) => `cd ${escapeShellArg(repo)} && git push`,
    description: "Push local commits to remote",
  },
  {
    id: "fetch",
    label: "Fetch",
    Icon: FetchIcon,
    command: (repo) => `cd ${escapeShellArg(repo)} && git fetch --all`,
    description: "Download remote changes without merging",
  },
];

const BRANCH_OPERATIONS: GitOperation[] = [
  {
    id: "switch",
    label: "Switch",
    Icon: BranchIcon,
    command: (repo, branch) => `cd ${escapeShellArg(repo)} && git checkout ${escapeShellArg(branch || "")}`,
    requiresBranch: true,
    description: "Switch to selected branch",
  },
  {
    id: "merge",
    label: "Merge",
    Icon: MergeIcon,
    command: (repo, branch) => `cd ${escapeShellArg(repo)} && git merge ${escapeShellArg(branch || "")}`,
    requiresBranch: true,
    description: "Merge selected branch into current",
  },
];

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

export const GitOperationsPanel: Component<GitOperationsPanelProps> = (props) => {
  const [ctx, setCtx] = createSignal<GitPanelContext | null>(null);
  const [selectedBranch, setSelectedBranch] = createSignal("");
  const [branches, setBranches] = createSignal<string[]>([]);
  const [loading, setLoading] = createSignal(false);

  const executeCommand = (command: string) => {
    const active = terminalsStore.getActive();
    if (active?.ref) {
      active.ref.write(`${command}\r`);
      props.onClose();
      props.onBranchChange?.();
    }
  };

  const handleOperation = (op: GitOperation) => {
    if (!props.repoPath || !isValidPath(props.repoPath)) return;
    if (op.requiresBranch) {
      const branch = selectedBranch();
      if (!branch || !isValidBranchName(branch)) return;
    }
    executeCommand(op.command(props.repoPath, selectedBranch()));
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

  return (
    <Show when={props.visible}>
      <div class={s.panel} data-testid="git-operations-panel">
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

          {/* Merge in progress */}
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
                      onClick={() => props.repoPath && executeCommand(action.command(props.repoPath))}
                      title={`${action.label} merge`}
                    >
                      {action.label}
                    </button>
                  )}
                </For>
              </div>
            </div>
          </Show>

          {/* Rebase in progress */}
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
                      onClick={() => props.repoPath && executeCommand(action.command(props.repoPath))}
                      title={`${action.label} rebase`}
                    >
                      {action.label}
                    </button>
                  )}
                </For>
              </div>
            </div>
          </Show>

          {/* Cherry-pick in progress */}
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
                      onClick={() => props.repoPath && executeCommand(action.command(props.repoPath))}
                      title={`${action.label} cherry-pick`}
                    >
                      {action.label}
                    </button>
                  )}
                </For>
              </div>
            </div>
          </Show>

          {/* Sync section */}
          <div class={s.section}>
            <div class={s.sectionTitle}>{t("gitOps.sync", "Sync")}</div>
            <div class={s.buttons}>
              <For each={SYNC_OPERATIONS}>
                {(op) => (
                  <button
                    class={s.btn}
                    onClick={() => handleOperation(op)}
                    disabled={!props.repoPath || isMergeInProgress()}
                    title={op.description}
                  >
                    <span class={s.btnIcon}><op.Icon /></span>
                    {op.label}
                  </button>
                )}
              </For>
            </div>
          </div>

          {/* Branch section */}
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
                disabled={isMergeInProgress() || isDetached()}
              />
            </div>
            <div class={s.buttons}>
              <For each={BRANCH_OPERATIONS}>
                {(op) => (
                  <button
                    class={s.btn}
                    onClick={() => handleOperation(op)}
                    disabled={!props.repoPath || !selectedBranch() || isMergeInProgress()}
                    title={op.description}
                  >
                    <span class={s.btnIcon}><op.Icon /></span>
                    {op.label}
                  </button>
                )}
              </For>
            </div>
          </div>

          {/* Stash section */}
          <div class={s.section}>
            <div class={s.sectionTitle}>
              {t("gitOps.stash", "Stash")}
              <Show when={(context()?.stash_count ?? 0) > 0}>
                {" "}({context()!.stash_count})
              </Show>
            </div>
            <div class={s.buttons}>
              <button
                class={s.btn}
                onClick={() => props.repoPath && executeCommand(`cd ${escapeShellArg(props.repoPath)} && git stash`)}
                disabled={!props.repoPath}
                title={t("gitOps.stashChanges", "Stash current changes")}
              >
                <span class={s.btnIcon}><StashIcon /></span>
                {t("gitOps.stashBtn", "Stash")}
              </button>
              <button
                class={s.btn}
                onClick={() => props.repoPath && executeCommand(`cd ${escapeShellArg(props.repoPath)} && git stash pop`)}
                disabled={!props.repoPath}
                title={t("gitOps.stashPop", "Apply and remove latest stash")}
              >
                <span class={s.btnIcon}><StashIcon /></span>
                {t("gitOps.popBtn", "Pop")}
              </button>
            </div>
          </div>
        </div>
      </div>
    </Show>
  );
};

export default GitOperationsPanel;
