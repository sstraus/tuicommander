import { afterEach, beforeEach, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/event", () => ({ emitTo: vi.fn() }));

async function freshStore() {
	vi.resetModules();
	return (await import("../stores/storiesUi")).storiesUi;
}

beforeEach(() => localStorage.clear());
afterEach(() => localStorage.clear());

it("hides Done items by default", async () => {
	// Catches: a first-time user opening the panel and seeing Done items.
	expect((await freshStore()).visibility()).toBe("open");
});

it("restores the chosen status filter after reopen", async () => {
	// Catches: hide-Done setting lost after reopen.
	const first = await freshStore();
	first.setVisibility("all");
	expect((await freshStore()).visibility()).toBe("all");
});

it("ignores an unknown stored value", async () => {
	// Catches: a corrupt stored value reaching the backend filter.
	localStorage.setItem("tuic.stories.visibility", "bogus");
	expect((await freshStore()).visibility()).toBe("open");
});
