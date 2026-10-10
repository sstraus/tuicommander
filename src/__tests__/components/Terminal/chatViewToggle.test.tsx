import { render } from "@solidjs/testing-library";
import { describe, expect, it, vi } from "vitest";

vi.mock("../../../hooks/usePty", () => ({
	usePty: () => ({
		createSession: vi.fn().mockResolvedValue("sess-toggle"),
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
	default: () => <div data-testid="canvas" />,
}));
vi.mock("../../../invoke", () => ({
	invoke: vi.fn(async () => ({
		epoch: 0,
		nextSeq: 0,
		reset: true,
		updates: [],
		unknownRows: 0,
		malformedRows: 0,
	})),
	listen: vi.fn(async () => () => {}),
}));

import { Terminal } from "../../../components/Terminal/Terminal";
import { terminalsStore } from "../../../stores/terminals";

/**
 * Chat mode must HIDE the grid and never unmount it: `CanvasTerminal` under a
 * disposed `<Show keyed>` with a frame event already queued froze the whole UI
 * (see the comment above that `<Show>` in Terminal.tsx).
 */
describe("chat view toggle", () => {
	// Catches: the Show-keyed disposal freeze / lost scroll when switching to chat.
	it("toggle_hides_canvas_without_unmounting_it", async () => {
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
		let rendered: ReturnType<typeof render>;
		try {
			rendered = render(() => <Terminal id={id} cwd="/tmp/repo" alwaysVisible />);
		} finally {
			globalThis.requestAnimationFrame = oldRaf;
			if (width) Object.defineProperty(HTMLElement.prototype, "offsetWidth", width);
			if (height) Object.defineProperty(HTMLElement.prototype, "offsetHeight", height);
		}
		const { findByTestId } = rendered;
		const canvas = await findByTestId("canvas");
		const hiddenAncestor = () => canvas.closest('[class*="contentHidden"]');
		expect(hiddenAncestor()).toBeNull();

		terminalsStore.setViewMode(id, "chat");
		await Promise.resolve();

		expect(canvas.isConnected).toBe(true);
		expect(hiddenAncestor()).not.toBeNull();
		// Catches: the absolute Compose handle covering the read-only Chat footer.
		expect(rendered.queryByTitle("Open compose editor (⌘I)")).toBeNull();
		expect(rendered.container.querySelector('[class*="composeHint"]')).toBeNull();
		terminalsStore.setViewMode(id, "cli");
		await Promise.resolve();
		expect(rendered.container.querySelector('[class*="composeHint"]')).not.toBeNull();
	});
});
