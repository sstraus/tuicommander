import { Component, For, Show, createEffect, createMemo, createSignal, onCleanup } from "solid-js";
import { commandPaletteStore } from "../../stores/commandPalette";
import { repositoriesStore } from "../../stores/repositories";
import { terminalsStore } from "../../stores/terminals";
import { paneLayoutStore } from "../../stores/paneLayout";
import { editorTabsStore } from "../../stores/editorTabs";
import { mdTabsStore } from "../../stores/mdTabs";
import { classifyDroppedFile } from "../../hooks/useFileDrop";
import { FileIcon } from "../FileBrowserPanel/FileIcon";
import type { ActionEntry } from "../../actions/actionRegistry";
import type { ContentMatch, DirEntry } from "../../types/fs";
import type { TerminalMatch } from "../../types";
import shared from "../shared/dialog.module.css";
import s from "./CommandPalette.module.css";

export interface CommandPaletteProps {
  actions: ActionEntry[];
}

export const CommandPalette: Component<CommandPaletteProps> = (props) => {
  const [selectedIndex, setSelectedIndex] = createSignal(0);
  let inputRef: HTMLInputElement | undefined;
  let listRef: HTMLDivElement | undefined;

  const isOpen = () => commandPaletteStore.state.isOpen;
  const mode = () => commandPaletteStore.mode();
  const searchQuery = () => commandPaletteStore.searchQuery();

  /** Filter and sort actions: recent first, then alphabetical. Substring match on label + category. */
  const filteredActions = createMemo(() => {
    if (mode() !== "command") return [];
    const query = commandPaletteStore.state.query.toLowerCase();
    const recent = commandPaletteStore.state.recentActions;
    let actions = props.actions;

    if (query) {
      actions = actions.filter(
        (a) => a.label.toLowerCase().includes(query) || a.category.toLowerCase().includes(query),
      );
    }

    // Sort: recent actions first (by recency rank), then alphabetical
    return [...actions].sort((a, b) => {
      const aRecent = recent.indexOf(a.id);
      const bRecent = recent.indexOf(b.id);
      if (aRecent !== -1 && bRecent !== -1) return aRecent - bRecent;
      if (aRecent !== -1) return -1;
      if (bRecent !== -1) return 1;
      return a.label.localeCompare(b.label);
    });
  });

  /** Item count for the current mode */
  const itemCount = () => {
    switch (mode()) {
      case "filename": return commandPaletteStore.state.filenameResults.length;
      case "content": return commandPaletteStore.state.contentResults.length;
      case "terminal": return commandPaletteStore.state.terminalResults.length;
      default: return filteredActions().length;
    }
  };

  // Reset selection when query or result lists change
  createEffect(() => {
    void commandPaletteStore.state.query;
    void commandPaletteStore.state.contentResults.length;
    void commandPaletteStore.state.terminalResults.length;
    setSelectedIndex(0);
  });

  // Focus input when opened
  createEffect(() => {
    if (isOpen()) {
      requestAnimationFrame(() => inputRef?.focus());
    }
  });

  // Scroll selected item into view
  createEffect(() => {
    const idx = selectedIndex();
    if (!listRef) return;
    const item = listRef.children[idx] as HTMLElement | undefined;
    item?.scrollIntoView({ block: "nearest" });
  });

  const executeAction = (action: ActionEntry) => {
    commandPaletteStore.recordUsage(action.id);
    commandPaletteStore.close();
    action.execute();
  };

  /** Open file in the appropriate tab based on extension */
  const openFile = (repoPath: string, filePath: string, line?: number) => {
    if (classifyDroppedFile(filePath) === "markdown") {
      mdTabsStore.add(repoPath, filePath);
    } else {
      editorTabsStore.add(repoPath, filePath, line);
    }
  };

  const openFileEntry = (entry: DirEntry) => {
    const repoPath = repositoriesStore.state.activeRepoPath ?? "";
    openFile(repoPath, entry.path);
    commandPaletteStore.close();
  };

  const openContentMatch = (match: ContentMatch) => {
    const repoPath = repositoriesStore.state.activeRepoPath ?? "";
    openFile(repoPath, match.path, match.line_number);
    commandPaletteStore.close();
  };

  /** Navigate to a terminal match: switch tab/pane, scroll to line, focus */
  const navigateToTerminalMatch = (match: TerminalMatch) => {
    const { terminalId, lineIndex } = match;
    terminalsStore.setActive(terminalId);
    // Activate the pane group containing this terminal
    const groupId = paneLayoutStore.getGroupForTab(terminalId);
    if (groupId) paneLayoutStore.setActiveGroup(groupId);
    commandPaletteStore.close();
    // Scroll after a tick to ensure terminal is mounted/focused
    requestAnimationFrame(() => {
      const term = terminalsStore.state.terminals[terminalId];
      term?.ref?.scrollToLine(lineIndex);
      term?.ref?.focus();
    });
  };

  // Keyboard navigation
  createEffect(() => {
    if (!isOpen()) return;

    const handleKeydown = (e: KeyboardEvent) => {
      const count = itemCount();

      switch (e.key) {
        case "ArrowDown":
          e.preventDefault();
          e.stopPropagation();
          setSelectedIndex((i) => Math.min(i + 1, count - 1));
          break;
        case "ArrowUp":
          e.preventDefault();
          e.stopPropagation();
          setSelectedIndex((i) => Math.max(i - 1, 0));
          break;
        case "Enter":
          e.preventDefault();
          e.stopPropagation();
          if (mode() === "filename") {
            const entry = commandPaletteStore.state.filenameResults[selectedIndex()];
            if (entry) openFileEntry(entry);
          } else if (mode() === "content") {
            const match = commandPaletteStore.state.contentResults[selectedIndex()];
            if (match) openContentMatch(match);
          } else if (mode() === "terminal") {
            const match = commandPaletteStore.state.terminalResults[selectedIndex()];
            if (match) navigateToTerminalMatch(match);
          } else {
            const action = filteredActions()[selectedIndex()];
            if (action) executeAction(action);
          }
          break;
        case "Escape":
          e.preventDefault();
          e.stopPropagation();
          commandPaletteStore.close();
          break;
      }
    };

    document.addEventListener("keydown", handleKeydown, true);
    onCleanup(() => document.removeEventListener("keydown", handleKeydown, true));
  });

  /** Render a match line with the matched text highlighted */
  const renderMatchLine = (match: { line_text?: string; lineText?: string; match_start?: number; matchStart?: number; match_end?: number; matchEnd?: number }) => {
    const text = match.line_text ?? match.lineText ?? "";
    const start = match.match_start ?? match.matchStart ?? 0;
    const end = match.match_end ?? match.matchEnd ?? 0;
    const before = text.slice(0, start);
    const highlighted = text.slice(start, end);
    const after = text.slice(end);
    return (
      <span class={s.matchLine}>
        {before}<mark class={s.matchHighlight}>{highlighted}</mark>{after}
      </span>
    );
  };

  const placeholder = () => {
    switch (mode()) {
      case "filename": return "Search files by name...";
      case "content": return "Search file contents... (min 3 chars)";
      case "terminal": return "Search terminal output... (min 3 chars)";
      default: return "Type a command...";
    }
  };

  const hasActiveRepo = () => !!repositoriesStore.state.activeRepoPath;

  return (
    <Show when={isOpen()}>
      <div class={shared.overlayTop} onClick={() => commandPaletteStore.close()}>
        <div class={s.palette} onClick={(e) => e.stopPropagation()}>
          <div class={s.search}>
            <input
              ref={inputRef}
              type="text"
              placeholder={placeholder()}
              value={commandPaletteStore.state.query}
              onInput={(e) => commandPaletteStore.setQuery(e.currentTarget.value)}
            />
          </div>

          <div class={s.list} ref={listRef}>
            {/* Filename search mode (! prefix) */}
            <Show when={mode() === "filename"}>
              <Show when={!hasActiveRepo()}>
                <div class={s.empty}>No repository selected</div>
              </Show>
              <Show when={hasActiveRepo() && searchQuery().length < 1}>
                <div class={s.empty}>Type a filename to search</div>
              </Show>
              <Show when={hasActiveRepo() && searchQuery().length >= 1 && commandPaletteStore.state.filenameSearching && commandPaletteStore.state.filenameResults.length === 0}>
                <div class={s.empty}>Searching...</div>
              </Show>
              <Show when={hasActiveRepo() && searchQuery().length >= 1 && !commandPaletteStore.state.filenameSearching && commandPaletteStore.state.filenameResults.length === 0}>
                <div class={s.empty}>No files found</div>
              </Show>
              <For each={commandPaletteStore.state.filenameResults}>
                {(entry, idx) => (
                  <div
                    class={`${s.item} ${idx() === selectedIndex() ? s.selected : ""}`}
                    onClick={() => openFileEntry(entry)}
                    onMouseEnter={() => setSelectedIndex(idx())}
                  >
                    <FileIcon name={entry.name} isDir={entry.is_dir} class={s.entryIcon} />
                    <span class={s.itemLabel}>{entry.name}</span>
                    <span class={s.contentPath}>{entry.path}</span>
                  </div>
                )}
              </For>
            </Show>

            {/* Content search mode (? prefix) */}
            <Show when={mode() === "content"}>
              <Show when={!hasActiveRepo()}>
                <div class={s.empty}>No repository selected</div>
              </Show>
              <Show when={hasActiveRepo() && searchQuery().length < 3}>
                <div class={s.empty}>Type at least 3 characters after ?</div>
              </Show>
              <Show when={hasActiveRepo() && searchQuery().length >= 3 && commandPaletteStore.state.contentSearching && commandPaletteStore.state.contentResults.length === 0}>
                <div class={s.empty}>Searching...</div>
              </Show>
              <Show when={hasActiveRepo() && searchQuery().length >= 3 && !commandPaletteStore.state.contentSearching && commandPaletteStore.state.contentResults.length === 0 && !commandPaletteStore.state.contentError}>
                <div class={s.empty}>No results</div>
              </Show>
              <Show when={commandPaletteStore.state.contentError}>
                <div class={s.empty}>Error: {commandPaletteStore.state.contentError}</div>
              </Show>
              <For each={commandPaletteStore.state.contentResults}>
                {(match, idx) => (
                  <div
                    class={`${s.item} ${s.contentItem} ${idx() === selectedIndex() ? s.selected : ""}`}
                    onClick={() => openContentMatch(match)}
                    onMouseEnter={() => setSelectedIndex(idx())}
                  >
                    <span class={s.contentPath}>{match.path}:{match.line_number}</span>
                    {renderMatchLine(match)}
                  </div>
                )}
              </For>
            </Show>

            {/* Terminal buffer search mode (~ prefix) */}
            <Show when={mode() === "terminal"}>
              <Show when={Object.keys(terminalsStore.state.terminals).length === 0}>
                <div class={s.empty}>No terminals open</div>
              </Show>
              <Show when={Object.keys(terminalsStore.state.terminals).length > 0 && searchQuery().length < 3}>
                <div class={s.empty}>Type at least 3 characters after ~</div>
              </Show>
              <Show when={Object.keys(terminalsStore.state.terminals).length > 0 && searchQuery().length >= 3 && commandPaletteStore.state.terminalSearching && commandPaletteStore.state.terminalResults.length === 0}>
                <div class={s.empty}>Searching...</div>
              </Show>
              <Show when={Object.keys(terminalsStore.state.terminals).length > 0 && searchQuery().length >= 3 && !commandPaletteStore.state.terminalSearching && commandPaletteStore.state.terminalResults.length === 0}>
                <div class={s.empty}>No results</div>
              </Show>
              <For each={commandPaletteStore.state.terminalResults}>
                {(match, idx) => (
                  <div
                    class={`${s.item} ${s.contentItem} ${idx() === selectedIndex() ? s.selected : ""}`}
                    onClick={() => navigateToTerminalMatch(match)}
                    onMouseEnter={() => setSelectedIndex(idx())}
                  >
                    <span class={s.contentPath}>{match.terminalName} :{match.lineIndex}</span>
                    {renderMatchLine(match)}
                  </div>
                )}
              </For>
            </Show>

            {/* Command mode */}
            <Show when={mode() === "command"}>
              <Show when={filteredActions().length === 0}>
                <div class={s.empty}>No matching commands</div>
              </Show>
              <For each={filteredActions()}>
                {(action, idx) => (
                  <div
                    class={`${s.item} ${idx() === selectedIndex() ? s.selected : ""}`}
                    onClick={() => executeAction(action)}
                    onMouseEnter={() => setSelectedIndex(idx())}
                  >
                    <span class={s.itemLabel}>{action.label}</span>
                    <span class={s.category}>{action.category}</span>
                    <Show when={action.keybinding}>
                      <kbd class={s.keybinding}>{action.keybinding}</kbd>
                    </Show>
                  </div>
                )}
              </For>
            </Show>
          </div>

          <div class={s.footer}>
            <span class={s.footerHint}><kbd>↑↓</kbd> navigate</span>
            <span class={s.footerHint}><kbd>↵</kbd> {mode() === "command" ? "execute" : "open"}</span>
            <span class={s.footerHint}><kbd>esc</kbd> close</span>
            <span class={s.footerHint}><kbd>!</kbd> files</span>
            <span class={s.footerHint}><kbd>?</kbd> content</span>
            <span class={s.footerHint}><kbd>~</kbd> terminals</span>
          </div>
        </div>
      </div>
    </Show>
  );
};
