import { createSignal } from "solid-js";
import { terminalsStore } from "../stores/terminals";
import { escapeShellArg, isValidPath } from "../utils";

/** Dependencies injected into useAppLazygit */
export interface AppLazygitDeps {
  pty: {
    close: (sessionId: string) => Promise<void>;
  };
  getCurrentRepoPath: () => string | undefined;
  getDefaultFontSize: () => number;
}

/** Lazygit integration: spawn inline, split pane, floating window */
export function useAppLazygit(deps: AppLazygitDeps) {
  const [lazygitAvailable, setLazygitAvailable] = createSignal(false);
  const [lazygitPaneVisible, setLazygitPaneVisible] = createSignal(false);
  const [lazygitFloating, setLazygitFloating] = createSignal(false);
  const [lazygitTermId, setLazygitTermId] = createSignal<string | null>(null);

  const buildLazygitCmd = (repoPath: string): string => {
    const base = `lazygit -p ${escapeShellArg(repoPath)}`;
    const configCheck = `test -f ${escapeShellArg(repoPath + "/.lazygit.yml")} && echo yml || (test -f ${escapeShellArg(repoPath + "/.lazygit.yaml")} && echo yaml || echo none)`;
    return `cfg=$(${configCheck}); if [ "$cfg" = "yml" ]; then lazygit -p ${escapeShellArg(repoPath)} --use-config-file ${escapeShellArg(repoPath + "/.lazygit.yml")}; elif [ "$cfg" = "yaml" ]; then lazygit -p ${escapeShellArg(repoPath)} --use-config-file ${escapeShellArg(repoPath + "/.lazygit.yaml")}; else ${base}; fi`;
  };

  const spawnLazygit = () => {
    const active = terminalsStore.getActive();
    if (!active?.ref) return;

    // Set tab name before writing — the complex shell one-liner would pollute the OSC title
    terminalsStore.update(active.id, { name: "lazygit", nameIsCustom: true });

    const repoPath = deps.getCurrentRepoPath();
    if (repoPath && isValidPath(repoPath)) {
      active.ref.write(`${buildLazygitCmd(repoPath)}\r`);
    } else {
      active.ref.write("lazygit\r");
    }
  };

  const openLazygitPane = async () => {
    const repoPath = deps.getCurrentRepoPath();
    if (!repoPath) return;

    // Close existing lazygit pane terminal if any
    const existingId = lazygitTermId();
    if (existingId) {
      const existing = terminalsStore.get(existingId);
      if (existing?.sessionId) {
        try { await deps.pty.close(existing.sessionId); } catch { /* ignore */ }
      }
      terminalsStore.remove(existingId);
    }

    const id = terminalsStore.add({
      sessionId: null,
      fontSize: deps.getDefaultFontSize(),
      name: "lazygit",
      cwd: repoPath,
      awaitingInput: null,
    });
    setLazygitTermId(id);
    terminalsStore.setActive(id);
    setLazygitPaneVisible(true);
  };

  const closeLazygitPane = async () => {
    const id = lazygitTermId();
    if (id) {
      const t = terminalsStore.get(id);
      if (t?.sessionId) {
        try { await deps.pty.close(t.sessionId); } catch { /* ignore */ }
      }
      terminalsStore.remove(id);
      setLazygitTermId(null);
    }
    setLazygitPaneVisible(false);
  };

  return {
    lazygitAvailable,
    setLazygitAvailable,
    lazygitPaneVisible,
    setLazygitPaneVisible,
    lazygitFloating,
    setLazygitFloating,
    lazygitTermId,
    buildLazygitCmd,
    spawnLazygit,
    openLazygitPane,
    closeLazygitPane,
  };
}
