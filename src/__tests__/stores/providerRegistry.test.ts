import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("../../invoke", () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
}));

import { invoke } from "../../invoke";
import { providerRegistryStore } from "../../stores/providerRegistry";
import type { ProviderEntry, ModelEntry, ProviderRegistry } from "../../stores/providerRegistry";

const mockInvoke = invoke as ReturnType<typeof vi.fn>;

const emptyRegistry: ProviderRegistry = {
  schema_version: 1,
  providers: [],
  models: [],
  slots: {},
  features: { enrichment_enabled: false },
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
    providerRegistryStore.setSlot("chat", sonnet.id);

    providerRegistryStore.removeProvider("anthropic-main");

    expect(providerRegistryStore.state.registry.providers).toHaveLength(0);
    expect(providerRegistryStore.state.registry.models).toHaveLength(0);
    expect(providerRegistryStore.state.registry.slots["chat"]).toBeUndefined();
    expect(providerRegistryStore.state.keyStatus["anthropic-main"]).toBeUndefined();
  });

  it("removeProvider does not affect unrelated providers", () => {
    const other: ProviderEntry = { id: "openai-main", type: "open_ai", label: "OpenAI" };
    const otherModel: ModelEntry = { id: "model-gpt4", provider_id: "openai-main", model_name: "gpt-4o", tier: "premium" };

    providerRegistryStore.addProvider(anthropic);
    providerRegistryStore.addProvider(other);
    providerRegistryStore.addModel(sonnet);
    providerRegistryStore.addModel(otherModel);
    providerRegistryStore.setSlot("chat", sonnet.id);
    providerRegistryStore.setSlot("headless", otherModel.id);

    providerRegistryStore.removeProvider("anthropic-main");

    expect(providerRegistryStore.state.registry.providers).toHaveLength(1);
    expect(providerRegistryStore.state.registry.providers[0].id).toBe("openai-main");
    expect(providerRegistryStore.state.registry.models).toHaveLength(1);
    expect(providerRegistryStore.state.registry.slots["headless"]).toBe("model-gpt4");
    expect(providerRegistryStore.state.registry.slots["chat"]).toBeUndefined();
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
    providerRegistryStore.setSlot("chat", sonnet.id);
    providerRegistryStore.setSlot("agent_mid", haiku.id);

    providerRegistryStore.removeModel(sonnet.id);

    expect(providerRegistryStore.state.registry.models).toHaveLength(1);
    expect(providerRegistryStore.state.registry.slots["chat"]).toBeUndefined();
    expect(providerRegistryStore.state.registry.slots["agent_mid"]).toBe(haiku.id);
  });

  // -- slots --

  it("setSlot assigns a model to a slot", () => {
    providerRegistryStore.addProvider(anthropic);
    providerRegistryStore.addModel(sonnet);
    providerRegistryStore.setSlot("chat", sonnet.id);
    expect(providerRegistryStore.state.registry.slots["chat"]).toBe(sonnet.id);
  });

  it("clearSlot removes a slot", () => {
    providerRegistryStore.addProvider(anthropic);
    providerRegistryStore.addModel(sonnet);
    providerRegistryStore.setSlot("chat", sonnet.id);
    providerRegistryStore.clearSlot("chat");
    expect(providerRegistryStore.state.registry.slots["chat"]).toBeUndefined();
  });

  // -- resolveSlot --

  it("resolveSlot returns provider and model for a configured slot", () => {
    providerRegistryStore.addProvider(anthropic);
    providerRegistryStore.addModel(sonnet);
    providerRegistryStore.setSlot("chat", sonnet.id);

    const resolved = providerRegistryStore.resolveSlot("chat");
    expect(resolved).not.toBeNull();
    expect(resolved!.provider.id).toBe("anthropic-main");
    expect(resolved!.model.id).toBe(sonnet.id);
  });

  it("resolveSlot returns null for unconfigured slot", () => {
    expect(providerRegistryStore.resolveSlot("chat")).toBeNull();
  });

  it("resolveSlot falls back from agent_low to agent_mid", () => {
    providerRegistryStore.addProvider(anthropic);
    providerRegistryStore.addModel(sonnet);
    providerRegistryStore.setSlot("agent_mid", sonnet.id);

    const resolved = providerRegistryStore.resolveSlot("agent_low");
    expect(resolved).not.toBeNull();
    expect(resolved!.model.id).toBe(sonnet.id);
  });

  it("resolveSlot uses agent_low over agent_mid when both set", () => {
    providerRegistryStore.addProvider(anthropic);
    providerRegistryStore.addModel(sonnet);
    providerRegistryStore.addModel(haiku);
    providerRegistryStore.setSlot("agent_mid", sonnet.id);
    providerRegistryStore.setSlot("agent_low", haiku.id);

    const resolved = providerRegistryStore.resolveSlot("agent_low");
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
