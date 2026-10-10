import { cleanup, fireEvent, render } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, expect, it, vi } from "vitest";
import { CommandInput } from "../components/CommandInput";
import { TerminalKeybar } from "../components/TerminalKeybar";

vi.mock("../../transport", () => ({
	buildHttpUrl: (path: string) => `http://localhost${path}`,
	rpc: vi.fn(async () => undefined),
	HttpRpcError: class extends Error {},
}));

import { rpc } from "../../transport";

afterEach(() => {
	cleanup();
	vi.clearAllMocks();
});

// Observed in IMG_0208.jpeg, 2026-10-10. The app's dictation log records
// "Computer, approva tu quelle" accepted at 1791620448686 and one
// "Hands-free turn typed now" at 1791620453361 for b495fcba-… .
// The echo ordering below exercises the race; no WS trace was captured then.
const spoken =
	"approva tu quelle schermate di fiducia degli e-hook, chi se ne frega, cioè perché ti fammi su ste cose? Non deve essere un blocker, mai. Sei il coordinatore.";

it("does not restore an already delivered hands-free phrase into the mobile composer", () => {
	const [inputLine, echo] = createSignal<string | null>(null);
	const { container } = render(() => <CommandInput sessionId="voice-1662" ptyInputLine={inputLine()} />);
	const input = container.querySelector("textarea")!;
	// The payload can be painted before the split Enter, or reported as a
	// queued prompt. Either way it belongs to the PTY, not a new local draft.
	echo(spoken);
	expect(input.value).toBe("");
	echo("");
	expect(input.value).toBe("");
});

it("does not import a delayed voice echo that extends the user's newer draft", () => {
	const [inputLine, echo] = createSignal<string | null>(null);
	const { container } = render(() => <CommandInput sessionId="voice-1662" ptyInputLine={inputLine()} />);
	const input = container.querySelector("textarea")!;
	fireEvent.input(input, { target: { value: "approva" } });
	echo(spoken);
	echo("");
	expect(input.value).toBe("approva");
	expect(vi.mocked(rpc).mock.calls).toEqual([["write_pty", { sessionId: "voice-1662", data: "approva" }]]);
});

it.each(["keyboard", "keybar"])("keeps %s Tab completion anchored to the user's draft", (source) => {
	const [inputLine, echo] = createSignal<string | null>(null);
	let tab: (() => void) | undefined;
	const { container, getByRole } = render(() => (
		<>
			<TerminalKeybar sessionId="voice-1662" onTabRequest={() => tab?.()} />
			<CommandInput sessionId="voice-1662" ptyInputLine={inputLine()} onRegisterTab={(fn) => (tab = fn)} />
		</>
	));
	const input = container.querySelector("textarea")!;
	fireEvent.input(input, { target: { value: "gi" } });
	if (source === "keyboard") fireEvent.keyDown(input, { key: "Tab" });
	else fireEvent.click(getByRole("button", { name: "Tab" }));
	echo("git ");
	expect(input.value).toBe("git ");
	// One completion is not permission to import further unsolicited input.
	echo("git status");
	expect(input.value).toBe("git ");
	fireEvent.input(input, { target: { value: "git log" } });
	expect(vi.mocked(rpc).mock.calls.map(([, args]) => args?.data)).toEqual(["gi", "\t", "log"]);
});

it("cancels completion ownership when the user edits before the echo arrives", () => {
	const [inputLine, echo] = createSignal<string | null>(null);
	const { container } = render(() => <CommandInput sessionId="voice-1662" ptyInputLine={inputLine()} />);
	const input = container.querySelector("textarea")!;
	fireEvent.input(input, { target: { value: "gi" } });
	fireEvent.keyDown(input, { key: "Tab" });
	fireEvent.input(input, { target: { value: "git" } });
	echo("git status");
	expect(input.value).toBe("git");
});

it("does not recreate a voice draft from an HTTP snapshot or reconnect echo", () => {
	const [inputLine, echo] = createSignal<string | null>(spoken);
	const { container } = render(() => <CommandInput sessionId="voice-1662" ptyInputLine={inputLine()} />);
	const input = container.querySelector("textarea")!;
	expect(input.value).toBe("");
	echo(null);
	echo(spoken);
	expect(input.value).toBe("");
});
