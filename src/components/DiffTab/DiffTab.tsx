import { Component, createEffect, createSignal, onCleanup } from "solid-js";
import { DiffViewer } from "../ui";
import { useRepository } from "../../hooks/useRepository";
import { repositoriesStore } from "../../stores/repositories";
import { diffTabsStore } from "../../stores/diffTabs";
import { uiStore, type DiffViewMode } from "../../stores/ui";
import { DomSearchEngine } from "../shared/DomSearchEngine";
import type { SearchOptions } from "../shared/DomSearchEngine";
import { SearchBar } from "../shared/SearchBar";
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

export const DiffTab: Component<DiffTabProps> = (props) => {
  const [diff, setDiff] = createSignal("");
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const repo = useRepository();

  // Search state
  const [searchVisible, setSearchVisible] = createSignal(false);
  const [matchIndex, setMatchIndex] = createSignal(-1);
  const [matchCount, setMatchCount] = createSignal(0);

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
    // Delay to let the library re-render after mode switch
    requestAnimationFrame(() => rerunSearch());
  });

  function rerunSearch() {
    if (!contentRef) return;
    // Recreate engine after mode switch (DOM structure changes)
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

  const mode = (): DiffViewMode => uiStore.state.diffViewMode;

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
    </div>
  );
};

export default DiffTab;
