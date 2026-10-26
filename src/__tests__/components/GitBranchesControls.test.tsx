import { fireEvent, render, screen } from "@solidjs/testing-library";
import { describe, expect, it, vi } from "vitest";

vi.mock("../../invoke", () => ({ invoke: vi.fn().mockResolvedValue([]) }));

import { BranchesTab } from "../../components/GitPanel/BranchesTab";

describe("shared branch actions", () => {
	it("catches grouping and create cancellation becoming inert after primitive adoption", async () => {
		render(() => <BranchesTab repoPath={null} />);
		const grouping = screen.getByTitle("Disable prefix grouping");
		expect(grouping).toHaveAttribute("aria-pressed", "true");
		fireEvent.click(grouping);
		expect(grouping).toHaveAttribute("aria-pressed", "false");
		fireEvent.click(screen.getByTitle("New branch"));
		expect(screen.getByPlaceholderText("New branch name...")).toBeInTheDocument();
		fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
		expect(screen.queryByPlaceholderText("New branch name...")).not.toBeInTheDocument();
		fireEvent.click(screen.getByTitle("New branch"));
		fireEvent.keyDown(screen.getByPlaceholderText("New branch name..."), { key: "Escape" });
		expect(screen.queryByPlaceholderText("New branch name...")).not.toBeInTheDocument();
	});
});
