import { Component, createEffect, createSignal, For, Show, on } from "solid-js";
import { invoke } from "../../invoke";
import { repositoriesStore } from "../../stores/repositories";
import { ConfirmDialog } from "../ConfirmDialog/ConfirmDialog";
import { cx } from "../../utils";
import s from "./StashesTab.module.css";

/** Mirrors the Rust StashEntry struct from git.rs */
interface StashEntry {
  index: number;
  ref_name: string;
  message: string;
  hash: string;
}

/** Mirrors the Rust GitCommandResult struct from git.rs */
interface GitCommandResult {
  success: boolean;
  stdout: string;
  stderr: string;
}

export interface StashesTabProps {
  repoPath: string | null;
}

/** Classify a diff line for syntax coloring */
function diffLineClass(line: string): string {
  if (line.startsWith("@@")) return s.diffHunk;
  if (line.startsWith("+")) return s.diffAdd;
  if (line.startsWith("-")) return s.diffDel;
  return s.diffContext;
}

export const StashesTab: Component<StashesTabProps> = (props) => {
  const [stashes, setStashes] = createSignal<StashEntry[]>([]);
  const [loading, setLoading] = createSignal(false);
  const [busyRef, setBusyRef] = createSignal<string | null>(null);
  const [expandedRef, setExpandedRef] = createSignal<string | null>(null);
  const [diffs, setDiffs] = createSignal<Record<string, string>>({});
  const [diffLoading, setDiffLoading] = createSignal<Record<string, boolean>>({});
  const [focusedIndex, setFocusedIndex] = createSignal(-1);

  // Confirm dialog state
  const [confirmVisible, setConfirmVisible] = createSignal(false);
  const [confirmTitle, setConfirmTitle] = createSignal("");
  const [confirmMessage, setConfirmMessage] = createSignal("");
  const [confirmAction, setConfirmAction] = createSignal<(() => void) | null>(null);

  async function fetchStashes(repoPath: string) {
    setLoading(true);
    try {
      const result = await invoke<StashEntry[]>("get_stash_list", { path: repoPath });
      setStashes(result);
    } catch {
      setStashes([]);
    } finally {
      setLoading(false);
    }
  }

  // Re-fetch when repo changes or revision bumps
  createEffect(
    on(
      () => {
        const repoPath = props.repoPath;
        if (repoPath) void repositoriesStore.getRevision(repoPath);
        return repoPath;
      },
      (repoPath) => {
        if (repoPath) fetchStashes(repoPath);
      },
    ),
  );

  async function runStashAction(args: string[]) {
    const repoPath = props.repoPath;
    if (!repoPath) return;
    const refName = args[args.length - 1];
    setBusyRef(refName);
    try {
      const result = await invoke<GitCommandResult>("run_git_command", {
        path: repoPath,
        args,
      });
      if (!result.success) {
        // Let the user see the error via the app's standard error handling
        throw new Error(result.stderr || "Git stash operation failed");
      }
      // Re-fetch stash list after successful operation
      await fetchStashes(repoPath);
    } finally {
      setBusyRef(null);
    }
  }

  function handleApply(refName: string) {
    runStashAction(["stash", "apply", refName]);
  }

  function showConfirm(title: string, message: string, action: () => void) {
    setConfirmTitle(title);
    setConfirmMessage(message);
    setConfirmAction(() => action);
    setConfirmVisible(true);
  }

  function handlePop(refName: string) {
    showConfirm(
      "Pop Stash",
      `Apply and remove "${refName}"?\nThis cannot be undone.`,
      () => runStashAction(["stash", "pop", refName]),
    );
  }

  function handleDrop(refName: string) {
    showConfirm(
      "Drop Stash",
      `Permanently delete "${refName}"?\nThis cannot be undone.`,
      () => runStashAction(["stash", "drop", refName]),
    );
  }

  function handleConfirm() {
    const action = confirmAction();
    setConfirmVisible(false);
    setConfirmAction(null);
    if (action) action();
  }

  function handleConfirmClose() {
    setConfirmVisible(false);
    setConfirmAction(null);
  }

  async function toggleDiff(refName: string) {
    if (expandedRef() === refName) {
      setExpandedRef(null);
      return;
    }
    setExpandedRef(refName);

    // Fetch diff if not cached
    if (!diffs()[refName]) {
      setDiffLoading((prev) => ({ ...prev, [refName]: true }));
      try {
        const result = await invoke<GitCommandResult>("run_git_command", {
          path: props.repoPath!,
          args: ["stash", "show", "-p", refName],
        });
        setDiffs((prev) => ({ ...prev, [refName]: result.stdout }));
      } catch {
        setDiffs((prev) => ({ ...prev, [refName]: "" }));
      } finally {
        setDiffLoading((prev) => ({ ...prev, [refName]: false }));
      }
    }
  }

  function handleListKeyDown(e: KeyboardEvent) {
    const total = stashes().length;
    if (total === 0) return;

    const idx = focusedIndex();

    if (e.key === "ArrowDown") {
      e.preventDefault();
      setFocusedIndex(Math.min(idx + 1, total - 1));
      return;
    }
    if (e.key === "ArrowUp") {
      e.preventDefault();
      setFocusedIndex(Math.max(idx - 1, 0));
      return;
    }
    if (e.key === "Enter" && idx >= 0 && idx < total) {
      e.preventDefault();
      toggleDiff(stashes()[idx].ref_name);
      return;
    }
  }

  return (
    <div class={s.container} onKeyDown={handleListKeyDown} tabIndex={-1}>
      <Show when={!loading()} fallback={<div class={s.empty}>Loading stashes...</div>}>
        <Show when={stashes().length > 0} fallback={<div class={s.empty}>No stashes</div>}>
          <div class={s.scrollContainer}>
            <For each={stashes()}>
              {(stash, i) => {
                const isBusy = () => busyRef() === stash.ref_name;
                const isExpanded = () => expandedRef() === stash.ref_name;
                const diff = () => diffs()[stash.ref_name];
                const isDiffLoading = () => diffLoading()[stash.ref_name];
                const isFocused = () => focusedIndex() === i();

                return (
                  <div
                    class={cx(s.stashEntry, isFocused() && s.stashEntryFocused)}
                    onClick={() => setFocusedIndex(i())}
                  >
                    {/* Line 1: ref name + message */}
                    <div class={s.stashHeader}>
                      <span class={s.stashRef}>{stash.ref_name}</span>
                      <span class={s.stashMessage}>{stash.message}</span>
                    </div>
                    {/* Line 2: action buttons + diff toggle */}
                    <div class={s.stashActions}>
                      <button
                        class={s.actionBtn}
                        onClick={() => handleApply(stash.ref_name)}
                        disabled={isBusy()}
                      >
                        Apply
                      </button>
                      <button
                        class={s.actionBtn}
                        onClick={() => handlePop(stash.ref_name)}
                        disabled={isBusy()}
                      >
                        Pop
                      </button>
                      <button
                        class={s.actionBtnDanger}
                        onClick={() => handleDrop(stash.ref_name)}
                        disabled={isBusy()}
                      >
                        Drop
                      </button>
                      <button
                        class={s.diffToggle}
                        onClick={() => toggleDiff(stash.ref_name)}
                      >
                        <span class={cx(s.chevron, isExpanded() && s.chevronOpen)}>&#9654;</span>
                        Show diff
                      </button>
                    </div>
                    {/* Expandable diff preview */}
                    <Show when={isExpanded()}>
                      <div class={s.diffPreview}>
                        <Show when={!isDiffLoading()} fallback={<div class={s.diffLoading}>Loading diff...</div>}>
                          <Show when={diff()} fallback={<div class={s.diffLoading}>No diff available</div>}>
                            <For each={diff()!.split("\n")}>
                              {(line) => (
                                <div class={cx(s.diffLine, diffLineClass(line))}>{line}</div>
                              )}
                            </For>
                          </Show>
                        </Show>
                      </div>
                    </Show>
                  </div>
                );
              }}
            </For>
          </div>
        </Show>
      </Show>

      <ConfirmDialog
        visible={confirmVisible()}
        title={confirmTitle()}
        message={confirmMessage()}
        confirmLabel="Confirm"
        kind="warning"
        onClose={handleConfirmClose}
        onConfirm={handleConfirm}
      />
    </div>
  );
};

export default StashesTab;
