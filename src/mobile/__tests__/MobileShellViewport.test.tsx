// @vitest-environment jsdom

import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { cleanup, render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import MobileApp from "../MobileApp";

vi.mock("../../transport", () => ({ rpc: vi.fn(async () => ({ repos: {} })) }));
vi.mock("../../invoke", () => ({ invoke: vi.fn() }));
vi.mock("../../stores/appLogger", () => ({ appLogger: { debug: vi.fn(), warn: vi.fn(), error: vi.fn() } }));
vi.mock("../../stores/settings", () => ({
	settingsStore: { hydrate: vi.fn(async () => {}), state: { chatMuteIntermediateText: true } },
}));
vi.mock("../../stores/ideas", () => ({ ideasStore: { hydrate: vi.fn() } }));
vi.mock("../useSessions", () => ({
	useSessions: () => ({
		aiChatAvailable: () => true,
		sessions: () => [],
		loading: () => false,
		refreshing: () => false,
		error: () => null,
		authError: () => false,
		refresh: vi.fn(),
		markSeen: vi.fn(),
	}),
}));
vi.mock("../useMobileNotifications", () => ({ useMobileNotifications: vi.fn() }));
vi.mock("../useVersionCheck", () => ({
	useVersionCheck: () => ({ updateAvailable: () => false, serverDown: () => false, applyUpdate: vi.fn() }),
}));
vi.mock("../../components/McpConfirmHost/McpConfirmHost", () => ({ McpConfirmHost: () => <div /> }));

// iPhone standalone PWA (Boss, 2026-10-12): 874 CSS px tall, 62 px top inset.
const SCREEN = 874;
const KEYBOARD_VV_HEIGHT = 480;

class FakeVisualViewport extends EventTarget {
	constructor(
		public height: number,
		public offsetTop: number,
	) {
		super();
	}
	move(height: number, offsetTop: number) {
		this.height = height;
		this.offsetTop = offsetTop;
		this.dispatchEvent(new Event("resize"));
	}
}

const originalInnerHeight = window.innerHeight;

function mountWith(vv: FakeVisualViewport, innerHeight: number) {
	Object.defineProperty(window, "visualViewport", { configurable: true, value: vv });
	Object.defineProperty(window, "innerHeight", { configurable: true, value: innerHeight });
	render(() => <MobileApp />);
}

const shellVar = (name: string) => document.documentElement.style.getPropertyValue(name);

beforeEach(() => {
	document.documentElement.removeAttribute("style");
});

afterEach(() => {
	cleanup();
	Object.defineProperty(window, "visualViewport", { configurable: true, value: undefined });
	Object.defineProperty(window, "innerHeight", { configurable: true, value: originalInnerHeight });
});

describe("mobile shell follows the visual viewport", () => {
	// Catches: the shell keeps auto height because the keyboard check compares visualViewport
	// with window.innerHeight, and innerHeight shrinks with the keyboard too
	// (interactive-widget=resizes-content), so the command input stays behind the keyboard.
	it("sizes the shell to the space above the keyboard when innerHeight shrinks with it", async () => {
		mountWith(new FakeVisualViewport(KEYBOARD_VV_HEIGHT, 0), KEYBOARD_VV_HEIGHT);
		await waitFor(() => expect(shellVar("--app-height")).toBe(`${KEYBOARD_VV_HEIGHT}px`));
		expect(shellVar("--app-top")).toBe("0px");
	});

	// Catches: the shell anchored to the layout viewport top while iOS scrolls the visual
	// viewport down to reveal the focused input.
	it("moves the shell top with visualViewport.offsetTop above the keyboard", async () => {
		mountWith(new FakeVisualViewport(KEYBOARD_VV_HEIGHT, 120), SCREEN);
		await waitFor(() => expect(shellVar("--app-top")).toBe("120px"));
		expect(shellVar("--app-height")).toBe(`${KEYBOARD_VV_HEIGHT}px`);
	});

	// Catches: the shell keeping the keyboard height after the keyboard closes.
	it("grows back to the full visual viewport when the keyboard closes", async () => {
		const vv = new FakeVisualViewport(KEYBOARD_VV_HEIGHT, 0);
		mountWith(vv, KEYBOARD_VV_HEIGHT);
		await waitFor(() => expect(shellVar("--app-height")).toBe(`${KEYBOARD_VV_HEIGHT}px`));
		vv.move(SCREEN - 62, 0);
		await waitFor(() => expect(shellVar("--app-height")).toBe(`${SCREEN - 62}px`));
	});
});

describe("terminal output overscroll", () => {
	const outputRule =
		readFileSync(resolve(__dirname, "../components/OutputView.module.css"), "utf-8").match(
			/\.output\s*\{([^}]*)\}/,
		)?.[1] ?? "";

	// Catches (story 2739): a drag past the bottom of the output chains to the page and pans the
	// visual viewport while the keyboard is open (measured on the iOS Simulator: offsetTop 0 → 28 px).
	it("keeps a drag past either end inside the output", () => {
		expect(outputRule).toMatch(/(^|[;\s])overscroll-behavior:\s*contain;/);
	});
});
