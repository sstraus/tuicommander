import { Component, createEffect, createSignal, Show, onMount, onCleanup } from "solid-js";
import { MarkdownRenderer } from "../ui";
import { appLogger } from "../../stores/appLogger";
import { ContextMenu, createContextMenu } from "../ContextMenu";
import { useRepository } from "../../hooks/useRepository";
import { repositoriesStore } from "../../stores/repositories";
import { editorTabsStore } from "../../stores/editorTabs";
import { diffTabsStore } from "../../stores/diffTabs";
import { invoke } from "../../invoke";
import { mdTabsStore, type MdTabData, type FileTab } from "../../stores/mdTabs";
import { CommentOverlay } from "./CommentOverlay";
import {
  insertTweakComment,
  removeTweakComment,
  updateTweakComment,
  type TweakComment,
} from "../../utils/tweakComments";
import { markdownProviderRegistry } from "../../plugins/markdownProviderRegistry";
import { DomSearchEngine } from "../shared/DomSearchEngine";
import type { SearchOptions } from "../shared/DomSearchEngine";
import { SearchBar } from "../shared/SearchBar";
import { t } from "../../i18n";
import { shortenHomePath } from "../../platform";
import e from "../shared/editor-header.module.css";
import s from "./MarkdownTab.module.css";

export interface MarkdownTabProps {
  tab: MdTabData;
  onClose?: () => void;
}

/** Public handle exposed via ref for external callers (e.g. App.tsx keybinding) */
export interface MarkdownTabHandle {
  openSearch: () => void;
}

export const MarkdownTab: Component<MarkdownTabProps> = (props) => {
  const [content, setContent] = createSignal("");
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [searchVisible, setSearchVisible] = createSignal(false);
  const [matchIndex, setMatchIndex] = createSignal(-1);
  const [matchCount, setMatchCount] = createSignal(0);
  const repo = useRepository();
  const contextMenu = createContextMenu();
  let wrapperRef: HTMLDivElement | undefined;
  let contentRef: HTMLDivElement | undefined;
  // Reactive signal so CommentOverlay mounts only after the rendered element exists.
  const [overlayContentEl, setOverlayContentEl] = createSignal<HTMLDivElement | undefined>();
  let engine: DomSearchEngine | undefined;
  let lastSearchTerm = "";
  let lastSearchOpts: SearchOptions = { caseSensitive: false, regex: false, wholeWord: false };
  let searchDebounceTimer: ReturnType<typeof setTimeout> | undefined;

  // Expose openSearch for external callers
  const handle: MarkdownTabHandle = {
    openSearch: () => {
      setSearchVisible(true);
    },
  };

  // Register handle on the mdTabsStore so App.tsx can access it
  createEffect(() => {
    mdTabsStore.setHandle(props.tab.id, handle);
    onCleanup(() => mdTabsStore.clearHandle(props.tab.id));
  });

  // When this tab is active, focus the wrapper so wheel events route by cursor
  // position rather than following xterm's retained textarea focus.
  const focusWrapper = () => requestAnimationFrame(() => wrapperRef?.focus({ preventScroll: true }));

  onMount(() => {
    if (mdTabsStore.state.activeId === props.tab.id) focusWrapper();
  });

  createEffect(() => {
    if (mdTabsStore.state.activeId === props.tab.id) focusWrapper();
  });

  /** Read file content — uses repo-scoped read for relative paths, external read for absolute.
   *  Absolute paths bypass the repo security check since they're already fully qualified. */
  const readFileContent = async (fsRoot: string | undefined, filePath: string): Promise<string> => {
    // Absolute paths: always use read_external_file (no repo constraint).
    // This avoids "Access denied" when the file is outside the tab's repoPath
    // (e.g. file from a different repo opened via terminal link click).
    if (filePath.startsWith("/")) {
      return await invoke<string>("read_external_file", { path: filePath });
    }
    return fsRoot
      ? await repo.readFile(fsRoot, filePath)
      : await invoke<string>("read_external_file", { path: filePath });
  };

  createEffect(() => {
    const tab = props.tab;

    if (tab.type === "file") {
      const { repoPath, filePath, fsRoot } = tab as FileTab;
      // Track revisions by repo path (keyed by the repo root, not the worktree)
      void (repoPath ? repositoriesStore.getRevision(repoPath) : 0);

      if (!filePath) {
        setContent("");
        return;
      }

      if (!content()) setLoading(true);
      setError(null);

      (async () => {
        try {
          const fileContent = await readFileContent(fsRoot || repoPath, filePath);
          if (!fileContent) {
            appLogger.warn("app", "readFileContent returned empty", { repoPath, filePath, fsRoot });
          }
          setContent(fileContent);
        } catch (err) {
          const msg = err instanceof Error ? err.message : String(err);
          appLogger.error("app", "readFileContent failed", { repoPath, filePath, fsRoot, error: msg });
          setError(msg);
          setContent("");
        } finally {
          setLoading(false);
        }
      })();
    } else if (tab.type === "virtual") {
      const { contentUri } = tab;

      setLoading(true);
      setError(null);

      (async () => {
        try {
          const result = await markdownProviderRegistry.resolve(contentUri);
          if (result === null) {
            setError("Content unavailable");
            setContent("");
          } else {
            setContent(result);
          }
        } catch (err) {
          setError(String(err));
          setContent("");
        } finally {
          setLoading(false);
        }
      })();
    } else {
      setContent("");
      setLoading(false);
    }
  });

  // Re-apply search when content changes
  createEffect(() => {
    // Subscribe to content changes
    content();
    if (!searchVisible() || !lastSearchTerm) return;
    // Wait for DOM to settle after innerHTML update
    requestAnimationFrame(() => rerunSearch());
  });

  // Poll for file changes (external edits, agent modifications).
  createEffect(() => {
    const tab = props.tab;
    if (tab.type !== "file" || !tab.filePath) return;
    const { filePath, fsRoot, repoPath } = tab as FileTab;
    const root = fsRoot || repoPath;

    const timer = setInterval(async () => {
      if (document.visibilityState === "hidden") return;
      if (mdTabsStore.state.activeId !== props.tab.id) return;
      try {
        const diskContent = await readFileContent(root, filePath);
        if (diskContent !== content()) setContent(diskContent);
      } catch {
        // File may have been deleted — ignore
      }
    }, 5000);
    onCleanup(() => clearInterval(timer));
  });

  /** Run search with current term and options */
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
    const idx = engine.next();
    setMatchIndex(idx);
  };

  const handleSearchPrev = () => {
    if (!engine || matchCount() === 0) return;
    const idx = engine.prev();
    setMatchIndex(idx);
  };

  const handleSearchClose = () => {
    engine?.clear();
    setSearchVisible(false);
    setMatchCount(0);
    setMatchIndex(-1);
    focusWrapper();
  };

  const handleMdLink = (href: string) => {
    const tab = props.tab;
    if (tab.type !== "file") return;
    const ft = tab as FileTab;
    const currentDir = ft.filePath.includes("/")
      ? ft.filePath.slice(0, ft.filePath.lastIndexOf("/"))
      : "";
    const resolved = currentDir ? `${currentDir}/${href}` : href;
    mdTabsStore.add(ft.repoPath, resolved, ft.fsRoot);
  };

  /** Write the updated markdown source back to disk and refresh displayed content. */
  const writeTweakedSource = async (updatedContent: string) => {
    const tab = props.tab;
    if (tab.type !== "file") return;
    const ft = tab as FileTab;
    const root = ft.fsRoot || ft.repoPath;

    try {
      if (root) {
        await invoke<void>("write_file", { repoPath: root, file: ft.filePath, content: updatedContent });
      } else if (ft.filePath.startsWith("/")) {
        await invoke<void>("write_external_file", { path: ft.filePath, content: updatedContent });
      } else {
        appLogger.error("app", "writeTweakedSource: cannot resolve write target", { filePath: ft.filePath });
        return;
      }
      setContent(updatedContent);
    } catch (err) {
      appLogger.error("app", "writeTweakedSource: write failed", err);
    }
  };

  const handleTweakSave = async (comment: TweakComment) => {
    const current = content();
    // If the id already exists in the source, it's an edit; otherwise it's a new insert.
    const isExisting = current.includes(`<!--tweak:begin:${comment.id}-->`);
    try {
      const updated = isExisting
        ? updateTweakComment(current, comment.id, comment.comment)
        : insertTweakComment(current, comment);
      await writeTweakedSource(updated);
    } catch (err) {
      appLogger.error("app", "handleTweakSave failed", err);
    }
  };

  const handleTweakDelete = async (id: string) => {
    try {
      const updated = removeTweakComment(content(), id);
      await writeTweakedSource(updated);
    } catch (err) {
      appLogger.error("app", "handleTweakDelete failed", err);
    }
  };

  const handleEdit = () => {
    const tab = props.tab;
    if (tab.type === "file") {
      const ft = tab as FileTab;
      editorTabsStore.add(ft.fsRoot || ft.repoPath, ft.filePath);
    }
  };

  const displayPath = () => {
    const tab = props.tab;
    return tab.type === "file" ? tab.filePath : tab.title;
  };

  const baseDir = () => {
    const tab = props.tab;
    if (tab.type !== "file") return undefined;
    const ft = tab as FileTab;
    const root = ft.fsRoot || ft.repoPath;
    if (!root && ft.filePath.startsWith("/")) {
      const lastSlash = ft.filePath.lastIndexOf("/");
      return lastSlash > 0 ? ft.filePath.slice(0, lastSlash) : "/";
    }
    const dir = ft.filePath.includes("/")
      ? ft.filePath.slice(0, ft.filePath.lastIndexOf("/"))
      : "";
    return dir ? `${root}/${dir}` : root;
  };

  const fullPath = () => {
    const tab = props.tab;
    if (tab.type !== "file") return null;
    const ft = tab as FileTab;
    const root = ft.fsRoot || ft.repoPath;
    return root ? `${root}/${ft.filePath}` : ft.filePath;
  };

  const handleCopyPath = () => {
    const path = fullPath();
    if (!path) return;
    navigator.clipboard.writeText(shortenHomePath(path)).catch((err) =>
      appLogger.error("app", "Failed to copy path", err),
    );
  };

  const handleHeaderContextMenu = (ev: MouseEvent) => {
    if (!fullPath()) return;
    ev.preventDefault();
    contextMenu.open(ev);
  };

  return (
    <div ref={wrapperRef} class={s.wrapper} tabIndex={-1}>
      <div class={e.header} onContextMenu={handleHeaderContextMenu}>
        <span class={e.filename} title={displayPath()}>
          {displayPath()}
        </span>
        <Show when={props.tab.type === "file"}>
          <button class={e.btn} onClick={handleEdit} title={t("markdownTab.edit", "Edit file")}>
            <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
              <path d="M11.13 1.47a1.5 1.5 0 0 1 2.12 0l1.28 1.28a1.5 1.5 0 0 1 0 2.12L5.9 13.5a1 1 0 0 1-.5.27l-3.5.87a.5.5 0 0 1-.6-.6l.87-3.5a1 1 0 0 1 .27-.5L11.13 1.47ZM12.2 2.53l-8.46 8.47-.58 2.34 2.34-.58 8.47-8.46-1.77-1.77Z"/>
            </svg>
            {" "}{t("markdownTab.editBtn", "Edit")}
          </button>
          <Show when={(props.tab as FileTab).repoPath}>
            <button
              class={e.btn}
              onClick={() => { const ft = props.tab as FileTab; diffTabsStore.add(ft.repoPath, ft.filePath, "M"); }}
              title={t("markdownTab.viewDiff", "View diff")}
            >
              <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
                <path d="M2 3h5v1H2zm0 3h5v1H2zm0 3h4v1H2zm7-6h5v1H9zm0 3h5v1H9zm0 3h4v1H9zM7.5 1v14M.5 0v16" fill="none" stroke="currentColor" stroke-width="1" opacity="0.5" />
                <path d="M4 12l-2 2 2 2" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
                <path d="M12 12l2 2-2 2" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
              </svg>
              {" "}{t("markdownTab.diffBtn", "Diff")}
            </button>
          </Show>
        </Show>
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

      <div class={s.content}>
        <MarkdownRenderer
          content={content()}
          baseDir={baseDir()}
          onLinkClick={handleMdLink}
          contentRef={(el) => { contentRef = el; setOverlayContentEl(el); }}
          emptyMessage={
            loading()
              ? t("markdownTab.loading", "Loading...")
              : error()
                ? `${t("markdownTab.error", "Error:")} ${error()}`
                : t("markdownTab.noContent", "No content")
          }
        />
      </div>

      {/* Mount CommentOverlay ONLY for the active file tab — otherwise every
          open markdown tab would attach its own selectionchange listener and
          they'd all fire on every cursor move across the app. */}
      <Show
        when={
          props.tab.type === "file" &&
          mdTabsStore.state.activeId === props.tab.id &&
          overlayContentEl()
        }
      >
        {(el) => (
          <CommentOverlay
            contentRef={el()}
            onSave={(c) => { void handleTweakSave(c); }}
            onDelete={(id) => { void handleTweakDelete(id); }}
          />
        )}
      </Show>

      <ContextMenu
        items={[{ label: t("markdownTab.copyPath", "Copy Path"), action: handleCopyPath }]}
        x={contextMenu.position().x}
        y={contextMenu.position().y}
        visible={contextMenu.visible()}
        onClose={contextMenu.close}
      />
    </div>
  );
};

export default MarkdownTab;
