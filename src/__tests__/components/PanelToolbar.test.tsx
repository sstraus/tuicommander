import { fireEvent, render, screen } from "@solidjs/testing-library";
import { describe, expect, it, vi } from "vitest";
import { PanelToolbar } from "../../components/shared/PanelToolbar";

describe("PanelToolbar", () => {
	it("keeps a menu item that sits behind a divider selectable", () => {
		// Catches: passing `divider` straight to Dropdown, which renders that item as a bare line,
		// so the destructive action placed last behind a separator vanishes.
		const close = vi.fn();
		render(() => (
			<PanelToolbar
				menuLabel="More actions"
				actions={[]}
				menu={[
					{ id: "copy", label: "Copy", onSelect: vi.fn() },
					{ id: "close", label: "Close", onSelect: close, divider: true },
				]}
			/>
		));
		fireEvent.click(screen.getByRole("button", { name: "More actions" }));
		expect(screen.getAllByTestId("dropdown-divider")).toHaveLength(1);
		fireEvent.click(screen.getByRole("menuitem", { name: "Close" }));
		expect(close).toHaveBeenCalledOnce();
		expect(screen.queryByRole("menu")).toBeNull();
	});

	it("draws no separator above the first item when the items before it are absent", () => {
		// Catches: Close issue alone in the menu with an empty line above it.
		render(() => (
			<PanelToolbar
				menuLabel="More actions"
				actions={[]}
				menu={[{ id: "close", label: "Close", onSelect: vi.fn(), divider: true }]}
			/>
		));
		fireEvent.click(screen.getByRole("button", { name: "More actions" }));
		expect(screen.queryAllByTestId("dropdown-divider")).toHaveLength(0);
		expect(screen.getByRole("menuitem", { name: "Close" })).toBeInTheDocument();
	});

	it("labels every fixed action with text, not only an icon", () => {
		// Catches: icon-only toolbar buttons, which UI v3 forbids.
		render(() => (
			<PanelToolbar
				menuLabel="More actions"
				actions={[{ id: "diff", label: "Diff", icon: <svg />, onClick: vi.fn() }]}
				menu={[]}
			/>
		));
		expect(screen.getByRole("button", { name: "Diff" })).toBeInTheDocument();
		expect(screen.queryByRole("button", { name: "More actions" })).toBeNull();
	});
});
