// @vitest-environment jsdom
import { readFileSync } from "node:fs";
import { cleanup, fireEvent, render, screen } from "@solidjs/testing-library";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { chatFontSize } from "../../__tests__/helpers/chatFontSize";
import { Transcript } from "../../components/AIChatPanel/Transcript";
import { CommandInput } from "../components/CommandInput";
import { mobileChatReadingStyle, mobileFontSizes } from "../mobileFontSize";
import { SettingsScreen } from "../screens/SettingsScreen";

vi.mock("../useMobileVoice", () => ({ useMobileVoice: () => ({ available: () => false }) }));
vi.mock("../mobileTheme", () => ({
	mobileTheme: () => "commander",
	loadMobileTheme: async () => "commander",
	setMobileTheme: vi.fn(),
}));

let stylesheet: HTMLStyleElement;
beforeEach(() => {
	vi.stubGlobal(
		"fetch",
		vi.fn(async () => ({ ok: false })),
	);
	// Vitest replaces module imports with non-scoped names; load the production
	// rules too so this asserts the rendered control's actual positioning.
	stylesheet = document.createElement("style");
	stylesheet.textContent =
		readFileSync("src/styles/chat-tokens.css", "utf8") +
		readFileSync("src/components/AIChatPanel/AIChatPanel.module.css", "utf8").replace(/:global\(([^)]+)\)/g, "$1");
	document.head.append(stylesheet);
});
afterEach(() => {
	cleanup();
	stylesheet.remove();
	vi.unstubAllGlobals();
});

// Catches: session answer actions taken out of flow cover the first prose line.
it.each([
	"💬 A long first answer line that wraps on the phone instead of leaving a Copy-sized gutter.",
	"💬 First paragraph.\n\n> A quotation below the answer.",
	"💬 First paragraph.\n\n```ts\nconst screen = 'mobile';\n```",
])("mobile_copy_stays_in_flow: %s", (text) => {
	const view = render(() => (
		<Transcript
			mobile
			terminalAgent="claude"
			entries={() => [{ id: "answer", kind: "agent", text }]}
			busy={() => false}
			emptyMessage=""
		/>
	));
	const copy = screen.getByRole("button", { name: "Assistant message actions" });
	const actions = copy.parentElement!.parentElement!;
	const content = view.container.querySelector("#markdown-content")!;
	expect(content.compareDocumentPosition(actions) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
	expect(["", "static", "relative"]).toContain(getComputedStyle(actions).position);
});

// Catches: the compact command editor keeps its old 16px text and outer padding.
it("reduces_command_editor_text_and_spacing_without_resizing_action_targets", () => {
	const editorStyle = document.createElement("style");
	editorStyle.textContent = readFileSync("src/mobile/components/CommandInput.module.css", "utf8").replace(
		/composes:[^;]+;/g,
		"",
	);
	document.head.append(editorStyle);
	try {
		const view = render(() => <CommandInput sessionId="layout-only" sessionExists={false} />);
		const editor = screen.getByRole("textbox");
		expect(getComputedStyle(editor).fontSize).toBe("15px");
		expect(getComputedStyle(editor).paddingTop).toBe("7.5px");
		expect(getComputedStyle(view.container.firstElementChild!).paddingTop).toBe("2px");
		// Catches: compact idle text also reducing the native focused size.
		(editor as HTMLTextAreaElement).disabled = false;
		(editor as HTMLTextAreaElement).focus();
		expect(getComputedStyle(editor).fontSize).toBe("16px");
	} finally {
		editorStyle.remove();
	}
});

// Catches: only --font-base grows, leaving older intermediate prose at fixed 12px.
it("mobile_font_change_updates_every_rendered_assistant_block", async () => {
	const view = render(() => (
		<>
			<SettingsScreen isConnected />
			<div style={mobileChatReadingStyle()}>
				<Transcript
					mobile
					terminalAgent="claude"
					entries={() => [
						{ id: "u1", kind: "user", text: "Earlier request" },
						{ id: "a1", kind: "agent", text: "💬 Earlier completed answer." },
						{ id: "u2", kind: "user", text: "Current request" },
						{ id: "a2", kind: "agent", text: "Older explanatory block." },
						{ id: "t1", kind: "tool", call: { toolCallId: "read1", title: "Read", status: "completed" } },
						{ id: "a3", kind: "agent", text: "Another earlier block." },
						{ id: "t2", kind: "tool", call: { toolCallId: "read2", title: "Read again", status: "completed" } },
						{ id: "a4", kind: "agent", text: "💬 Newest answer." },
					]}
					busy={() => false}
					emptyMessage=""
				/>
			</div>
		</>
	));
	try {
		await screen.findByText("Newest answer.");
		const blocks = [...view.container.querySelectorAll<HTMLElement>(".assistantMsg [id=markdown-content] p")];
		expect(blocks, view.container.innerHTML).toHaveLength(4);
		fireEvent.click(screen.getByRole("button", { name: "Larger Chat font" }));
		fireEvent.click(screen.getByRole("button", { name: "Larger Chat font" }));
		expect(screen.getByRole("status", { name: "Chat font size" }).textContent).toBe("16 px");
		expect(blocks.map(chatFontSize)).toEqual(["16px", "16px", "16px", "16px"]);
		fireEvent.click(screen.getByRole("button", { name: "Smaller Chat font" }));
		fireEvent.click(screen.getByRole("button", { name: "Smaller Chat font" }));
		expect(blocks.map(chatFontSize)).toEqual(["14px", "14px", "14px", "14px"]);
	} finally {
		while (mobileFontSizes.chat() > 14) mobileFontSizes.step("chat", -1);
	}
});
