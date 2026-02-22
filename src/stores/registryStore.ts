import { createStore } from "solid-js/store";
import { invoke } from "../invoke";

/** A plugin entry from the remote registry */
export interface RegistryEntry {
  id: string;
  name: string;
  description: string;
  author: string;
  repo: string;
  latestVersion: string;
  minAppVersion: string;
  capabilities: string[];
  downloadUrl: string;
}

interface RegistryStoreState {
  entries: RegistryEntry[];
  loading: boolean;
  error: string | null;
  lastFetched: number | null;
}

const CACHE_TTL_MS = 60 * 60 * 1000; // 1 hour

const [state, setState] = createStore<RegistryStoreState>({
  entries: [],
  loading: false,
  error: null,
  lastFetched: null,
});

async function fetch(): Promise<void> {
  // Skip if recently fetched (within TTL)
  if (state.lastFetched && Date.now() - state.lastFetched < CACHE_TTL_MS) return;
  if (state.loading) return;

  setState({ loading: true, error: null });
  try {
    const entries = await invoke<RegistryEntry[]>("fetch_plugin_registry");
    setState({ entries, loading: false, lastFetched: Date.now() });
  } catch (err) {
    setState({ loading: false, error: String(err) });
  }
}

/** Force a fresh fetch, ignoring TTL cache */
async function refresh(): Promise<void> {
  setState({ lastFetched: null });
  await fetch();
}

/** Check if a plugin has an update available */
function hasUpdate(installedId: string, installedVersion: string): RegistryEntry | null {
  const entry = state.entries.find((e) => e.id === installedId);
  if (!entry) return null;
  // Simple string comparison — semver-aware comparison could be added later
  return entry.latestVersion !== installedVersion ? entry : null;
}

export const registryStore = {
  state,
  fetch,
  refresh,
  hasUpdate,
};
