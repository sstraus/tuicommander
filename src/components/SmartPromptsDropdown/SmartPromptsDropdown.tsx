import { Component, Show, For, createSignal, createMemo, createEffect, onCleanup } from "solid-js";
import { promptLibraryStore, type SavedPrompt } from "../../stores/promptLibrary";
import { smartPromptsDropdownStore } from "../../stores/smartPromptsDropdown";
import { terminalsStore } from "../../stores/terminals";
import { useSmartPrompts } from "../../hooks/useSmartPrompts";
import { appLogger } from "../../stores/appLogger";
import { VariableInputDialog } from "./VariableInputDialog";
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

/** Detect WHY prompts are disabled — returns a user-friendly status message or null if all good */
function getDisabledReason(): string | null {
  const active = terminalsStore.getActive();
  if (!active?.sessionId) return "No active terminal — open a terminal first";
  if (!active.agentType) return "No AI agent detected in the active terminal";
  if (terminalsStore.isBusy(active.id)) return "Agent is busy — wait for it to finish";
  return null;
}

export const SmartPromptsDropdown: Component<SmartPromptsDropdownProps> = (props) => {
  const open = () => smartPromptsDropdownStore.state.isOpen;
  const [search, setSearch] = createSignal("");
  const [variablePrompt, setVariablePrompt] = createSignal<{ prompt: SavedPrompt; unresolved: string[] } | null>(null);
  const smartPrompts = useSmartPrompts();
  let wrapperRef: HTMLDivElement | undefined;
  let searchRef: HTMLInputElement | undefined;

  /** Get toolbar-placed prompts, filtered by search */
  const filteredPrompts = createMemo(() => {
    const all = promptLibraryStore.getSmartByPlacement("toolbar");
    const query = search().toLowerCase().trim();
    if (!query) return all;
    return all.filter((p) =>
      p.name.toLowerCase().includes(query) ||
      p.description?.toLowerCase().includes(query),
    );
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

  const disabledReason = createMemo(() => getDisabledReason());

  const close = () => {
    smartPromptsDropdownStore.close();
    setSearch("");
    setVariablePrompt(null);
    // Restore keyboard focus to the active terminal — without this,
    // focus falls to document.body and typing stops reaching xterm.
    requestAnimationFrame(() => terminalsStore.getActive()?.ref?.focus());
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
      if (e.key === "Escape") {
        if (variablePrompt()) {
          setVariablePrompt(null);
        } else {
          close();
        }
      }
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
    if (result.reason === "unresolved_variables" && result.output) {
      const unresolved = JSON.parse(result.output) as string[];
      setVariablePrompt({ prompt, unresolved });
      return;
    }
    if (result.ok) {
      close();
    } else {
      appLogger.error("prompts", `Failed to execute "${prompt.name}": ${result.reason}`);
    }
  };

  const handleVariableSubmit = async (values: Record<string, string>) => {
    const vp = variablePrompt();
    if (!vp) return;
    setVariablePrompt(null);

    const result = await smartPrompts.executeSmartPrompt(vp.prompt, values);
    if (result.ok) {
      close();
    } else {
      appLogger.error("prompts", `Failed to execute "${vp.prompt.name}": ${result.reason}`);
    }
  };

  return (
    <div class={s.wrapper} ref={wrapperRef}>
      <button
        class={cx(s.trigger, open() && s.triggerActive)}
        onClick={toggle}
        title="Smart Prompts Library"
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

          <Show when={disabledReason()}>
            <div class={s.statusBanner}>
              <svg viewBox="0 0 16 16" width="12" height="12" fill="currentColor" class={s.statusIcon}>
                <path d="M8 1.5a6.5 6.5 0 1 0 0 13 6.5 6.5 0 0 0 0-13zM0 8a8 8 0 1 1 16 0A8 8 0 0 1 0 8zm9 3a1 1 0 1 1-2 0 1 1 0 0 1 2 0zm-.25-6.25a.75.75 0 0 0-1.5 0v3.5a.75.75 0 0 0 1.5 0v-3.5z" />
              </svg>
              {disabledReason()}
            </div>
          </Show>

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
                            title={!enabled() ? check().reason : prompt.description ?? ""}
                            onClick={() => enabled() && handleItemClick(prompt)}
                          >
                            <div class={s.itemContent}>
                              <span class={s.itemName}>{prompt.name}</span>
                              <Show when={prompt.description}>
                                <span class={s.itemDesc}>{prompt.description}</span>
                              </Show>
                            </div>
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

      <Show when={variablePrompt()}>
        {(vp) => (
          <VariableInputDialog
            variables={vp().unresolved}
            promptName={vp().prompt.name}
            onSubmit={handleVariableSubmit}
            onCancel={() => setVariablePrompt(null)}
          />
        )}
      </Show>
    </div>
  );
};
