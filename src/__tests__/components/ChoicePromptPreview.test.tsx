import { render } from "@solidjs/testing-library";
import { expect, it, vi } from "vitest";
import { ChoicePromptOverlay } from "../../mobile/components/ChoicePromptOverlay";
import type { ChoicePrompt } from "../../mobile/useSessions";

// Captured native labels; the pane text is a rendering input, not a parser oracle.
const prompt: ChoicePrompt = {
	title:
		"← ☐ Posizione UI ☐ Accesso root ☐ Endpoint Sec ✔ Submit →\nDove mettiamo l’ispezione profonda nell’interfaccia?",
	selection_mode: "navigate-enter",
	options: [
		{
			key: "1",
			label: "Schede nel dettaglio processo (Recommended)",
			highlighted: true,
			destructive: false,
			hint: "Overview | Files | Network | Binary | Logs | Captures\nPath                         Size\n/usr/bin/example             12 KB",
		},
		{ key: "2", label: "Nuova scheda Inspect", highlighted: false, destructive: false },
		{ key: "3", label: "Chat about this", highlighted: false, destructive: false },
	],
};

// Catches: desktop's embedded and mobile's dropup variants flatten or discard the native preview.
it.each([true, false])("keeps native preview text separate from answer labels (embedded=%s)", (embedded) => {
	const select = vi.fn();
	const view = render(() => <ChoicePromptOverlay prompt={prompt} embedded={embedded} onSelect={select} />);
	expect(view.getByText("Schede nel dettaglio processo (Recommended)").tagName).toBe("SPAN");
	expect(view.container.querySelector("pre")?.textContent).toBe(prompt.options[0].hint);
	expect(view.getByText(/Dove mettiamo/).textContent).toBe(prompt.title);
	view.getByRole("button", { name: /Chat about this/ }).click();
	expect(select).toHaveBeenCalledWith("3");
});

// Catches: adding preview blocks changes the inline presentation of ordinary shortcut hints.
it("keeps a single-line native shortcut hint inline", () => {
	const view = render(() => (
		<ChoicePromptOverlay
			prompt={{
				title: "Allow?",
				options: [{ key: "1", label: "Yes", highlighted: true, destructive: false, hint: "shift+tab" }],
			}}
			onSelect={() => {}}
		/>
	));
	expect(view.getByText("shift+tab").tagName).toBe("SPAN");
	expect(view.container.querySelector("pre")).toBeNull();
});
