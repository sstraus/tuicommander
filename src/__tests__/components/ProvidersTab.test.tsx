import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent } from "@solidjs/testing-library";
import { mockInvoke } from "../mocks/tauri";

const anthropic = {
  id: "anthropic-main",
  type: "anthropic" as const,
  label: "Anthropic",
  base_url: null,
};

const sonnet = {
  id: "model-sonnet",
  provider_id: "anthropic-main",
  model_name: "claude-sonnet-4-5",
  tier: "standard" as const,
};

const mockStore = vi.hoisted(() => ({
  state: {
    registry: {
      schema_version: 1,
      providers: [] as typeof anthropic[],
      models: [] as typeof sonnet[],
      slots: {} as Record<string, string>,
      features: {},
    },
    keyStatus: {} as Record<string, boolean>,
    loaded: true,
  },
  addProvider: vi.fn(),
  removeProvider: vi.fn(),
  addModel: vi.fn(),
  removeModel: vi.fn(),
  setSlot: vi.fn(),
  clearSlot: vi.fn(),
  saveKey: vi.fn(),
  deleteKey: vi.fn(),
  resolveSlot: vi.fn(() => null),
  _reset: vi.fn(),
}));

vi.mock("../../stores/providerRegistry", () => ({
  providerRegistryStore: mockStore,
}));

import { ProvidersTab } from "../../components/SettingsPanel/tabs/ProvidersTab";

describe("ProvidersTab", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockStore.state.registry.providers = [];
    mockStore.state.registry.models = [];
    mockStore.state.registry.slots = {};
    mockStore.state.keyStatus = {};
    mockInvoke.mockResolvedValue(undefined);
  });

  // -- Provider list --

  it("renders empty state when no providers", () => {
    const { getByText } = render(() => <ProvidersTab />);
    expect(getByText(/No providers configured/)).toBeTruthy();
  });

  it("renders provider cards for each provider", () => {
    mockStore.state.registry.providers = [anthropic];
    const { getByTestId } = render(() => <ProvidersTab />);
    expect(getByTestId("provider-card-anthropic-main")).toBeTruthy();
  });

  it("shows provider label and type", () => {
    mockStore.state.registry.providers = [anthropic];
    const { getByText } = render(() => <ProvidersTab />);
    expect(getByText("Anthropic")).toBeTruthy();
  });

  it("shows model count", () => {
    mockStore.state.registry.providers = [anthropic];
    mockStore.state.registry.models = [sonnet];
    const { getByText } = render(() => <ProvidersTab />);
    expect(getByText(/Models \(1\)/)).toBeTruthy();
  });

  it("shows key status indicator", () => {
    mockStore.state.registry.providers = [anthropic];
    mockStore.state.keyStatus = { "anthropic-main": true };
    const { getByTestId } = render(() => <ProvidersTab />);
    expect(getByTestId("key-status-anthropic-main").textContent).toContain("✓ key");
  });

  it("shows 'no key' when key missing", () => {
    mockStore.state.registry.providers = [anthropic];
    mockStore.state.keyStatus = { "anthropic-main": false };
    const { getByTestId } = render(() => <ProvidersTab />);
    expect(getByTestId("key-status-anthropic-main").textContent).toContain("no key");
  });

  // -- Remove provider --

  it("calls removeProvider when × clicked", () => {
    mockStore.state.registry.providers = [anthropic];
    const { getByTestId } = render(() => <ProvidersTab />);
    fireEvent.click(getByTestId("remove-provider-anthropic-main"));
    expect(mockStore.removeProvider).toHaveBeenCalledWith("anthropic-main");
  });

  // -- Add provider form --

  it("shows add provider form when + Add clicked", () => {
    const { getByTestId, getByText } = render(() => <ProvidersTab />);
    fireEvent.click(getByTestId("add-provider-btn"));
    expect(getByTestId("add-provider-form")).toBeTruthy();
    expect(getByText("Add Provider")).toBeTruthy();
  });

  it("cancels add provider form", () => {
    const { getByTestId, queryByTestId, getByText } = render(() => <ProvidersTab />);
    fireEvent.click(getByTestId("add-provider-btn"));
    fireEvent.click(getByText("Cancel"));
    expect(queryByTestId("add-provider-form")).toBeNull();
  });

  // -- Model CRUD --

  it("renders model entries", () => {
    mockStore.state.registry.providers = [anthropic];
    mockStore.state.registry.models = [sonnet];
    const { getByTestId } = render(() => <ProvidersTab />);
    expect(getByTestId("model-entry-model-sonnet")).toBeTruthy();
  });

  it("calls removeModel when model × clicked", () => {
    mockStore.state.registry.providers = [anthropic];
    mockStore.state.registry.models = [sonnet];
    const { getByTestId } = render(() => <ProvidersTab />);
    fireEvent.click(getByTestId("remove-model-model-sonnet"));
    expect(mockStore.removeModel).toHaveBeenCalledWith("model-sonnet");
  });

  it("shows add model form when + Add model clicked", () => {
    mockStore.state.registry.providers = [anthropic];
    const { getByTestId } = render(() => <ProvidersTab />);
    fireEvent.click(getByTestId("add-model-btn-anthropic-main"));
    expect(getByTestId("add-model-form")).toBeTruthy();
  });

  // -- Slot assignments --

  it("renders slot assignment section", () => {
    const { getByTestId } = render(() => <ProvidersTab />);
    expect(getByTestId("slot-assignments")).toBeTruthy();
  });

  it("renders all 3 slot rows", () => {
    const { getByTestId } = render(() => <ProvidersTab />);
    for (const slot of ["main", "triage", "headless"]) {
      expect(getByTestId(`slot-row-${slot}`)).toBeTruthy();
    }
    // headless slot-select is only shown when External API is active
    for (const slot of ["main", "triage"]) {
      expect(getByTestId(`slot-select-${slot}`)).toBeTruthy();
    }
  });

  it("calls setSlot when slot dropdown changes", () => {
    mockStore.state.registry.providers = [anthropic];
    mockStore.state.registry.models = [sonnet];
    const { getByTestId } = render(() => <ProvidersTab />);
    fireEvent.change(getByTestId("slot-select-main"), { target: { value: "model-sonnet" } });
    expect(mockStore.setSlot).toHaveBeenCalledWith("main", "model-sonnet");
  });

  it("calls clearSlot when empty option selected", () => {
    mockStore.state.registry.providers = [anthropic];
    mockStore.state.registry.models = [sonnet];
    mockStore.state.registry.slots = { main: "model-sonnet" };
    const { getByTestId } = render(() => <ProvidersTab />);
    fireEvent.change(getByTestId("slot-select-main"), { target: { value: "" } });
    expect(mockStore.clearSlot).toHaveBeenCalledWith("main");
  });

  it("shows test button when slot is configured", () => {
    mockStore.state.registry.providers = [anthropic];
    mockStore.state.registry.models = [sonnet];
    mockStore.state.registry.slots = { main: "model-sonnet" };
    const { getByTestId } = render(() => <ProvidersTab />);
    expect(getByTestId("test-slot-main")).toBeTruthy();
  });
});
