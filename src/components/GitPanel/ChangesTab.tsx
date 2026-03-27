import { Component, createEffect, createMemo, createSignal, For, Show, onCleanup } from "solid-js";
import { invoke } from "../../invoke";
import { repositoriesStore } from "../../stores/repositories";
import { terminalsStore } from "../../stores/terminals";
import { diffTabsStore, isDiffStatus } from "../../stores/diffTabs";
import { appLogger } from "../../stores/appLogger";
import { ConfirmDialog } from "../ConfirmDialog";
import { SmartButtonStrip } from "../SmartButtonStrip/SmartButtonStrip";
import { cx, globToRegex } from "../../utils";
import type { CommitLogEntry, WorkingTreeStatus } from "./types";
import s from "./ChangesTab.module.css";

/** Unified file entry for rendering */
interface FileEntry {
  path: string;
  status: string;
  additions: number;
  deletions: number;
}

export interface ChangesTabProps {
  repoPath: string | null;
  /** Repo key for store operations (bumpRevision). Falls back to repoPath. */
  storeRepoPath?: string | null;
  onFileSelect?: (path: string) => void;
}

// SVG icons as inline components (monochrome, fill="currentColor")
const StageIcon = () => (
  <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
    <path d="M8 1a.5.5 0 0 1 .5.5v6.793l2.146-2.147a.5.5 0 0 1 .708.708l-3 3a.5.5 0 0 1-.708 0l-3-3a.5.5 0 1 1 .708-.708L7.5 8.293V1.5A.5.5 0 0 1 8 1zM2 13.5a.5.5 0 0 1 .5-.5h11a.5.5 0 0 1 0 1h-11a.5.5 0 0 1-.5-.5z" />
  </svg>
);

const UnstageIcon = () => (
  <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
    <path d="M8 15a.5.5 0 0 1-.5-.5V7.707L5.354 9.854a.5.5 0 1 1-.708-.708l3-3a.5.5 0 0 1 .708 0l3 3a.5.5 0 0 1-.708.708L8.5 7.707V14.5A.5.5 0 0 1 8 15zM2 2.5a.5.5 0 0 1 .5-.5h11a.5.5 0 0 1 0 1h-11a.5.5 0 0 1-.5-.5z" />
  </svg>
);

const DiffIcon = () => (
  <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
    <path d="M2 3.5A1.5 1.5 0 0 1 3.5 2h9A1.5 1.5 0 0 1 14 3.5v9a1.5 1.5 0 0 1-1.5 1.5h-9A1.5 1.5 0 0 1 2 12.5v-9zM3.5 3a.5.5 0 0 0-.5.5v9a.5.5 0 0 0 .5.5h9a.5.5 0 0 0 .5-.5v-9a.5.5 0 0 0-.5-.5h-9z" />
    <path d="M8 5a.5.5 0 0 1 .5.5v2h2a.5.5 0 0 1 0 1h-2v2a.5.5 0 0 1-1 0v-2h-2a.5.5 0 0 1 0-1h2v-2A.5.5 0 0 1 8 5z" />
  </svg>
);

const DiscardIcon = () => (
  <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
    <path d="M4.646 4.646a.5.5 0 0 1 .708 0L8 7.293l2.646-2.647a.5.5 0 0 1 .708.708L8.707 8l2.647 2.646a.5.5 0 0 1-.708.708L8 8.707l-2.646 2.647a.5.5 0 0 1-.708-.708L7.293 8 4.646 5.354a.5.5 0 0 1 0-.708z" />
  </svg>
);

const CheckIcon = () => (
  <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
    <path d="M13.78 4.22a.75.75 0 0 1 0 1.06l-7.25 7.25a.75.75 0 0 1-1.06 0L2.22 9.28a.75.75 0 0 1 1.06-1.06L6 10.94l6.72-6.72a.75.75 0 0 1 1.06 0z" />
  </svg>
);

/** Map status code to CSS class */
function statusClass(status: string): string {
  switch (status) {
    case "M": return s.statusM;
    case "A": return s.statusA;
    case "D": return s.statusD;
    case "R": return s.statusR;
    case "?": return s.statusUntracked;
    default: return s.statusUntracked;
  }
}

/** Split path into directory and basename */
function splitPath(filePath: string): { dir: string; base: string } {
  const lastSlash = filePath.lastIndexOf("/");
  if (lastSlash === -1) return { dir: "", base: filePath };
  return { dir: filePath.slice(0, lastSlash + 1), base: filePath.slice(lastSlash + 1) };
}

export const ChangesTab: Component<ChangesTabProps> = (props) => {
  const [staged, setStaged] = createSignal<FileEntry[]>([]);
  const [unstaged, setUnstaged] = createSignal<FileEntry[]>([]);
  const [stagedExpanded, setStagedExpanded] = createSignal(true);
  const [unstagedExpanded, setUnstagedExpanded] = createSignal(true);
  const [confirmDiscard, setConfirmDiscard] = createSignal<FileEntry | null>(null);
  const [confirmDiscardAll, setConfirmDiscardAll] = createSignal(false);
  const [focusedIndex, setFocusedIndex] = createSignal(-1);
  const [filterQuery, setFilterQuery] = createSignal("");
  /** Repo key for store operations (revision tracking). May differ from repoPath in worktrees. */
  const storeKey = () => props.storeRepoPath || props.repoPath;

  /** Filtered staged files (glob wildcard support) */
  const filteredStaged = createMemo(() => {
    const q = filterQuery().trim();
    if (!q) return staged();
    const re = globToRegex(q);
    return staged().filter((f) => re.test(f.path));
  });

  /** Filtered unstaged files (glob wildcard support) */
  const filteredUnstaged = createMemo(() => {
    const q = filterQuery().trim();
    if (!q) return unstaged();
    const re = globToRegex(q);
    return unstaged().filter((f) => re.test(f.path));
  });

  /** True when at least one terminal with a detected agent has cwd inside this repo */
  const hasAgentForRepo = createMemo(() => {
    const repoPath = props.repoPath;
    if (!repoPath) return false;
    return Object.values(terminalsStore.state.terminals).some(
      (t) => t.agentType && t.cwd?.startsWith(repoPath),
    );
  });

  // Commit section signals
  const [commitMsg, setCommitMsg] = createSignal("");
  const [isAmend, setIsAmend] = createSignal(false);
  const [committing, setCommitting] = createSignal(false);
  const [commitError, setCommitError] = createSignal<string | null>(null);
  const [commitSuccess, setCommitSuccess] = createSignal(false);
  // Stores the user-typed message when switching to amend mode
  let savedDraftMsg = "";
  let successTimeout: ReturnType<typeof setTimeout> | undefined;

  onCleanup(() => {
    if (successTimeout) clearTimeout(successTimeout);
  });

  // Fetch working tree status on mount and revision changes
  createEffect(() => {
    const repoPath = props.repoPath;
    if (!repoPath) {
      setStaged([]);
      setUnstaged([]);
      return;
    }

    // Subscribe to revision changes
    void repositoriesStore.getRevision(storeKey() || repoPath);

    let cancelled = false;
    onCleanup(() => { cancelled = true; });

    invoke<WorkingTreeStatus>("get_working_tree_status", { path: repoPath })
      .then((status) => {
        if (cancelled) return;
        setStaged(status.staged.map((e) => ({
          path: e.path, status: e.status, additions: e.additions ?? 0, deletions: e.deletions ?? 0,
        })));

        // Combine unstaged (tracked changes) and untracked into one list
        const combined: FileEntry[] = [
          ...status.unstaged.map((e) => ({
            path: e.path, status: e.status, additions: e.additions ?? 0, deletions: e.deletions ?? 0,
          })),
          ...status.untracked.map((p) => ({ path: p, status: "?", additions: 0, deletions: 0 })),
        ];
        setUnstaged(combined);
      })
      .catch((err) => {
        if (cancelled) return;
        appLogger.error("git", "Failed to get working tree status", err);
        setStaged([]);
        setUnstaged([]);
      });
  });

  // --- Actions ---

  async function stageFile(filePath: string) {
    if (!props.repoPath) return;
    try {
      await invoke("git_stage_files", { path: props.repoPath, files: [filePath] });
      repositoriesStore.bumpRevision(storeKey()!);
    } catch (err) {
      appLogger.error("git", `Failed to stage ${filePath}`, err);
    }
  }

  async function unstageFile(filePath: string) {
    if (!props.repoPath) return;
    try {
      await invoke("git_unstage_files", { path: props.repoPath, files: [filePath] });
      repositoriesStore.bumpRevision(storeKey()!);
    } catch (err) {
      appLogger.error("git", `Failed to unstage ${filePath}`, err);
    }
  }

  async function discardFile(filePath: string) {
    if (!props.repoPath) return;
    try {
      await invoke("git_discard_files", { path: props.repoPath, files: [filePath] });
      repositoriesStore.bumpRevision(storeKey()!);
    } catch (err) {
      appLogger.error("git", `Failed to discard ${filePath}`, err);
    }
  }

  async function stageAll() {
    if (!props.repoPath) return;
    const files = unstaged().map((f) => f.path);
    if (files.length === 0) return;
    try {
      await invoke("git_stage_files", { path: props.repoPath, files });
      repositoriesStore.bumpRevision(storeKey()!);
    } catch (err) {
      appLogger.error("git", "Failed to stage all files", err);
    }
  }

  async function unstageAll() {
    if (!props.repoPath) return;
    const files = staged().map((f) => f.path);
    if (files.length === 0) return;
    try {
      await invoke("git_unstage_files", { path: props.repoPath, files });
      repositoriesStore.bumpRevision(storeKey()!);
    } catch (err) {
      appLogger.error("git", "Failed to unstage all files", err);
    }
  }

  async function discardAll() {
    if (!props.repoPath) return;
    // Only discard tracked modified files, not untracked
    const files = unstaged()
      .filter((f) => f.status !== "?")
      .map((f) => f.path);
    if (files.length === 0) return;
    try {
      await invoke("git_discard_files", { path: props.repoPath, files });
      repositoriesStore.bumpRevision(storeKey()!);
    } catch (err) {
      appLogger.error("git", "Failed to discard all files", err);
    }
  }

  function openDiff(file: FileEntry, section: "staged" | "unstaged") {
    if (!props.repoPath || !isDiffStatus(file.status)) return;
    diffTabsStore.add(
      props.repoPath,
      file.path,
      file.status,
      section === "staged" ? "staged" : undefined,
      file.status === "?" || undefined,
    );
  }

  async function doCommit() {
    const repoPath = props.repoPath;
    if (!repoPath) return;
    const msg = commitMsg().trim();
    if (!msg && !isAmend()) return;
    if (staged().length === 0 && !isAmend()) return;

    setCommitting(true);
    setCommitError(null);
    setCommitSuccess(false);
    try {
      await invoke<string>("git_commit", {
        path: repoPath,
        message: msg,
        amend: isAmend() || null,
      });
      setCommitMsg("");
      setIsAmend(false);
      savedDraftMsg = "";
      repositoriesStore.bumpRevision(storeKey() || repoPath);
      setCommitSuccess(true);
      if (successTimeout) clearTimeout(successTimeout);
      successTimeout = setTimeout(() => setCommitSuccess(false), 3000);
    } catch (err) {
      const errStr = typeof err === "string" ? err : String(err);
      setCommitError(errStr);
      appLogger.error("git", "Commit failed", err);
    } finally {
      setCommitting(false);
    }
  }

  async function toggleAmend() {
    const next = !isAmend();
    if (next) {
      // Save current draft before overwriting with last commit message
      savedDraftMsg = commitMsg();
      if (props.repoPath) {
        try {
          const log = await invoke<CommitLogEntry[]>("get_commit_log", {
            path: props.repoPath,
            count: 1,
          });
          if (log.length > 0) {
            setCommitMsg(log[0].subject);
          }
        } catch (err) {
          appLogger.error("git", "Failed to fetch last commit for amend", err);
        }
      }
    } else {
      // Restore previously typed message
      setCommitMsg(savedDraftMsg);
      savedDraftMsg = "";
    }
    setIsAmend(next);
    setCommitError(null);
  }

  function handleCommitKeyDown(e: KeyboardEvent) {
    // Cmd+Enter (Mac) or Ctrl+Enter (others) to commit
    if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      doCommit();
    }
  }

  /** Auto-resize textarea to fit content */
  function autoResize(el: HTMLTextAreaElement) {
    el.style.height = "auto";
    // Clamp between 2 lines (~38px) and 6 lines (~114px) at font-sm (12px + line-height ~1.6)
    const minH = 38;
    const maxH = 114;
    el.style.height = `${Math.max(minH, Math.min(el.scrollHeight, maxH))}px`;
  }

  function handleConfirmDiscard() {
    const file = confirmDiscard();
    if (file) {
      discardFile(file.path);
      setConfirmDiscard(null);
    }
  }

  function handleConfirmDiscardAll() {
    discardAll();
    setConfirmDiscardAll(false);
  }

  /** Build a flat list of visible file entries for keyboard navigation */
  function visibleFiles(): { file: FileEntry; section: "staged" | "unstaged" }[] {
    const result: { file: FileEntry; section: "staged" | "unstaged" }[] = [];
    if (stagedExpanded()) {
      for (const f of staged()) result.push({ file: f, section: "staged" });
    }
    if (unstagedExpanded()) {
      for (const f of unstaged()) result.push({ file: f, section: "unstaged" });
    }
    return result;
  }

  function handleListKeyDown(e: KeyboardEvent) {
    const files = visibleFiles();
    if (files.length === 0) return;

    const idx = focusedIndex();

    if (e.key === "ArrowDown") {
      e.preventDefault();
      setFocusedIndex(Math.min(idx + 1, files.length - 1));
      return;
    }
    if (e.key === "ArrowUp") {
      e.preventDefault();
      setFocusedIndex(Math.max(idx - 1, 0));
      return;
    }

    if (idx < 0 || idx >= files.length) return;
    const { file, section } = files[idx];

    if (e.key === " ") {
      e.preventDefault();
      if (section === "unstaged") stageFile(file.path);
      else unstageFile(file.path);
      return;
    }
    if (e.key === "Enter") {
      e.preventDefault();
      openDiff(file, section);
      return;
    }
    if ((e.key === "Delete" || e.key === "Backspace") && section === "unstaged") {
      e.preventDefault();
      setConfirmDiscard(file);
      return;
    }
  }

  // --- Rendering helpers ---

  function renderFileEntry(file: FileEntry, section: "staged" | "unstaged", flatIndex: number) {
    const { dir, base } = splitPath(file.path);
    return (
      <div
        class={cx(s.fileEntry, focusedIndex() === flatIndex && s.fileFocused)}
        title={file.path}
        data-focused={focusedIndex() === flatIndex ? "" : undefined}
        onClick={() => { setFocusedIndex(flatIndex); openDiff(file, section); props.onFileSelect?.(file.path); }}
      >
        <span class={cx(s.statusBadge, statusClass(file.status))}>{file.status}</span>
        <span class={s.filePath}>
          <Show when={dir}>
            <span class={s.fileDir}>{dir}</span>
          </Show>
          <span class={s.fileBasename}>{base}</span>
        </span>
        <Show when={file.additions > 0 || file.deletions > 0}>
          <span class={s.diffStats}>
            <Show when={file.additions > 0}>
              <span class={s.diffAdd}>+{file.additions}</span>
            </Show>
            <Show when={file.deletions > 0}>
              <span class={s.diffDel}>-{file.deletions}</span>
            </Show>
          </span>
        </Show>
        <span class={s.actions}>
          <Show when={section === "staged"}>
            <button
              class={s.actionBtn}
              title="Unstage"
              onClick={(e) => { e.stopPropagation(); unstageFile(file.path); }}
            >
              <UnstageIcon />
            </button>
            <button
              class={s.actionBtn}
              title="Diff"
              onClick={(e) => { e.stopPropagation(); openDiff(file, section); }}
            >
              <DiffIcon />
            </button>
          </Show>
          <Show when={section === "unstaged"}>
            <button
              class={cx(s.actionBtn, s.actionBtnStage)}
              title="Stage"
              onClick={(e) => { e.stopPropagation(); stageFile(file.path); }}
            >
              <StageIcon />
            </button>
            <Show when={file.status !== "?"}>
              <button
                class={s.actionBtn}
                title="Diff"
                onClick={(e) => { e.stopPropagation(); openDiff(file, section); }}
              >
                <DiffIcon />
              </button>
            </Show>
            <button
              class={cx(s.actionBtn, s.actionBtnDanger)}
              title="Discard changes"
              onClick={(e) => { e.stopPropagation(); setConfirmDiscard(file); }}
            >
              <DiscardIcon />
            </button>
          </Show>
        </span>
      </div>
    );
  }

  return (
    <div class={s.container} onKeyDown={handleListKeyDown} tabIndex={-1}>
      {/* Commit section */}
      <Show when={props.repoPath}>
        <div class={s.commitSection} onKeyDown={handleCommitKeyDown}>
          <textarea
            class={s.commitTextarea}
            placeholder="Commit message..."
            value={commitMsg()}
            onInput={(e) => {
              setCommitMsg(e.currentTarget.value);
              autoResize(e.currentTarget);
            }}
            ref={(el) => {
              // Set initial height after mount
              requestAnimationFrame(() => autoResize(el));
            }}
            disabled={committing()}
          />
          <div class={s.commitRow}>
            <button
              class={cx(s.commitBtn, committing() && s.commitBtnDisabled)}
              disabled={committing() || (!commitMsg().trim() && !isAmend()) || (staged().length === 0 && !isAmend())}
              onClick={doCommit}
              title="Commit staged changes"
            >
              <Show when={committing()} fallback="Commit">
                <span class={s.spinner} />
                Committing...
              </Show>
            </button>
            <label class={s.amendLabel}>
              <input
                type="checkbox"
                class={s.amendCheckbox}
                checked={isAmend()}
                onChange={toggleAmend}
                disabled={committing()}
              />
              Amend
            </label>
          </div>
          <Show when={commitError()}>
            <div class={s.commitError}>{commitError()}</div>
          </Show>
          <Show when={commitSuccess()}>
            <div class={s.commitSuccess}>
              <CheckIcon /> Committed successfully
            </div>
          </Show>
        </div>
      </Show>

      {/* Smart prompt buttons — visible when an agent terminal exists for this repo */}
      <Show when={props.repoPath && hasAgentForRepo()}>
        <SmartButtonStrip placement="git-changes" repoPath={props.repoPath!} />
      </Show>

      {/* Empty state */}
      <Show when={!props.repoPath}>
        <div class={s.empty}>No repository selected</div>
      </Show>

      <Show when={props.repoPath && staged().length === 0 && unstaged().length === 0}>
        <div class={s.empty}>No changes</div>
      </Show>

      {/* Filter input */}
      <Show when={staged().length > 0 || unstaged().length > 0}>
        <div class={s.filterRow}>
          <input
            type="text"
            class={s.filterInput}
            placeholder="Filter... (*, ** wildcards)"
            value={filterQuery()}
            onInput={(e) => setFilterQuery(e.currentTarget.value)}
          />
          <Show when={filterQuery()}>
            <button class={s.filterClear} onClick={() => setFilterQuery("")}>&times;</button>
          </Show>
        </div>
      </Show>

      {/* STAGED section */}
      <Show when={filteredStaged().length > 0}>
        <div class={s.sectionHeader} onClick={() => setStagedExpanded((v) => !v)}>
          <span class={cx(s.chevron, !stagedExpanded() && s.chevronCollapsed)}>&#x25BC;</span>
          <span class={s.sectionLabel}>Staged</span>
          <span class={s.sectionCount}>
            {filterQuery() ? `${filteredStaged().length}/${staged().length}` : staged().length}
          </span>
          <button
            class={s.sectionAction}
            title="Unstage all"
            onClick={(e) => { e.stopPropagation(); unstageAll(); }}
          >
            Unstage all
          </button>
        </div>
        <Show when={stagedExpanded()}>
          <For each={filteredStaged()}>
            {(file, i) => renderFileEntry(file, "staged", i())}
          </For>
        </Show>
      </Show>

      {/* CHANGES (unstaged + untracked) section */}
      <Show when={filteredUnstaged().length > 0}>
        <div class={s.sectionHeader} onClick={() => setUnstagedExpanded((v) => !v)}>
          <span class={cx(s.chevron, !unstagedExpanded() && s.chevronCollapsed)}>&#x25BC;</span>
          <span class={s.sectionLabel}>Changes</span>
          <span class={s.sectionCount}>
            {filterQuery() ? `${filteredUnstaged().length}/${unstaged().length}` : unstaged().length}
          </span>
          <button
            class={s.sectionAction}
            title="Stage all"
            onClick={(e) => { e.stopPropagation(); stageAll(); }}
          >
            Stage all
          </button>
          <button
            class={s.sectionAction}
            title="Discard all tracked changes"
            onClick={(e) => { e.stopPropagation(); setConfirmDiscardAll(true); }}
          >
            Discard all
          </button>
        </div>
        <Show when={unstagedExpanded()}>
          <For each={filteredUnstaged()}>
            {(file, i) => {
              const offset = stagedExpanded() ? filteredStaged().length : 0;
              return renderFileEntry(file, "unstaged", offset + i());
            }}
          </For>
        </Show>
      </Show>

      {/* Discard single file confirmation */}
      <ConfirmDialog
        visible={confirmDiscard() !== null}
        title="Discard changes"
        message={`Discard all changes to "${confirmDiscard()?.path ?? ""}"?\nThis cannot be undone.`}
        confirmLabel="Discard"
        kind="warning"
        onClose={() => setConfirmDiscard(null)}
        onConfirm={handleConfirmDiscard}
      />

      {/* Discard all confirmation */}
      <ConfirmDialog
        visible={confirmDiscardAll()}
        title="Discard all changes"
        message="Discard all tracked working tree changes?\nThis cannot be undone."
        confirmLabel="Discard all"
        kind="warning"
        onClose={() => setConfirmDiscardAll(false)}
        onConfirm={handleConfirmDiscardAll}
      />
    </div>
  );
};

export default ChangesTab;
