import { Component, createEffect, createSignal, onCleanup, Show } from "solid-js";
import { DiffViewer } from "../ui";
import { useRepository } from "../../hooks/useRepository";
import { repositoriesStore } from "../../stores/repositories";
import { diffTabsStore } from "../../stores/diffTabs";
import { uiStore, type DiffViewMode } from "../../stores/ui";
import { DomSearchEngine } from "../shared/DomSearchEngine";
import type { SearchOptions } from "../shared/DomSearchEngine";
import { SearchBar } from "../shared/SearchBar";
import { ConfirmDialog } from "../ConfirmDialog";
import { invoke } from "../../invoke";
import { appLogger } from "../../stores/appLogger";
import { t } from "../../i18n";
import { cx } from "../../utils";
import { editorTabsStore } from "../../stores/editorTabs";
import { extractHunks, buildPartialPatch } from "./diffPatch";
import s from "./DiffTab.module.css";

export interface DiffTabProps {
  tabId: string;
  repoPath: string;
  filePath: string;
  scope?: string;
  untracked?: boolean;
  onClose?: () => void;
}

/** Check if this diff tab supports restore actions (working tree or staged, not commit/untracked) */
function canRestore(scope?: string, untracked?: boolean): boolean {
  if (untracked) return false;
  // scope is undefined (working tree) or "staged" — both support restore
  // scope is a commit hash — read-only
  if (!scope || scope === "staged") return true;
  return false;
}

export const DiffTab: Component<DiffTabProps> = (props) => {
  const [diff, setDiff] = createSignal("");
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const repo = useRepository();

  // Search state
  const [searchVisible, setSearchVisible] = createSignal(false);
  const [matchIndex, setMatchIndex] = createSignal(-1);
  const [matchCount, setMatchCount] = createSignal(0);

  // Hunk restore state
  const [confirmVisible, setConfirmVisible] = createSignal(false);
  const [pendingHunkPatch, setPendingHunkPatch] = createSignal<string | null>(null);
  const [hoverHunkIdx, setHoverHunkIdx] = createSignal<number | null>(null);

  // Line-level selection state
  const [selectedLines, setSelectedLines] = createSignal<Set<number>>(new Set<number>());
  const [selectedHunkIdx, setSelectedHunkIdx] = createSignal<number | null>(null);
  let lastClickedLine = -1; // For shift-click range selection

  // Clear selection when diff changes
  createEffect(() => {
    diff();
    setSelectedLines(new Set<number>());
    setSelectedHunkIdx(null);
    lastClickedLine = -1;
  });

  let contentRef: HTMLElement | undefined;
  let engine: DomSearchEngine | undefined;
  let lastSearchTerm = "";
  let lastSearchOpts: SearchOptions = { caseSensitive: false, regex: false, wholeWord: false };
  let searchDebounceTimer: ReturnType<typeof setTimeout> | undefined;

  // Expose openSearch for Cmd+F
  createEffect(() => {
    diffTabsStore.setHandle(props.tabId, { openSearch: () => setSearchVisible(true) });
    onCleanup(() => diffTabsStore.clearHandle(props.tabId));
  });

  // Load file diff when props change or the repo revision bumps (git index/HEAD changed)
  createEffect(() => {
    const repoPath = props.repoPath;
    const filePath = props.filePath;
    const scope = props.scope;
    void (repoPath ? repositoriesStore.getRevision(repoPath) : 0);

    if (!repoPath || !filePath) {
      setDiff("");
      return;
    }

    setLoading(true);
    setError(null);

    (async () => {
      try {
        const diffContent = await repo.getFileDiff(repoPath, filePath, scope, props.untracked);
        setDiff(diffContent);
      } catch (err) {
        setError(String(err));
        setDiff("");
      } finally {
        setLoading(false);
      }
    })();
  });

  // Re-apply search when diff content changes or view mode changes
  createEffect(() => {
    diff();
    uiStore.state.diffViewMode;
    if (!searchVisible() || !lastSearchTerm) return;
    requestAnimationFrame(() => rerunSearch());
  });

  function rerunSearch() {
    if (!contentRef) return;
    engine = new DomSearchEngine(contentRef);
    const count = engine.search(lastSearchTerm, lastSearchOpts);
    setMatchCount(count);
    setMatchIndex(count > 0 ? 0 : -1);
  }

  const handleSearch = (term: string, opts: SearchOptions) => {
    lastSearchTerm = term;
    lastSearchOpts = opts;

    clearTimeout(searchDebounceTimer);
    if (!term) {
      engine?.clear();
      setMatchCount(0);
      setMatchIndex(-1);
      return;
    }

    searchDebounceTimer = setTimeout(() => {
      rerunSearch();
    }, 150);
  };

  const handleSearchNext = () => {
    if (!engine || matchCount() === 0) return;
    setMatchIndex(engine.next());
  };

  const handleSearchPrev = () => {
    if (!engine || matchCount() === 0) return;
    setMatchIndex(engine.prev());
  };

  const handleSearchClose = () => {
    engine?.clear();
    setSearchVisible(false);
    setMatchCount(0);
    setMatchIndex(-1);
  };

  /** Force unified mode for one-sided diffs (new/deleted files) where split wastes half the screen */
  const mode = (): DiffViewMode => {
    const d = diff();
    if (d && (d.includes("new file mode") || d.includes("deleted file mode"))) return "unified";
    return uiStore.state.diffViewMode;
  };

  // --- Hunk restore ---

  /** Find the hunk index from a DOM element within the diff content */
  function findHunkIndex(el: HTMLElement): number | null {
    // Walk up to find a .diff-line-hunk-action or .diff-line-hunk-content
    const hunkRow = el.closest("tr, [class*='diff-line-hunk']")?.closest("tr");
    if (!hunkRow || !contentRef) return null;
    // Count how many hunk rows precede this one in the DOM
    const allHunkRows = contentRef.querySelectorAll("[class*='diff-line-hunk-content']");
    for (let i = 0; i < allHunkRows.length; i++) {
      if (allHunkRows[i].closest("tr") === hunkRow) return i;
    }
    return null;
  }

  function handleRevertClick(hunkIdx: number) {
    const hunks = extractHunks(diff());
    if (hunkIdx < 0 || hunkIdx >= hunks.length) return;
    setPendingHunkPatch(hunks[hunkIdx]);
    setConfirmVisible(true);
  }

  async function confirmRevert() {
    const patch = pendingHunkPatch();
    if (!patch) return;
    setConfirmVisible(false);
    setPendingHunkPatch(null);

    try {
      await invoke("git_apply_reverse_patch", {
        path: props.repoPath,
        patch,
        scope: props.scope || undefined,
      });
    } catch (err) {
      appLogger.error("git", "Failed to revert hunk", err);
    }
    clearSelection();
    // The repo revision bump from git changes will trigger diff reload automatically
  }

  function cancelRevert() {
    setConfirmVisible(false);
    setPendingHunkPatch(null);
  }

  const isStaged = () => props.scope === "staged";

  // --- Line-level selection ---

  /** Find which hunk and line index a DOM element belongs to */
  function findLineInfo(el: HTMLElement): { hunkIdx: number; lineIdx: number; isChange: boolean } | null {
    const row = el.closest("tr");
    if (!row || !contentRef) return null;

    // Check if this is an addition or deletion row (not context or hunk header)
    const hasAdd = row.querySelector("[class*='diff-line-add']");
    const hasDel = row.querySelector("[class*='diff-line-del']");
    if (!hasAdd && !hasDel) return null;

    // Find hunk index: count how many hunk headers precede this row
    const allRows = Array.from(contentRef.querySelectorAll("tr"));
    const rowIdx = allRows.indexOf(row);
    if (rowIdx < 0) return null;

    let hunkIdx = -1;
    let lineIdx = -1;
    let lineCount = 0;
    for (let i = 0; i <= rowIdx; i++) {
      const r = allRows[i];
      if (r.querySelector("[class*='diff-line-hunk']")) {
        hunkIdx++;
        lineCount = 0;
      } else if (i === rowIdx) {
        lineIdx = lineCount;
      } else {
        lineCount++;
      }
    }

    return hunkIdx >= 0 ? { hunkIdx, lineIdx, isChange: true } : null;
  }

  function handleLineClick(e: MouseEvent) {
    if (!canRestore(props.scope, props.untracked)) return;
    // Don't interfere with revert button clicks
    if ((e.target as HTMLElement).closest(`.${s.revertBtn}`)) return;

    const info = findLineInfo(e.target as HTMLElement);
    if (!info) return;

    // If clicking in a different hunk, reset selection
    if (selectedHunkIdx() !== null && selectedHunkIdx() !== info.hunkIdx) {
      setSelectedLines(new Set<number>());
    }
    setSelectedHunkIdx(info.hunkIdx);

    const prev = selectedLines();
    const next = new Set(prev);

    if (e.shiftKey && lastClickedLine >= 0) {
      // Shift+click: select range
      const start = Math.min(lastClickedLine, info.lineIdx);
      const end = Math.max(lastClickedLine, info.lineIdx);
      // Get all change lines in the hunk to only select those in range
      const hunks = extractHunks(diff());
      if (info.hunkIdx < hunks.length) {
        const hunkLines = hunks[info.hunkIdx].split("\n");
        const bodyStart = hunkLines.findIndex((l) => l.startsWith("@@"));
        if (bodyStart >= 0) {
          const body = hunkLines.slice(bodyStart + 1);
          for (let i = start; i <= end; i++) {
            if (i < body.length && (body[i].startsWith("+") || body[i].startsWith("-"))) {
              next.add(i);
            }
          }
        }
      }
    } else {
      // Toggle single line
      if (next.has(info.lineIdx)) {
        next.delete(info.lineIdx);
      } else {
        next.add(info.lineIdx);
      }
    }

    lastClickedLine = info.lineIdx;
    setSelectedLines(next);

    // Apply visual selection to DOM rows
    applyLineSelectionStyles();
  }

  /** Apply/remove CSS class on selected rows */
  function applyLineSelectionStyles() {
    if (!contentRef) return;
    const allRows = contentRef.querySelectorAll("tr");
    const sel = selectedLines();
    const hIdx = selectedHunkIdx();
    let currentHunk = -1;
    let lineCount = 0;

    allRows.forEach((row) => {
      if (row.querySelector("[class*='diff-line-hunk']")) {
        currentHunk++;
        lineCount = 0;
      } else {
        if (currentHunk === hIdx && sel.has(lineCount)) {
          row.classList.add(s.lineSelected);
        } else {
          row.classList.remove(s.lineSelected);
        }
        lineCount++;
      }
    });
  }

  // Re-apply styles when selection changes
  createEffect(() => {
    selectedLines();
    selectedHunkIdx();
    requestAnimationFrame(() => applyLineSelectionStyles());
  });

  function handleRestoreSelected() {
    const hIdx = selectedHunkIdx();
    if (hIdx === null) return;
    const sel = selectedLines();
    if (sel.size === 0) return;

    const patch = buildPartialPatch(diff(), hIdx, sel);
    if (!patch) return;

    setPendingHunkPatch(patch);
    setConfirmVisible(true);
  }

  function clearSelection() {
    setSelectedLines(new Set<number>());
    setSelectedHunkIdx(null);
    lastClickedLine = -1;
    if (contentRef) {
      contentRef.querySelectorAll(`.${s.lineSelected}`).forEach((el) =>
        el.classList.remove(s.lineSelected),
      );
    }
  }

  const selectedCount = () => selectedLines().size;

  const confirmTitle = () => {
    if (selectedCount() > 0) {
      const count = selectedCount();
      return isStaged()
        ? `Unstage ${count} selected line${count > 1 ? "s" : ""}?`
        : `Discard ${count} selected line${count > 1 ? "s" : ""}?`;
    }
    return isStaged()
      ? t("diffTab.unstageHunk", "Unstage this change?")
      : t("diffTab.discardHunk", "Discard this change?");
  };
  const confirmMessage = () => isStaged()
    ? t("diffTab.unstageHunkMsg", "This will remove this hunk from the staging area. The changes will remain in your working directory.")
    : t("diffTab.discardHunkMsg", "This will permanently discard this change. This cannot be undone.");

  return (
    <div class={s.content}>
      {/* Toolbar with view mode toggle */}
      <div class={s.toolbar}>
        <button
          class={cx(s.modeBtn, mode() === "split" && s.modeBtnActive)}
          onClick={() => uiStore.setDiffViewMode("split")}
          title={t("diffTab.splitView", "Side-by-side")}
        >
          <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
            <path d="M1 2h6v12H1V2zm8 0h6v12H9V2zM2 3v10h4V3H2zm8 0v10h4V3h-4z" />
          </svg>
        </button>
        <button
          class={cx(s.modeBtn, mode() === "unified" && s.modeBtnActive)}
          onClick={() => uiStore.setDiffViewMode("unified")}
          title={t("diffTab.unifiedView", "Inline")}
        >
          <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
            <path d="M1 2h14v12H1V2zm1 1v10h12V3H2z" />
          </svg>
        </button>
        <div style={{ "margin-left": "auto" }}>
          <button
            class={s.modeBtn}
            onClick={() => editorTabsStore.add(props.repoPath, props.filePath)}
            title={t("diffTab.editFile", "Edit file")}
          >
            <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
              <path d="M12.1 1.3a1.5 1.5 0 0 1 2.1 0l.5.5a1.5 1.5 0 0 1 0 2.1L5.8 12.8l-3.5.9.9-3.5L12.1 1.3zM11 3.4 4.1 10.3l-.5 1.9 1.9-.5L12.4 4.8 11 3.4z" />
            </svg>
          </button>
        </div>
      </div>
      <SearchBar
        visible={searchVisible()}
        onSearch={handleSearch}
        onNext={handleSearchNext}
        onPrev={handleSearchPrev}
        onClose={handleSearchClose}
        matchIndex={matchIndex()}
        matchCount={matchCount()}
      />
      <div
        class={s.diffWrapper}
        onMouseOver={(e) => {
          if (!canRestore(props.scope, props.untracked)) return;
          const idx = findHunkIndex(e.target as HTMLElement);
          setHoverHunkIdx(idx);
        }}
        onMouseOut={() => setHoverHunkIdx(null)}
        onClick={(e) => {
          // Delegate click on revert buttons
          const btn = (e.target as HTMLElement).closest(`.${s.revertBtn}`);
          if (btn) {
            const idx = parseInt(btn.getAttribute("data-hunk-idx") || "-1", 10);
            if (idx >= 0) handleRevertClick(idx);
            return;
          }
          // Line-level selection
          handleLineClick(e);
        }}
      >
        <DiffViewer
          diff={diff()}
          mode={mode()}
          contentRef={(el: HTMLElement) => { contentRef = el; }}
          emptyMessage={
            loading()
              ? t("diffTab.loading", "Loading diff...")
              : error()
                ? `${t("diffTab.error", "Error:")} ${error()}`
                : t("diffTab.noChanges", "No changes")
          }
        />
        {/* Floating revert buttons — rendered after diff, positioned via CSS */}
        <Show when={canRestore(props.scope, props.untracked) && diff().trim()}>
          <HunkRevertOverlay
            diff={diff()}
            contentRef={contentRef}
            hoverIdx={hoverHunkIdx()}
            isStaged={isStaged()}
          />
        </Show>
        {/* Floating restore-selected button */}
        <Show when={canRestore(props.scope, props.untracked) && selectedCount() > 0}>
          <div class={s.restoreSelectedBar}>
            <button class={s.restoreSelectedBtn} onClick={handleRestoreSelected}>
              {isStaged() ? "Unstage" : "Discard"} {selectedCount()} line{selectedCount() > 1 ? "s" : ""}
            </button>
            <button class={s.clearSelectionBtn} onClick={clearSelection} title="Clear selection">
              &times;
            </button>
          </div>
        </Show>
      </div>
      <ConfirmDialog
        visible={confirmVisible()}
        title={confirmTitle()}
        message={confirmMessage()}
        confirmLabel={isStaged() ? t("diffTab.unstage", "Unstage") : t("diffTab.discard", "Discard")}
        kind={isStaged() ? "info" : "warning"}
        onConfirm={confirmRevert}
        onClose={cancelRevert}
      />
    </div>
  );
};

/** Overlay that positions revert buttons on hunk header rows */
const HunkRevertOverlay: Component<{
  diff: string;
  contentRef: HTMLElement | undefined;
  hoverIdx: number | null;
  isStaged: boolean;
}> = (props) => {
  // Position revert buttons on hunk header rows using the DOM
  const buttons = () => {
    if (!props.contentRef) return [];
    const hunkEls = props.contentRef.querySelectorAll("[class*='diff-line-hunk-content']");
    const result: Array<{ top: number; idx: number }> = [];
    const containerRect = props.contentRef.getBoundingClientRect();
    hunkEls.forEach((el, i) => {
      const row = el.closest("tr");
      if (!row) return;
      const rect = row.getBoundingClientRect();
      result.push({ top: rect.top - containerRect.top + props.contentRef!.scrollTop, idx: i });
    });
    return result;
  };

  return (
    <>
      {buttons().map((b) => (
        <button
          class={cx(s.revertBtn, props.hoverIdx === b.idx && s.revertBtnVisible)}
          style={{ top: `${b.top}px` }}
          data-hunk-idx={b.idx}
          title={props.isStaged
            ? t("diffTab.unstageHunk", "Unstage this change")
            : t("diffTab.discardHunk", "Discard this change")
          }
          aria-label={props.isStaged ? "Unstage hunk" : "Discard hunk"}
        >
          <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor">
            <path d="M2 8a6 6 0 1 1 12 0A6 6 0 0 1 2 8zm6-4a4 4 0 1 0 0 8 4 4 0 0 0 0-8zM6.5 7.5l3-2v4l-3-2z" />
          </svg>
        </button>
      ))}
    </>
  );
};

export default DiffTab;
