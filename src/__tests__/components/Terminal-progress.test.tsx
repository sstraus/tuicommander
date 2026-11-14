import { render, waitFor } from "@solidjs/testing-library";
import { invoke } from "@tauri-apps/api/core";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { ptyOptions } = vi.hoisted(() => ({
	ptyOptions: new Map<string, { onParsed?: (frame: { type: string; event: unknown }) => void }>(),
}));

vi.mock("../../hooks/usePty", () => ({
	usePty: () => ({
		createSession: vi.fn().mockResolvedValue("fresh-session"),
		resize: vi.fn().mockResolvedValue(undefined),
		sendCommand: vi.fn().mockResolvedValue(undefined),
		close: vi.fn(),
		getKittyFlags: vi.fn().mockResolvedValue(0),
	}),
}));
vi.mock("../../transport", async (importOriginal) => ({
	...(await importOriginal<typeof import("../../transport")>()),
	rpc: vi.fn().mockResolvedValue(undefined),
	subscribePty: (
		sessionId: string,
		_onData: unknown,
		_onExit: unknown,
		options: typeof ptyOptions extends Map<string, infer V> ? V : never,
	) => {
		ptyOptions.set(sessionId, options);
		return Promise.resolve(() => {});
	},
}));
vi.mock("@tauri-apps/api/core", () => ({
	invoke: vi.fn(async () => undefined),
}));
vi.mock("../../stores/agentConfigs", () => ({
	ensureAgentConfigsForRepo: vi.fn().mockResolvedValue({ getEnvFlags: () => ({}) }),
	agentConfigsForRepo: () => ({ getEnvFlags: () => ({}) }),
}));
vi.mock("../../components/Terminal/glyphCache", async (importOriginal) => ({
	...(await importOriginal<typeof import("../../components/Terminal/glyphCache")>()),
	getSharedMetrics: () => ({ cellWidth: 8, cellHeight: 16 }),
}));
vi.mock("../../components/Terminal/CanvasTerminal", () => ({
	default: () => <div data-testid="canvas" />,
}));

import { TabBar } from "../../components/TabBar/TabBar";
import { Terminal } from "../../components/Terminal/Terminal";
import { appLogger } from "../../stores/appLogger";
import { terminalsStore } from "../../stores/terminals";

const openedTabs: string[] = [];

beforeEach(() => {
	ptyOptions.clear();
	vi.mocked(invoke).mockClear();
	vi.spyOn(appLogger, "info").mockImplementation(() => {});
	vi.spyOn(appLogger, "debug").mockImplementation(() => {});
	vi.spyOn(appLogger.diag, "info").mockImplementation(() => {});
});

afterEach(() => {
	for (const id of openedTabs.splice(0)) terminalsStore.remove(id);
	vi.restoreAllMocks();
});

/** Mounts a live terminal and its tab, and returns a sender for OSC 9;4 events. */
async function openTerminalWithTab() {
	const id = terminalsStore.add({
		sessionId: "live-session",
		cwd: "/Gits/alpha",
		name: "agent",
		fontSize: 13,
		awaitingInput: null,
	});
	openedTabs.push(id);
	const oldRaf = globalThis.requestAnimationFrame;
	const width = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "offsetWidth");
	const height = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "offsetHeight");
	Object.defineProperty(HTMLElement.prototype, "offsetWidth", { configurable: true, get: () => 800 });
	Object.defineProperty(HTMLElement.prototype, "offsetHeight", { configurable: true, get: () => 600 });
	globalThis.requestAnimationFrame = (cb) => {
		cb(0);
		return 1;
	};
	let container: HTMLElement;
	try {
		({ container } = render(() => (
			<>
				<TabBar
					onTabSelect={() => {}}
					onTabClose={() => {}}
					onCloseOthers={() => {}}
					onCloseToRight={() => {}}
					onNewTab={() => {}}
				/>
				<Terminal id={id} cwd="/Gits/alpha" alwaysVisible />
			</>
		)));
	} finally {
		globalThis.requestAnimationFrame = oldRaf;
		if (width) Object.defineProperty(HTMLElement.prototype, "offsetWidth", width);
		if (height) Object.defineProperty(HTMLElement.prototype, "offsetHeight", height);
	}
	await waitFor(() => expect(invoke).toHaveBeenCalledWith("get_shell_state", { sessionId: "live-session" }));
	const send = (state: number, value: number) =>
		ptyOptions.get("live-session")?.onParsed?.({ type: "parsed", event: { type: "progress", state, value } });
	const bar = () => container.querySelector(".tab .progress") as HTMLElement | null;
	return { send, bar };
}

describe("OSC 9;4 progress in the tab bar", () => {
	// Catches: state 3 mapped to value 0, so an indeterminate busy agent shows a zero-width bar.
	it("renders an indeterminate report as a full-width bar", async () => {
		const { send, bar } = await openTerminalWithTab();
		send(3, 0);
		expect(bar()).not.toBeNull();
		expect(bar()?.style.transform).toBe("scaleX(1)");
	});

	// Catches: an indeterminate marker that sticks after a valued report, or state 4 (paused) ignored.
	it("returns to value width for states 1, 2 and 4, and clears on state 0", async () => {
		const { send, bar } = await openTerminalWithTab();
		send(3, 0);
		send(1, 50);
		expect(bar()?.style.transform).toBe("scaleX(0.5)");
		send(2, 70);
		expect(bar()?.style.transform).toBe("scaleX(0.7)");
		send(4, 40);
		expect(bar()?.style.transform).toBe("scaleX(0.4)");
		send(0, 0);
		expect(bar()).toBeNull();
	});
});
