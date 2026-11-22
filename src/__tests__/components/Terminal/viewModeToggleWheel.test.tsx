import { fireEvent, render } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";

const canvasWheel = vi.hoisted(() => vi.fn());

vi.mock("../../../hooks/usePty", () => ({
	usePty: () => ({
		createSession: vi.fn().mockResolvedValue("sess-wheel"),
		resize: vi.fn(),
		close: vi.fn(),
		getKittyFlags: vi.fn().mockResolvedValue(0),
	}),
}));
vi.mock("../../../transport", async (importOriginal) => ({
	...(await importOriginal<typeof import("../../../transport")>()),
	rpc: vi.fn(),
	subscribePty: () => Promise.resolve(() => {}),
}));
vi.mock("@tauri-apps/api/core", () => ({
	invoke: vi.fn(async (cmd: string) => (cmd === "get_session_foreground_process" ? "claude" : undefined)),
}));
vi.mock("../../../stores/agentConfigs", () => ({
	ensureAgentConfigsForRepo: vi.fn().mockResolvedValue({ getEnvFlags: () => ({}) }),
	agentConfigsForRepo: () => ({ getEnvFlags: () => ({}) }),
}));
vi.mock("../../../components/Terminal/glyphCache", async (importOriginal) => ({
	...(await importOriginal<typeof import("../../../components/Terminal/glyphCache")>()),
	getSharedMetrics: () => ({ cellWidth: 8, cellHeight: 16 }),
}));
vi.mock("../../../components/Terminal/CanvasTerminal", () => ({
	default: (props: { onRef?: (ref: unknown) => void }) => {
		props.onRef?.({
			focus: () => {},
			refresh: () => {},
			resubscribe: async () => {},
			lostGrid: () => false,
			getSelectionText: () => "",
			searchFind: async () => ({ index: -1, count: 0 }),
			searchNext: () => ({ index: -1, count: 0 }),
			searchPrev: () => ({ index: -1, count: 0 }),
			searchClear: () => {},
			paste: () => {},
			wheel: canvasWheel,
		});
		return <div data-testid="canvas" />;
	},
}));
vi.mock("../../../invoke", () => ({
	invoke: vi.fn(async (cmd: string) => {
		if (cmd === "get_session_foreground_process") return "claude";
		if (cmd !== "chat_view_snapshot") return undefined;
		return { epoch: 0, nextSeq: 0, reset: true, updates: [], unknownRows: 0, malformedRows: 0 };
	}),
	listen: vi.fn(async () => () => {}),
}));

import { Terminal } from "../../../components/Terminal/Terminal";
import { terminalsStore } from "../../../stores/terminals";

async function mountClaudeTerminal() {
	await import("../../../components/ComposePanel/ComposePanel");
	const id = terminalsStore.add({
		sessionId: "live-session",
		cwd: "/tmp/repo",
		repoPath: "/tmp/repo",
		name: "claude",
		fontSize: 13,
		awaitingInput: null,
	});
	terminalsStore.update(id, { agentType: "claude", agentSessionId: "uuid" });
	// The canvas mounts only once the container has a size.
	const width = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "offsetWidth");
	const height = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "offsetHeight");
	Object.defineProperty(HTMLElement.prototype, "offsetWidth", { configurable: true, get: () => 800 });
	Object.defineProperty(HTMLElement.prototype, "offsetHeight", { configurable: true, get: () => 600 });
	const oldRaf = globalThis.requestAnimationFrame;
	globalThis.requestAnimationFrame = (cb) => {
		cb(0);
		return 1;
	};
	try {
		const rendered = render(() => <Terminal id={id} cwd="/tmp/repo" alwaysVisible />);
		await rendered.findByTestId("canvas");
		return { id, rendered };
	} finally {
		globalThis.requestAnimationFrame = oldRaf;
		if (width) Object.defineProperty(HTMLElement.prototype, "offsetWidth", width);
		if (height) Object.defineProperty(HTMLElement.prototype, "offsetHeight", height);
	}
}

/**
 * The CLI | Chat switch floats over the top-right corner of whichever view is
 * showing. It is a sibling of both views, so a wheel event that lands on it
 * reaches neither: the scroll stalls while the pointer crosses it and the
 * gesture acceleration restarts afterwards (story 1847-feb9).
 */
describe("CLI | Chat switch wheel", () => {
	beforeEach(() => {
		canvasWheel.mockClear();
		for (const id of terminalsStore.getIds()) terminalsStore.remove(id);
	});

	// Catches: wheel over the switch never reaching the terminal's own wheel handler.
	it("hands a wheel over the switch to the terminal canvas in CLI view", async () => {
		const { rendered } = await mountClaudeTerminal();
		const event = new WheelEvent("wheel", { deltaY: 30, bubbles: true, cancelable: true });
		rendered.getByText("Chat").dispatchEvent(event);
		expect(canvasWheel).toHaveBeenCalledTimes(1);
		expect(canvasWheel).toHaveBeenCalledWith(event);
	});

	// Catches: the forwarding only covering the CLI view, leaving the Chat transcript stalled under the switch.
	it("scrolls the chat transcript for a wheel over the switch in Chat view", async () => {
		const { id, rendered } = await mountClaudeTerminal();
		terminalsStore.setViewMode(id, "chat");
		const transcript = await rendered.findByLabelText("Chat transcript");
		const scrollBy = vi.fn();
		transcript.scrollBy = scrollBy;
		fireEvent.wheel(rendered.getByText("CLI"), { deltaY: 40, deltaX: 0 });
		expect(scrollBy).toHaveBeenCalledWith({ top: 40, left: 0 });
		expect(canvasWheel).not.toHaveBeenCalled();
	});

	// Catches: the forwarding swallowing clicks on the switch.
	it("still switches view on click", async () => {
		const { id, rendered } = await mountClaudeTerminal();
		fireEvent.click(rendered.getByText("Chat"));
		expect(terminalsStore.get(id)?.viewMode).toBe("chat");
	});
});
