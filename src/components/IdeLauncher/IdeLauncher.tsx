import { Component, For, Show, createSignal, onCleanup, createEffect, onMount } from "solid-js";
import { invoke } from "../../invoke";
import { settingsStore, IDE_NAMES, IDE_ICON_PATHS, IDE_CATEGORIES } from "../../stores/settings";
import type { IdeType } from "../../stores/settings";
import { useRepository } from "../../hooks/useRepository";
import { getModifierSymbol } from "../../platform";

export interface IdeLauncherProps {
  repoPath?: string;
  runCommand?: string;
  onOpenInIde?: (ide: IdeType) => void;
  onRun?: (shiftKey: boolean) => void;
}

/** IDE icon component - renders the SVG icon at specified size */
const IdeIcon: Component<{ ide: IdeType; size?: number }> = (props) => {
  const size = () => props.size ?? 14;
  return (
    <img
      class="ide-launcher-icon"
      src={IDE_ICON_PATHS[props.ide]}
      width={size()}
      height={size()}
      alt={IDE_NAMES[props.ide]}
    />
  );
};

export const IdeLauncher: Component<IdeLauncherProps> = (props) => {
  const [isOpen, setIsOpen] = createSignal(false);
  const [installedIdes, setInstalledIdes] = createSignal<string[]>([]);
  const repo = useRepository();

  // Detect installed IDEs on mount
  onMount(async () => {
    try {
      const installed = await invoke<string[]>("detect_installed_ides");
      setInstalledIdes(installed);
    } catch (err) {
      console.error("Failed to detect installed IDEs:", err);
      setInstalledIdes(["terminal", "finder"]);
    }
  });

  const categoryOrder = [
    { key: "editors", label: "Code Editors" },
    { key: "terminals", label: "Terminals" },
    { key: "git", label: "Git Tools" },
    { key: "utilities", label: "System" },
  ];

  // Filter IDE list to only installed ones
  const filterInstalled = (ides: IdeType[]): IdeType[] => {
    return ides.filter((ide) => installedIdes().includes(ide));
  };

  let dropdownRef: HTMLDivElement | undefined;

  // Close dropdown on outside click
  createEffect(() => {
    if (!isOpen()) return;

    const handleClickOutside = (e: MouseEvent) => {
      if (dropdownRef && !dropdownRef.contains(e.target as Node)) {
        setIsOpen(false);
      }
    };

    document.addEventListener("mousedown", handleClickOutside);
    onCleanup(() => document.removeEventListener("mousedown", handleClickOutside));
  });

  // Handle keyboard
  createEffect(() => {
    if (!isOpen()) return;

    const handleKeydown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        setIsOpen(false);
      }
    };

    document.addEventListener("keydown", handleKeydown);
    onCleanup(() => document.removeEventListener("keydown", handleKeydown));
  });

  const handleOpenIn = async (ide: IdeType) => {
    if (!props.repoPath) return;

    settingsStore.setIde(ide);
    setIsOpen(false);

    try {
      await repo.openInApp(props.repoPath, ide);
      props.onOpenInIde?.(ide);
    } catch (err) {
      console.error("Failed to open in IDE:", err);
    }
  };

  const handleLaunchCurrent = async () => {
    if (!props.repoPath) return;

    try {
      await repo.openInApp(props.repoPath, currentIde());
      props.onOpenInIde?.(currentIde());
    } catch (err) {
      console.error("Failed to open in IDE:", err);
    }
  };

  const handleRun = (e: MouseEvent) => {
    setIsOpen(false);
    props.onRun?.(e.shiftKey);
  };

  const runLabel = () => {
    const cmd = props.runCommand;
    if (!cmd) return "Run...";
    const maxLen = 20;
    return cmd.length > maxLen ? `Run: ${cmd.slice(0, maxLen)}...` : `Run: ${cmd}`;
  };

  const currentIde = () => settingsStore.state.ide;

  return (
    <div class="ide-launcher" ref={dropdownRef}>
      <div class="ide-launcher-split">
        {/* Main button - launches current IDE */}
        <button
          class="ide-launcher-btn ide-launcher-main"
          onClick={handleLaunchCurrent}
          disabled={!props.repoPath}
          title={`Open in ${IDE_NAMES[currentIde()]}`}
        >
          <IdeIcon ide={currentIde()} />
          <span class="ide-launcher-name">{IDE_NAMES[currentIde()]}</span>
        </button>
        {/* Arrow button - opens dropdown */}
        <button
          class="ide-launcher-btn ide-launcher-arrow-btn"
          onClick={() => setIsOpen(!isOpen())}
          title="Choose editor"
        >
          <span class="ide-launcher-arrow">{isOpen() ? "▲" : "▼"}</span>
        </button>
      </div>

      <Show when={isOpen()}>
        <div class="ide-launcher-dropdown">
          <For each={categoryOrder}>
            {(cat) => {
              const items = () => filterInstalled(IDE_CATEGORIES[cat.key]);
              return (
                <Show when={items().length > 0}>
                  <Show when={cat.key !== "editors"}>
                    <div class="ide-launcher-divider" />
                  </Show>
                  <div class="ide-launcher-section">
                    <div class="ide-launcher-section-title">{cat.label}</div>
                    <For each={items()}>
                      {(ide) => (
                        <button
                          class={`ide-launcher-item ${currentIde() === ide ? "selected" : ""}`}
                          onClick={() => handleOpenIn(ide)}
                          disabled={!props.repoPath}
                        >
                          <IdeIcon ide={ide} />
                          <span class="ide-launcher-item-name">{IDE_NAMES[ide]}</span>
                          <Show when={currentIde() === ide}>
                            <span class="ide-launcher-item-check">✓</span>
                          </Show>
                        </button>
                      )}
                    </For>
                  </div>
                </Show>
              );
            }}
          </For>

          {/* Actions */}
          <div class="ide-launcher-divider" />
          <div class="ide-launcher-section">
            <button
              class="ide-launcher-item ide-launcher-action"
              onClick={handleRun}
              disabled={!props.repoPath}
            >
              <span class="ide-launcher-icon ide-launcher-icon-emoji">▶</span>
              <span class="ide-launcher-item-name">{runLabel()}</span>
              <span class="ide-launcher-shortcut">{getModifierSymbol()}R</span>
            </button>
          </div>
        </div>
      </Show>
    </div>
  );
};

export default IdeLauncher;
