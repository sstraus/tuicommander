import { Component, For, Show, createSignal, createMemo } from "solid-js";
import { getModifierSymbol, isMacOS } from "../../../platform";
import { t } from "../../../i18n";
import { keybindingsStore } from "../../../stores/keybindings";
import { settingsStore } from "../../../stores/settings";
import { normalizeCombo, type ActionName } from "../../../keybindingDefaults";
import { comboToDisplay } from "../../../utils/hotkey";
import { keyEventToCombo, validateGlobalHotkeyCombo } from "../../../utils/keyRecorder";
import { isTauri } from "../../../transport";
import { KeyComboCapture } from "../../shared/KeyComboCapture";
import { appLogger } from "../../../stores/appLogger";
import s from "../Settings.module.css";

interface ShortcutEntry {
  action?: ActionName;
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
      { action: "new-terminal", keys: keyFor("new-terminal"), description: t("helpPanel.newTerminalTab", "New terminal tab") },
      { action: "close-terminal", keys: keyFor("close-terminal"), description: t("helpPanel.closeTerminalTab", "Close terminal tab") },
      { action: "run-command", keys: keyFor("run-command"), description: t("helpPanel.runSavedCommand", `Run saved command (${editKey} to edit)`) },
      { action: "reopen-closed-tab", keys: keyFor("reopen-closed-tab"), description: t("helpPanel.reopenClosedTab", "Reopen closed tab") },
      { keys: keyFor("switch-tab-1", `${mod}1-9`), description: t("helpPanel.switchToTab", "Switch to tab by number") },
      { keys: isMacOS() ? "⌃⇧Tab" : "Ctrl+Shift+Tab", description: t("helpPanel.previousTab", "Previous tab") },
      { keys: isMacOS() ? "⌃Tab" : "Ctrl+Tab", description: t("helpPanel.nextTab", "Next tab") },
      { action: "clear-terminal", keys: keyFor("clear-terminal"), description: t("helpPanel.clearTerminal", "Clear terminal") },
      { action: "find-in-terminal", keys: keyFor("find-in-terminal"), description: t("helpPanel.findInContent", "Find in content") },
      { keys: `${mod}G`, description: t("helpPanel.findNext", "Find next match") },
      { keys: `${isMacOS() ? "⌘⇧G" : "Shift+F3"}`, description: t("helpPanel.findPrevious", "Find previous match") },
      { keys: `${mod}C`, description: t("helpPanel.copySelection", "Copy selection") },
      { keys: `${mod}V`, description: t("helpPanel.pasteToTerminal", "Paste to terminal") },
      { action: "scroll-to-top", keys: keyFor("scroll-to-top"), description: "Scroll to top" },
      { action: "scroll-to-bottom", keys: keyFor("scroll-to-bottom"), description: "Scroll to bottom" },
      { action: "scroll-page-up", keys: keyFor("scroll-page-up"), description: "Scroll page up" },
      { action: "scroll-page-down", keys: keyFor("scroll-page-down"), description: "Scroll page down" },
      { action: "zoom-pane", keys: keyFor("zoom-pane"), description: "Toggle zoom pane" },
    ],
  },
  {
    title: t("helpPanel.zoom", "Zoom"),
    shortcuts: [
      { action: "zoom-in", keys: keyFor("zoom-in"), description: t("helpPanel.zoomIn", "Zoom in") },
      { action: "zoom-out", keys: keyFor("zoom-out"), description: t("helpPanel.zoomOut", "Zoom out") },
      { action: "zoom-reset", keys: keyFor("zoom-reset"), description: t("helpPanel.resetZoom", "Reset zoom") },
    ],
  },
  {
    title: t("helpPanel.panels", "Panels"),
    shortcuts: [
      { action: "toggle-markdown", keys: keyFor("toggle-markdown"), description: t("helpPanel.toggleMarkdownPanel", "Toggle markdown panel") },
      { action: "toggle-file-browser", keys: keyFor("toggle-file-browser"), description: t("helpPanel.toggleFileBrowser", "Toggle file browser") },
      { action: "toggle-settings", keys: keyFor("toggle-settings"), description: t("helpPanel.openSettings", "Open settings") },
      { action: "toggle-task-queue", keys: keyFor("toggle-task-queue"), description: t("helpPanel.toggleTaskQueue", "Toggle task queue") },
      { action: "toggle-error-log", keys: keyFor("toggle-error-log"), description: "Toggle error log" },
      { action: "toggle-mcp-popup", keys: keyFor("toggle-mcp-popup"), description: "Toggle MCP popup" },
      { action: "worktree-manager", keys: keyFor("worktree-manager"), description: "Worktree manager" },
      { action: "clear-scrollback", keys: keyFor("clear-scrollback"), description: "Clear scrollback" },
      { action: "toggle-notes", keys: keyFor("toggle-notes"), description: t("helpPanel.toggleIdeasPanel", "Toggle ideas panel") },
      { action: "toggle-help", keys: keyFor("toggle-help"), description: t("helpPanel.toggleHelpPanel", "Toggle help panel") },
      { action: "command-palette", keys: keyFor("command-palette"), description: t("helpPanel.commandPalette", "Command palette") },
      { action: "activity-dashboard", keys: keyFor("activity-dashboard"), description: t("helpPanel.activityDashboard", "Activity dashboard") },
      { action: "prompt-library", keys: keyFor("prompt-library"), description: "Prompt library" },
    ],
  },
  {
    title: t("helpPanel.git", "Git"),
    shortcuts: [
      { action: "toggle-git-ops", keys: keyFor("toggle-git-ops"), description: t("helpPanel.gitPanel", "Git Panel") },
      { action: "toggle-branches-tab", keys: keyFor("toggle-branches-tab"), description: "Branches" },
      { action: "quick-branch-switch", keys: keyFor("quick-branch-switch"), description: "Quick branch switch" },
      { action: "toggle-diff-scroll", keys: keyFor("toggle-diff-scroll"), description: "Diff scroll view" },
    ],
  },
  {
    title: t("helpPanel.splitPanes", "Split Panes"),
    shortcuts: [
      { action: "split-vertical", keys: keyFor("split-vertical"), description: t("helpPanel.splitVertically", "Split vertically (side by side)") },
      { action: "split-horizontal", keys: keyFor("split-horizontal"), description: t("helpPanel.splitHorizontally", "Split horizontally (stacked)") },
      { keys: "\u2325\u2190/\u2192", description: t("helpPanel.navigatePanesVertical", "Navigate panes (vertical split)") },
      { keys: "\u2325\u2191/\u2193", description: t("helpPanel.navigatePanesHorizontal", "Navigate panes (horizontal split)") },
      { action: "close-terminal", keys: keyFor("close-terminal"), description: t("helpPanel.closeActivePane", "Close active pane (or tab if single)") },
    ],
  },
  {
    title: t("helpPanel.sidebarNavigation", "Sidebar & Navigation"),
    shortcuts: [
      { action: "toggle-sidebar", keys: keyFor("toggle-sidebar"), description: t("helpPanel.toggleSidebar", "Toggle sidebar") },
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
      { action: "toggle-file-browser", keys: keyFor("toggle-file-browser"), description: t("helpPanel.toggleFileBrowser", "Toggle file browser panel") },
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

/** Pencil SVG icon for the edit button */
const PencilIcon = () => (
  <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
    <path d="M12.146.854a.5.5 0 0 1 .708 0l2.292 2.292a.5.5 0 0 1 0 .708l-9.5 9.5a.5.5 0 0 1-.168.11l-4 1.5a.5.5 0 0 1-.65-.65l1.5-4a.5.5 0 0 1 .11-.168l9.5-9.5zM11.207 2.5 3.5 10.207 2.293 13.707l3.5-1.207L13.5 4.793 11.207 2.5z"/>
  </svg>
);

/** Reset icon (counter-clockwise arrow) */
const ResetIcon = () => (
  <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
    <path d="M8 3a5 5 0 1 0 4.546 2.914.5.5 0 1 1 .908-.418A6 6 0 1 1 8 2v1z"/>
    <path d="M8 4.466V.534a.25.25 0 0 1 .41-.192l2.36 1.966a.25.25 0 0 1 0 .384L8.41 4.658A.25.25 0 0 1 8 4.466z"/>
  </svg>
);

export const KeyboardShortcutsTab: Component = () => {
  const [filter, setFilter] = createSignal("");
  const [editingAction, setEditingAction] = createSignal<ActionName | null>(null);
  const [conflict, setConflict] = createSignal<{ action: ActionName; combo: string } | null>(null);
  let inputRef: HTMLInputElement | undefined;

  const filteredSections = createMemo(() => {
    // Read version for reactivity
    keybindingsStore.version;
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
  });

  function startEditing(action: ActionName) {
    setConflict(null);
    setEditingAction(action);
  }

  function cancelEditing() {
    setEditingAction(null);
    setConflict(null);
  }

  async function handleKeyDown(e: KeyboardEvent) {
    const action = editingAction();
    if (!action) return;

    e.preventDefault();
    e.stopPropagation();

    // Escape cancels
    if (e.key === "Escape") {
      cancelEditing();
      return;
    }

    const combo = keyEventToCombo(e);
    if (!combo) return; // modifier-only

    // Check for conflicts
    const normalized = normalizeCombo(combo);
    const existingAction = keybindingsStore.getActionForCombo(normalized);
    if (existingAction && existingAction !== action) {
      setConflict({ action: existingAction, combo });
      return;
    }

    await keybindingsStore.setOverride(action, combo);
    cancelEditing();
  }

  async function confirmConflictReplace() {
    const action = editingAction();
    const conf = conflict();
    if (!action || !conf) return;

    // Unbind the conflicting action first by setting it to the old action's key
    // (or just override both — the setOverride will naturally displace the old binding)
    await keybindingsStore.setOverride(action, conf.combo);
    cancelEditing();
  }

  // --- Global Hotkey helpers ---
  const [globalHotkeyError, setGlobalHotkeyError] = createSignal<string | null>(null);

  /** Convert DOM-style combo (Cmd+X, Ctrl+X) to Tauri Shortcut::from_str format.
   *  Keeps Cmd and Ctrl distinct — no CommandOrControl merging. */
  function comboToTauri(combo: string): string {
    return combo
      .replace(/\bCmd\b/g, "Super")
      .replace(/\bCtrl\b/g, "Control");
  }

  /** Convert Tauri format back to display format */
  function tauriToDisplay(combo: string): string {
    return combo
      .replace(/\bSuper\b/g, "Cmd")
      .replace(/\bControl\b/g, "Ctrl");
  }

  async function handleGlobalHotkeyChange(combo: string) {
    setGlobalHotkeyError(null);
    try {
      const validated = validateGlobalHotkeyCombo(combo);
      await settingsStore.setGlobalHotkey(comboToTauri(validated));
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      setGlobalHotkeyError(msg);
      appLogger.error("config", "Failed to register", err);
    }
  }

  async function clearGlobalHotkey() {
    setGlobalHotkeyError(null);
    try {
      await settingsStore.setGlobalHotkey(null);
    } catch (err) {
      appLogger.error("config", "Failed to clear", err);
    }
  }

  /** Temporarily unregister global hotkey while capturing to avoid conflict */
  async function handleGlobalCapturingChange(capturing: boolean) {
    const current = settingsStore.state.globalHotkey;
    if (!current) return;
    try {
      if (capturing) {
        // Temporarily unregister so the OS doesn't intercept the keypress
        await settingsStore.setGlobalHotkey(null);
      }
      // On capture end, the new combo is set via handleGlobalHotkeyChange
    } catch (err) {
      appLogger.warn("config", "Failed to temporarily unregister global hotkey during capture", err);
    }
  }

  return (
    <div class={s.section}>
      <h3>{t("settings.keyboardShortcuts", "Keyboard Shortcuts")}</h3>

      <Show when={isTauri()}>
        <div class={s.group}>
          <label>{t("settings.globalHotkey", "Global Hotkey (Toggle Window)")}</label>
          <p class={s.hint} style={{ "margin-bottom": "8px" }}>
            {t("settings.globalHotkeyHint", "Set an OS-level shortcut to show/hide TUICommander from any application.")}
          </p>
          <div style={{ display: "flex", "align-items": "center", gap: "8px" }}>
            <KeyComboCapture
              value={settingsStore.state.globalHotkey ? tauriToDisplay(settingsStore.state.globalHotkey) : ""}
              onChange={handleGlobalHotkeyChange}
              placeholder={t("settings.globalHotkeyPlaceholder", "Click to set hotkey")}
              onCapturingChange={handleGlobalCapturingChange}
            />
            <Show when={settingsStore.state.globalHotkey}>
              <button
                onClick={clearGlobalHotkey}
                style={{
                  background: "none",
                  border: "1px solid var(--border)",
                  "border-radius": "var(--radius-md)",
                  padding: "4px 8px",
                  color: "var(--fg-secondary)",
                  cursor: "pointer",
                  "font-size": "var(--font-sm)",
                }}
              >
                {t("settings.clear", "Clear")}
              </button>
            </Show>
          </div>
          <Show when={globalHotkeyError()}>
            <p style={{ color: "var(--error)", "font-size": "var(--font-sm)", "margin-top": "4px" }}>
              {globalHotkeyError()}
            </p>
          </Show>
        </div>
      </Show>

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
        {t("helpPanel.editableNote", "Click the pencil icon to rebind a shortcut. Press Escape to cancel.")}
      </p>

      <For each={filteredSections()}>
        {(section) => (
          <div class={s.group}>
            <label>{section.title}</label>
            <table style={{ width: "100%", "border-collapse": "collapse" }}>
              <For each={section.shortcuts}>
                {(shortcut) => (
                  <tr style={{ cursor: "default" }}>
                    <td style={{ width: "140px", padding: "4px 8px", "vertical-align": "middle" }}>
                      <Show when={editingAction() === shortcut.action} fallback={
                        <div style={{ display: "flex", "align-items": "center", gap: "4px" }}>
                          <kbd style={{
                            display: "inline-block",
                            padding: "2px 8px",
                            background: shortcut.action && keybindingsStore.isOverridden(shortcut.action)
                              ? "var(--accent-subtle, rgba(100, 149, 237, 0.15))"
                              : "var(--bg-tertiary)",
                            border: "1px solid var(--border)",
                            "border-radius": "var(--radius-md)",
                            "font-family": "var(--font-mono)",
                            "font-size": "var(--font-sm)",
                            color: shortcut.action && keybindingsStore.isOverridden(shortcut.action)
                              ? "var(--accent)"
                              : "var(--fg-secondary)",
                          }}>{shortcut.keys}</kbd>
                          <Show when={shortcut.action}>
                            <button
                              onClick={() => startEditing(shortcut.action!)}
                              title="Edit shortcut"
                              style={{
                                background: "none",
                                border: "none",
                                cursor: "pointer",
                                color: "var(--fg-muted)",
                                padding: "2px",
                                "border-radius": "var(--radius-sm)",
                                display: "flex",
                                "align-items": "center",
                                opacity: "0.5",
                                transition: "opacity 0.15s",
                              }}
                              onMouseEnter={(e) => { e.currentTarget.style.opacity = "1"; }}
                              onMouseLeave={(e) => { e.currentTarget.style.opacity = "0.5"; }}
                            >
                              <PencilIcon />
                            </button>
                            <Show when={keybindingsStore.isOverridden(shortcut.action!)}>
                              <button
                                onClick={() => keybindingsStore.resetAction(shortcut.action!)}
                                title="Reset to default"
                                style={{
                                  background: "none",
                                  border: "none",
                                  cursor: "pointer",
                                  color: "var(--fg-muted)",
                                  padding: "2px",
                                  "border-radius": "var(--radius-sm)",
                                  display: "flex",
                                  "align-items": "center",
                                  opacity: "0.5",
                                  transition: "opacity 0.15s",
                                }}
                                onMouseEnter={(e) => { e.currentTarget.style.opacity = "1"; }}
                                onMouseLeave={(e) => { e.currentTarget.style.opacity = "0.5"; }}
                              >
                                <ResetIcon />
                              </button>
                            </Show>
                          </Show>
                        </div>
                      }>
                        <RecordingIndicator
                          conflict={conflict()}
                          onKeyDown={handleKeyDown}
                          onCancel={cancelEditing}
                          onConfirmReplace={confirmConflictReplace}
                        />
                      </Show>
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

      <div style={{ "margin-top": "16px", "text-align": "right" }}>
        <button
          class={s.hint}
          onClick={() => keybindingsStore.resetAll()}
          style={{
            background: "none",
            border: "1px solid var(--border)",
            "border-radius": "var(--radius-md)",
            padding: "4px 12px",
            color: "var(--fg-secondary)",
            cursor: "pointer",
            "font-size": "var(--font-sm)",
          }}
        >
          {t("helpPanel.resetAllDefaults", "Reset all to defaults")}
        </button>
      </div>
    </div>
  );
};

/** Inline recording indicator shown when editing a shortcut */
const RecordingIndicator: Component<{
  conflict: { action: ActionName; combo: string } | null;
  onKeyDown: (e: KeyboardEvent) => void;
  onCancel: () => void;
  onConfirmReplace: () => void;
}> = (props) => {
  // Auto-focus and attach keydown listener
  const handleMount = (el: HTMLDivElement) => {
    el.focus();
  };

  return (
    <div style={{ display: "flex", "flex-direction": "column", gap: "4px" }}>
      <div
        ref={handleMount}
        tabIndex={0}
        onKeyDown={props.onKeyDown}
        onBlur={props.onCancel}
        style={{
          display: "inline-flex",
          "align-items": "center",
          padding: "2px 8px",
          background: "var(--bg-tertiary)",
          border: "2px solid var(--accent)",
          "border-radius": "var(--radius-md)",
          "font-family": "var(--font-mono)",
          "font-size": "var(--font-sm)",
          color: "var(--accent)",
          animation: "pulse-opacity 1.5s ease-in-out infinite",
          outline: "none",
          "min-width": "80px",
          "white-space": "nowrap",
        }}
      >
        {props.conflict
          ? comboToDisplay(props.conflict.combo)
          : t("helpPanel.pressKey", "Press a key...")}
      </div>
      <Show when={props.conflict}>
        <div style={{
          "font-size": "var(--font-xs)",
          color: "var(--warning)",
          display: "flex",
          "align-items": "center",
          gap: "4px",
        }}>
          <span>Already used by "{props.conflict!.action}"</span>
          <button
            onMouseDown={(e) => { e.preventDefault(); props.onConfirmReplace(); }}
            style={{
              background: "none",
              border: "1px solid var(--warning)",
              "border-radius": "var(--radius-sm)",
              padding: "0 4px",
              color: "var(--warning)",
              cursor: "pointer",
              "font-size": "var(--font-xs)",
            }}
          >
            Replace
          </button>
        </div>
      </Show>
    </div>
  );
};
