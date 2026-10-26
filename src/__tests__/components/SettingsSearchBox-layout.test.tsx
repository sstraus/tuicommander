import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { fireEvent, render } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { describe, expect, it } from "vitest";
import { SettingsSearchBox } from "../../components/SettingsPanel/SettingsSearch";
import { SettingsShell } from "../../components/SettingsPanel/SettingsShell";

// Chromium measured the shipped placeholder at 94.2817px in the sweep fixture.
// Happy DOM has no layout engine; test the production CSS space budget instead.
describe("SettingsSearchBox layout", () => {
	it.each([140, 280])("does not clip the placeholder at %ipx nav width", (width) => {
		const style = document.createElement("style");
		style.textContent = [
			"src/components/SettingsPanel/Settings.module.css",
			"src/components/ui/FormControls.module.css",
		]
			.map((file) => readFileSync(resolve(process.cwd(), file), "utf8"))
			.join("\n");
		document.head.append(style);
		try {
			const { container } = render(() => (
				<SettingsShell
					visible
					title="Settings"
					onClose={() => {}}
					tabs={[]}
					activeTab="general"
					onTabChange={() => {}}
					navWidth={width}
					navHeader={<SettingsSearchBox value="" onInput={() => {}} />}
				>
					content
				</SettingsShell>
			));
			const box = getComputedStyle(container.querySelector(".searchBox")!);
			const input = getComputedStyle(container.querySelector("input")!);
			const header = getComputedStyle(container.querySelector(".navSearch")!);
			const icon = container.querySelector("svg")!;
			const available =
				width -
				1 -
				parseFloat(header.paddingLeft) -
				parseFloat(header.paddingRight) -
				Number(icon.getAttribute("width")) -
				parseFloat(box.gap) -
				parseFloat(box.marginLeft) -
				parseFloat(box.marginRight) -
				2 -
				parseFloat(input.paddingLeft) -
				parseFloat(input.paddingRight);
			expect(available).toBeGreaterThanOrEqual(94.2817);
		} finally {
			style.remove();
		}
	});

	it("keeps query entry and the clear control functional", () => {
		const [query, setQuery] = createSignal("");
		const { getByRole } = render(() => <SettingsSearchBox value={query()} onInput={setQuery} />);
		fireEvent.input(getByRole("textbox"), { target: { value: "terminal" } });
		expect(query()).toBe("terminal");
		fireEvent.click(getByRole("button", { name: "Clear search" }));
		expect(query()).toBe("");
	});
});
