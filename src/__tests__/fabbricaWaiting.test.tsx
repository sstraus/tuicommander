import { fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { listen } from "@tauri-apps/api/event";
import { createRoot } from "solid-js";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { WaitingOnYou } from "../components/StoriesDialog/WaitingOnYou";
import { useWaitingItems } from "../hooks/useWaitingItems";
import { activityStore } from "../stores/activityStore";
import { storiesUi } from "../stores/storiesUi";
import { waitingItemsStore } from "../stores/waitingItems";
import { mockInvoke } from "./mocks/tauri";

// The item `fabbrica_attention.rs` builds from the recorded help_list fixture
// (src-tauri/src/fixtures/fabbrica/help_list.json).
const approval = {
	id: "fabbrica:asks:connection/1/session:q1",
	kind: "approval",
	title: "Push branch feat/x?",
	detail: "Fabbrica approval · test-agent",
	options: [{ id: "allow", label: "Allow once" }],
	project: "",
	planId: null,
	runId: null,
	story: null,
};
const manualCheck = {
	id: "story:s1",
	kind: "manual_check",
	title: "Review the layout",
	detail: "Manual checks are waiting.",
	options: [],
	project: "/reviewed-project",
	planId: "p1",
	runId: null,
	story: { id: "s1", title: "Review the layout", status: "qa" },
};
const CONSOLE = "http://127.0.0.1:8080/console/";
const ready = { state: "ready", console: CONSOLE };

let snapshot: { items: unknown[]; health: unknown[]; warnings: string[]; fabbrica: unknown };

beforeEach(async () => {
	snapshot = { items: [approval, manualCheck], health: [], warnings: [], fabbrica: ready };
	mockInvoke.mockImplementation(async (command: string) =>
		command === "boss_attention" ? structuredClone(snapshot) : undefined,
	);
	await activityStore.hydrate();
});

afterEach(() => {
	storiesUi.close();
	activityStore.clearAll();
	waitingItemsStore._resetForTest();
	vi.restoreAllMocks();
	vi.clearAllMocks();
});

const reads = () => mockInvoke.mock.calls.filter(([command]) => command === "boss_attention").length;

/** Deliver the backend invalidation the Fabbrica poller sends, and wait for the reread. */
async function emitChanged(): Promise<void> {
	const before = reads();
	for (const [name, handler] of vi.mocked(listen).mock.calls) {
		if (name === "boss-attention-changed") handler({ event: name, id: 1, payload: {} });
	}
	await waitFor(() => expect(reads()).toBeGreaterThan(before));
	await waitFor(() => expect(waitingItemsStore.loading()).toBe(false));
}

const bellTitles = () =>
	activityStore.state.items.filter((item) => item.sectionId === "waiting").map((item) => item.title);

function mountBellAndSection(): () => void {
	const dispose = createRoot((disposeRoot) => {
		useWaitingItems();
		return disposeRoot;
	});
	// Fabbrica items show in every project's section, not only one project's.
	render(() => <WaitingOnYou project="/another-project" onSelect={() => {}} />);
	return dispose;
}

// Catches: an outage reads as nothing to do — Fabbrica unknown renders as an
// empty section and the bell drops the entries it already notified.
it("unreachable_fabbrica_shows_unknown_not_empty", async () => {
	const dispose = mountBellAndSection();
	try {
		expect(await screen.findByRole("button", { name: /Push branch feat\/x\?/ })).toBeInTheDocument();
		await waitFor(() => expect(bellTitles()).toContain("Push branch feat/x?"));
		snapshot = {
			items: [manualCheck],
			health: [],
			warnings: [],
			fabbrica: { state: "unknown", reason: "tasks_list: connection refused", console: CONSOLE },
		};
		await emitChanged();
		const section = screen.getByRole("region", { name: "Waiting on you" });
		expect(section).toHaveTextContent("Fabbrica state is unknown: tasks_list: connection refused");
		expect(bellTitles()).toContain("Push branch feat/x?");
	} finally {
		dispose();
	}
});

// Catches: a missing OAuth consent reads as nothing to do.
it("fabbrica_without_consent_asks_to_connect", async () => {
	snapshot = { items: [], health: [], warnings: [], fabbrica: { state: "connect", console: CONSOLE } };
	render(() => <WaitingOnYou project="/another-project" onSelect={() => {}} />);
	const section = await screen.findByRole("region", { name: "Waiting on you" });
	expect(section).toHaveTextContent("Connect Fabbrica");
});

// Catches: a stale item stays after the answer — the section or the bell keeps
// a Fabbrica question that the next complete read no longer lists.
it("answered_fabbrica_item_leaves_view", async () => {
	const dispose = mountBellAndSection();
	try {
		await screen.findByRole("button", { name: /Push branch feat\/x\?/ });
		await waitFor(() => expect(bellTitles().sort()).toEqual(["Push branch feat/x?", "Review the layout"]));
		snapshot = { items: [manualCheck], health: [], warnings: [], fabbrica: ready };
		await emitChanged();
		await waitFor(() => expect(screen.queryByRole("button", { name: /Push branch feat\/x\?/ })).toBeNull());
		expect(bellTitles()).toEqual(["Review the layout"]);
	} finally {
		dispose();
	}
});

// Catches: a Fabbrica row or bell entry routes into Plans and Stories (no
// project, no story) instead of the Fabbrica console where it is answered.
it("fabbrica_item_opens_the_console", async () => {
	const open = vi.spyOn(window, "open").mockImplementation(() => null);
	const opened = () => [
		...open.mock.calls.map(([url]) => String(url)),
		...mockInvoke.mock.calls
			.filter(([command]) => command === "open_default_app")
			.map(([, args]) => (args as { target: string }).target),
	];
	const dispose = mountBellAndSection();
	try {
		await fireEvent.click(await screen.findByRole("button", { name: /Push branch feat\/x\?/ }));
		await waitFor(() => expect(opened()).toEqual([CONSOLE]));
		const entry = activityStore.state.items.find((item) => item.title === "Push branch feat/x?");
		entry?.onClick?.();
		await waitFor(() => expect(opened()).toEqual([CONSOLE, CONSOLE]));
		expect(storiesUi.visible()).toBe(false);
	} finally {
		dispose();
	}
});

// The item `fabbrica_attention.rs` builds from the recorded outside_requests_list fixture
// (src-tauri/src/fixtures/fabbrica/outside_requests_list.json).
const outsideRequest = {
	id: "fabbrica:outside:d1",
	kind: "outside_request",
	title: "Run https://github.com/acme/app/issues/5#d1?",
	detail: "Fabbrica outside request · acme/app · carol: ask please look at the build",
	options: [],
	project: "",
	planId: null,
	runId: null,
	story: null,
};

// Catches: outside requests missing or notified twice — the row or bell entry is absent,
// duplicated after a repeat read, or kept after the request leaves the ask state.
it("fabbrica_outside_request_listed_once", async () => {
	snapshot = { items: [outsideRequest], health: [], warnings: [], fabbrica: ready };
	const dispose = mountBellAndSection();
	try {
		const title = outsideRequest.title;
		await screen.findByRole("button", { name: /Run https:\/\/github.com\/acme\/app\/issues\/5#d1/ });
		await waitFor(() => expect(bellTitles()).toEqual([title]));
		await emitChanged();
		expect(screen.getAllByRole("button", { name: /issues\/5#d1/ })).toHaveLength(1);
		expect(bellTitles()).toEqual([title]);
		snapshot = { items: [], health: [], warnings: [], fabbrica: ready };
		await emitChanged();
		await waitFor(() => expect(screen.queryByRole("button", { name: /issues\/5#d1/ })).toBeNull());
		expect(bellTitles()).toEqual([]);
	} finally {
		dispose();
	}
});

// Catches: the Accept action calls the wrong command or id — it must send the Fabbrica
// request id (without the `fabbrica:outside:` prefix) and the decision, and nothing else.
it("fabbrica_outside_request_answer_sends_request_id_and_decision", async () => {
	snapshot = { items: [outsideRequest], health: [], warnings: [], fabbrica: ready };
	const dispose = mountBellAndSection();
	try {
		await fireEvent.click(await screen.findByRole("button", { name: "Accept" }));
		await waitFor(() =>
			expect(mockInvoke).toHaveBeenCalledWith("answer_outside_request", { requestId: "d1", decision: "accept" }),
		);
		await fireEvent.click(screen.getByRole("button", { name: "Decline" }));
		await waitFor(() =>
			expect(mockInvoke).toHaveBeenCalledWith("answer_outside_request", { requestId: "d1", decision: "decline" }),
		);
	} finally {
		dispose();
	}
});
