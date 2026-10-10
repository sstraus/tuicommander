import { cleanup, fireEvent, render } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, describe, expect, it, vi } from "vitest";
import { CommandInput } from "../components/CommandInput";
import { TerminalKeybar } from "../components/TerminalKeybar";

vi.mock("../../transport", () => ({
	buildHttpUrl: (path: string) => `http://localhost${path}`,
	rpc: vi.fn().mockResolvedValue(undefined),
	HttpRpcError: class extends Error {},
}));

afterEach(cleanup);

describe("mobile history editing", () => {
	it("keeps explicitly recalled history editable instead of appending new typing to an invisible command", () => {
		const [inputLine, setInputLine] = createSignal<string | null>(null);
		const view = render(() => (
			<>
				<TerminalKeybar sessionId="history-session" />
				<CommandInput sessionId="history-session" ptyInputLine={inputLine()} />
			</>
		));
		fireEvent.click(view.getByRole("button", { name: "↑" }));
		// The terminal returns the line the user explicitly recalled. The
		// composer must expose it before an edit can produce the intended delta.
		setInputLine("echo old");
		const composer = view.getByRole("textbox") as HTMLTextAreaElement;
		expect(composer.value).toBe("echo old");
		fireEvent.input(composer, { target: { value: "echo new" } });
		expect(composer.value).toBe("echo new");
	});
});
