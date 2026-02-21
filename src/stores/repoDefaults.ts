import { createStore } from "solid-js/store";
import { invoke } from "../invoke";

/** Global defaults applied to all repos unless overridden per-repo */
export interface RepoDefaults {
  baseBranch: string;
  copyIgnoredFiles: boolean;
  copyUntrackedFiles: boolean;
  setupScript: string;
  runScript: string;
}

const INITIAL_DEFAULTS: RepoDefaults = {
  baseBranch: "automatic",
  copyIgnoredFiles: false,
  copyUntrackedFiles: false,
  setupScript: "",
  runScript: "",
};

function createRepoDefaultsStore() {
  const [state, setState] = createStore<RepoDefaults>({ ...INITIAL_DEFAULTS });

  function save(): void {
    invoke("save_repo_defaults", {
      config: {
        base_branch: state.baseBranch,
        copy_ignored_files: state.copyIgnoredFiles,
        copy_untracked_files: state.copyUntrackedFiles,
        setup_script: state.setupScript,
        run_script: state.runScript,
      },
    }).catch((err) => console.error("Failed to save repo defaults:", err));
  }

  return {
    state,

    async hydrate(): Promise<void> {
      try {
        const loaded = await invoke<{
          base_branch?: string;
          copy_ignored_files?: boolean;
          copy_untracked_files?: boolean;
          setup_script?: string;
          run_script?: string;
        } | null>("load_repo_defaults");
        if (loaded) {
          setState({
            baseBranch: loaded.base_branch ?? INITIAL_DEFAULTS.baseBranch,
            copyIgnoredFiles: loaded.copy_ignored_files ?? INITIAL_DEFAULTS.copyIgnoredFiles,
            copyUntrackedFiles: loaded.copy_untracked_files ?? INITIAL_DEFAULTS.copyUntrackedFiles,
            setupScript: loaded.setup_script ?? INITIAL_DEFAULTS.setupScript,
            runScript: loaded.run_script ?? INITIAL_DEFAULTS.runScript,
          });
        }
      } catch (err) {
        console.debug("Failed to hydrate repo defaults:", err);
      }
    },

    setBaseBranch(value: string): void {
      setState("baseBranch", value);
      save();
    },

    setCopyIgnoredFiles(value: boolean): void {
      setState("copyIgnoredFiles", value);
      save();
    },

    setCopyUntrackedFiles(value: boolean): void {
      setState("copyUntrackedFiles", value);
      save();
    },

    setSetupScript(value: string): void {
      setState("setupScript", value);
      save();
    },

    setRunScript(value: string): void {
      setState("runScript", value);
      save();
    },
  };
}

export const repoDefaultsStore = createRepoDefaultsStore();
