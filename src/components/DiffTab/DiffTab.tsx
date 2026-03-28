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
import s from "./DiffTab.module.css";

export interface DiffTabProps {
  tabId: string;
  repoPath: string;
  filePath: string;
  scope?: string;
  untracked?: boolean;
  onClose?: () => void;
}

/** Extract individual hunks from a unified diff string.
 *  Each hunk includes the diff header (diff --git, ---, +++) and one @@ block. */
function extractHunks(diff: string): string[] {
  const lines = diff.split("\n");
  // Collect the file header lines (everything before first @@)
  let headerEnd = 0;
  for (let i = 0; i < lines.length; i++) {
    if (lines[i].startsWith("@@")) { headerEnd = i; break; }
  }
  const fileHeader = lines.slice(0, headerEnd).join("\n");

  // Split on @@ boundaries
  const hunks: string[] = [];
  let hunkStart = -1;
  for (let i = headerEnd; i < lines.length; i++) {
    if (lines[i].startsWith("@@")) {
      if (hunkStart >= 0) {
        hunks.push(fileHeader + "\n" + lines.slice(hunkStart, i).join("\n"));
      }
      hunkStart = i;
    }
  }
  if (hunkStart >= 0) {
    hunks.push(fileHeader + "\n" + lines.slice(hunkStart).join("\n"));
  }
  return hunks;
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
    // The repo revision bump from git changes will trigger diff reload automatically
  }

  function cancelRevert() {
    setConfirmVisible(false);
    setPendingHunkPatch(null);
  }

  const isStaged = () => props.scope === "staged";
  const confirmTitle = () => isStaged()
    ? t("diffTab.unstageHunk", "Unstage this change?")
    : t("diffTab.discardHunk", "Discard this change?");
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
          if (!btn) return;
          const idx = parseInt(btn.getAttribute("data-hunk-idx") || "-1", 10);
          if (idx >= 0) handleRevertClick(idx);
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
