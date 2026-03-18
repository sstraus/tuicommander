import { Component, createEffect, createSignal, onCleanup } from "solid-js";
import { DiffViewer } from "../ui";
import { useRepository } from "../../hooks/useRepository";
import { repositoriesStore } from "../../stores/repositories";
import { diffTabsStore } from "../../stores/diffTabs";
import { DomSearchEngine } from "../shared/DomSearchEngine";
import type { SearchOptions } from "../shared/DomSearchEngine";
import { SearchBar } from "../shared/SearchBar";
import { t } from "../../i18n";
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

  // Re-apply search when diff content changes
  createEffect(() => {
    diff();
    if (!searchVisible() || !lastSearchTerm) return;
    requestAnimationFrame(() => rerunSearch());
  });

  function rerunSearch() {
    if (!contentRef) return;
    if (!engine) engine = new DomSearchEngine(contentRef);
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

  return (
    <div class={s.content}>
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
