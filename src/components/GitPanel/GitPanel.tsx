import { Component, createSignal, Match, Switch } from "solid-js";
import { PanelResizeHandle } from "../ui/PanelResizeHandle";
import { cx } from "../../utils";
import { ChangesTab } from "./ChangesTab";
import { LogTab } from "./LogTab";
import { StashesTab } from "./StashesTab";
import { HistoryTab } from "./HistoryTab";
import { BlameTab } from "./BlameTab";
import p from "../shared/panel.module.css";
import s from "./GitPanel.module.css";

type GitTab = "changes" | "log" | "stashes" | "history" | "blame";

const TABS: { id: GitTab; label: string }[] = [
  { id: "changes", label: "Changes" },
  { id: "log", label: "Log" },
  { id: "stashes", label: "Stashes" },
  { id: "history", label: "History" },
  { id: "blame", label: "Blame" },
];

export interface GitPanelProps {
  visible: boolean;
  repoPath: string | null;
  onClose: () => void;
}

export const GitPanel: Component<GitPanelProps> = (props) => {
  const [activeTab, setActiveTab] = createSignal<GitTab>("changes");
  const [historyFile, _setHistoryFile] = createSignal<string | null>(null);
  const [blameFile, _setBlameFile] = createSignal<string | null>(null);

  function handlePanelKeyDown(e: KeyboardEvent) {
    // Escape closes the panel
    if (e.key === "Escape") {
      e.preventDefault();
      props.onClose();
      return;
    }

    // Ctrl+1-5 (or Cmd+1-5) switches tabs within the panel
    const mod = e.metaKey || e.ctrlKey;
    if (mod && e.key >= "1" && e.key <= "5") {
      const idx = parseInt(e.key) - 1;
      if (idx < TABS.length) {
        e.preventDefault();
        e.stopPropagation();
        setActiveTab(TABS[idx].id);
      }
    }
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
      <div class={p.content}>
        <Switch>
          <Match when={activeTab() === "changes"}>
            <ChangesTab repoPath={props.repoPath} />
          </Match>
          <Match when={activeTab() === "log"}>
            <LogTab repoPath={props.repoPath} />
          </Match>
          <Match when={activeTab() === "stashes"}>
            <StashesTab repoPath={props.repoPath} />
          </Match>
          <Match when={activeTab() === "history"}>
            <HistoryTab repoPath={props.repoPath} filePath={historyFile()} />
          </Match>
          <Match when={activeTab() === "blame"}>
            <BlameTab repoPath={props.repoPath} filePath={blameFile()} />
          </Match>
        </Switch>
      </div>
    </div>
  );
};

export default GitPanel;
