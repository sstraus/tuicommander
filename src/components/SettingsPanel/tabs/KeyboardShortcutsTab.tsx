import { Component, For, Show, createSignal } from "solid-js";
import { getModifierSymbol, isMacOS } from "../../../platform";
import { t } from "../../../i18n";
import { keybindingsStore } from "../../../stores/keybindings";
import type { ActionName } from "../../../keybindingDefaults";
import { comboToDisplay } from "../../../utils/hotkey";
import s from "../Settings.module.css";

interface ShortcutEntry {
  keys: string;
  description: string;
}

interface ShortcutSection {
  title: string;
  shortcuts: ShortcutEntry[];
}

/** Get display string for an action from the keybindings store, or fallback */
export function keyFor(action: ActionName, fallback?: string): string {
  const combo = keybindingsStore.getKeyForAction(action);
  if (!combo) return fallback ?? "";
  return comboToDisplay(combo);
}

function getShortcutSections(): ShortcutSection[] {
  const mod = getModifierSymbol();
  const editKey = keyFor("edit-command");
  return [
  {
    title: t("helpPanel.terminal", "Terminal"),
    shortcuts: [
      { keys: keyFor("new-terminal"), description: t("helpPanel.newTerminalTab", "New terminal tab") },
      { keys: keyFor("close-terminal"), description: t("helpPanel.closeTerminalTab", "Close terminal tab") },
      { keys: keyFor("run-command"), description: t("helpPanel.runSavedCommand", `Run saved command (${editKey} to edit)`) },
      { keys: keyFor("reopen-closed-tab"), description: t("helpPanel.reopenClosedTab", "Reopen closed tab") },
      { keys: keyFor("switch-tab-1", `${mod}1-9`), description: t("helpPanel.switchToTab", "Switch to tab by number") },
      { keys: keyFor("prev-tab"), description: t("helpPanel.previousTab", "Previous tab") },
      { keys: keyFor("next-tab"), description: t("helpPanel.nextTab", "Next tab") },
      { keys: keyFor("clear-terminal"), description: t("helpPanel.clearTerminal", "Clear terminal") },
      { keys: keyFor("find-in-terminal"), description: t("helpPanel.findInTerminal", "Find in terminal") },
      { keys: `${mod}G`, description: t("helpPanel.findNext", "Find next match") },
      { keys: `${isMacOS() ? "⌘⇧G" : "Shift+F3"}`, description: t("helpPanel.findPrevious", "Find previous match") },
      { keys: `${mod}C`, description: t("helpPanel.copySelection", "Copy selection") },
      { keys: `${mod}V`, description: t("helpPanel.pasteToTerminal", "Paste to terminal") },
    ],
  },
  {
    title: t("helpPanel.zoom", "Zoom"),
    shortcuts: [
      { keys: keyFor("zoom-in"), description: t("helpPanel.zoomIn", "Zoom in") },
      { keys: keyFor("zoom-out"), description: t("helpPanel.zoomOut", "Zoom out") },
      { keys: keyFor("zoom-reset"), description: t("helpPanel.resetZoom", "Reset zoom") },
    ],
  },
  {
    title: t("helpPanel.panels", "Panels"),
    shortcuts: [
      { keys: keyFor("toggle-diff"), description: t("helpPanel.toggleDiffPanel", "Toggle git diff panel") },
      { keys: keyFor("toggle-markdown"), description: t("helpPanel.toggleMarkdownPanel", "Toggle markdown panel") },
      { keys: keyFor("toggle-settings"), description: t("helpPanel.openSettings", "Open settings") },
      { keys: keyFor("toggle-task-queue"), description: t("helpPanel.toggleTaskQueue", "Toggle task queue") },
      { keys: keyFor("toggle-prompt-library"), description: t("helpPanel.promptLibrary", "Prompt library") },
      { keys: keyFor("toggle-notes"), description: t("helpPanel.toggleIdeasPanel", "Toggle ideas panel") },
      { keys: keyFor("toggle-help"), description: t("helpPanel.toggleHelpPanel", "Toggle help panel") },
      { keys: keyFor("command-palette"), description: t("helpPanel.commandPalette", "Command palette") },
      { keys: keyFor("activity-dashboard"), description: t("helpPanel.activityDashboard", "Activity dashboard") },
    ],
  },
  {
    title: t("helpPanel.git", "Git"),
    shortcuts: [
      { keys: keyFor("open-lazygit"), description: t("helpPanel.openLazygit", "Open lazygit in terminal") },
      { keys: keyFor("toggle-git-ops"), description: t("helpPanel.gitOperationsPanel", "Git operations panel") },
      { keys: keyFor("open-lazygit-pane"), description: t("helpPanel.lazygitSplitPane", "Lazygit split pane") },
    ],
  },
  {
    title: t("helpPanel.splitPanes", "Split Panes"),
    shortcuts: [
      { keys: keyFor("split-vertical"), description: t("helpPanel.splitVertically", "Split vertically (side by side)") },
      { keys: keyFor("split-horizontal"), description: t("helpPanel.splitHorizontally", "Split horizontally (stacked)") },
      { keys: "\u2325\u2190/\u2192", description: t("helpPanel.navigatePanesVertical", "Navigate panes (vertical split)") },
      { keys: "\u2325\u2191/\u2193", description: t("helpPanel.navigatePanesHorizontal", "Navigate panes (horizontal split)") },
      { keys: keyFor("close-terminal"), description: t("helpPanel.closeActivePane", "Close active pane (or tab if single)") },
    ],
  },
  {
    title: t("helpPanel.sidebarNavigation", "Sidebar & Navigation"),
    shortcuts: [
      { keys: keyFor("toggle-sidebar"), description: t("helpPanel.toggleSidebar", "Toggle sidebar") },
      { keys: keyFor("switch-branch-1", `${mod}^1-9`), description: t("helpPanel.quickSwitchBranch", "Quick switch to branch") },
      { keys: `Hold ${mod}^`, description: t("helpPanel.showQuickSwitcher", "Show quick switcher") },
      { keys: t("helpPanel.dragRepo", "Drag repo"), description: t("helpPanel.reorderRepos", "Reorder repos or move between groups") },
      { keys: t("helpPanel.dragGroup", "Drag group"), description: t("helpPanel.reorderGroups", "Reorder groups") },
      { keys: t("helpPanel.rightClickGroup", "Right-click group"), description: t("helpPanel.renameColorDelete", "Rename, change color, delete") },
    ],
  },
  {
    title: t("helpPanel.fileBrowserEditor", "File Browser & Editor"),
    shortcuts: [
      { keys: keyFor("toggle-file-browser"), description: t("helpPanel.toggleFileBrowser", "Toggle file browser panel") },
      { keys: "\u2191/\u2193", description: t("helpPanel.navigateFileList", "Navigate file list (when focused)") },
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

export const KeyboardShortcutsTab: Component = () => {
  const [filter, setFilter] = createSignal("");
  let inputRef: HTMLInputElement | undefined;

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
    <div class={s.section}>
      <h3>{t("settings.keyboardShortcuts", "Keyboard Shortcuts")}</h3>

      <div class={s.group}>
        <input
          ref={inputRef}
          type="text"
          placeholder={t("helpPanel.searchPlaceholder", "Search shortcuts...")}
          value={filter()}
          onInput={(e) => setFilter(e.currentTarget.value)}
        />
      </div>

      <p class={s.hint} style={{ "margin-bottom": "12px" }}>
        {t("helpPanel.menuNote", "These shortcuts are also available via the system menu bar.")}
      </p>

      <For each={filteredSections()}>
        {(section) => (
          <div class={s.group}>
            <label>{section.title}</label>
            <table style={{ width: "100%", "border-collapse": "collapse" }}>
              <For each={section.shortcuts}>
                {(shortcut) => (
                  <tr style={{ cursor: "default" }}>
                    <td style={{ width: "120px", padding: "4px 8px", "vertical-align": "middle" }}>
                      <kbd style={{
                        display: "inline-block",
                        padding: "2px 8px",
                        background: "var(--bg-tertiary)",
                        border: "1px solid var(--border)",
                        "border-radius": "var(--radius-md)",
                        "font-family": "var(--font-mono)",
                        "font-size": "var(--font-sm)",
                        color: "var(--fg-secondary)",
                      }}>{shortcut.keys}</kbd>
                    </td>
                    <td style={{ padding: "4px 8px", "font-size": "var(--font-base)", color: "var(--fg-secondary)" }}>
                      {shortcut.description}
                    </td>
                  </tr>
                )}
              </For>
            </table>
          </div>
        )}
      </For>

      <Show when={filteredSections().length === 0}>
        <p class={s.hint} style={{ "text-align": "center", padding: "20px 0" }}>
          {t("helpPanel.noResults", "No shortcuts match your search")}
        </p>
      </Show>
    </div>
  );
};
