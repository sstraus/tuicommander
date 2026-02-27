import { Component, For, Show, createEffect, createSignal, onCleanup } from "solid-js";
import { promptLibraryStore, type SavedPrompt, type PromptCategory } from "../../stores/promptLibrary";
import { terminalsStore } from "../../stores/terminals";
import { usePty } from "../../hooks/usePty";
import { appLogger } from "../../stores/appLogger";
import { t } from "../../i18n";
import { cx } from "../../utils";
import { KeyComboCapture } from "../shared/KeyComboCapture";
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

export const PromptDrawer: Component<PromptDrawerProps> = (props) => {
  const [selectedIndex, setSelectedIndex] = createSignal(0);
  const [showEditor, setShowEditor] = createSignal(false);
  const [editingPrompt, setEditingPrompt] = createSignal<SavedPrompt | null>(null);
  const [variableValues, setVariableValues] = createSignal<Record<string, string>>({});
  const [showVariableDialog, setShowVariableDialog] = createSignal(false);
  const [pendingPrompt, setPendingPrompt] = createSignal<SavedPrompt | null>(null);

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
      // If showing variable dialog, handle differently
      if (showVariableDialog()) {
        if (e.key === "Escape") {
          setShowVariableDialog(false);
          setPendingPrompt(null);
        }
        return;
      }

      // If showing editor, handle differently
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
    const variables = await promptLibraryStore.extractVariables(prompt.content);

    // If prompt has variables, show variable dialog
    if (variables.length > 0) {
      setPendingPrompt(prompt);
      setVariableValues(
        variables.reduce((acc, v) => {
          const promptVar = prompt.variables?.find((pv) => pv.name === v);
          acc[v] = promptVar?.defaultValue || "";
          return acc;
        }, {} as Record<string, string>)
      );
      setShowVariableDialog(true);
      return;
    }

    await doInject(prompt, {}, executeImmediately);
  };

  /** Actually inject the prompt */
  const doInject = async (
    prompt: SavedPrompt,
    variables: Record<string, string>,
    executeImmediately: boolean = false
  ) => {
    const activeTerminal = terminalsStore.getActive();
    if (!activeTerminal?.sessionId) return;

    // Process content with variables
    let content = await promptLibraryStore.processContent(prompt, variables);

    // Add newline if executing immediately
    if (executeImmediately) {
      content += "\n";
    }

    // Write to terminal
    try {
      await pty.write(activeTerminal.sessionId, content);
      promptLibraryStore.markAsUsed(prompt.id);
      promptLibraryStore.closeDrawer();
      props.onClose?.();
    } catch (err) {
      appLogger.error("app", "Failed to inject prompt", err);
    }
  };

  /** Handle variable dialog submit */
  const handleVariableSubmit = (executeImmediately: boolean) => {
    const prompt = pendingPrompt();
    if (!prompt) return;

    doInject(prompt, variableValues(), executeImmediately);
    setShowVariableDialog(false);
    setPendingPrompt(null);
  };

  /** Create new prompt */
  const createNewPrompt = () => {
    setEditingPrompt(null);
    setShowEditor(true);
  };

  /** Edit existing prompt */
  const editPrompt = (prompt: SavedPrompt) => {
    setEditingPrompt(prompt);
    setShowEditor(true);
  };

  /** Delete prompt */
  const deletePrompt = (prompt: SavedPrompt) => {
    if (confirm(`Delete prompt "${prompt.name}"?`)) {
      promptLibraryStore.deletePrompt(prompt.id);
    }
  };

  return (
    <Show when={isOpen()}>
      <div class={s.overlay} onClick={() => promptLibraryStore.closeDrawer()}>
        <div class={s.drawer} onClick={(e) => e.stopPropagation()}>
          {/* Header */}
          <div class={s.header}>
            <h3>{t("promptDrawer.title", "Prompt Library")}</h3>
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
                {(prompt, index) => (
                  <div
                    class={cx(s.promptItem, index() === selectedIndex() && s.selected)}
                    onClick={() => injectPrompt(prompt)}
                    onDblClick={() => injectPrompt(prompt, true)}
                  >
                    <div class={s.itemContent}>
                      <div class={s.itemName}>
                        {prompt.isFavorite && <span class={s.favoriteStar}>★</span>}
                        {prompt.name}
                        {prompt.shortcut && <span class={s.shortcut}>{prompt.shortcut}</span>}
                      </div>
                      <Show when={prompt.description}>
                        <div class={s.itemDescription}>{prompt.description}</div>
                      </Show>
                    </div>
                    <div class={s.itemActions}>
                      <button
                        title={t("promptDrawer.edit", "Edit")}
                        onClick={(e) => {
                          e.stopPropagation();
                          editPrompt(prompt);
                        }}
                      >
                        ✎
                      </button>
                      <button
                        title={prompt.isFavorite ? t("promptDrawer.unfavorite", "Unfavorite") : t("promptDrawer.favorite", "Favorite")}
                        onClick={(e) => {
                          e.stopPropagation();
                          promptLibraryStore.toggleFavorite(prompt.id);
                        }}
                      >
                        {prompt.isFavorite ? "★" : "☆"}
                      </button>
                      <button
                        title={t("promptDrawer.delete", "Delete")}
                        onClick={(e) => {
                          e.stopPropagation();
                          deletePrompt(prompt);
                        }}
                      >
                        🗑
                      </button>
                    </div>
                  </div>
                )}
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
            <h4>{t("promptDrawer.fillVariables", "Fill in variables")}</h4>
            <For each={Object.keys(variableValues())}>
              {(varName) => (
                <div class={s.variableInput}>
                  <label>{varName}</label>
                  <input
                    type="text"
                    value={variableValues()[varName] || ""}
                    onInput={(e) =>
                      setVariableValues((v) => ({ ...v, [varName]: e.currentTarget.value }))
                    }
                    placeholder={pendingPrompt()?.variables?.find((v) => v.name === varName)?.description || varName}
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
              // For new prompts, ensure required fields are present
              if (promptData.name && promptData.content) {
                promptLibraryStore.createPrompt({
                  name: promptData.name,
                  content: promptData.content,
                  description: promptData.description,
                  shortcut: promptData.shortcut,
                  category: "custom",
                  isFavorite: false,
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
  );
};

/** Prompt editor dialog */
interface PromptEditorProps {
  prompt: SavedPrompt | null;
  onSave: (data: Partial<SavedPrompt>) => void;
  onCancel: () => void;
}

const PromptEditor: Component<PromptEditorProps> = (props) => {
  const [name, setName] = createSignal(props.prompt?.name || "");
  const [description, setDescription] = createSignal(props.prompt?.description || "");
  const [content, setContent] = createSignal(props.prompt?.content || "");
  const [shortcut, setShortcut] = createSignal(props.prompt?.shortcut || "");

  const handleSave = () => {
    if (!name().trim() || !content().trim()) {
      alert("Name and content are required");
      return;
    }

    props.onSave({
      name: name().trim(),
      description: description().trim() || undefined,
      content: content(),
      shortcut: shortcut().trim() || undefined,
    });
  };

  return (
    <div class={s.editorOverlay} onClick={props.onCancel}>
      <div class={s.editor} onClick={(e) => e.stopPropagation()}>
        <h4>{props.prompt ? t("promptDrawer.editPrompt", "Edit Prompt") : t("promptDrawer.newPromptTitle", "New Prompt")}</h4>

        <div class={s.editorField}>
          <label>{t("promptDrawer.nameLabel", "Name *")}</label>
          <input
            type="text"
            value={name()}
            onInput={(e) => setName(e.currentTarget.value)}
            placeholder={t("promptDrawer.namePlaceholder", "My Prompt")}
            autofocus
          />
        </div>

        <div class={s.editorField}>
          <label>{t("promptDrawer.descriptionLabel", "Description")}</label>
          <input
            type="text"
            value={description()}
            onInput={(e) => setDescription(e.currentTarget.value)}
            placeholder={t("promptDrawer.descriptionPlaceholder", "What does this prompt do?")}
          />
        </div>

        <div class={s.editorField}>
          <label>{t("promptDrawer.contentLabel", "Content * (use {variable} for variables)")}</label>
          <textarea
            value={content()}
            onInput={(e) => setContent(e.currentTarget.value)}
            placeholder={t("promptDrawer.contentPlaceholder", "Enter your prompt text here...")}
            rows={6}
          />
        </div>

        <div class={s.editorField}>
          <label>{t("promptDrawer.shortcutLabel", "Keyboard Shortcut")}</label>
          <KeyComboCapture
            value={shortcut()}
            onChange={setShortcut}
            placeholder={t("promptDrawer.shortcutPlaceholder", "Click to set shortcut")}
          />
        </div>

        <div class={s.editorActions}>
          <button onClick={handleSave}>{t("promptDrawer.save", "Save")}</button>
          <button onClick={props.onCancel}>{t("promptDrawer.cancel", "Cancel")}</button>
        </div>
      </div>
    </div>
  );
};

export default PromptDrawer;
