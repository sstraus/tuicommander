import { Component, For, Show, createSignal, createMemo, onMount } from "solid-js";
import { promptLibraryStore, type SavedPrompt, type SmartPlacement } from "../../../stores/promptLibrary";
import { SMART_PROMPTS_BUILTIN } from "../../../data/smartPromptsBuiltIn";
import { agentConfigsStore } from "../../../stores/agentConfigs";
import { useAgentDetection } from "../../../hooks/useAgentDetection";
import { AGENTS, type AgentType } from "../../../agents";
import { useConfirmDialog } from "../../../hooks/useConfirmDialog";
import { ConfirmDialog } from "../../ConfirmDialog";
import { KeyComboCapture } from "../../shared/KeyComboCapture";
import s from "../Settings.module.css";
import sp from "./SmartPromptsTab.module.css";

// All placements for the checkbox grid
const ALL_PLACEMENTS: SmartPlacement[] = [
  "toolbar", "git-changes", "git-branches", "pr-popover", "issue-popover", "terminal-context", "command-palette", "file-context",
];

// Categories extracted from smart prompt tags
const CATEGORIES = [
  "git", "review", "pr", "merge", "ci", "investigation", "code", "release",
] as const;

type Category = (typeof CATEGORIES)[number];

/** Built-in defaults indexed by ID for quick lookup */
const BUILTIN_BY_ID = new Map(SMART_PROMPTS_BUILTIN.map((p) => [p.id, p]));

/** Context variables available for smart prompt templates */
interface VarDef { name: string; description: string; group: string }

const CONTEXT_VARIABLES: VarDef[] = [
  // Git
  { name: "branch", description: "Current branch name", group: "Git" },
  { name: "base_branch", description: "Base branch (main/master/develop)", group: "Git" },
  { name: "diff", description: "Full working tree diff", group: "Git" },
  { name: "staged_diff", description: "Staged changes diff", group: "Git" },
  { name: "changed_files", description: "git status --short", group: "Git" },
  { name: "dirty_files_count", description: "Number of modified files", group: "Git" },
  { name: "commit_log", description: "Last 20 commits (oneline)", group: "Git" },
  { name: "last_commit", description: "Last commit hash + subject", group: "Git" },
  { name: "conflict_files", description: "Files with merge conflicts", group: "Git" },
  { name: "stash_list", description: "Stash entries", group: "Git" },
  { name: "branch_status", description: "Ahead/behind remote tracking", group: "Git" },
  { name: "remote_url", description: "Remote origin URL", group: "Git" },
  { name: "current_user", description: "Git user.name", group: "Git" },
  { name: "repo_name", description: "Repository directory name", group: "Git" },
  { name: "repo_path", description: "Full repository path", group: "Git" },
  { name: "repo_owner", description: "GitHub owner from remote URL", group: "Git" },
  { name: "repo_slug", description: "Repository name from remote URL", group: "Git" },
  // GitHub
  { name: "pr_number", description: "PR number for current branch", group: "GitHub" },
  { name: "pr_title", description: "PR title", group: "GitHub" },
  { name: "pr_url", description: "PR URL", group: "GitHub" },
  { name: "pr_state", description: "open / closed / merged", group: "GitHub" },
  { name: "pr_author", description: "PR author username", group: "GitHub" },
  { name: "pr_labels", description: "PR labels (comma-separated)", group: "GitHub" },
  { name: "pr_additions", description: "Lines added in PR", group: "GitHub" },
  { name: "pr_deletions", description: "Lines deleted in PR", group: "GitHub" },
  { name: "merge_status", description: "Mergeable status", group: "GitHub" },
  { name: "review_decision", description: "Review decision", group: "GitHub" },
  { name: "pr_checks", description: "CI check summary", group: "GitHub" },
  // Terminal
  { name: "agent_type", description: "Detected agent (claude, codex...)", group: "Terminal" },
  { name: "cwd", description: "Terminal working directory", group: "Terminal" },
  // File (populated for placement="file-context" hosts)
  { name: "file_path", description: "Absolute path of selected file/folder", group: "File" },
  { name: "file_rel_path", description: "Path relative to repo root", group: "File" },
  { name: "file_name", description: "Basename (foo.ts)", group: "File" },
  { name: "file_ext", description: "Extension including dot (.ts)", group: "File" },
  { name: "file_dir", description: "Parent directory absolute path", group: "File" },
  { name: "file_is_dir", description: "'true' if a folder, else 'false'", group: "File" },
];

/** Variables whose runtime value is controlled by repository contents
 * (branch names, commit messages, PR titles, remote URLs). When they are
 * substituted into a shell-execution template, quoting is mandatory —
 * otherwise a crafted branch like `main'; rm -rf ~ ;#` escapes `sh -c`.
 * The Rust backend quotes these for us via `process_prompt_content_shell_safe`;
 * this list drives a UI warning so the author knows the risk surface. */
const REPO_CONTROLLED_VARIABLES: ReadonlySet<string> = new Set([
  "branch",
  "base_branch",
  "diff",
  "staged_diff",
  "changed_files",
  "commit_log",
  "last_commit",
  "conflict_files",
  "stash_list",
  "remote_url",
  "current_user",
  "repo_name",
  "repo_owner",
  "repo_slug",
  "pr_title",
  "pr_author",
  "pr_labels",
  "pr_url",
  "pr_state",
  "pr_checks",
  "merge_status",
  "review_decision",
  "agent_type",
  "cwd",
]);

/** Return the subset of repo-controlled variables referenced in content. */
export function repoControlledVarsInContent(content: string): string[] {
  const found = new Set<string>();
  const re = /\{([^{}]+)\}/g;
  let match: RegExpExecArray | null;
  while ((match = re.exec(content)) !== null) {
    if (REPO_CONTROLLED_VARIABLES.has(match[1])) {
      found.add(match[1]);
    }
  }
  return [...found].sort();
}

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

/** Variable insertion dropdown */
const VariableDropdown: Component<{
  onInsert: (varName: string) => void;
}> = (props) => {
  const [open, setOpen] = createSignal(false);

  // Group variables
  const groups = createMemo(() => {
    const map = new Map<string, VarDef[]>();
    for (const v of CONTEXT_VARIABLES) {
      const list = map.get(v.group) ?? [];
      list.push(v);
      map.set(v.group, list);
    }
    return map;
  });

  return (
    <div class={sp.varDropdownWrap}>
      <button
        class={sp.varDropdownBtn}
        onClick={() => setOpen(!open())}
        type="button"
      >
        Insert variable...
        <span class={sp.varChevron} classList={{ [sp.varChevronOpen]: open() }}>&#9660;</span>
      </button>
      <Show when={open()}>
        <div class={sp.varDropdownList}>
          <For each={Array.from(groups().entries())}>
            {([group, vars]) => (
              <>
                <div class={sp.varGroupLabel}>{group}</div>
                <For each={vars}>
                  {(v) => (
                    <button
                      class={sp.varItem}
                      type="button"
                      onClick={() => { props.onInsert(v.name); setOpen(false); }}
                    >
                      <span class={sp.varName}>{`{${v.name}}`}</span>
                      <span class={sp.varDesc}>{v.description}</span>
                    </button>
                  )}
                </For>
              </>
            )}
          </For>
        </div>
      </Show>
    </div>
  );
};

/** Inline editor shown when a prompt row is expanded */
const PromptEditor: Component<{
  prompt: SavedPrompt;
  onClose: () => void;
}> = (props) => {
  const isBuiltIn = () => !!props.prompt.builtIn;
  const builtInDefault = () => BUILTIN_BY_ID.get(props.prompt.id);
  const dialogs = useConfirmDialog();
  let textareaRef: HTMLTextAreaElement | undefined;

  const handleContentChange = (content: string) => {
    promptLibraryStore.updatePrompt(props.prompt.id, { content });
  };

  const handleNameChange = (name: string) => {
    if (!isBuiltIn()) {
      promptLibraryStore.updatePrompt(props.prompt.id, { name });
    }
  };

  const handleDescriptionChange = (description: string) => {
    promptLibraryStore.updatePrompt(props.prompt.id, { description });
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

  const handleInsertVariable = (varName: string) => {
    const el = textareaRef;
    if (!el) return;
    const insertion = `{${varName}}`;
    const start = el.selectionStart;
    const end = el.selectionEnd;
    const before = el.value.slice(0, start);
    const after = el.value.slice(end);
    const newContent = before + insertion + after;
    handleContentChange(newContent);
    // Restore cursor position after insertion
    requestAnimationFrame(() => {
      el.focus();
      const pos = start + insertion.length;
      el.setSelectionRange(pos, pos);
    });
  };

  const isOverridden = () => {
    const def = builtInDefault();
    return def ? promptLibraryStore.isOverridden(props.prompt.id, def.content) : false;
  };

  return (
    <div class={sp.promptExpanded}>
      {/* Name */}
      <div class={sp.editorSection}>
        <label class={sp.editorLabel}>Name *</label>
        <input
          class={sp.editorInput}
          type="text"
          value={props.prompt.name}
          disabled={isBuiltIn()}
          onInput={(e) => handleNameChange(e.currentTarget.value)}
        />
      </div>

      {/* Description */}
      <div class={sp.editorSection}>
        <label class={sp.editorLabel}>Description</label>
        <input
          class={sp.editorInput}
          type="text"
          value={props.prompt.description ?? ""}
          onInput={(e) => handleDescriptionChange(e.currentTarget.value)}
          placeholder="Short description of what this prompt does"
        />
      </div>

      {/* Content + Variable dropdown */}
      <div class={sp.editorSection}>
        <label class={sp.editorLabel}>Content *</label>
        <textarea
          class={sp.editorTextarea}
          value={props.prompt.content}
          onInput={(e) => handleContentChange(e.currentTarget.value)}
          ref={textareaRef}
        />
        <VariableDropdown onInsert={handleInsertVariable} />
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

      {/* Execution Mode + Auto-execute row */}
      <div class={sp.editorRow}>
        <div class={sp.editorSection} style={{ flex: "1" }}>
          <label class={sp.editorLabel}>Execution Mode</label>
          <select
            class={sp.editorInput}
            value={props.prompt.executionMode ?? "inject"}
            onChange={(e) => {
              const mode = e.currentTarget.value as "inject" | "headless" | "api" | "shell";
              const update: Partial<SavedPrompt> = { executionMode: mode };
              if (mode === "inject") { update.outputTarget = undefined; update.systemPrompt = undefined; }
              if (mode === "shell") { update.systemPrompt = undefined; }
              promptLibraryStore.updatePrompt(props.prompt.id, update);
            }}
          >
            <option value="inject">Inject into terminal</option>
            <option value="shell">Shell script (direct run)</option>
            <option value="headless">Headless (one-shot CLI)</option>
            <option value="api">API (LLM direct)</option>
          </select>
        </div>

        <Show when={(props.prompt.executionMode ?? "inject") === "inject"}>
          <div class={sp.editorSection} style={{ flex: "1" }}>
            <label class={sp.editorLabel}>Auto-execute</label>
            <label class={sp.autoExecLabel}>
              <input
                type="checkbox"
                checked={props.prompt.autoExecute ?? false}
                onChange={handleAutoExecuteToggle}
              />
              <span>Send immediately</span>
            </label>
            <p class={sp.fieldHint}>Uncheck to review before sending</p>
          </div>
        </Show>
      </div>

      {/* Shell-mode warning for repo-controlled variable substitution.
          Even though values are shell-quoted in the backend, authors should
          know which values come from the repo so they can design the
          template accordingly. */}
      <Show
        when={
          props.prompt.executionMode === "shell" &&
          repoControlledVarsInContent(props.prompt.content).length > 0
        }
      >
        <p class={sp.fieldHint} style={{ color: "var(--color-warning, #c07a00)" }}>
          ⚠ Shell mode substitutes repo-controlled variables (
          {repoControlledVarsInContent(props.prompt.content).join(", ")}). Values
          are shell-quoted automatically, but a template that uses them outside
          argument position (e.g. `eval {"{branch}"}`) can still be abused by
          crafted branch names / PR titles.
        </p>
      </Show>

      {/* Output Target (headless and api modes) */}
      <Show when={(props.prompt.executionMode ?? "inject") !== "inject"}>
        <div class={sp.editorSection}>
          <label class={sp.editorLabel}>Output Target</label>
          <select
            class={sp.editorInput}
            value={props.prompt.outputTarget ?? ""}
            onChange={(e) => {
              const val = e.currentTarget.value || undefined;
              promptLibraryStore.updatePrompt(props.prompt.id, {
                outputTarget: val as SavedPrompt["outputTarget"],
              });
            }}
          >
            <option value="">None (return in result)</option>
            <option value="commit-message">Commit message</option>
            <option value="clipboard">Clipboard</option>
            <option value="toast">Notification</option>
            <option value="panel">Panel</option>
          </select>
        </div>
      </Show>

      {/* System Prompt (api mode only) */}
      <Show when={props.prompt.executionMode === "api"}>
        <div class={sp.editorSection}>
          <label class={sp.editorLabel}>System Prompt</label>
          <textarea
            class={sp.editorTextarea}
            rows={3}
            value={props.prompt.systemPrompt ?? ""}
            placeholder="Instructions for the LLM (e.g. 'You are a Git expert. Output only the requested content.')"
            onInput={(e) => {
              promptLibraryStore.updatePrompt(props.prompt.id, {
                systemPrompt: e.currentTarget.value || undefined,
              });
            }}
          />
          <p class={sp.fieldHint}>Sent as the system message to the LLM provider</p>
        </div>
      </Show>

      {/* Shortcut */}
      <div class={sp.editorSection}>
        <label class={sp.editorLabel}>Keyboard Shortcut</label>
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
  const detection = useAgentDetection();
  const groups = createMemo(() => groupByCategory(getAllSmartPrompts()));

  onMount(() => {
    detection.detectAll();
  });

  /** Agents that have a headless template and are installed on this machine */
  const headlessAgents = createMemo(() =>
    detection.getAvailable()
      .filter((a) => a.type !== "git" && AGENTS[a.type]?.defaultHeadlessTemplate)
      .map((a) => a.type),
  );

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

      <div class={s.group}>
        <label>Headless Agent</label>
        <select
          value={agentConfigsStore.getHeadlessAgent() ?? ""}
          onChange={(e) => {
            const val = e.currentTarget.value;
            agentConfigsStore.setHeadlessAgent(val ? val as AgentType : null);
          }}
        >
          <option value="">— Not configured —</option>
          <For each={headlessAgents()}>
            {(type) => {
              const configs = () => agentConfigsStore.getRunConfigs(type);
              return (
                <>
                  <Show
                    when={configs().length > 0}
                    fallback={<option value={type}>{AGENTS[type]?.name ?? type}</option>}
                  >
                    <optgroup label={AGENTS[type]?.name ?? type}>
                      <option value={type}>{AGENTS[type]?.name ?? type} (default)</option>
                      <For each={configs()}>
                        {(cfg) => (
                          <option value={`${type}:${cfg.name}`}>
                            {cfg.name}
                            {cfg.is_default ? " (default)" : ""}
                          </option>
                        )}
                      </For>
                    </optgroup>
                  </Show>
                </>
              );
            }}
          </For>
          <option value="api">External API</option>
        </select>
        <p class={s.hint}>
          Agent CLI used for headless prompts (e.g. generate commit message) when no agent is running in the active terminal.
          {detection.loading() ? " Detecting..." : ""}
        </p>
      </div>

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
