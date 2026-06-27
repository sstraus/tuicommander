import { render } from "@solidjs/testing-library";
import { describe, expect, it } from "vitest";
import { BranchIcon } from "../../components/Sidebar/RepoSection";

/** Read the color class off the rendered icon span. */
function iconClass(el: HTMLElement | null): string {
	return el?.querySelector(".branchIcon")?.className ?? "";
}

describe("BranchIcon color", () => {
	it("a worktree with no open terminal is idle (grey), even if the repo has tabs elsewhere", () => {
		const { container } = render(() => (
			<BranchIcon isMainBranch={false} isMainWorktree={false} branchHasTerminals={false} />
		));
		expect(iconClass(container)).toContain("branchIconIdle");
		expect(iconClass(container)).not.toContain("branchIconWorktree");
	});

	it("a worktree with an open terminal shows the worktree (green) base color", () => {
		const { container } = render(() => (
			<BranchIcon isMainBranch={false} isMainWorktree={false} branchHasTerminals={true} />
		));
		expect(iconClass(container)).toContain("branchIconWorktree");
	});

	it("the main branch with an open terminal shows the main (yellow) base color", () => {
		const { container } = render(() => (
			<BranchIcon isMainBranch={true} isMainWorktree={true} branchHasTerminals={true} />
		));
		expect(iconClass(container)).toContain("branchIconMain");
	});

	it("the main branch with no open terminal is idle (grey)", () => {
		const { container } = render(() => (
			<BranchIcon isMainBranch={true} isMainWorktree={true} branchHasTerminals={false} />
		));
		expect(iconClass(container)).toContain("branchIconIdle");
	});

	it("busy/question/error override the idle and base colors", () => {
		const busy = render(() => (
			<BranchIcon isMainBranch={false} isMainWorktree={false} branchHasTerminals={true} hasBusy={true} />
		));
		expect(iconClass(busy.container)).toContain("branchIconActivity");

		const question = render(() => (
			<BranchIcon isMainBranch={false} isMainWorktree={false} branchHasTerminals={false} hasQuestion={true} />
		));
		expect(iconClass(question.container)).toContain("branchIconQuestion");
	});
});
