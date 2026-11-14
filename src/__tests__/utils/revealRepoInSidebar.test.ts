import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../invoke", () => ({
	invoke: vi.fn().mockResolvedValue(undefined),
	listen: vi.fn().mockResolvedValue(vi.fn()),
}));

import { repositoriesStore } from "../../stores/repositories";
import { revealRepoInSidebar } from "../../utils/revealRepoInSidebar";

describe("revealRepoInSidebar", () => {
	let scrolled: string[];

	beforeEach(() => {
		scrolled = [];
		document.body.innerHTML = "";
		vi.stubGlobal("requestAnimationFrame", (cb: FrameRequestCallback) => {
			cb(0);
			return 0;
		});
		Element.prototype.scrollIntoView = function (this: Element) {
			scrolled.push(this.getAttribute("data-sidebar-repo") ?? "");
		};
	});

	afterEach(() => {
		repositoriesStore._testCancelPendingSave();
		vi.unstubAllGlobals();
	});

	function mountRow(path: string) {
		const row = document.createElement("div");
		row.setAttribute("data-sidebar-repo", path);
		document.body.appendChild(row);
	}

	it("ctrl_p_repo_pick_reveals_repo_in_sidebar: scrolls the picked repo row into view", () => {
		const path = "/reveal/a b";
		repositoriesStore.add({ path, displayName: "A" });
		mountRow(path);

		revealRepoInSidebar(path);

		// catches: quick-open switches the tab but never touches the sidebar scroll
		expect(scrolled).toEqual([path]);
	});

	it("expands a collapsed group and a collapsed repo", () => {
		const path = "/reveal/grouped";
		repositoriesStore.add({ path, displayName: "G" });
		const groupId = repositoriesStore.createGroup("Team") as string;
		repositoriesStore.addRepoToGroup(path, groupId);
		repositoriesStore.toggleGroupCollapsed(groupId);
		repositoriesStore.toggleCollapsed(path);
		expect(repositoriesStore.getGroupForRepo(path)?.collapsed).toBe(true);
		expect(repositoriesStore.get(path)?.collapsed).toBe(true);

		revealRepoInSidebar(path);

		// catches: the row stays hidden inside a collapsed group
		expect(repositoriesStore.getGroupForRepo(path)?.collapsed).toBe(false);
		expect(repositoriesStore.get(path)?.collapsed).toBe(false);
	});

	it("leaves an already expanded group alone", () => {
		const path = "/reveal/open";
		repositoriesStore.add({ path, displayName: "O" });
		const groupId = repositoriesStore.createGroup("Open") as string;
		repositoriesStore.addRepoToGroup(path, groupId);

		revealRepoInSidebar(path);

		// catches: an unconditional toggle collapses a group that was open
		expect(repositoriesStore.getGroupForRepo(path)?.collapsed).toBeFalsy();
	});
});
