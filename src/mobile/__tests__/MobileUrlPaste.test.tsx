// @vitest-environment jsdom
import { cleanup, fireEvent, render } from "@solidjs/testing-library";
import { afterEach, expect, it, vi } from "vitest";
import { CommandInput } from "../components/CommandInput";

vi.mock("../../transport", () => ({ rpc: vi.fn(async () => undefined), HttpRpcError: class extends Error {} }));
afterEach(cleanup);
function paste(input: HTMLTextAreaElement, text: string) {
	const native = fireEvent.paste(input, { clipboardData: { getData: () => text } });
	if (native) {
		input.setRangeText(text, input.selectionStart, input.selectionEnd, "end");
		fireEvent.input(input);
	}
}
// Catches: typing after a pasted URL extends the URL instead of starting prose.
it("pasted_url_gets_a_space_and_caret_after_it", () => {
	const view = render(() => <CommandInput sessionId="url-paste" />);
	const input = view.container.querySelector("textarea")!;
	paste(input, "https://example.com/review");
	expect(input.value).toBe("https://example.com/review ");
	expect(input.selectionStart).toBe(input.value.length);
	expect(input.selectionEnd).toBe(input.value.length);
});

// Catches: URL spacing duplicates separators, replaces the wrong selection, or edits ordinary paste.
it.each([
	["", "", "See https://example.com/review", "See https://example.com/review ", 31],
	["", " next", "https://example.com", "https://example.com next", 19],
	["", "\nnext", "https://example.com", "https://example.com\nnext", 19],
	["", "", "https://example.com ", "https://example.com ", 20],
	["", "", "ordinary words", "ordinary words", 14],
	["", "", "https://", "https://", 8],
	["Look ", "suffix", "https://example.com", "Look https://example.com suffix", 25],
])("preserves the paste boundary for %j + %j + %j", (prefix, suffix, text, expected, caret) => {
	const view = render(() => <CommandInput sessionId="url-paste-boundary" />);
	const input = view.container.querySelector("textarea")!;
	fireEvent.input(input, { target: { value: `${prefix}REPLACE${suffix}` } });
	input.setSelectionRange(prefix.length, prefix.length + 7);
	paste(input, text);
	expect(input.value).toBe(expected);
	expect(input.selectionStart).toBe(caret);
	expect(input.selectionEnd).toBe(caret);
});
