import { createStore, produce } from "solid-js/store";
import { invoke } from "../invoke";
import { appLogger } from "./appLogger";

// ---------------------------------------------------------------------------
// Types (mirror Rust serialized names — snake_case)
// ---------------------------------------------------------------------------

export type ProviderType =
  | "anthropic" | "open_ai" | "gemini" | "deep_seek" | "mistral"
  | "fireworks" | "samba_nova" | "moonshot" | "xai" | "zai"
  | "open_router" | "requesty" | "lite_llm"
  | "ollama" | "lm_studio" | "bedrock" | "vertex" | "custom";

export type ModelTier = "economic" | "standard" | "premium";

export type SlotName = "main" | "triage" | "headless";

export interface ProviderEntry {
  id: string;
  type: ProviderType;
  label: string;
  base_url?: string | null;
}

export interface ModelEntry {
  id: string;
  provider_id: string;
  model_name: string;
  tier: ModelTier;
}

export interface Features {}

export interface ProviderRegistry {
  schema_version: number;
  providers: ProviderEntry[];
  models: ModelEntry[];
  slots: Record<string, string>;
  features: Features;
}

interface StoreState {
  registry: ProviderRegistry;
  keyStatus: Record<string, boolean>;
  loaded: boolean;
}

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

function createProviderRegistryStore() {
  const [state, setState] = createStore<StoreState>({
    registry: {
      schema_version: 1,
      providers: [],
      models: [],
      slots: {},
      features: {},
    },
    keyStatus: {},
    loaded: false,
  });

  async function hydrate(): Promise<void> {
    const registry = await invoke<ProviderRegistry>("load_provider_registry");
    const keyStatus: Record<string, boolean> = {};
    await Promise.all(
      registry.providers.map(async (p) => {
        try {
          keyStatus[p.id] = await invoke<boolean>("get_provider_api_key_exists", {
            providerId: p.id,
          });
        } catch {
          keyStatus[p.id] = false;
        }
      })
    );
    setState({ registry, keyStatus, loaded: true });
  }

  async function save(): Promise<void> {
    await invoke("save_provider_registry", { registry: state.registry });
  }

  async function saveKey(providerId: string, key: string): Promise<void> {
    await invoke("save_provider_api_key", { providerId, key });
    setState("keyStatus", providerId, true);
  }

  async function deleteKey(providerId: string): Promise<void> {
    await invoke("delete_provider_api_key", { providerId });
    setState("keyStatus", providerId, false);
  }

  function addProvider(entry: ProviderEntry): void {
    setState(
      produce((s) => {
        s.registry.providers.push(entry);
      })
    );
    void save();
  }

  function removeProvider(id: string): void {
    setState(
      produce((s) => {
        const modelIds = s.registry.models
          .filter((m) => m.provider_id === id)
          .map((m) => m.id);
        // Remove models belonging to this provider
        s.registry.models = s.registry.models.filter((m) => m.provider_id !== id);
        // Clear slots that point to any of those models
        for (const [slot, modelId] of Object.entries(s.registry.slots)) {
          if (modelIds.includes(modelId)) {
            delete s.registry.slots[slot];
          }
        }
        // Remove provider
        s.registry.providers = s.registry.providers.filter((p) => p.id !== id);
        delete s.keyStatus[id];
      })
    );
    void invoke("delete_provider_api_key", { providerId: id }).catch((e: unknown) => {
      appLogger.warn("settings", `Failed to delete API key for provider ${id}: ${String(e)}`);
    });
    void save();
  }

  function addModel(entry: ModelEntry): void {
    setState(
      produce((s) => {
        s.registry.models.push(entry);
      })
    );
    void save();
  }

  function removeModel(id: string): void {
    setState(
      produce((s) => {
        s.registry.models = s.registry.models.filter((m) => m.id !== id);
        for (const [slot, modelId] of Object.entries(s.registry.slots)) {
          if (modelId === id) {
            delete s.registry.slots[slot];
          }
        }
      })
    );
    void save();
  }

  function setSlot(slot: SlotName, modelId: string): void {
    setState("registry", "slots", slot, modelId);
    void save();
  }

  function clearSlot(slot: SlotName): void {
    setState(
      produce((s) => {
        delete s.registry.slots[slot];
      })
    );
    void save();
  }

  function resolveSlot(
    slot: SlotName
  ): { provider: ProviderEntry; model: ModelEntry } | null {
    const modelId = state.registry.slots[slot];
    if (!modelId) return null;
    const model = state.registry.models.find((m) => m.id === modelId);
    if (!model) return null;
    const provider = state.registry.providers.find(
      (p) => p.id === model.provider_id
    );
    if (!provider) return null;
    return { provider, model };
  }

  function _reset(): void {
    setState(
      produce((s) => {
        s.registry = {
          schema_version: 1,
          providers: [],
          models: [],
          slots: {},
          features: {},
        };
        s.keyStatus = {};
        s.loaded = false;
      })
    );
  }

  return {
    state,
    hydrate,
    save,
    saveKey,
    deleteKey,
    addProvider,
    removeProvider,
    addModel,
    removeModel,
    setSlot,
    clearSlot,
    resolveSlot,
    _reset,
  };
}

export const providerRegistryStore = createProviderRegistryStore();
