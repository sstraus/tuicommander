import { Component, Show, For, createSignal, createEffect, onCleanup } from "solid-js";
import { getModifierSymbol } from "../../platform";
import { t } from "../../i18n";
import s from "./HelpPanel.module.css";

interface ShortcutEntry {
  keys: string;
  description: string;
}

interface ShortcutSection {
  title: string;
  shortcuts: ShortcutEntry[];
}

function getShortcutSections(): ShortcutSection[] {
  const mod = getModifierSymbol();
  return [
  {
    title: t("helpPanel.terminal", "Terminal"),
    shortcuts: [
      { keys: `${mod}T`, description: t("helpPanel.newTerminalTab", "New terminal tab") },
      { keys: `${mod}W`, description: t("helpPanel.closeTerminalTab", "Close terminal tab") },
      { keys: `${mod}R`, description: t("helpPanel.runSavedCommand", `Run saved command (${mod}⇧R to edit)`) },
      { keys: `${mod}⇧T`, description: t("helpPanel.reopenClosedTab", "Reopen closed tab") },
      { keys: `${mod}1-9`, description: t("helpPanel.switchToTab", "Switch to tab by number") },
      { keys: `${mod}⇧[`, description: t("helpPanel.previousTab", "Previous tab") },
      { keys: `${mod}⇧]`, description: t("helpPanel.nextTab", "Next tab") },
      { keys: `${mod}L`, description: t("helpPanel.clearTerminal", "Clear terminal") },
      { keys: `${mod}C`, description: t("helpPanel.copySelection", "Copy selection") },
      { keys: `${mod}V`, description: t("helpPanel.pasteToTerminal", "Paste to terminal") },
    ],
  },
  {
    title: t("helpPanel.zoom", "Zoom"),
    shortcuts: [
      { keys: `${mod}+`, description: t("helpPanel.zoomIn", "Zoom in") },
      { keys: `${mod}-`, description: t("helpPanel.zoomOut", "Zoom out") },
      { keys: `${mod}0`, description: t("helpPanel.resetZoom", "Reset zoom") },
    ],
  },
  {
    title: t("helpPanel.panels", "Panels"),
    shortcuts: [
      { keys: `${mod}⇧D`, description: t("helpPanel.toggleDiffPanel", "Toggle git diff panel") },
      { keys: `${mod}M`, description: t("helpPanel.toggleMarkdownPanel", "Toggle markdown panel") },
      { keys: `${mod},`, description: t("helpPanel.openSettings", "Open settings") },
      { keys: `${mod}J`, description: t("helpPanel.toggleTaskQueue", "Toggle task queue") },
      { keys: `${mod}K`, description: t("helpPanel.promptLibrary", "Prompt library") },
      { keys: `${mod}N`, description: t("helpPanel.toggleIdeasPanel", "Toggle ideas panel") },
      { keys: `${mod}?`, description: t("helpPanel.toggleHelpPanel", "Toggle help panel") },
    ],
  },
  {
    title: t("helpPanel.git", "Git"),
    shortcuts: [
      { keys: `${mod}G`, description: t("helpPanel.openLazygit", "Open lazygit in terminal") },
      { keys: `${mod}⇧G`, description: t("helpPanel.gitOperationsPanel", "Git operations panel") },
      { keys: `${mod}⇧L`, description: t("helpPanel.lazygitSplitPane", "Lazygit split pane") },
    ],
  },
  {
    title: t("helpPanel.splitPanes", "Split Panes"),
    shortcuts: [
      { keys: `${mod}\\`, description: t("helpPanel.splitVertically", "Split vertically (side by side)") },
      { keys: `${mod}⌥\\`, description: t("helpPanel.splitHorizontally", "Split horizontally (stacked)") },
      { keys: "⌥←/→", description: t("helpPanel.navigatePanesVertical", "Navigate panes (vertical split)") },
      { keys: "⌥↑/↓", description: t("helpPanel.navigatePanesHorizontal", "Navigate panes (horizontal split)") },
      { keys: `${mod}W`, description: t("helpPanel.closeActivePane", "Close active pane (or tab if single)") },
    ],
  },
  {
    title: t("helpPanel.sidebarNavigation", "Sidebar & Navigation"),
    shortcuts: [
      { keys: `${mod}[`, description: t("helpPanel.toggleSidebar", "Toggle sidebar") },
      { keys: `${mod}^1-9`, description: t("helpPanel.quickSwitchBranch", "Quick switch to branch") },
      { keys: `Hold ${mod}^`, description: t("helpPanel.showQuickSwitcher", "Show quick switcher") },
      { keys: t("helpPanel.dragRepo", "Drag repo"), description: t("helpPanel.reorderRepos", "Reorder repos or move between groups") },
      { keys: t("helpPanel.dragGroup", "Drag group"), description: t("helpPanel.reorderGroups", "Reorder groups") },
      { keys: t("helpPanel.rightClickGroup", "Right-click group"), description: t("helpPanel.renameColorDelete", "Rename, change color, delete") },
    ],
  },
  {
    title: t("helpPanel.fileBrowserEditor", "File Browser & Editor"),
    shortcuts: [
      { keys: `${mod}E`, description: t("helpPanel.toggleFileBrowser", "Toggle file browser panel") },
      { keys: "↑/↓", description: t("helpPanel.navigateFileList", "Navigate file list (when focused)") },
      { keys: "Enter", description: t("helpPanel.openFile", "Open file or enter directory") },
      { keys: "Backspace", description: t("helpPanel.goToParent", "Go to parent directory") },
      { keys: `${mod}S`, description: t("helpPanel.saveFile", "Save file (when editor focused)") },
    ],
  },
  {
    title: t("helpPanel.voiceDictation", "Voice Dictation"),
    shortcuts: [
      { keys: "Hold F5", description: t("helpPanel.pushToTalk", "Push-to-talk, works globally (configurable in Settings)") },
      { keys: t("helpPanel.holdMicBtn", "Hold Mic btn"), description: t("helpPanel.statusBarMic", "StatusBar mic button (hold to record)") },
    ],
  },
  ];
}

export interface HelpPanelProps {
  visible: boolean;
  onClose: () => void;
}

export const HelpPanel: Component<HelpPanelProps> = (props) => {
  const [filter, setFilter] = createSignal("");
  let inputRef: HTMLInputElement | undefined;

  // Close on Escape
  createEffect(() => {
    if (!props.visible) return;

    const handleKeydown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        props.onClose();
      }
    };

    document.addEventListener("keydown", handleKeydown);
    onCleanup(() => document.removeEventListener("keydown", handleKeydown));

    // Focus search input when opening
    requestAnimationFrame(() => inputRef?.focus());
  });

  const filteredSections = () => {
    const q = filter().toLowerCase();
    if (!q) return getShortcutSections();

    return getShortcutSections()
      .map((section) => ({
        ...section,
        shortcuts: section.shortcuts.filter(
          (sc) =>
            sc.keys.toLowerCase().includes(q) ||
            sc.description.toLowerCase().includes(q)
        ),
      }))
      .filter((section) => section.shortcuts.length > 0);
  };

  return (
    <Show when={props.visible}>
      <div class={s.overlay} onClick={props.onClose}>
        <div class={s.panel} onClick={(e) => e.stopPropagation()}>
          <div class={s.header}>
            <h2>{t("helpPanel.title", "Keyboard Shortcuts")}</h2>
            <button class={s.close} onClick={props.onClose}>
              &times;
            </button>
          </div>

          <div class={s.search}>
            <input
              ref={inputRef}
              type="text"
              placeholder={t("helpPanel.searchPlaceholder", "Search shortcuts...")}
              value={filter()}
              onInput={(e) => setFilter(e.currentTarget.value)}
            />
          </div>

          <div class={s.content}>
            <p class={s.menuNote}>
              {t("helpPanel.menuNote", "These shortcuts are also available via the system menu bar.")}
            </p>
            <For each={filteredSections()}>
              {(section) => (
                <div class={s.section}>
                  <h3 class={s.sectionTitle}>{section.title}</h3>
                  <table class={s.table}>
                    <For each={section.shortcuts}>
                      {(shortcut) => (
                        <tr>
                          <td class={s.key}>
                            <kbd>{shortcut.keys}</kbd>
                          </td>
                          <td class={s.desc}>{shortcut.description}</td>
                        </tr>
                      )}
                    </For>
                  </table>
                </div>
              )}
            </For>
            <Show when={filteredSections().length === 0}>
              <div class={s.empty}>{t("helpPanel.noResults", "No shortcuts match your search")}</div>
            </Show>
          </div>
        </div>
      </div>
    </Show>
  );
};

export default HelpPanel;
