import { Component, For, Show, createEffect, createSignal, onCleanup } from "solid-js";
import { promptLibraryStore, type SavedPrompt, type PromptCategory, type SmartPlacement } from "../../stores/promptLibrary";
import { SMART_PROMPTS_BUILTIN, VARIABLE_DESCRIPTIONS } from "../../data/smartPromptsBuiltIn";
import { terminalsStore } from "../../stores/terminals";
import { usePty } from "../../hooks/usePty";
import { appLogger } from "../../stores/appLogger";
import { t } from "../../i18n";
import { cx } from "../../utils";
import { KeyComboCapture } from "../shared/KeyComboCapture";
import { useConfirmDialog } from "../../hooks/useConfirmDialog";
import { ConfirmDialog } from "../ConfirmDialog";
import s from "./PromptDrawer.module.css";

export interface PromptDrawerProps {
  onClose?: () => void;
}

/** Category labels */
const CATEGORY_LABELS: Record<PromptCategory | "all", string> = {
  all: "All",
  custom: "Custom",
  recent: "Recent",
  favorite: "Favorites",
};

const ALL_PLACEMENTS: SmartPlacement[] = [
  "toolbar", "git-changes", "git-branches", "pr-popover", "terminal-context", "command-palette",
];

/** Built-in defaults indexed by ID for quick lookup */
const BUILTIN_BY_ID = new Map(SMART_PROMPTS_BUILTIN.map((p) => [p.id, p]));

export const PromptDrawer: Component<PromptDrawerProps> = (props) => {
  const [selectedIndex, setSelectedIndex] = createSignal(0);
  const [showEditor, setShowEditor] = createSignal(false);
  const [editingPrompt, setEditingPrompt] = createSignal<SavedPrompt | null>(null);
  const [variableValues, setVariableValues] = createSignal<Record<string, string>>({});
  const [showVariableDialog, setShowVariableDialog] = createSignal(false);
  const [pendingPrompt, setPendingPrompt] = createSignal<SavedPrompt | null>(null);
  const dialogs = useConfirmDialog();

  const pty = usePty();

  const filteredPrompts = () => promptLibraryStore.getFilteredPrompts();
  const isOpen = () => promptLibraryStore.state.drawerOpen;

  // Reset selection when prompts change
  createEffect(() => {
    filteredPrompts();
    setSelectedIndex(0);
  });

  // Keyboard navigation
  createEffect(() => {
    if (!isOpen()) return;

    const handleKeydown = (e: KeyboardEvent) => {
      if (showVariableDialog()) {
        if (e.key === "Escape") {
          setShowVariableDialog(false);
          setPendingPrompt(null);
        }
        return;
      }

      if (showEditor()) {
        if (e.key === "Escape") {
          setShowEditor(false);
          setEditingPrompt(null);
        }
        return;
      }

      const prompts = filteredPrompts();

      switch (e.key) {
        case "ArrowDown":
          e.preventDefault();
          setSelectedIndex((i) => Math.min(i + 1, prompts.length - 1));
          break;
        case "ArrowUp":
          e.preventDefault();
          setSelectedIndex((i) => Math.max(i - 1, 0));
          break;
        case "Enter":
          e.preventDefault();
          if (prompts[selectedIndex()]) {
            injectPrompt(prompts[selectedIndex()]);
          }
          break;
        case "Escape":
          e.preventDefault();
          promptLibraryStore.closeDrawer();
          props.onClose?.();
          break;
        case "n":
          if (e.ctrlKey || e.metaKey) {
            e.preventDefault();
            createNewPrompt();
          }
          break;
        case "e":
          if (e.ctrlKey || e.metaKey) {
            e.preventDefault();
            if (prompts[selectedIndex()]) {
              editPrompt(prompts[selectedIndex()]);
            }
          }
          break;
        case "f":
          if (e.ctrlKey || e.metaKey) {
            e.preventDefault();
            if (prompts[selectedIndex()]) {
              promptLibraryStore.toggleFavorite(prompts[selectedIndex()].id);
            }
          }
          break;
      }
    };

    document.addEventListener("keydown", handleKeydown);
    onCleanup(() => document.removeEventListener("keydown", handleKeydown));
  });

  /** Inject prompt into active terminal */
  const injectPrompt = async (prompt: SavedPrompt, executeImmediately: boolean = false) => {
    if (prompt.enabled === false) return;

    const variables = await promptLibraryStore.extractVariables(prompt.content);

    if (variables.length > 0) {
      setPendingPrompt(prompt);
      // Pre-populate with auto-resolved values where possible
      const autoVars: Record<string, string> = {};
      for (const v of variables) {
        const promptVar = prompt.variables?.find((pv) => pv.name === v);
        autoVars[v] = promptVar?.defaultValue || "";
      }
      setVariableValues(autoVars);
      setShowVariableDialog(true);
      return;
    }

    await doInject(prompt, {}, executeImmediately);
  };

  const doInject = async (
    prompt: SavedPrompt,
    variables: Record<string, string>,
    executeImmediately: boolean = false,
  ) => {
    const activeTerminal = terminalsStore.getActive();
    if (!activeTerminal?.sessionId) return;

    let content = await promptLibraryStore.processContent(prompt, variables);

    if (executeImmediately) {
      content += "\n";
    }

    try {
      await pty.write(activeTerminal.sessionId, content);
      promptLibraryStore.markAsUsed(prompt.id);
      promptLibraryStore.closeDrawer();
      props.onClose?.();
      requestAnimationFrame(() => terminalsStore.getActive()?.ref?.focus());
    } catch (err) {
      appLogger.error("app", "Failed to inject prompt", err);
    }
  };

  const handleVariableSubmit = (executeImmediately: boolean) => {
    const prompt = pendingPrompt();
    if (!prompt) return;

    doInject(prompt, variableValues(), executeImmediately);
    setShowVariableDialog(false);
    setPendingPrompt(null);
  };

  const createNewPrompt = () => {
    setEditingPrompt(null);
    setShowEditor(true);
  };

  const editPrompt = (prompt: SavedPrompt) => {
    setEditingPrompt(prompt);
    setShowEditor(true);
  };

  const deletePrompt = async (prompt: SavedPrompt) => {
    if (prompt.builtIn) return;
    const confirmed = await dialogs.confirm({
      title: "Delete prompt?",
      message: `Delete "${prompt.name}"?`,
      okLabel: "Delete",
      kind: "warning",
    });
    if (confirmed) {
      promptLibraryStore.deletePrompt(prompt.id);
    }
  };

  const toggleEnabled = (e: MouseEvent, prompt: SavedPrompt) => {
    e.stopPropagation();
    promptLibraryStore.updatePrompt(prompt.id, { enabled: prompt.enabled === false });
  };

  return (
    <>
    <Show when={isOpen()}>
      <div class={s.overlay} onClick={() => promptLibraryStore.closeDrawer()}>
        <div class={s.drawer} onClick={(e) => e.stopPropagation()}>
          {/* Header */}
          <div class={s.header}>
            <h3>{t("promptDrawer.title", "Smart Prompts Library")}</h3>
            <button class={s.close} onClick={() => promptLibraryStore.closeDrawer()}>
              &times;
            </button>
          </div>

          {/* Search */}
          <div class={s.search}>
            <input
              type="text"
              placeholder={t("promptDrawer.searchPlaceholder", "Search prompts... (type to filter)")}
              value={promptLibraryStore.state.searchQuery}
              onInput={(e) => promptLibraryStore.setSearchQuery(e.currentTarget.value)}
              autofocus
            />
          </div>

          {/* Categories */}
          <div class={s.categories}>
            <For each={Object.entries(CATEGORY_LABELS)}>
              {([category, label]) => (
                <button
                  class={cx(s.categoryBtn, promptLibraryStore.state.selectedCategory === category && s.categoryBtnActive)}
                  onClick={() => promptLibraryStore.setSelectedCategory(category as PromptCategory | "all")}
                >
                  {label}
                </button>
              )}
            </For>
          </div>

          {/* Prompt list */}
          <div class={s.list}>
            <Show
              when={filteredPrompts().length > 0}
              fallback={
                <div class={s.empty}>
                  <p>{t("promptDrawer.noPrompts", "No prompts found")}</p>
                  <button onClick={createNewPrompt}>{t("promptDrawer.createFirst", "Create your first prompt")}</button>
                </div>
              }
            >
              <For each={filteredPrompts()}>
                {(prompt, index) => {
                  const isDisabled = () => prompt.enabled === false;
                  return (
                    <div
                      class={cx(s.promptItem, index() === selectedIndex() && s.selected, isDisabled() && s.itemDisabled)}
                      onClick={() => injectPrompt(prompt)}
                      onDblClick={() => injectPrompt(prompt, true)}
                    >
                      <div class={s.itemContent}>
                        <div class={s.itemName}>
                          {prompt.isFavorite && <span class={s.favoriteStar}><svg viewBox="0 0 16 16" width="10" height="10" fill="currentColor"><path d="M8 .25a.75.75 0 0 1 .673.418l1.882 3.815 4.21.612a.75.75 0 0 1 .416 1.279l-3.046 2.97.719 4.192a.75.75 0 0 1-1.088.791L8 12.347l-3.766 1.98a.75.75 0 0 1-1.088-.79l.72-4.194L.818 6.374a.75.75 0 0 1 .416-1.28l4.21-.611L7.327.668A.75.75 0 0 1 8 .25z" /></svg></span>}
                          {prompt.name}
                          <span class={s.itemBadges}>
                            <span class={s.tagBadge} data-type={prompt.executionMode ?? "inject"}>{prompt.executionMode ?? "inject"}</span>
                            <Show when={prompt.builtIn}>
                              <span class={s.tagBadge} data-type="builtin">built-in</span>
                            </Show>
                            <For each={prompt.placement ?? []}>
                              {(p) => <span class={s.tagBadge} data-type="placement">{p}</span>}
                            </For>
                          </span>
                          {prompt.shortcut && <span class={s.shortcut}>{prompt.shortcut}</span>}
                        </div>
                        <Show when={prompt.description}>
                          <div class={s.itemDescription}>{prompt.description}</div>
                        </Show>
                      </div>
                      <div class={s.itemActions}>
                        <button
                          title={isDisabled() ? "Enable" : "Disable"}
                          onClick={(e) => toggleEnabled(e, prompt)}
                          class={cx(s.toggleBtn, isDisabled() && s.toggleOff)}
                        >
                          <svg viewBox="0 0 16 16" width="12" height="12" fill="currentColor">
                            {isDisabled()
                              ? <circle cx="8" cy="8" r="6" fill="none" stroke="currentColor" stroke-width="1.5" />
                              : <circle cx="8" cy="8" r="6" />}
                          </svg>
                        </button>
                        <button
                          title="Edit"
                          onClick={(e) => { e.stopPropagation(); editPrompt(prompt); }}
                        >
                          <svg viewBox="0 0 16 16" width="12" height="12" fill="currentColor"><path d="M11.013 1.427a1.75 1.75 0 0 1 2.474 0l1.086 1.086a1.75 1.75 0 0 1 0 2.474l-8.61 8.61c-.21.21-.47.364-.756.445l-3.251.93a.75.75 0 0 1-.927-.928l.929-3.25c.081-.286.235-.547.445-.758l8.61-8.61z" /></svg>
                        </button>
                        <button
                          title={prompt.isFavorite ? "Unfavorite" : "Favorite"}
                          onClick={(e) => { e.stopPropagation(); promptLibraryStore.toggleFavorite(prompt.id); }}
                        >
                          <svg viewBox="0 0 16 16" width="12" height="12" fill="currentColor">
                            {prompt.isFavorite
                              ? <path d="M8 .25a.75.75 0 0 1 .673.418l1.882 3.815 4.21.612a.75.75 0 0 1 .416 1.279l-3.046 2.97.719 4.192a.75.75 0 0 1-1.088.791L8 12.347l-3.766 1.98a.75.75 0 0 1-1.088-.79l.72-4.194L.818 6.374a.75.75 0 0 1 .416-1.28l4.21-.611L7.327.668A.75.75 0 0 1 8 .25z" />
                              : <path d="M8 .25a.75.75 0 0 1 .673.418l1.882 3.815 4.21.612a.75.75 0 0 1 .416 1.279l-3.046 2.97.719 4.192a.75.75 0 0 1-1.088.791L8 12.347l-3.766 1.98a.75.75 0 0 1-1.088-.79l.72-4.194L.818 6.374a.75.75 0 0 1 .416-1.28l4.21-.611L7.327.668A.75.75 0 0 1 8 .25zm0 2.445L6.615 5.5a.75.75 0 0 1-.564.41l-3.097.45 2.24 2.184a.75.75 0 0 1 .216.664l-.528 3.084 2.769-1.456a.75.75 0 0 1 .698 0l2.77 1.456-.53-3.084a.75.75 0 0 1 .216-.664l2.24-2.183-3.096-.45a.75.75 0 0 1-.564-.41L8 2.694z" />}
                          </svg>
                        </button>
                        <Show when={!prompt.builtIn}>
                          <button
                            title="Delete"
                            onClick={(e) => { e.stopPropagation(); deletePrompt(prompt); }}
                          >
                            <svg viewBox="0 0 16 16" width="12" height="12" fill="currentColor"><path d="M11 1.75V3h2.25a.75.75 0 0 1 0 1.5H2.75a.75.75 0 0 1 0-1.5H5V1.75C5 .784 5.784 0 6.75 0h2.5C10.216 0 11 .784 11 1.75zM6.5 1.75V3h3V1.75a.25.25 0 0 0-.25-.25h-2.5a.25.25 0 0 0-.25.25zM3.613 5.5l.806 8.87A1.75 1.75 0 0 0 6.163 16h3.674a1.75 1.75 0 0 0 1.744-1.63l.806-8.87H3.613z" /></svg>
                          </button>
                        </Show>
                      </div>
                    </div>
                  );
                }}
              </For>
            </Show>
          </div>

          {/* Footer */}
          <div class={s.footer}>
            <button onClick={createNewPrompt}>{t("promptDrawer.newPrompt", "+ New Prompt")}</button>
            <span class={s.hint}>
              {t("promptDrawer.hint", "↑↓ Navigate • Enter Insert • Ctrl+N New • Esc Close")}
            </span>
          </div>
        </div>
      </div>

      {/* Variable Dialog */}
      <Show when={showVariableDialog() && pendingPrompt()}>
        <div class={s.variableOverlay} onClick={() => setShowVariableDialog(false)}>
          <div class={s.variableDialog} onClick={(e) => e.stopPropagation()}>
            <h4>{pendingPrompt()?.name ?? "Fill in variables"}</h4>
            <For each={Object.keys(variableValues())}>
              {(varName) => (
                <div class={s.variableInput}>
                  <label class={s.varLabel}>{`{${varName}}`}</label>
                  <Show when={VARIABLE_DESCRIPTIONS[varName]}>
                    <span class={s.varDesc}>{VARIABLE_DESCRIPTIONS[varName]}</span>
                  </Show>
                  <input
                    type="text"
                    value={variableValues()[varName] || ""}
                    onInput={(e) =>
                      setVariableValues((v) => ({ ...v, [varName]: e.currentTarget.value }))
                    }
                    placeholder={VARIABLE_DESCRIPTIONS[varName] ?? varName}
                  />
                </div>
              )}
            </For>
            <div class={s.variableActions}>
              <button onClick={() => handleVariableSubmit(false)}>{t("promptDrawer.insert", "Insert")}</button>
              <button onClick={() => handleVariableSubmit(true)}>{t("promptDrawer.insertAndRun", "Insert & Run")}</button>
              <button onClick={() => setShowVariableDialog(false)}>{t("promptDrawer.cancel", "Cancel")}</button>
            </div>
          </div>
        </div>
      </Show>

      {/* Editor Dialog */}
      <Show when={showEditor()}>
        <PromptEditor
          prompt={editingPrompt()}
          onSave={(promptData) => {
            if (editingPrompt()) {
              promptLibraryStore.updatePrompt(editingPrompt()!.id, promptData);
            } else {
              if (promptData.name && promptData.content) {
                promptLibraryStore.createPrompt({
                  name: promptData.name,
                  content: promptData.content,
                  description: promptData.description,
                  shortcut: promptData.shortcut,
                  category: "custom",
                  isFavorite: false,
                  tags: promptData.tags,
                  placement: promptData.placement,
                  autoExecute: promptData.autoExecute,
                  executionMode: promptData.executionMode,
                  outputTarget: promptData.outputTarget,
                  enabled: true,
                });
              }
            }
            setShowEditor(false);
            setEditingPrompt(null);
          }}
          onCancel={() => {
            setShowEditor(false);
            setEditingPrompt(null);
          }}
        />
      </Show>
    </Show>
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
    </>
  );
};

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
];

/** Variable insertion dropdown */
const VariableDropdown: Component<{
  onInsert: (varName: string) => void;
}> = (props) => {
  const [open, setOpen] = createSignal(false);

  const groups = () => {
    const map = new Map<string, VarDef[]>();
    for (const v of CONTEXT_VARIABLES) {
      const list = map.get(v.group) ?? [];
      list.push(v);
      map.set(v.group, list);
    }
    return map;
  };

  return (
    <div class={s.varDropdownWrap}>
      <button
        class={s.varDropdownBtn}
        onClick={() => setOpen(!open())}
        type="button"
      >
        Insert variable...
        <span class={s.varChevron} classList={{ [s.varChevronOpen]: open() }}>&#9660;</span>
      </button>
      <Show when={open()}>
        <div class={s.varDropdownList}>
          <For each={Array.from(groups().entries())}>
            {([group, vars]) => (
              <>
                <div class={s.varGroupLabel}>{group}</div>
                <For each={vars}>
                  {(v) => (
                    <button
                      class={s.varItem}
                      type="button"
                      onClick={() => { props.onInsert(v.name); setOpen(false); }}
                    >
                      <span class={s.varItemName}>{`{${v.name}}`}</span>
                      <span class={s.varItemDesc}>{v.description}</span>
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

/** Full-featured prompt editor dialog */
interface PromptEditorProps {
  prompt: SavedPrompt | null;
  onSave: (data: Partial<SavedPrompt>) => void;
  onCancel: () => void;
}

const PromptEditor: Component<PromptEditorProps> = (props) => {
  const isBuiltIn = () => !!props.prompt?.builtIn;
  const builtInDefault = () => props.prompt ? BUILTIN_BY_ID.get(props.prompt.id) : undefined;
  let textareaRef: HTMLTextAreaElement | undefined;

  const [name, setName] = createSignal(props.prompt?.name || "");
  const [description, setDescription] = createSignal(props.prompt?.description || "");
  const [content, setContent] = createSignal(props.prompt?.content || "");
  const [shortcut, setShortcut] = createSignal(props.prompt?.shortcut || "");
  const [placement, setPlacement] = createSignal<SmartPlacement[]>(props.prompt?.placement ?? []);
  const [autoExecute, setAutoExecute] = createSignal(props.prompt?.autoExecute ?? false);
  const [executionMode, setExecutionMode] = createSignal<"inject" | "headless" | "api">(props.prompt?.executionMode ?? "inject");
  const [outputTarget, setOutputTarget] = createSignal<SavedPrompt["outputTarget"]>(props.prompt?.outputTarget);
  const [systemPrompt, setSystemPrompt] = createSignal(props.prompt?.systemPrompt ?? "");
  const [validationError, setValidationError] = createSignal<string | null>(null);

  const isOverridden = () => {
    const def = builtInDefault();
    return def ? content() !== def.content : false;
  };

  const handleReset = () => {
    const def = builtInDefault();
    if (def) {
      setContent(def.content);
      setName(def.name);
      setDescription(def.description ?? "");
      setPlacement(def.placement ?? []);
      setAutoExecute(def.autoExecute ?? false);
      setExecutionMode(def.executionMode ?? "inject");
      setOutputTarget(def.outputTarget);
      setSystemPrompt(def.systemPrompt ?? "");
    }
  };

  const handlePlacementToggle = (p: SmartPlacement) => {
    setPlacement((prev) =>
      prev.includes(p) ? prev.filter((x) => x !== p) : [...prev, p],
    );
  };

  const handleInsertVariable = (varName: string) => {
    const el = textareaRef;
    if (!el) return;
    const insertion = `{${varName}}`;
    const start = el.selectionStart;
    const end = el.selectionEnd;
    const before = el.value.slice(0, start);
    const after = el.value.slice(end);
    setContent(before + insertion + after);
    requestAnimationFrame(() => {
      el.focus();
      const pos = start + insertion.length;
      el.setSelectionRange(pos, pos);
    });
  };

  const handleSave = () => {
    if (!name().trim() || !content().trim()) {
      setValidationError("Name and content are required");
      return;
    }
    setValidationError(null);

    props.onSave({
      name: name().trim(),
      description: description().trim() || undefined,
      content: content(),
      shortcut: shortcut().trim() || undefined,
      placement: placement().length > 0 ? placement() : undefined,
      autoExecute: autoExecute(),
      executionMode: executionMode(),
      outputTarget: executionMode() !== "inject" ? outputTarget() : undefined,
      systemPrompt: executionMode() === "api" ? (systemPrompt().trim() || undefined) : undefined,
    });
  };

  return (
    <div class={s.editorOverlay} onClick={props.onCancel}>
      <div class={s.editor} onClick={(e) => e.stopPropagation()}>
        <h4>
          {props.prompt ? "Edit Prompt" : "New Prompt"}
          <Show when={isBuiltIn()}>
            <span class={s.badge} style={{ "margin-left": "8px" }}>built-in</span>
          </Show>
        </h4>

        <Show when={validationError()}>
          <p class={s.validationError}>{validationError()}</p>
        </Show>

        {/* Name */}
        <div class={s.editorField}>
          <label>Name *</label>
          <input
            type="text"
            value={name()}
            disabled={isBuiltIn()}
            onInput={(e) => { setName(e.currentTarget.value); setValidationError(null); }}
            placeholder="My Prompt"
            autofocus
          />
        </div>

        {/* Description */}
        <div class={s.editorField}>
          <label>Description</label>
          <input
            type="text"
            value={description()}
            onInput={(e) => setDescription(e.currentTarget.value)}
            placeholder="What does this prompt do?"
          />
        </div>

        {/* Content + Variable dropdown */}
        <div class={s.editorField}>
          <label>Content *</label>
          <textarea
            ref={textareaRef}
            value={content()}
            onInput={(e) => setContent(e.currentTarget.value)}
            placeholder="Enter your prompt text here..."
            rows={6}
          />
          <VariableDropdown onInsert={handleInsertVariable} />
        </div>

        {/* Placement */}
        <div class={s.editorField}>
          <label>Placement</label>
          <div class={s.placementGrid}>
            <For each={ALL_PLACEMENTS}>
              {(p) => (
                <label class={s.placementCheck}>
                  <input
                    type="checkbox"
                    checked={placement().includes(p)}
                    onChange={() => handlePlacementToggle(p)}
                  />
                  <span>{p}</span>
                </label>
              )}
            </For>
          </div>
        </div>

        {/* Execution Mode + Auto-execute */}
        <div class={s.editorFieldRow}>
          <div class={s.editorField}>
            <label>Execution Mode</label>
            <select
              value={executionMode()}
              onChange={(e) => {
                const mode = e.currentTarget.value as "inject" | "headless" | "api";
                setExecutionMode(mode);
                if (mode === "inject") { setOutputTarget(undefined); setSystemPrompt(""); }
              }}
            >
              <option value="inject">Inject into terminal</option>
              <option value="headless">Headless (one-shot CLI)</option>
              <option value="api">API (LLM direct)</option>
            </select>
          </div>

          <Show when={executionMode() === "inject"}>
            <div class={s.editorField}>
              <label>Auto-execute</label>
              <label class={s.toggleRow}>
                <input
                  type="checkbox"
                  checked={autoExecute()}
                  onChange={() => setAutoExecute(!autoExecute())}
                />
                <span>Send immediately</span>
              </label>
              <span class={s.fieldHint}>Uncheck to review before sending</span>
            </div>
          </Show>
        </div>

        {/* Output Target (headless and api modes) */}
        <Show when={executionMode() !== "inject"}>
          <div class={s.editorField}>
            <label>Output Target</label>
            <select
              value={outputTarget() ?? ""}
              onChange={(e) => {
                const val = e.currentTarget.value || undefined;
                setOutputTarget(val as SavedPrompt["outputTarget"]);
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
        <Show when={executionMode() === "api"}>
          <div class={s.editorField}>
            <label>System Prompt</label>
            <textarea
              rows={3}
              value={systemPrompt()}
              placeholder="Instructions for the LLM (e.g. 'You are a Git expert.')"
              onInput={(e) => setSystemPrompt(e.currentTarget.value)}
            />
            <span class={s.fieldHint}>Sent as the system message to the LLM</span>
          </div>
        </Show>

        {/* Shortcut */}
        <div class={s.editorField}>
          <label>Keyboard Shortcut</label>
          <KeyComboCapture
            value={shortcut()}
            onChange={setShortcut}
            placeholder="Click to set shortcut"
          />
        </div>

        {/* Actions */}
        <div class={s.editorActions}>
          <Show when={isBuiltIn() && isOverridden()}>
            <button class={s.resetBtn} onClick={handleReset}>Reset to Default</button>
          </Show>
          <div style={{ flex: "1" }} />
          <button onClick={handleSave}>Save</button>
          <button onClick={props.onCancel}>Cancel</button>
        </div>
      </div>
    </div>
  );
};

export default PromptDrawer;
