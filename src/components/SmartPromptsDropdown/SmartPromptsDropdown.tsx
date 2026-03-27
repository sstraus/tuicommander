import { Component, Show, For, createSignal, createMemo, createEffect, onCleanup } from "solid-js";
import { promptLibraryStore, type SavedPrompt } from "../../stores/promptLibrary";
import { smartPromptsDropdownStore } from "../../stores/smartPromptsDropdown";
import { useSmartPrompts } from "../../hooks/useSmartPrompts";
import { appLogger } from "../../stores/appLogger";
import { cx } from "../../utils";
import s from "./SmartPromptsDropdown.module.css";

/** Category display order and labels */
const CATEGORY_ORDER: Record<string, { label: string; order: number }> = {
  git: { label: "GIT", order: 0 },
  review: { label: "REVIEW", order: 1 },
  pr: { label: "PULL REQUESTS", order: 2 },
  merge: { label: "MERGE", order: 3 },
  ci: { label: "CI & QUALITY", order: 4 },
  investigation: { label: "INVESTIGATION", order: 5 },
  code: { label: "CODE", order: 6 },
};

export interface SmartPromptsDropdownProps {
  repoPath?: string;
  onOpenSettings?: () => void;
}

export const SmartPromptsDropdown: Component<SmartPromptsDropdownProps> = (props) => {
  const open = () => smartPromptsDropdownStore.state.isOpen;
  const [search, setSearch] = createSignal("");
  const smartPrompts = useSmartPrompts();
  let wrapperRef: HTMLDivElement | undefined;
  let searchRef: HTMLInputElement | undefined;

  /** Get toolbar-placed prompts, filtered by search */
  const filteredPrompts = createMemo(() => {
    const all = promptLibraryStore.getSmartByPlacement("toolbar");
    const query = search().toLowerCase().trim();
    if (!query) return all;
    return all.filter((p) => p.name.toLowerCase().includes(query));
  });

  /** Group prompts by their first non-"smart" tag */
  const groupedPrompts = createMemo(() => {
    const groups: { tag: string; label: string; order: number; prompts: SavedPrompt[] }[] = [];
    const groupMap = new Map<string, SavedPrompt[]>();

    for (const prompt of filteredPrompts()) {
      const tag = prompt.tags?.find((t) => t !== "smart") ?? "other";
      let list = groupMap.get(tag);
      if (!list) {
        list = [];
        groupMap.set(tag, list);
      }
      list.push(prompt);
    }

    for (const [tag, prompts] of groupMap) {
      const meta = CATEGORY_ORDER[tag] ?? { label: tag.toUpperCase(), order: 99 };
      groups.push({ tag, label: meta.label, order: meta.order, prompts });
    }

    groups.sort((a, b) => a.order - b.order);
    return groups;
  });

  const close = () => {
    smartPromptsDropdownStore.close();
    setSearch("");
  };

  const toggle = () => {
    if (open()) {
      close();
    } else {
      smartPromptsDropdownStore.open();
    }
  };

  /** Focus search input when dropdown opens */
  createEffect(() => {
    if (open() && searchRef) {
      requestAnimationFrame(() => searchRef?.focus());
    }
  });

  /** Close on Escape */
  createEffect(() => {
    if (!open()) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") close();
    };
    document.addEventListener("keydown", handler);
    onCleanup(() => document.removeEventListener("keydown", handler));
  });

  /** Close on outside click */
  createEffect(() => {
    if (!open()) return;
    const handler = (e: MouseEvent) => {
      if (wrapperRef && !wrapperRef.contains(e.target as Node)) {
        close();
      }
    };
    document.addEventListener("mousedown", handler);
    onCleanup(() => document.removeEventListener("mousedown", handler));
  });

  const handleItemClick = async (prompt: SavedPrompt) => {
    const check = smartPrompts.canExecute(prompt);
    if (!check.ok) return;

    const result = await smartPrompts.executeSmartPrompt(prompt);
    if (result.reason === "unresolved_variables") {
      appLogger.warn("prompts", `Prompt "${prompt.name}" has unresolved variables: ${result.output}`);
      // Full variable input dialog will be wired by a later story
      return;
    }
    if (result.ok) {
      close();
    } else {
      appLogger.error("prompts", `Failed to execute "${prompt.name}": ${result.reason}`);
    }
  };

  return (
    <div class={s.wrapper} ref={wrapperRef}>
      <button
        class={cx(s.trigger, open() && s.triggerActive)}
        onClick={toggle}
        title="Smart Prompts"
      >
        <svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor">
          <path d="M9.504.43a1.516 1.516 0 0 1 2.437 1.713L10.415 5.5h2.123c1.57 0 2.346 1.909 1.22 3.004l-7.34 7.142a1.249 1.249 0 0 1-1.847-.041 1.249 1.249 0 0 1-.137-1.363L5.96 10.5H3.462c-1.57 0-2.346-1.909-1.22-3.004L9.504.43z" />
        </svg>
      </button>

      <Show when={open()}>
        <div class={s.overlay} onClick={close} />
        <div class={s.dropdown}>
          <div class={s.searchWrapper}>
            <input
              ref={searchRef}
              class={s.searchInput}
              type="text"
              placeholder="Search prompts..."
              value={search()}
              onInput={(e) => setSearch(e.currentTarget.value)}
            />
          </div>

          <div class={s.list}>
            <Show
              when={groupedPrompts().length > 0}
              fallback={<div class={s.empty}>No matching prompts</div>}
            >
              <For each={groupedPrompts()}>
                {(group) => (
                  <>
                    <div class={s.categoryHeader}>{group.label}</div>
                    <For each={group.prompts}>
                      {(prompt) => {
                        const check = () => smartPrompts.canExecute(prompt);
                        const enabled = () => check().ok;
                        return (
                          <div
                            class={cx(s.item, !enabled() && s.itemDisabled)}
                            title={!enabled() ? check().reason : prompt.description ?? prompt.name}
                            onClick={() => enabled() && handleItemClick(prompt)}
                          >
                            <span class={s.itemName}>{prompt.name}</span>
                            <Show when={prompt.shortcut}>
                              <span class={s.itemShortcut}>{prompt.shortcut}</span>
                            </Show>
                          </div>
                        );
                      }}
                    </For>
                  </>
                )}
              </For>
            </Show>
          </div>

          <Show when={props.onOpenSettings}>
            <div class={s.footer}>
              <button
                class={s.manageLink}
                onClick={() => {
                  close();
                  props.onOpenSettings?.();
                }}
              >
                Manage Smart Prompts...
              </button>
            </div>
          </Show>
        </div>
      </Show>
    </div>
  );
};
