import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render } from "@solidjs/testing-library";
import type { PluginPanelTab } from "../../stores/mdTabs";

// Mock pluginRegistry to avoid Tauri calls
vi.mock("../../plugins/pluginRegistry", () => ({
  pluginRegistry: {
    handlePanelMessage: vi.fn(),
    registerPanelSendChannel: vi.fn(),
    unregisterPanelSendChannel: vi.fn(),
  },
}));

// Mock Tauri APIs
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(vi.fn()),
  emit: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: vi.fn(() => ({
    listen: vi.fn().mockResolvedValue(vi.fn()),
  })),
}));

// Track whether addEventListener("message") was called inside an onMount callback.
// We wrap solid-js onMount to set a flag during its execution.
let insideOnMount = false;
let messageListenerCalledInsideOnMount: boolean | null = null;

vi.mock("solid-js", async (importOriginal) => {
  const actual = await importOriginal<typeof import("solid-js")>();
  return {
    ...actual,
    onMount: (fn: () => void) => {
      return actual.onMount(() => {
        insideOnMount = true;
        fn();
        insideOnMount = false;
      });
    },
  };
});

import { PluginPanel } from "../../components/PluginPanel/PluginPanel";
import { pluginRegistry } from "../../plugins/pluginRegistry";

function makeTab(overrides: Partial<PluginPanelTab> = {}): PluginPanelTab {
  return {
    id: "tab-1",
    type: "plugin",
    pluginId: "test-plugin",
    title: "Test Plugin",
    html: "<html><body>hello</body></html>",
    ...overrides,
  } as PluginPanelTab;
}

describe("PluginPanel", () => {
  let addEventListenerSpy: ReturnType<typeof vi.spyOn>;
  let removeEventListenerSpy: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    insideOnMount = false;
    messageListenerCalledInsideOnMount = null;

    const originalAdd = window.addEventListener.bind(window);
    addEventListenerSpy = vi.spyOn(window, "addEventListener").mockImplementation(
      (event: string, ...rest: unknown[]) => {
        if (event === "message") {
          messageListenerCalledInsideOnMount = insideOnMount;
        }
        return originalAdd(event as keyof WindowEventMap, ...(rest as [EventListenerOrEventListenerObject, (boolean | AddEventListenerOptions)?]));
      },
    );
    removeEventListenerSpy = vi.spyOn(window, "removeEventListener");
  });

  afterEach(() => {
    addEventListenerSpy.mockRestore();
    removeEventListenerSpy.mockRestore();
  });

  it("addEventListener('message') is called inside onMount — not at component body evaluation", () => {
    const tab = makeTab();
    render(() => <PluginPanel tab={tab} />);

    // The message listener MUST have been registered inside onMount, not at component body level
    expect(messageListenerCalledInsideOnMount).toBe(true);
  });

  it("registers message listener on window during mount", () => {
    const tab = makeTab();
    render(() => <PluginPanel tab={tab} />);

    const messageCalls = addEventListenerSpy.mock.calls.filter(
      ([event]) => event === "message",
    );
    expect(messageCalls).toHaveLength(1);
  });

  it("removes message listener on unmount", () => {
    const tab = makeTab();
    const { unmount } = render(() => <PluginPanel tab={tab} />);

    // Capture which handler was registered
    const [, registeredHandler] = addEventListenerSpy.mock.calls.find(
      ([event]) => event === "message",
    )!;

    unmount();

    const removeCalls = removeEventListenerSpy.mock.calls.filter(
      ([event]) => event === "message",
    );
    expect(removeCalls).toHaveLength(1);
    expect(removeCalls[0][1]).toBe(registeredHandler);
  });

  it("registers send channel via pluginRegistry on mount", () => {
    const tab = makeTab();
    render(() => <PluginPanel tab={tab} />);

    expect(pluginRegistry.registerPanelSendChannel).toHaveBeenCalledWith(
      "tab-1",
      expect.any(Function),
    );
  });

  it("unregisters send channel via pluginRegistry on unmount", () => {
    const tab = makeTab();
    const { unmount } = render(() => <PluginPanel tab={tab} />);
    unmount();

    expect(pluginRegistry.unregisterPanelSendChannel).toHaveBeenCalledWith("tab-1");
  });

  it("routes non-system messages without throwing (source guard prevents routing when no iframe)", () => {
    const tab = makeTab();
    render(() => <PluginPanel tab={tab} />);

    // Get the registered handler
    const [, handler] = addEventListenerSpy.mock.calls.find(
      ([event]) => event === "message",
    )!;

    // Simulate a message from an unknown source — handler guards on iframeRef.contentWindow
    const fakeEvent = new MessageEvent("message", {
      data: { type: "custom", payload: "test" },
    });
    expect(() => (handler as EventListener)(fakeEvent)).not.toThrow();
  });

  it("renders an iframe element", () => {
    const tab = makeTab();
    const { container } = render(() => <PluginPanel tab={tab} />);
    const iframe = container.querySelector("iframe");
    expect(iframe).not.toBeNull();
  });

  it("renders iframe with sandbox allow-scripts", () => {
    const tab = makeTab();
    const { container } = render(() => <PluginPanel tab={tab} />);
    const iframe = container.querySelector("iframe");
    expect(iframe?.getAttribute("sandbox")).toBe("allow-scripts");
  });
});
