import { Component, onMount } from "solid-js";
import { invoke } from "../invoke";
import { repositoriesStore } from "../stores/repositories";
import { terminalsStore } from "../stores/terminals";
import { createPanelSyncReceiver } from "../utils/panelSync";
import { initPanelWindow } from "../hooks/initPanelWindow";
import { NotesPanel } from "../components/NotesPanel";
import type { PanelAdapter } from "../panelRouter";
import { uiStore } from "../stores/ui";

const DetachedNotesPanel: Component<{ params: URLSearchParams }> = (props) => {
  const repoPath = props.params.get("repoPath");
  const { emitAction } = createPanelSyncReceiver<null>("notes");

  onMount(() => {
    void initPanelWindow();
  });

  return (
    <NotesPanel
      visible={true}
      repoPath={repoPath}
      mode="detached"
      onClose={() => window.close()}
      onSendToTerminal={(text) => {
        void emitAction("sendToTerminal", { text });
        void invoke("focus_main_window");
      }}
    />
  );
};

export const notesPanelAdapter: PanelAdapter = {
  id: "notes",
  title: "Notes",
  defaultSize: { width: 450, height: 600 },
  toggle: () => uiStore.toggleNotesPanel(),
  onDetach: () => uiStore.setNotesPanelVisible(false),
  detachParams: () => {
    const repoPath = repositoriesStore.state.activeRepoPath;
    return repoPath ? { repoPath } : {};
  },
  handleAction(action: string, data: unknown) {
    if (action === "sendToTerminal" && data) {
      const d = data as Record<string, unknown>;
      const text = d.text as string;
      const active = terminalsStore.getActive();
      if (active?.ref) {
        active.ref.write(`${text}\r`);
        requestAnimationFrame(() => active.ref?.focus());
      }
      void invoke("focus_main_window");
    }
  },
  Component: DetachedNotesPanel,
};
