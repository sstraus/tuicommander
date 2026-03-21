import { Component, createEffect, createSignal, Match, Show, Switch } from "solid-js";
import { PanelResizeHandle } from "../ui/PanelResizeHandle";
import { cx } from "../../utils";
import { ChangesTab } from "./ChangesTab";
import { LogTab } from "./LogTab";
import { StashesTab } from "./StashesTab";
import { BranchesTab } from "./BranchesTab";
import { HistoryTab } from "./HistoryTab";
import { BlameTab } from "./BlameTab";
import p from "../shared/panel.module.css";
import s from "./GitPanel.module.css";

type GitTab = "changes" | "log" | "stashes" | "branches";

const TABS: { id: GitTab; label: string }[] = [
  { id: "changes", label: "Changes" },
  { id: "log", label: "Log" },
  { id: "stashes", label: "Stashes" },
  { id: "branches", label: "Branches" },
];

export interface GitPanelProps {
  visible: boolean;
  repoPath: string | null;
  /** Effective filesystem root (worktree path when on a linked worktree) */
  fsRoot?: string | null;
  onClose: () => void;
  /** When set, switches to the given tab (used by external shortcuts like toggle-branches-tab) */
  requestedTab?: GitTab | null;
}

export const GitPanel: Component<GitPanelProps> = (props) => {
  const [activeTab, setActiveTab] = createSignal<GitTab>("changes");
  const [selectedFile, setSelectedFile] = createSignal<string | null>(null);
  const [historyExpanded, setHistoryExpanded] = createSignal(false);
  const [blameExpanded, setBlameExpanded] = createSignal(false);
  const gitPath = () => (props.fsRoot || props.repoPath) as string | null;

  // Switch to the requested tab when an external action (e.g. keyboard shortcut) specifies one
  createEffect(() => {
    const tab = props.requestedTab;
    if (tab) setActiveTab(tab);
  });

  function handlePanelKeyDown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      props.onClose();
      return;
    }

    const mod = e.metaKey || e.ctrlKey;
    if (mod && e.key >= "1" && e.key <= String(TABS.length)) {
      const idx = parseInt(e.key) - 1;
      if (idx < TABS.length) {
        e.preventDefault();
        e.stopPropagation();
        setActiveTab(TABS[idx].id);
      }
    }
  }

  /** Split path into basename for compact display */
  function basename(path: string): string {
    const i = path.lastIndexOf("/");
    return i === -1 ? path : path.slice(i + 1);
  }

  return (
    <div
      id="git-panel"
      class={cx(s.panel, !props.visible && s.hidden)}
      tabIndex={0}
      onKeyDown={handlePanelKeyDown}
    >
      <PanelResizeHandle panelId="git-panel" />
      <div class={p.header}>
        <div class={s.tabs}>
          {TABS.map((tab) => (
            <button
              class={cx(s.tab, activeTab() === tab.id && s.tabActive)}
              onClick={() => setActiveTab(tab.id)}
            >
              {tab.label}
            </button>
          ))}
        </div>
        <button class={p.close} onClick={props.onClose} title="Close">
          &times;
        </button>
      </div>
      {/* Main tab content */}
      <div class={s.tabContent}>
        <Switch>
          <Match when={activeTab() === "changes"}>
            <ChangesTab
              repoPath={props.visible ? gitPath() : null}
              storeRepoPath={props.visible ? props.repoPath : null}
              onFileSelect={setSelectedFile}
            />
          </Match>
          <Match when={activeTab() === "log"}>
            <LogTab repoPath={props.visible ? gitPath() : null} />
          </Match>
          <Match when={activeTab() === "stashes"}>
            <StashesTab repoPath={props.visible ? gitPath() : null} />
          </Match>
          <Match when={activeTab() === "branches"}>
            <BranchesTab repoPath={props.visible ? gitPath() : null} />
          </Match>
        </Switch>
      </div>
      {/* Sub-panels: History & Blame — only visible in Changes tab */}
      <Show when={activeTab() === "changes"}>
      <div class={s.subPanels}>
        <div
          class={s.subPanelHeader}
          onClick={() => setHistoryExpanded((v) => !v)}
        >
          <span class={cx(s.subChevron, !historyExpanded() && s.subChevronCollapsed)}>&#x25BC;</span>
          <span class={s.subPanelLabel}>History</span>
          <Show when={selectedFile()}>
            <span class={s.subPanelFile}>{basename(selectedFile()!)}</span>
          </Show>
        </div>
        <Show when={historyExpanded()}>
          <div class={s.subPanelBody}>
            <HistoryTab
              repoPath={props.visible ? gitPath() : null}
              filePath={selectedFile()}
            />
          </div>
        </Show>
        <div
          class={s.subPanelHeader}
          onClick={() => setBlameExpanded((v) => !v)}
        >
          <span class={cx(s.subChevron, !blameExpanded() && s.subChevronCollapsed)}>&#x25BC;</span>
          <span class={s.subPanelLabel}>Blame</span>
          <Show when={selectedFile()}>
            <span class={s.subPanelFile}>{basename(selectedFile()!)}</span>
          </Show>
        </div>
        <Show when={blameExpanded()}>
          <div class={s.subPanelBody}>
            <BlameTab
              repoPath={props.visible ? gitPath() : null}
              filePath={selectedFile()}
            />
          </div>
        </Show>
      </div>
      </Show>
    </div>
  );
};

export default GitPanel;
