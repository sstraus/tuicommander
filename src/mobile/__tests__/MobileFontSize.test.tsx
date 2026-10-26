// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { invoke, subscribePty } = vi.hoisted(() => ({ invoke: vi.fn(), subscribePty: vi.fn() }));
vi.mock("../../invoke", () => ({ invoke, listen: vi.fn(async () => () => {}) }));
vi.mock("../../transport", () => ({ subscribePty }));
vi.mock("../useMobileVoice", () => ({ useMobileVoice: () => ({ available: () => false }) }));
vi.mock("../../stores/appLogger", () => ({ appLogger: { warn: vi.fn(), error: vi.fn(), info: vi.fn() } }));

beforeEach(() => {
	vi.resetModules();
	localStorage.clear();
	invoke.mockResolvedValue(undefined);
	subscribePty.mockResolvedValue(() => {});
	vi.stubGlobal(
		"fetch",
		vi.fn(async () => ({ ok: true, json: async () => ({ lines: ["CLI answer"], total_lines: 1, offset: 0 }) })),
	);
});
afterEach(() => {
	cleanup();
	vi.unstubAllGlobals();
});

describe("font preferences after a real module reload", () => {
	let beforeReload: {
		terminal: string | null;
		chat: string | null;
		savedTerminal: string | null;
		savedChat: string | null;
	};
	let view: ReturnType<typeof render>;

	// resetModules invalidates evaluation as well as the font signals. Prepare
	// both real module graphs in setup, including the actual Settings writes;
	// beforeAll warming alone cannot remove reevaluation from a timed test.
	beforeEach(async () => {
		const { SettingsScreen } = await import("../screens/SettingsScreen");
		const first = render(() => <SettingsScreen isConnected />);
		fireEvent.click(screen.getByRole("button", { name: "Larger Terminal font" }));
		fireEvent.click(screen.getByRole("button", { name: "Larger Terminal font" }));
		for (let i = 0; i < 4; i++) fireEvent.click(screen.getByRole("button", { name: "Larger Chat font" }));
		beforeReload = {
			terminal: screen.getByRole("status", { name: "Terminal font size" }).textContent,
			chat: screen.getByRole("status", { name: "Chat font size" }).textContent,
			savedTerminal: localStorage.getItem("tuic-mobile-terminal-font-size"),
			savedChat: localStorage.getItem("tuic-mobile-chat-font-size"),
		};
		first.unmount();
		vi.resetModules();
		const [{ OutputView }, { CommandInput }, { useSessionChatView }] = await Promise.all([
			import("../components/OutputView"),
			import("../components/CommandInput"),
			import("../components/useSessionChatView"),
		]);
		invoke.mockResolvedValue({
			epoch: 1,
			nextSeq: 1,
			reset: true,
			updates: [
				{ sessionUpdate: "agent_message_chunk", messageId: "answer", content: { type: "text", text: "Chat answer" } },
			],
		});
		view = render(() => {
			const chat = useSessionChatView({
				sessionId: () => "font-test",
				agentType: () => "claude",
				sessionExists: () => true,
				subscribeWake: () => () => {},
			});
			return (
				<>
					<OutputView sessionId="font-test" />
					<CommandInput sessionId="font-test" />
					<chat.Toggle />
					<chat.View />
				</>
			);
		});
	});

	// Catches: independent font preferences are lost on reload or one view changes the other.
	it("restores independent Terminal and Chat sizes without resizing the composer", async () => {
		expect(beforeReload.terminal).toBe("16 px");
		expect(beforeReload.chat).toBe("18 px");
		expect(beforeReload.savedTerminal).toBe("16");
		expect(beforeReload.savedChat).toBe("18");
		await screen.findByText("CLI answer");
		expect(view.container.querySelector("pre")?.style.fontSize).toBe("16px");
		expect(view.container.querySelector("textarea")?.style.getPropertyValue("--mobile-font-size")).toBe("");
		fireEvent.click(screen.getByRole("button", { name: "Chat" }));
		await screen.findByText("Chat answer");
		const transcript = view.container.querySelector("[data-mobile-reading]") as HTMLElement;
		expect(transcript.style.getPropertyValue("--font-base")).toBe("18px");
	});
});

describe("invalid stored font preferences", () => {
	let SettingsScreen: typeof import("../screens/SettingsScreen").SettingsScreen;
	beforeEach(async () => {
		localStorage.setItem("tuic-mobile-terminal-font-size", "NaN");
		localStorage.setItem("tuic-mobile-chat-font-size", "999");
		({ SettingsScreen } = await import("../screens/SettingsScreen"));
	});

	// Catches: invalid storage or repeated taps break bounds, or one control moves the other.
	it("restores existing defaults for invalid values and bounds each control independently", async () => {
		render(() => <SettingsScreen isConnected />);
		expect(screen.getByRole("status", { name: "Terminal font size" }).textContent).toBe("14 px");
		expect(screen.getByRole("status", { name: "Chat font size" }).textContent).toBe("14 px");
		for (let i = 0; i < 20; i++) fireEvent.click(screen.getByRole("button", { name: "Larger Terminal font" }));
		expect(screen.getByRole("status", { name: "Terminal font size" }).textContent).toBe("24 px");
		expect((screen.getByRole("button", { name: "Larger Terminal font" }) as HTMLButtonElement).disabled).toBe(true);
		expect(screen.getByRole("status", { name: "Chat font size" }).textContent).toBe("14 px");
		for (let i = 0; i < 20; i++) fireEvent.click(screen.getByRole("button", { name: "Smaller Chat font" }));
		await waitFor(() => expect(screen.getByRole("status", { name: "Chat font size" }).textContent).toBe("10 px"));
		expect((screen.getByRole("button", { name: "Smaller Chat font" }) as HTMLButtonElement).disabled).toBe(true);
		expect(screen.getByRole("status", { name: "Terminal font size" }).textContent).toBe("24 px");
	});
});
