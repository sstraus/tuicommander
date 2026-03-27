import { Component, For, Show, createSignal } from "solid-js";
import { promptLibraryStore, type SavedPrompt, type SmartPlacement } from "../../../stores/promptLibrary";
import { SMART_PROMPTS_BUILTIN } from "../../../data/smartPromptsBuiltIn";
import { useConfirmDialog } from "../../../hooks/useConfirmDialog";
import { ConfirmDialog } from "../../ConfirmDialog";
import { KeyComboCapture } from "../../shared/KeyComboCapture";
import s from "../Settings.module.css";
import sp from "./SmartPromptsTab.module.css";

// All placements for the checkbox grid
const ALL_PLACEMENTS: SmartPlacement[] = [
  "toolbar", "git-changes", "git-branches", "pr-popover", "tab-context", "command-palette",
];

// Categories extracted from smart prompt tags
const CATEGORIES = [
  "git", "review", "pr", "merge", "ci", "investigation", "code", "release",
] as const;

type Category = (typeof CATEGORIES)[number];

/** Built-in defaults indexed by ID for quick lookup */
const BUILTIN_BY_ID = new Map(SMART_PROMPTS_BUILTIN.map((p) => [p.id, p]));

/** Derive the category from a prompt's tags */
function promptCategory(prompt: SavedPrompt): Category | "other" {
  const tags = prompt.tags ?? [];
  for (const cat of CATEGORIES) {
    if (tags.includes(cat)) return cat;
  }
  return "other";
}

/** Collect all smart prompts (enabled + disabled), merging store state with built-in defaults */
function getAllSmartPrompts(): SavedPrompt[] {
  const all = promptLibraryStore.getAllPrompts().filter((p) => p.tags?.includes("smart"));
  // Include built-in defaults that aren't in the store yet
  const storeIds = new Set(all.map((p) => p.id));
  for (const builtin of SMART_PROMPTS_BUILTIN) {
    if (!storeIds.has(builtin.id)) {
      all.push(builtin);
    }
  }
  return all.sort((a, b) => a.name.localeCompare(b.name));
}

/** Group prompts by category */
function groupByCategory(prompts: SavedPrompt[]): Map<string, SavedPrompt[]> {
  const groups = new Map<string, SavedPrompt[]>();
  for (const p of prompts) {
    const cat = promptCategory(p);
    const list = groups.get(cat) ?? [];
    list.push(p);
    groups.set(cat, list);
  }
  return groups;
}

// ---------------------------------------------------------------------------
// Sub-components
// ---------------------------------------------------------------------------

/** Inline editor shown when a prompt row is expanded */
const PromptEditor: Component<{
  prompt: SavedPrompt;
  onClose: () => void;
}> = (props) => {
  const isBuiltIn = () => !!props.prompt.builtIn;
  const builtInDefault = () => BUILTIN_BY_ID.get(props.prompt.id);
  const dialogs = useConfirmDialog();

  const handleContentChange = (content: string) => {
    promptLibraryStore.updatePrompt(props.prompt.id, { content });
  };

  const handleNameChange = (name: string) => {
    if (!isBuiltIn()) {
      promptLibraryStore.updatePrompt(props.prompt.id, { name });
    }
  };

  const handlePlacementToggle = (placement: SmartPlacement) => {
    const current = props.prompt.placement ?? [];
    const next = current.includes(placement)
      ? current.filter((p) => p !== placement)
      : [...current, placement];
    promptLibraryStore.updatePrompt(props.prompt.id, { placement: next });
  };

  const handleAutoExecuteToggle = () => {
    promptLibraryStore.updatePrompt(props.prompt.id, { autoExecute: !props.prompt.autoExecute });
  };

  const handleShortcutChange = (combo: string) => {
    promptLibraryStore.updatePrompt(props.prompt.id, { shortcut: combo });
  };

  const handleReset = () => {
    const def = builtInDefault();
    if (def) {
      promptLibraryStore.resetToDefault(props.prompt.id, def);
    }
  };

  const handleDelete = async () => {
    const confirmed = await dialogs.confirm({
      title: "Delete smart prompt?",
      message: `Delete "${props.prompt.name}"? This cannot be undone.`,
      okLabel: "Delete",
      kind: "warning",
    });
    if (confirmed) {
      promptLibraryStore.deletePrompt(props.prompt.id);
      props.onClose();
    }
  };

  const isOverridden = () => {
    const def = builtInDefault();
    return def ? promptLibraryStore.isOverridden(props.prompt.id, def.content) : false;
  };

  return (
    <div class={sp.promptExpanded}>
      {/* Name */}
      <div class={sp.editorSection}>
        <label class={sp.editorLabel}>Name</label>
        <input
          class={sp.editorInput}
          type="text"
          value={props.prompt.name}
          disabled={isBuiltIn()}
          onInput={(e) => handleNameChange(e.currentTarget.value)}
        />
      </div>

      {/* Content */}
      <div class={sp.editorSection}>
        <label class={sp.editorLabel}>Content</label>
        <textarea
          class={sp.editorTextarea}
          value={props.prompt.content}
          onInput={(e) => handleContentChange(e.currentTarget.value)}
        />
      </div>

      {/* Placement */}
      <div class={sp.editorSection}>
        <label class={sp.editorLabel}>Placement</label>
        <div class={sp.placementGrid}>
          <For each={ALL_PLACEMENTS}>
            {(placement) => (
              <label class={sp.placementCheck}>
                <input
                  type="checkbox"
                  checked={props.prompt.placement?.includes(placement) ?? false}
                  onChange={() => handlePlacementToggle(placement)}
                />
                <span>{placement}</span>
              </label>
            )}
          </For>
        </div>
      </div>

      {/* Auto-execute toggle */}
      <div class={sp.editorSection}>
        <div class={s.toggle}>
          <input
            type="checkbox"
            checked={props.prompt.autoExecute ?? false}
            onChange={handleAutoExecuteToggle}
          />
          <span>Auto-execute</span>
        </div>
        <p class={s.hint}>Run immediately without confirmation when triggered</p>
      </div>

      {/* Shortcut */}
      <div class={sp.editorSection}>
        <label class={sp.editorLabel}>Shortcut</label>
        <KeyComboCapture
          value={props.prompt.shortcut ?? ""}
          onChange={handleShortcutChange}
          placeholder="Click to set shortcut"
        />
      </div>

      {/* Actions */}
      <div class={sp.editorActions}>
        <Show when={isBuiltIn() && isOverridden()}>
          <button class={sp.smallBtn} onClick={handleReset}>
            Reset to Default
          </button>
        </Show>
        <Show when={!isBuiltIn()}>
          <button class={sp.dangerBtn} onClick={handleDelete}>
            Delete
          </button>
        </Show>
      </div>

      <ConfirmDialog
        visible={dialogs.dialogState() !== null}
        title={dialogs.dialogState()?.title ?? ""}
        message={dialogs.dialogState()?.message ?? ""}
        confirmLabel={dialogs.dialogState()?.confirmLabel}
        cancelLabel={dialogs.dialogState()?.cancelLabel}
        kind={dialogs.dialogState()?.kind}
        onClose={dialogs.handleClose}
        onConfirm={dialogs.handleConfirm}
      />
    </div>
  );
};

/** Single prompt row with toggle, badges, and click-to-expand */
const PromptRow: Component<{ prompt: SavedPrompt }> = (props) => {
  const [expanded, setExpanded] = createSignal(false);

  const isEnabled = () => props.prompt.enabled !== false;
  const mode = () => props.prompt.executionMode ?? "inject";
  const builtInDefault = () => BUILTIN_BY_ID.get(props.prompt.id);
  const hasUpdate = () => {
    const def = builtInDefault();
    return def ? promptLibraryStore.hasUpdate(props.prompt.id, def.builtInVersion ?? 1) : false;
  };

  const handleToggle = (e: Event) => {
    e.stopPropagation();
    promptLibraryStore.updatePrompt(props.prompt.id, { enabled: !isEnabled() });
  };

  return (
    <div class={sp.promptRow}>
      <div class={sp.promptHeader} onClick={() => setExpanded(!expanded())}>
        {/* Enable/disable toggle */}
        <label class={s.toggle} onClick={(e) => e.stopPropagation()}>
          <input
            type="checkbox"
            checked={isEnabled()}
            onChange={handleToggle}
          />
        </label>

        {/* Icon placeholder */}
        <span class={sp.promptIcon}>{props.prompt.icon ?? ""}</span>

        {/* Name */}
        <span class={sp.promptName} style={{ opacity: isEnabled() ? 1 : 0.5 }}>
          {props.prompt.name}
        </span>

        {/* Badges */}
        <div class={sp.promptMeta}>
          <span class={sp.badge} data-type={mode()}>{mode()}</span>
          <For each={props.prompt.placement ?? []}>
            {(p) => <span class={sp.badge} data-type="placement">{p}</span>}
          </For>
          <Show when={props.prompt.builtIn}>
            <span class={sp.badge} data-type="builtin">Built-in</span>
          </Show>
          <Show when={hasUpdate()}>
            <span class={sp.badge} data-type="update">Updated</span>
          </Show>
        </div>
      </div>

      <Show when={expanded()}>
        <PromptEditor prompt={props.prompt} onClose={() => setExpanded(false)} />
      </Show>
    </div>
  );
};

/** Collapsible category group */
const CategoryGroup: Component<{ category: string; prompts: SavedPrompt[] }> = (props) => {
  const [open, setOpen] = createSignal(true);

  return (
    <>
      <div class={sp.categoryHeader} onClick={() => setOpen(!open())}>
        <span class={sp.categoryChevron} classList={{ [sp.open]: open() }}>&#9654;</span>
        <span>{props.category}</span>
        <span class={sp.categoryCount}>{props.prompts.length}</span>
      </div>
      <Show when={open()}>
        <For each={props.prompts}>
          {(prompt) => <PromptRow prompt={prompt} />}
        </For>
      </Show>
    </>
  );
};

// ---------------------------------------------------------------------------
// Main tab
// ---------------------------------------------------------------------------

export const SmartPromptsTab: Component = () => {
  const groups = () => groupByCategory(getAllSmartPrompts());

  // Ordered categories, with "other" at the end if present
  const orderedCategories = () => {
    const g = groups();
    const ordered: string[] = [];
    for (const cat of CATEGORIES) {
      if (g.has(cat)) ordered.push(cat);
    }
    if (g.has("other")) ordered.push("other");
    return ordered;
  };

  const handleNewPrompt = () => {
    promptLibraryStore.createPrompt({
      name: "New Smart Prompt",
      content: "",
      category: "custom",
      isFavorite: false,
      tags: ["smart"],
      enabled: true,
      placement: ["command-palette"],
      executionMode: "inject",
      autoExecute: false,
    });
  };

  return (
    <div class={s.section}>
      <h3>Smart Prompts</h3>
      <p class={s.hint} style={{ "margin-bottom": "16px" }}>
        AI-powered actions that can be triggered from the toolbar, context menus, and command palette.
        Enable, disable, or customize the prompt content and placement for each action.
      </p>

      <div class={sp.promptList}>
        <For each={orderedCategories()}>
          {(cat) => <CategoryGroup category={cat} prompts={groups().get(cat)!} />}
        </For>
      </div>

      <button class={sp.addBtn} onClick={handleNewPrompt}>
        + New Smart Prompt
      </button>
    </div>
  );
};
