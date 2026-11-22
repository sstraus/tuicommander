import { fireEvent, render, screen } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../invoke", () => ({ invoke: vi.fn(() => Promise.resolve(null)), listen: vi.fn() }));

import { makeTerminal } from "../helpers/store";

type Stores = {
	terminals: typeof import("../../stores/terminals").terminalsStore;
	repos: typeof import("../../stores/repositories").repositoriesStore;
	needsYou: typeof import("../../stores/needsYou");
};

async function load(): Promise<Stores> {
	vi.resetModules();
	return {
		terminals: (await import("../../stores/terminals")).terminalsStore,
		repos: (await import("../../stores/repositories")).repositoriesStore,
		needsYou: await import("../../stores/needsYou"),
	};
}

function agentIn(stores: Stores, repo: string, branch: string, name: string): string {
	if (!stores.repos.state.repositories[repo]) stores.repos.add({ path: repo, displayName: repo.slice(1) });
	stores.repos.setWorkspace(repo, branch, { branchName: branch });
	const id = stores.terminals.add(makeTerminal({ name, agentType: "claude" }));
	stores.repos.addTerminalToWorkspace(repo, branch, id);
	return id;
}

describe("Needs you", () => {
	beforeEach(() => vi.useFakeTimers({ toFake: ["Date"] }));
	afterEach(() => vi.useRealTimers());

	// Catches: the list (and Cmd+U) visiting sessions in creation order instead of
	// putting the one forgotten longest first, and listing sessions nobody waits on.
	it("lists only waiting sessions, oldest first, with repo, branch and question", async () => {
		const stores = await load();
		const first = agentIn(stores, "/ego", "feat/goal", "a");
		const second = agentIn(stores, "/mdkb", "fix/lock", "b");
		agentIn(stores, "/ego", "main", "idle");

		vi.setSystemTime(1_000);
		stores.terminals.setAwaitingInput(second, "error");
		expect(stores.needsYou.waitingEntries().map((e) => e.id)).toEqual([second]);
		vi.setSystemTime(5_000);
		stores.terminals.update(first, { questionText: "Proceed with 41 files?" });
		stores.terminals.setAwaitingInput(first, "question");

		const entries = stores.needsYou.waitingEntries();
		expect(entries.map((e) => e.id)).toEqual([second, first]);
		expect(entries[1]).toMatchObject({
			repoName: "ego",
			branchName: "feat/goal",
			question: "Proceed with 41 files?",
			kind: "question",
		});
		expect(stores.needsYou.waitingCountForRepo("/ego")).toBe(1);

		stores.terminals.clearAwaitingInput(second);
		expect(stores.needsYou.waitingEntries().map((e) => e.id)).toEqual([first]);
	});

	// Catches: a pill that stays on screen with nothing waiting, or a list that does not open the session.
	it("toolbar pill appears only while someone waits and opens the chosen session", async () => {
		const stores = await load();
		const { NeedsYouButton } = await import("../../components/Toolbar/NeedsYouButton");
		const id = agentIn(stores, "/ego", "feat/goal", "a");
		agentIn(stores, "/ego", "main", "other");
		render(() => <NeedsYouButton />);
		expect(screen.queryByTestId("needs-you")).toBeNull();

		stores.terminals.update(id, { questionText: "Ship it?" });
		stores.terminals.setAwaitingInput(id, "question");
		const pill = await screen.findByTestId("needs-you");
		expect(pill.textContent).toContain("Needs you");
		expect(pill.textContent).toContain("1");

		fireEvent.click(pill);
		const row = await screen.findByText(/ego \/ feat\/goal · Ship it\?/);
		fireEvent.click(row);
		expect(stores.terminals.state.activeId).toBe(id);

		stores.terminals.clearAwaitingInput(id);
		expect(screen.queryByTestId("needs-you")).toBeNull();
	});
});

describe("NeedsYouBanner placement", () => {
	const viewport = { top: 100, bottom: 300 } as DOMRect;
	const row = (top: number, height = 20) =>
		({ getBoundingClientRect: () => ({ top, bottom: top + height, height }) }) as unknown as HTMLElement;

	// Catches: a row scrolled out of the list counted as visible, so the banner never shows.
	it("classifies rows against the visible part of the list", async () => {
		const { placementOf } = await import("../../components/Sidebar/NeedsYouBanner");
		expect(placementOf(null, viewport)).toBe("hidden");
		expect(placementOf(row(150, 0), viewport)).toBe("hidden");
		expect(placementOf(row(60), viewport)).toBe("above");
		expect(placementOf(row(320), viewport)).toBe("below");
		expect(placementOf(row(150), viewport)).toBe("visible");
		expect(placementOf(row(290), viewport)).toBe("visible");
	});

	// Catches: a waiting session inside a closed repository staying invisible, and a banner
	// click that neither opens the repository nor the session.
	it("shows a closed repository's waiting session and reveals it on click", async () => {
		const stores = await load();
		const { NeedsYouBanner } = await import("../../components/Sidebar/NeedsYouBanner");
		const id = agentIn(stores, "/ego", "feat/goal", "a");
		stores.repos.toggleExpanded("/ego");
		expect(stores.repos.state.repositories["/ego"].expanded).toBe(false);
		const scroller = document.createElement("div");
		document.body.append(scroller);
		render(() => <NeedsYouBanner scroller={() => scroller} />);
		expect(screen.queryByTestId("needs-you-banner")).toBeNull();

		stores.terminals.setAwaitingInput(id, "question");
		const banner = await screen.findByTestId("needs-you-banner");
		expect(banner.textContent).toContain("1 need you");
		expect(banner.textContent).toContain("ego");

		fireEvent.click(banner);
		expect(stores.repos.state.repositories["/ego"].expanded).toBe(true);
		expect(stores.terminals.state.activeId).toBe(id);
		scroller.remove();
	});
});
