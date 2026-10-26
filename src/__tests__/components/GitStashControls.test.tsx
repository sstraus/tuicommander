import { fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { expect, it, vi } from "vitest";

const h = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("../../invoke", () => ({ invoke: h.invoke }));

import { StashesTab } from "../../components/GitPanel/StashesTab";

it("catches stash controls bypassing confirmation or losing diff state", async () => {
	const stash = { index: 0, ref_name: "stash@{0}", message: "Fixture", hash: "abc" };
	h.invoke.mockImplementation(async (command: string) =>
		command === "get_stash_list" ? [stash] : command === "git_stash_show" ? "+added" : undefined,
	);
	render(() => <StashesTab repoPath="/repo" />);
	fireEvent.click(await screen.findByRole("button", { name: "Drop" }));
	expect(screen.getByText('Permanently delete "stash@{0}"? This cannot be undone.')).toBeInTheDocument();
	expect(h.invoke.mock.calls.some(([command]) => command === "git_stash_drop")).toBe(false);
	fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
	const toggle = screen.getByRole("button", { name: /Show diff/ });
	fireEvent.click(toggle);
	expect(toggle).toHaveAttribute("aria-expanded", "true");
	expect(await screen.findByText("+added")).toBeInTheDocument();
	fireEvent.click(toggle);
	expect(toggle).toHaveAttribute("aria-expanded", "false");
	expect(screen.queryByText("+added")).not.toBeInTheDocument();
	fireEvent.click(screen.getByRole("button", { name: "Apply" }));
	await waitFor(() =>
		expect(h.invoke).toHaveBeenCalledWith("git_stash_apply", { path: "/repo", stashRef: "stash@{0}" }),
	);
});
