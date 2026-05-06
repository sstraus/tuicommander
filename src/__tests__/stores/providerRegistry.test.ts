import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("../../invoke", () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
}));

import { invoke } from "../../invoke";
import { providerRegistryStore } from "../../stores/providerRegistry";
import type { ProviderEntry, ModelEntry, ProviderRegistry } from "../../stores/providerRegistry";

const mockInvoke = invoke as ReturnType<typeof vi.fn>;

const emptyRegistry: ProviderRegistry = {
  schema_version: 3,
  providers: [],
  models: [],
  slots: {},
  features: {},
};

const anthropic: ProviderEntry = {
  id: "anthropic-main",
  type: "anthropic",
  label: "Anthropic",
};

const sonnet: ModelEntry = {
  id: "model-sonnet",
  provider_id: "anthropic-main",
  model_name: "claude-sonnet-4-5",
  tier: "standard",
};

const haiku: ModelEntry = {
  id: "model-haiku",
  provider_id: "anthropic-main",
  model_name: "claude-haiku-4-5",
  tier: "economic",
};

function resetStore() {
  providerRegistryStore._reset();
}

describe("providerRegistryStore", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetStore();
  });

  // -- hydrate --

  it("hydrate loads registry and key status", async () => {
    const registry: ProviderRegistry = {
      ...emptyRegistry,
      providers: [anthropic],
    };
    mockInvoke
      .mockResolvedValueOnce(registry) // load_provider_registry
      .mockResolvedValueOnce(true);    // get_provider_api_key_exists

    await providerRegistryStore.hydrate();

    expect(providerRegistryStore.state.loaded).toBe(true);
    expect(providerRegistryStore.state.registry.providers).toHaveLength(1);
    expect(providerRegistryStore.state.keyStatus["anthropic-main"]).toBe(true);
  });

  it("hydrate handles key check failure gracefully", async () => {
    const registry: ProviderRegistry = {
      ...emptyRegistry,
      providers: [anthropic],
    };
    mockInvoke
      .mockResolvedValueOnce(registry)
      .mockRejectedValueOnce(new Error("keyring error"));

    await providerRegistryStore.hydrate();

    expect(providerRegistryStore.state.keyStatus["anthropic-main"]).toBe(false);
  });

  // -- addProvider / removeProvider --

  it("addProvider appends and saves", () => {
    providerRegistryStore.addProvider(anthropic);
    expect(providerRegistryStore.state.registry.providers).toHaveLength(1);
    expect(providerRegistryStore.state.registry.providers[0].id).toBe("anthropic-main");
  });

  it("removeProvider removes provider, cascades models and slots, deletes key", () => {
    providerRegistryStore.addProvider(anthropic);
    providerRegistryStore.addModel(sonnet);
    providerRegistryStore.setSlot("main", sonnet.id);

    providerRegistryStore.removeProvider("anthropic-main");

    expect(providerRegistryStore.state.registry.providers).toHaveLength(0);
    expect(providerRegistryStore.state.registry.models).toHaveLength(0);
    expect(providerRegistryStore.state.registry.slots["main"]).toBeUndefined();
    expect(providerRegistryStore.state.keyStatus["anthropic-main"]).toBeUndefined();
  });

  it("removeProvider does not affect unrelated providers", () => {
    const other: ProviderEntry = { id: "openai-main", type: "open_ai", label: "OpenAI" };
    const otherModel: ModelEntry = { id: "model-gpt4", provider_id: "openai-main", model_name: "gpt-4o", tier: "premium" };

    providerRegistryStore.addProvider(anthropic);
    providerRegistryStore.addProvider(other);
    providerRegistryStore.addModel(sonnet);
    providerRegistryStore.addModel(otherModel);
    providerRegistryStore.setSlot("main", sonnet.id);
    providerRegistryStore.setSlot("headless", otherModel.id);

    providerRegistryStore.removeProvider("anthropic-main");

    expect(providerRegistryStore.state.registry.providers).toHaveLength(1);
    expect(providerRegistryStore.state.registry.providers[0].id).toBe("openai-main");
    expect(providerRegistryStore.state.registry.models).toHaveLength(1);
    expect(providerRegistryStore.state.registry.slots["headless"]).toBe("model-gpt4");
    expect(providerRegistryStore.state.registry.slots["main"]).toBeUndefined();
  });

  // -- addModel / removeModel --

  it("addModel appends and saves", () => {
    providerRegistryStore.addProvider(anthropic);
    providerRegistryStore.addModel(sonnet);
    expect(providerRegistryStore.state.registry.models).toHaveLength(1);
  });

  it("removeModel removes model and clears slots referencing it", () => {
    providerRegistryStore.addProvider(anthropic);
    providerRegistryStore.addModel(sonnet);
    providerRegistryStore.addModel(haiku);
    providerRegistryStore.setSlot("main", sonnet.id);
    providerRegistryStore.setSlot("triage", haiku.id);

    providerRegistryStore.removeModel(sonnet.id);

    expect(providerRegistryStore.state.registry.models).toHaveLength(1);
    expect(providerRegistryStore.state.registry.slots["main"]).toBeUndefined();
    expect(providerRegistryStore.state.registry.slots["triage"]).toBe(haiku.id);
  });

  // -- slots --

  it("setSlot assigns a model to a slot", () => {
    providerRegistryStore.addProvider(anthropic);
    providerRegistryStore.addModel(sonnet);
    providerRegistryStore.setSlot("main", sonnet.id);
    expect(providerRegistryStore.state.registry.slots["main"]).toBe(sonnet.id);
  });

  it("clearSlot removes a slot", () => {
    providerRegistryStore.addProvider(anthropic);
    providerRegistryStore.addModel(sonnet);
    providerRegistryStore.setSlot("main", sonnet.id);
    providerRegistryStore.clearSlot("main");
    expect(providerRegistryStore.state.registry.slots["main"]).toBeUndefined();
  });

  // -- resolveSlot --

  it("resolveSlot returns provider and model for a configured slot", () => {
    providerRegistryStore.addProvider(anthropic);
    providerRegistryStore.addModel(sonnet);
    providerRegistryStore.setSlot("main", sonnet.id);

    const resolved = providerRegistryStore.resolveSlot("main");
    expect(resolved).not.toBeNull();
    expect(resolved!.provider.id).toBe("anthropic-main");
    expect(resolved!.model.id).toBe(sonnet.id);
  });

  it("resolveSlot returns null for unconfigured slot", () => {
    expect(providerRegistryStore.resolveSlot("main")).toBeNull();
  });

  it("resolveSlot resolves triage slot independently", () => {
    providerRegistryStore.addProvider(anthropic);
    providerRegistryStore.addModel(sonnet);
    providerRegistryStore.addModel(haiku);
    providerRegistryStore.setSlot("main", sonnet.id);
    providerRegistryStore.setSlot("triage", haiku.id);

    const resolved = providerRegistryStore.resolveSlot("triage");
    expect(resolved).not.toBeNull();
    expect(resolved!.model.id).toBe(haiku.id);
  });

  it("resolveSlot resolves headless slot independently", () => {
    providerRegistryStore.addProvider(anthropic);
    providerRegistryStore.addModel(haiku);
    providerRegistryStore.setSlot("headless", haiku.id);

    const resolved = providerRegistryStore.resolveSlot("headless");
    expect(resolved!.model.id).toBe(haiku.id);
  });

  // -- key management --

  it("saveKey updates keyStatus", async () => {
    mockInvoke.mockResolvedValueOnce(undefined); // save_provider_api_key
    await providerRegistryStore.saveKey("anthropic-main", "sk-ant-test");
    expect(providerRegistryStore.state.keyStatus["anthropic-main"]).toBe(true);
  });

  it("deleteKey clears keyStatus", async () => {
    mockInvoke.mockResolvedValueOnce(undefined); // save_provider_api_key
    await providerRegistryStore.saveKey("anthropic-main", "sk-ant-test");
    mockInvoke.mockResolvedValueOnce(undefined); // delete_provider_api_key
    await providerRegistryStore.deleteKey("anthropic-main");
    expect(providerRegistryStore.state.keyStatus["anthropic-main"]).toBe(false);
  });
});
