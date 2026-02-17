import { Component, Show, For, createSignal, createEffect, onCleanup } from "solid-js";
import { getModifierSymbol } from "../../platform";

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
    title: "Terminal",
    shortcuts: [
      { keys: `${mod}T`, description: "New terminal tab" },
      { keys: `${mod}W`, description: "Close terminal tab" },
      { keys: `${mod}R`, description: `Run saved command (${mod}⇧R to edit)` },
      { keys: `${mod}⇧T`, description: "Reopen closed tab" },
      { keys: `${mod}1-9`, description: "Switch to tab by number" },
      { keys: `${mod}⇧[`, description: "Previous tab" },
      { keys: `${mod}⇧]`, description: "Next tab" },
      { keys: `${mod}L`, description: "Clear terminal" },
      { keys: `${mod}C`, description: "Copy selection" },
      { keys: `${mod}V`, description: "Paste to terminal" },
    ],
  },
  {
    title: "Zoom",
    shortcuts: [
      { keys: `${mod}+`, description: "Zoom in" },
      { keys: `${mod}-`, description: "Zoom out" },
      { keys: `${mod}0`, description: "Reset zoom" },
    ],
  },
  {
    title: "Panels",
    shortcuts: [
      { keys: `${mod}D`, description: "Toggle git diff panel" },
      { keys: `${mod}M`, description: "Toggle markdown panel" },
      { keys: `${mod},`, description: "Open settings" },
      { keys: `${mod}J`, description: "Toggle task queue" },
      { keys: `${mod}K`, description: "Prompt library" },
      { keys: `${mod}?`, description: "Toggle help panel" },
    ],
  },
  {
    title: "Git",
    shortcuts: [
      { keys: `${mod}G`, description: "Open lazygit in terminal" },
      { keys: `${mod}⇧G`, description: "Git operations panel" },
      { keys: `${mod}⇧L`, description: "Lazygit split pane" },
    ],
  },
  {
    title: "Split Panes",
    shortcuts: [
      { keys: `${mod}\\`, description: "Split vertically (side by side)" },
      { keys: `${mod}⌥\\`, description: "Split horizontally (stacked)" },
      { keys: "⌥←/→", description: "Navigate panes (vertical split)" },
      { keys: "⌥↑/↓", description: "Navigate panes (horizontal split)" },
      { keys: `${mod}W`, description: "Close active pane (or tab if single)" },
    ],
  },
  {
    title: "Sidebar & Navigation",
    shortcuts: [
      { keys: `${mod}[`, description: "Toggle sidebar" },
      { keys: `${mod}^1-9`, description: "Quick switch to branch" },
      { keys: `Hold ${mod}^`, description: "Show quick switcher" },
    ],
  },
  {
    title: "Voice Dictation",
    shortcuts: [
      { keys: "Hold F5", description: "Push-to-talk, works globally (configurable in Settings)" },
      { keys: "Hold Mic btn", description: "StatusBar mic button (hold to record)" },
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
          (s) =>
            s.keys.toLowerCase().includes(q) ||
            s.description.toLowerCase().includes(q)
        ),
      }))
      .filter((section) => section.shortcuts.length > 0);
  };

  return (
    <Show when={props.visible}>
      <div class="help-overlay" onClick={props.onClose}>
        <div class="help-panel" onClick={(e) => e.stopPropagation()}>
          <div class="help-header">
            <h2>Keyboard Shortcuts</h2>
            <button class="help-close" onClick={props.onClose}>
              &times;
            </button>
          </div>

          <div class="help-search">
            <input
              ref={inputRef}
              type="text"
              placeholder="Search shortcuts..."
              value={filter()}
              onInput={(e) => setFilter(e.currentTarget.value)}
            />
          </div>

          <div class="help-content">
            <p class="help-menu-note">
              These shortcuts are also available via the system menu bar.
            </p>
            <For each={filteredSections()}>
              {(section) => (
                <div class="help-section">
                  <h3 class="help-section-title">{section.title}</h3>
                  <table class="help-table">
                    <For each={section.shortcuts}>
                      {(shortcut) => (
                        <tr>
                          <td class="help-key">
                            <kbd>{shortcut.keys}</kbd>
                          </td>
                          <td class="help-desc">{shortcut.description}</td>
                        </tr>
                      )}
                    </For>
                  </table>
                </div>
              )}
            </For>
            <Show when={filteredSections().length === 0}>
              <div class="help-empty">No shortcuts match your search</div>
            </Show>
          </div>
        </div>
      </div>
    </Show>
  );
};

export default HelpPanel;
