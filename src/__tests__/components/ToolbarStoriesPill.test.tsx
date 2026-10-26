import { render, screen, waitFor } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../invoke", () => ({ invoke: vi.fn(), listen: vi.fn().mockResolvedValue(vi.fn()) }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn().mockResolvedValue(undefined) }));
vi.mock("@tauri-apps/api/event", () => ({
	listen: vi.fn().mockResolvedValue(vi.fn()),
	emit: vi.fn().mockResolvedValue(undefined),
}));
vi.mock("@tauri-apps/api/window", () => ({
	getCurrentWindow: vi.fn(() => ({
		listen: vi.fn().mockResolvedValue(vi.fn()),
		setTitle: vi.fn().mockResolvedValue(undefined),
	})),
}));
vi.mock("../../components/IdeLauncher", () => ({ IdeLauncher: () => <div /> }));
vi.mock("../../components/PrDetailPopover/PrDetailPopover", () => ({ PrDetailPopover: () => <div /> }));

import { Toolbar } from "../../components/Toolbar/Toolbar";
import { invoke } from "../../invoke";
import { storiesUi } from "../../stores/storiesUi";

type Summary = { activePlans: number; inProgress: number; done: number; total: number };
const summaryCalls = () =>
	vi
		.mocked(invoke)
		.mock.calls.filter(
			([cmd, args]) =>
				cmd === "story_action_command" && (args as { action: { action: string } }).action.action === "summary",
		);
const reply = (value: Summary) => ({ type: "summary", value });
const idle: Summary = { activePlans: 0, inProgress: 0, done: 0, total: 0 };
const busy: Summary = { activePlans: 2, inProgress: 3, done: 5, total: 8 };

function mockSummary(...values: Array<Summary | Promise<Summary>>) {
	const queue = [...values];
	vi.mocked(invoke).mockImplementation(async (cmd: string) => {
		if (cmd !== "story_action_command") return undefined;
		const next = (queue.length > 1 ? queue.shift() : queue[0]) as Summary | Promise<Summary>;
		return reply(await next);
	});
}

describe("Plans and Stories toolbar button", () => {
	beforeEach(() => {
		vi.mocked(invoke).mockReset();
		storiesUi.close();
	});

	it("stays the bare icon when no plan is active", async () => {
		mockSummary(idle);
		render(() => <Toolbar repoPath="/repo" />);
		await waitFor(() => expect(summaryCalls()).toHaveLength(1));
		const button = screen.getByRole("button", { name: "Plans and Stories" });
		expect(button.querySelector("[data-testid='stories-pill']")).toBeNull();
		expect(button.textContent).toBe("");
	});

	it("widens into a pill with counts and progress in words when work is active", async () => {
		mockSummary(busy);
		render(() => <Toolbar repoPath="/repo" />);
		const button = await screen.findByRole("button", {
			name: "Plans and Stories: active plans 2, stories in progress 3, stories done 5 of 8",
		});
		expect(button.getAttribute("title")).toBe(button.getAttribute("aria-label"));
		expect(button.querySelector("[data-testid='stories-pill']")?.textContent).toBe("23");
		const bar = button.querySelector("[role='progressbar']");
		expect(bar?.getAttribute("aria-valuenow")).toBe("5");
		expect(bar?.getAttribute("aria-valuemax")).toBe("8");
	});

	it("drops the pill when the last in-progress story completes and the dialog closes", async () => {
		mockSummary(busy, idle);
		render(() => <Toolbar repoPath="/repo" />);
		await screen.findByRole("button", { name: /active plans 2/ });
		storiesUi.open("/repo");
		storiesUi.close();
		await waitFor(() => expect(screen.getByRole("button", { name: "Plans and Stories" })).toBeTruthy());
		expect(document.querySelector("[data-testid='stories-pill']")).toBeNull();
	});

	it("forgets the previous repo's counts at once when the repo changes", async () => {
		let resolveB: (value: Summary) => void = () => {};
		const pendingB = new Promise<Summary>((resolve) => {
			resolveB = resolve;
		});
		mockSummary(busy, pendingB);
		const [repo, setRepo] = createSignal("/a");
		render(() => <Toolbar repoPath={repo()} />);
		await screen.findByRole("button", { name: /active plans 2/ });
		setRepo("/b");
		await waitFor(() => expect(document.querySelector("[data-testid='stories-pill']")).toBeNull());
		resolveB(idle);
	});

	it("keeps the last counts when a later poll fails", async () => {
		let calls = 0;
		vi.mocked(invoke).mockImplementation(async (cmd: string) => {
			if (cmd !== "story_action_command") return undefined;
			if (++calls > 1) throw new Error("offline");
			return reply(busy);
		});
		render(() => <Toolbar repoPath="/repo" />);
		await screen.findByRole("button", { name: /active plans 2/ });
		storiesUi.open("/repo");
		storiesUi.close();
		await waitFor(() => expect(calls).toBe(2));
		await new Promise((r) => setTimeout(r, 20));
		expect(document.querySelector("[data-testid='stories-pill']")).not.toBeNull();
	});

	it("keeps the bare icon when the summary call fails", async () => {
		vi.mocked(invoke).mockImplementation(async (cmd: string) => {
			if (cmd === "story_action_command") throw new Error("no backend");
			return undefined;
		});
		render(() => <Toolbar repoPath="/repo" />);
		await waitFor(() => expect(summaryCalls()).toHaveLength(1));
		expect(document.querySelector("[data-testid='stories-pill']")).toBeNull();
	});

	it("re-reads on a 60 s tick while the window is visible", async () => {
		vi.useFakeTimers({ toFake: ["setInterval", "clearInterval"] });
		try {
			mockSummary(idle);
			render(() => <Toolbar repoPath="/repo" />);
			await waitFor(() => expect(summaryCalls()).toHaveLength(1));
			await vi.advanceTimersByTimeAsync(60_000);
			expect(summaryCalls()).toHaveLength(2);
		} finally {
			vi.useRealTimers();
		}
	});
});
