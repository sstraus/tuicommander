import { fireEvent, render } from "@solidjs/testing-library";
import { describe, expect, it, vi } from "vitest";
import { getShortcutSections, KeyboardShortcutsTab } from "../../components/SettingsPanel/tabs/KeyboardShortcutsTab";
import { ACTION_NAMES } from "../../keybindingDefaults";
import { settingsStore } from "../../stores/settings";

describe("KeyboardShortcutsTab", () => {
	// Catches: migrating Rebind loses its capture handler or Escape changes the saved binding.
	it("cancels a rebind without changing its current shortcut", () => {
		const view = render(() => <KeyboardShortcutsTab />);
		const row = view.container.querySelector("tr")!;
		const before = row.querySelector("kbd")!.textContent;
		fireEvent.click(row.querySelector("button")!);
		const recorder = view.getByText("Press a key...");
		fireEvent.keyDown(recorder, { key: "Escape", code: "Escape" });
		expect(row.querySelector("kbd")!.textContent).toBe(before);
		expect(row.querySelector("button")!.textContent).toBe("Rebind");
	});
	// Catches: named rebind controls disappearing or obsolete permanent walkthroughs returning.
	it("shows named Rebind actions without a permanent walkthrough", () => {
		const view = render(() => <KeyboardShortcutsTab />);
		expect(view.getByRole("region", { name: "Shortcut groups" })).toBeTruthy();
		expect(view.getAllByRole("button", { name: "Rebind" }).length).toBeGreaterThan(0);
		expect(view.container.textContent).not.toContain("Click Rebind to change a shortcut.");
		expect(view.container.textContent).not.toContain("pencil icon");
	});

	it("renders the heading", () => {
		const { container } = render(() => <KeyboardShortcutsTab />);
		const heading = container.querySelector("h2");
		expect(heading).not.toBeNull();
		expect(heading!.textContent).toBe("Keyboard Shortcuts");
	});

	it("keeps each action before its binding controls in reading and focus order", () => {
		// Catches a visual-only reversal that leaves screen readers hearing keys before the action.
		const { container } = render(() => <KeyboardShortcutsTab />);
		const firstRow = container.querySelector("tr");
		expect(firstRow?.firstElementChild?.textContent).toBe("New terminal tab");
		expect(firstRow?.lastElementChild?.querySelector("button")?.textContent).toContain("Rebind");
	});

	it("has a search input", () => {
		const { container } = render(() => <KeyboardShortcutsTab />);
		const input = container.querySelector("input[type='text']");
		expect(input).not.toBeNull();
		expect(input!.getAttribute("placeholder")).toBe("Search shortcuts...");
	});

	it("renders shortcut sections with labels", () => {
		const { container } = render(() => <KeyboardShortcutsTab />);
		const labels = container.querySelectorAll("label");
		const labelTexts = Array.from(labels).map((l) => l.textContent);
		expect(labelTexts).toContain("Terminal");
		expect(labelTexts).toContain("Zoom");
		expect(labelTexts).toContain("Panels");
		expect(labelTexts).toContain("Git");
	});

	it("renders keyboard shortcuts in kbd elements", () => {
		const { container } = render(() => <KeyboardShortcutsTab />);
		const kbds = container.querySelectorAll("kbd");
		expect(kbds.length).toBeGreaterThan(0);
	});

	it("filters shortcuts by search text", async () => {
		const { container } = render(() => <KeyboardShortcutsTab />);
		const input = container.querySelector("input[type='text']") as HTMLInputElement;

		fireEvent.input(input, { target: { value: "zoom" } });

		const labels = container.querySelectorAll("label");
		const labelTexts = Array.from(labels).map((l) => l.textContent);
		expect(labelTexts).toContain("Zoom");
		// Terminal section also visible because "Toggle zoom pane" matches "zoom"
		expect(labelTexts).not.toContain("Git");
	});

	it("shows empty message when no shortcuts match", async () => {
		const { container } = render(() => <KeyboardShortcutsTab />);
		const input = container.querySelector("input[type='text']") as HTMLInputElement;

		fireEvent.input(input, { target: { value: "xyznonexistent" } });

		expect(container.textContent).toContain("No shortcuts match your search");
	});
});

describe("KeyboardShortcutsTab completeness", () => {
	// Actions intentionally NOT rendered as individual rebindable rows because the
	// panel surfaces them another way:
	//   - prev-tab / next-tab: shown as fixed Ctrl+Tab / Ctrl+Shift+Tab info rows
	//     (the primary binding is intercepted natively, not rebindable here).
	//   - switch-tab-N / switch-branch-N: collapsed into single "⌘1-9" / "⌘^1-9"
	//     info rows rather than nine separate entries each.
	const EXEMPT = new Set<string>([
		"prev-tab",
		"next-tab",
		...Array.from({ length: 9 }, (_, i) => `switch-tab-${i + 1}`),
		...Array.from({ length: 9 }, (_, i) => `switch-branch-${i + 1}`),
	]);

	it("renders a row for every canonical action (no silent drift from ACTION_NAMES)", () => {
		// toggle-ai-chat is gated behind the AI-chat feature flag; force it on so
		// the completeness check covers it too.
		vi.spyOn(settingsStore, "isAiChatEnabled").mockReturnValue(true);

		const displayed = new Set<string>();
		for (const section of getShortcutSections()) {
			for (const sc of section.shortcuts) {
				if (sc.action) displayed.add(sc.action);
			}
		}

		const missing = ACTION_NAMES.filter((a) => !EXEMPT.has(a) && !displayed.has(a));
		expect(missing).toEqual([]);
	});
});
