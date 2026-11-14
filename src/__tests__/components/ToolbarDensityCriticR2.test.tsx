import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { cleanup, fireEvent, render } from "@solidjs/testing-library";
import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("../../components/IdeLauncher", () => ({ IdeLauncher: () => <div /> }));
vi.mock("../../components/PrDetailPopover/PrDetailPopover", () => ({ PrDetailPopover: () => <div /> }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn().mockResolvedValue(undefined) }));
vi.mock("@tauri-apps/api/event", () => ({
	listen: vi.fn().mockResolvedValue(vi.fn()),
	emit: vi.fn().mockResolvedValue(undefined),
}));
vi.mock("@tauri-apps/api/window", () => ({
	getCurrentWindow: vi.fn(() => ({ listen: vi.fn().mockResolvedValue(vi.fn()), setTitle: vi.fn() })),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn().mockResolvedValue(null) }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn().mockResolvedValue(undefined) }));

import { Toolbar } from "../../components/Toolbar/Toolbar";
import { uiStore } from "../../stores/ui";

afterEach(() => {
	cleanup();
	uiStore._testCancelPendingSave();
});

describe("Toolbar density marker mode marker (critic 1351)", () => {
	it("tells rich from compact and harness: only nondefault modes carry the active class, and data-mode tracks the letter — catches default and filtered modes rendering identically", () => {
		const { container } = render(() => <Toolbar />);
		const toggle = container.querySelector("[data-testid='sidebar-density-toggle']") as HTMLElement;
		const label = () => container.querySelector("[data-testid='sidebar-density-label']")?.textContent;
		const seen: Array<[string | null, string | undefined, boolean]> = [];
		for (let i = 0; i < 3; i++) {
			seen.push([toggle.getAttribute("data-mode"), label(), toggle.className.includes("filterToggleActive")]);
			fireEvent.click(toggle);
		}
		expect(uiStore.state.sidebarDensityMode).toBe(seen[0][0]);
		expect(seen.map(([m, l]) => `${m}:${l}`).sort()).toEqual(["compact:C", "harness:H", "rich:R"]);
		expect(Object.fromEntries(seen.map(([m, , active]) => [m, active]))).toEqual({
			harness: true,
			compact: true,
			rich: false,
		});
	});
});

// Catches shared ghost styles overriding the active filter after the Button migration.
describe("Toolbar active filter colors", () => {
	it.each([false, true])(
		"keeps active filter accent regardless of stylesheet order (shared last: %s)",
		(sharedLast) => {
			const paths = ["src/components/ui/FormControls.module.css", "src/components/Toolbar/Toolbar.module.css"];
			if (sharedLast) paths.reverse();
			const sheet = document.createElement("style");
			sheet.textContent =
				":root { --accent: #007acc; --text-on-accent: #ffffff; --fg-secondary: #a0a0a0; }\n" +
				paths.map((path) => readFileSync(resolve(process.cwd(), path), "utf8")).join("\n");
			document.head.append(sheet);
			uiStore.setSidebarVisible(true);
			uiStore.setRepoFilterActiveOnly(true);
			try {
				const { container } = render(() => <Toolbar />);
				const filter = container.querySelector(
					"button[title='Showing active repos only — click to show all']",
				) as HTMLButtonElement;
				expect(filter).not.toBeNull();
				expect(getComputedStyle(filter).backgroundColor).toBe("#007acc");
				expect(getComputedStyle(filter).color).toBe("#ffffff");
				const density = container.querySelector("[data-testid='sidebar-density-toggle']") as HTMLButtonElement;
				for (let i = 0; i < 3; i++) {
					expect(getComputedStyle(density).backgroundColor).toBe(
						uiStore.state.sidebarDensityMode === "rich" ? "transparent" : "#007acc",
					);
					fireEvent.click(density);
				}
				fireEvent.click(filter);
				expect(uiStore.state.repoFilterActiveOnly).toBe(false);
				expect(getComputedStyle(filter).backgroundColor).toBe("transparent");
				expect(getComputedStyle(filter).color).toBe("#a0a0a0");
				fireEvent.click(filter);
				expect(getComputedStyle(filter).backgroundColor).toBe("#007acc");
			} finally {
				uiStore.setRepoFilterActiveOnly(false);
				sheet.remove();
			}
		},
	);
});
