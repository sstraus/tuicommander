import { repoSettingsStore } from "../stores/repoSettings";
import { repositoriesStore } from "../stores/repositories";

/** Color inheritance: repo color > group color > undefined */
export function getRepoColor(repoPath: string): string | undefined {
  return repoSettingsStore.get(repoPath)?.color
    || repositoriesStore.getGroupForRepo(repoPath)?.color
    || undefined;
}
