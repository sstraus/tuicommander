import { Component, Show, For, createSignal, createEffect } from "solid-js";
import { invoke } from "../../invoke";
import { terminalsStore } from "../../stores/terminals";
import { repositoriesStore } from "../../stores/repositories";
import { escapeShellArg, isValidBranchName, isValidPath } from "../../utils";

export interface GitOperationsPanelProps {
  visible: boolean;
  repoPath: string | null;
  currentBranch: string | null;
  repoStatus: "clean" | "dirty" | "conflict" | "merge" | "unknown";
  onClose: () => void;
  onBranchChange?: () => void;
}

interface GitOperation {
  id: string;
  label: string;
  icon: string;
  command: (repoPath: string, branch?: string) => string;
  requiresBranch?: boolean;
  description: string;
}

const GIT_OPERATIONS: GitOperation[] = [
  {
    id: "pull",
    label: "Pull",
    icon: "↓",
    command: (repo) => `cd ${escapeShellArg(repo)} && git pull`,
    description: "Fetch and merge changes from remote",
  },
  {
    id: "push",
    label: "Push",
    icon: "↑",
    command: (repo) => `cd ${escapeShellArg(repo)} && git push`,
    description: "Push local commits to remote",
  },
  {
    id: "fetch",
    label: "Fetch",
    icon: "⟳",
    command: (repo) => `cd ${escapeShellArg(repo)} && git fetch --all`,
    description: "Download remote changes without merging",
  },
  {
    id: "merge",
    label: "Merge",
    icon: "⊕",
    command: (repo, branch) => `cd ${escapeShellArg(repo)} && git merge ${escapeShellArg(branch || "")}`,
    requiresBranch: true,
    description: "Merge another branch into current",
  },
  {
    id: "checkout",
    label: "Checkout",
    icon: "⎇",
    command: (repo, branch) => `cd ${escapeShellArg(repo)} && git checkout ${escapeShellArg(branch || "")}`,
    requiresBranch: true,
    description: "Switch to another branch",
  },
];

const MERGE_OPERATIONS = [
  {
    id: "abort",
    label: "Abort Merge",
    icon: "✕",
    command: (repo: string) => `cd ${escapeShellArg(repo)} && git merge --abort`,
    description: "Cancel the current merge",
  },
  {
    id: "continue",
    label: "Continue Merge",
    icon: "→",
    command: (repo: string) => `cd ${escapeShellArg(repo)} && git merge --continue`,
    description: "Continue after resolving conflicts",
  },
  {
    id: "ours",
    label: "Accept Ours",
    icon: "◀",
    command: (repo: string) => `cd ${escapeShellArg(repo)} && git checkout --ours .`,
    description: "Keep our version for all conflicts",
  },
  {
    id: "theirs",
    label: "Accept Theirs",
    icon: "▶",
    command: (repo: string) => `cd ${escapeShellArg(repo)} && git checkout --theirs .`,
    description: "Keep their version for all conflicts",
  },
];

export const GitOperationsPanel: Component<GitOperationsPanelProps> = (props) => {
  const [selectedBranch, setSelectedBranch] = createSignal<string>("");
  const [branches, setBranches] = createSignal<string[]>([]);
  const [loadingBranches, setLoadingBranches] = createSignal(false);

  // Execute git command in terminal
  const executeCommand = (command: string) => {
    const active = terminalsStore.getActive();
    if (active?.ref) {
      active.ref.write(`${command}\r`);
      props.onClose();
      props.onBranchChange?.();
    }
  };

  // Handle operation click
  const handleOperation = (op: GitOperation) => {
    if (!props.repoPath) return;
    if (!isValidPath(props.repoPath)) return;

    if (op.requiresBranch) {
      const branch = selectedBranch();
      if (!branch || !isValidBranchName(branch)) {
        return; // Need to select a valid branch first
      }
    }

    const command = op.command(props.repoPath, selectedBranch());
    executeCommand(command);
  };

  // Fetch branches when panel opens
  createEffect(() => {
    // Track repo revision so branch list refreshes on git operations
    void (props.repoPath ? repositoriesStore.getRevision(props.repoPath) : 0);
    if (props.visible && props.repoPath) {
      fetchBranches();
    }
  });

  const fetchBranches = async () => {
    if (!props.repoPath) return;

    setLoadingBranches(true);
    try {
      const result = await invoke<Array<{ name: string; is_current: boolean; is_remote: boolean }>>(
        "get_git_branches",
        { path: props.repoPath }
      );
      // Only show local branches, exclude current
      const branchNames = result
        .filter((b) => !b.is_remote)
        .map((b) => b.name);
      setBranches(branchNames);
    } catch (err) {
      console.error("Failed to fetch branches:", err);
      setBranches([]);
    } finally {
      setLoadingBranches(false);
    }
  };

  const isMergeInProgress = () => props.repoStatus === "merge" || props.repoStatus === "conflict";

  return (
    <Show when={props.visible}>
      <div class="git-ops-panel">
        <div class="git-ops-header">
          <h3>Git Operations</h3>
          <button class="git-ops-close" onClick={props.onClose}>
            &times;
          </button>
        </div>

        <div class="git-ops-content">
          {/* Current status */}
          <div class="git-ops-status">
            <span class="git-ops-branch">{props.currentBranch || "No branch"}</span>
            <span class={`git-ops-state git-ops-state-${props.repoStatus}`}>
              {props.repoStatus}
            </span>
          </div>

          {/* Merge in progress section */}
          <Show when={isMergeInProgress()}>
            <div class="git-ops-section git-ops-merge-section">
              <div class="git-ops-section-title">Merge in Progress</div>
              <div class="git-ops-warning">
                Resolve conflicts before continuing
              </div>
              <div class="git-ops-buttons">
                <For each={MERGE_OPERATIONS}>
                  {(op) => (
                    <button
                      class="git-ops-btn"
                      onClick={() => props.repoPath && executeCommand(op.command(props.repoPath))}
                      title={op.description}
                    >
                      <span class="git-ops-btn-icon">{op.icon}</span>
                      {op.label}
                    </button>
                  )}
                </For>
              </div>
            </div>
          </Show>

          {/* Quick actions */}
          <div class="git-ops-section">
            <div class="git-ops-section-title">Quick Actions</div>
            <div class="git-ops-buttons">
              <For each={GIT_OPERATIONS.filter((op) => !op.requiresBranch)}>
                {(op) => (
                  <button
                    class="git-ops-btn"
                    onClick={() => handleOperation(op)}
                    disabled={!props.repoPath || isMergeInProgress()}
                    title={op.description}
                  >
                    <span class="git-ops-btn-icon">{op.icon}</span>
                    {op.label}
                  </button>
                )}
              </For>
            </div>
          </div>

          {/* Branch operations */}
          <div class="git-ops-section">
            <div class="git-ops-section-title">Branch Operations</div>

            {/* Branch selector */}
            <div class="git-ops-branch-select">
              <select
                value={selectedBranch()}
                onChange={(e) => setSelectedBranch(e.currentTarget.value)}
                disabled={loadingBranches() || isMergeInProgress()}
              >
                <option value="">Select a branch...</option>
                <For each={branches().filter((b) => b !== props.currentBranch)}>
                  {(branch) => <option value={branch}>{branch}</option>}
                </For>
              </select>
            </div>

            <div class="git-ops-buttons">
              <For each={GIT_OPERATIONS.filter((op) => op.requiresBranch)}>
                {(op) => (
                  <button
                    class="git-ops-btn"
                    onClick={() => handleOperation(op)}
                    disabled={!props.repoPath || !selectedBranch() || isMergeInProgress()}
                    title={op.description}
                  >
                    <span class="git-ops-btn-icon">{op.icon}</span>
                    {op.label}
                  </button>
                )}
              </For>
            </div>
          </div>

          {/* Stash operations */}
          <div class="git-ops-section">
            <div class="git-ops-section-title">Stash</div>
            <div class="git-ops-buttons">
              <button
                class="git-ops-btn"
                onClick={() => props.repoPath && executeCommand(`cd ${escapeShellArg(props.repoPath)} && git stash`)}
                disabled={!props.repoPath}
                title="Stash current changes"
              >
                <span class="git-ops-btn-icon">⊡</span>
                Stash
              </button>
              <button
                class="git-ops-btn"
                onClick={() => props.repoPath && executeCommand(`cd ${escapeShellArg(props.repoPath)} && git stash pop`)}
                disabled={!props.repoPath}
                title="Apply and remove latest stash"
              >
                <span class="git-ops-btn-icon">⊞</span>
                Pop
              </button>
            </div>
          </div>
        </div>
      </div>
    </Show>
  );
};

export default GitOperationsPanel;
