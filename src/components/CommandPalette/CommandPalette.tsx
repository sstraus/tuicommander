import { Component, For, Show, createEffect, createMemo, createSignal, onCleanup } from "solid-js";
import { commandPaletteStore } from "../../stores/commandPalette";
import type { ActionEntry } from "../../actions/actionRegistry";
import s from "./CommandPalette.module.css";

export interface CommandPaletteProps {
  actions: ActionEntry[];
}

export const CommandPalette: Component<CommandPaletteProps> = (props) => {
  const [selectedIndex, setSelectedIndex] = createSignal(0);
  let inputRef: HTMLInputElement | undefined;
  let listRef: HTMLDivElement | undefined;

  const isOpen = () => commandPaletteStore.state.isOpen;

  /** Filter and sort actions: recent first, then alphabetical. Substring match on label + category. */
  const filteredActions = createMemo(() => {
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

  // Reset selection when query changes
  createEffect(() => {
    commandPaletteStore.state.query;
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

  // Keyboard navigation
  createEffect(() => {
    if (!isOpen()) return;

    const handleKeydown = (e: KeyboardEvent) => {
      const items = filteredActions();

      switch (e.key) {
        case "ArrowDown":
          e.preventDefault();
          e.stopPropagation();
          setSelectedIndex((i) => Math.min(i + 1, items.length - 1));
          break;
        case "ArrowUp":
          e.preventDefault();
          e.stopPropagation();
          setSelectedIndex((i) => Math.max(i - 1, 0));
          break;
        case "Enter":
          e.preventDefault();
          e.stopPropagation();
          if (items[selectedIndex()]) {
            executeAction(items[selectedIndex()]);
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

  const executeAction = (action: ActionEntry) => {
    commandPaletteStore.recordUsage(action.id);
    commandPaletteStore.close();
    action.execute();
  };

  return (
    <Show when={isOpen()}>
      <div class={s.overlay} onClick={() => commandPaletteStore.close()}>
        <div class={s.palette} onClick={(e) => e.stopPropagation()}>
          <div class={s.search}>
            <input
              ref={inputRef}
              type="text"
              placeholder="Type a command..."
              value={commandPaletteStore.state.query}
              onInput={(e) => commandPaletteStore.setQuery(e.currentTarget.value)}
            />
          </div>

          <div class={s.list} ref={listRef}>
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
          </div>

          <div class={s.footer}>
            <span class={s.footerHint}><kbd>↑↓</kbd> navigate</span>
            <span class={s.footerHint}><kbd>↵</kbd> execute</span>
            <span class={s.footerHint}><kbd>esc</kbd> close</span>
          </div>
        </div>
      </div>
    </Show>
  );
};
