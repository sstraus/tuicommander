import { cleanup, fireEvent, render, screen } from "@solidjs/testing-library";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { CommandInput } from "../components/CommandInput";
import { SettingsScreen } from "../screens/SettingsScreen";

vi.mock("../../transport", async (original) => ({
	...(await original<typeof import("../../transport")>()),
	rpc: vi.fn(async () => ({})),
}));
vi.mock("../mobileTheme", () => ({
	mobileTheme: () => "commander",
	loadMobileTheme: async () => "commander",
	setMobileTheme: vi.fn(),
}));
vi.mock("../useMobileVoice", () => ({ useMobileVoice: () => ({ available: () => false }) }));

beforeEach(() => {
	localStorage.clear();
	vi.stubGlobal(
		"fetch",
		vi.fn(async () => ({ ok: true, json: async () => ({ version: "test" }) })),
	);
});
afterEach(() => {
	cleanup();
	vi.unstubAllGlobals();
});

function expectCorrection(enabled: boolean) {
	const composer = screen.getByRole("textbox");
	expect(composer.getAttribute("autocorrect")).toBe(enabled ? "on" : "off");
	expect(composer.getAttribute("spellcheck")).toBe(String(enabled));
	expect(composer.getAttribute("autocapitalize")).toBe(enabled ? "sentences" : "off");
	expect(composer.getAttribute("autocomplete")).toBe("off");
}

// Catches: hard-coded off attributes ignoring the default or persisted opt-out.
it.each([null, "true", "false"])("renders keyboard correction for saved preference %s", (saved) => {
	if (saved !== null) localStorage.setItem("tuic-mobile-keyboard-correction", saved);
	render(() => <CommandInput sessionId="correction-test" />);
	expectCorrection(saved !== "false");
});

// Catches: Settings saves an opt-out but an open or remounted composer keeps the old attributes.
it("persists the Settings toggle across reloads and updates an open composer", () => {
	const mount = () =>
		render(() => (
			<>
				<SettingsScreen isConnected />
				<CommandInput sessionId="correction-test" />
			</>
		));
	mount();
	const toggle = () => screen.getByRole("switch", { name: "Autocorrect and spellcheck" });
	expect((toggle() as HTMLInputElement).checked).toBe(true);
	fireEvent.click(toggle());
	expectCorrection(false);
	expect(localStorage.getItem("tuic-mobile-keyboard-correction")).toBe("false");
	cleanup();
	mount();
	expect((toggle() as HTMLInputElement).checked).toBe(false);
	expectCorrection(false);
	fireEvent.click(toggle());
	expectCorrection(true);
	expect(localStorage.getItem("tuic-mobile-keyboard-correction")).toBe("true");
	cleanup();
	mount();
	expect((toggle() as HTMLInputElement).checked).toBe(true);
	expectCorrection(true);
});
