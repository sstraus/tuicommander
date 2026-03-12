import { Component, createSignal, Match, Switch } from "solid-js";
import { PanelResizeHandle } from "../ui/PanelResizeHandle";
import { cx } from "../../utils";
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

  return (
    <div id="git-panel" class={cx(s.panel, !props.visible && s.hidden)}>
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
            <div class={s.placeholder}>Changes tab placeholder</div>
          </Match>
          <Match when={activeTab() === "log"}>
            <div class={s.placeholder}>Log tab placeholder</div>
          </Match>
          <Match when={activeTab() === "stashes"}>
            <div class={s.placeholder}>Stashes tab placeholder</div>
          </Match>
          <Match when={activeTab() === "history"}>
            <div class={s.placeholder}>History tab placeholder</div>
          </Match>
          <Match when={activeTab() === "blame"}>
            <div class={s.placeholder}>Blame tab placeholder</div>
          </Match>
        </Switch>
      </div>
    </div>
  );
};

export default GitPanel;
