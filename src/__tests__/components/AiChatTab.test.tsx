import { cleanup, fireEvent, render } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { mockInvoke, mockListen } = vi.hoisted(() => ({
	mockInvoke: vi.fn(),
	mockListen: vi.fn().mockResolvedValue(vi.fn()),
}));

vi.mock("../../invoke", () => ({
	invoke: mockInvoke,
	listen: mockListen,
}));

vi.mock("../../stores/appLogger", () => ({
	appLogger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

vi.mock("../../stores/toasts", () => ({
	toastsStore: { add: vi.fn() },
}));

import { AiChatTab } from "../../components/SettingsPanel/tabs/AiChatTab";

// The reasoning dropdown is the select that carries the effort options.
function reasoningSelect(container: HTMLElement): HTMLSelectElement {
	const select = Array.from(container.querySelectorAll("select")).find((el) =>
		el.querySelector('option[value="medium"]'),
	);
	if (!select) throw new Error("reasoning select not found");
	return select as HTMLSelectElement;
}

// Resolve invoke per command so onMount's loads don't reject.
function invokeImpl(config: { temperature?: number; reasoning_effort?: string } = {}) {
	return (cmd: string) => {
		if (cmd === "load_ai_chat_config") return Promise.resolve({ temperature: 0.7, ...config });
		if (cmd === "load_scheduler_config") return Promise.resolve({ jobs: [] });
		return Promise.resolve(undefined);
	};
}

describe("AiChatTab extended-thinking setting", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		mockInvoke.mockImplementation(invokeImpl());
		mockListen.mockResolvedValue(vi.fn());
	});

	afterEach(() => {
		cleanup();
	});

	it("renders the effort options Auto/Off/Low/Medium/High", () => {
		const { container } = render(() => <AiChatTab />);
		const values = Array.from(reasoningSelect(container).options).map((o) => o.value);
		expect(values).toEqual(["auto", "off", "low", "medium", "high"]);
	});

	it("defaults to auto", () => {
		const { container } = render(() => <AiChatTab />);
		expect(reasoningSelect(container).value).toBe("auto");
	});

	it("reflects the persisted value loaded from config", async () => {
		mockInvoke.mockImplementation(invokeImpl({ reasoning_effort: "high" }));
		const { container } = render(() => <AiChatTab />);
		await vi.waitFor(() => {
			expect(reasoningSelect(container).value).toBe("high");
		});
	});

	it("persists the chosen effort to the AI chat config", async () => {
		const { container } = render(() => <AiChatTab />);
		// Let onMount's async config load settle before changing the dropdown,
		// otherwise the load resets the signal after our change.
		await vi.waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("load_ai_chat_config"));
		const select = reasoningSelect(container);
		fireEvent.change(select, { target: { value: "low" } });
		// saveConfig is debounced by 500ms — wait past the debounce.
		await new Promise((r) => setTimeout(r, 700));
		expect(mockInvoke).toHaveBeenCalledWith(
			"save_ai_chat_config",
			expect.objectContaining({ config: expect.objectContaining({ reasoning_effort: "low" }) }),
		);
	});
});
