// @vitest-environment jsdom
import { cleanup, fireEvent, render } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { createStore } from "solid-js/store";
import { afterEach, expect, it, vi } from "vitest";
import { SessionsScreen } from "../screens/SessionsScreen";
import { reconcileSessions, type SessionInfo, type SessionState } from "../useSessions";

afterEach(() => {
	cleanup();
	vi.useRealTimers();
});

function session(id: string, state: Partial<SessionState>): SessionInfo {
	return {
		session_id: id,
		display_name: id,
		cwd: "/work/project",
		worktree_path: null,
		worktree_branch: null,
		state: { awaiting_input: false, rate_limited: false, last_activity_ms: 0, shell_state: "idle", ...state },
	};
}

function header(view: ReturnType<typeof render>) {
	return Array.from(view.container.querySelectorAll('[data-testid="metric-card"]'), (tile) => tile.textContent);
}

// Catches: counting raw awaiting_input or total sessions instead of the row's prioritized status.
it("matches Working, Idle and Awaiting row badges and excludes finished and exited sessions", async () => {
	const [sessions, setSessions] = createSignal([
		session("working", { agent_state: "working" }),
		session("shell", { shell_state: "busy" }),
		session("idle", { agent_state: "idle" }),
		session("question", { agent_state: "awaiting_input" }),
		session("finished", { agent_state: "completed" }),
		session("error", { awaiting_input: true, last_error: "Failed" }),
		session("limited", { awaiting_input: true, rate_limited: true }),
		session("delegated", { active_sub_tasks: 1 }),
	]);
	const onSelect = vi.fn();
	const view = render(() => (
		<SessionsScreen
			sessions={sessions()}
			loading={false}
			refreshing={false}
			error={null}
			onRefresh={() => {}}
			onSelectSession={onSelect}
		/>
	));
	expect(header(view)).toEqual(["3Working", "1Idle", "1Awaiting"]);
	const badges = Array.from(view.container.querySelectorAll("span"), (element) => element.textContent);
	expect(badges.filter((label) => label === "Working")).toHaveLength(3);
	expect(badges.filter((label) => label === "Idle")).toHaveLength(2);
	expect(badges.filter((label) => label === "Input")).toHaveLength(1);
	await fireEvent.click(view.getByRole("button", { name: "1Awaiting" }));
	expect(onSelect).toHaveBeenCalledWith("question");
	// Exited sessions leave the live snapshot; they must also leave the counts.
	setSessions((current) => current.filter((item) => item.session_id !== "working"));
	expect(header(view)).toEqual(["2Working", "1Idle", "1Awaiting"]);
});

// Catches: the header switches to Idle while the row still holds its Working badge.
it("shares the row's Working hold and immediate question override", () => {
	vi.useFakeTimers();
	const [item, setItem] = createStore(session("agent", { agent_state: "working" }));
	const view = render(() => (
		<SessionsScreen
			sessions={[item]}
			loading={false}
			refreshing={false}
			error={null}
			onRefresh={() => {}}
			onSelectSession={() => {}}
		/>
	));
	setItem("state", "agent_state", "idle");
	expect(header(view)).toEqual(["1Working", "0Idle", "0Awaiting"]);
	vi.advanceTimersByTime(2000);
	expect(header(view)).toEqual(["0Working", "1Idle", "0Awaiting"]);
	setItem("state", "agent_state", "working");
	setItem("state", "agent_state", "awaiting_input");
	expect(header(view)).toEqual(["0Working", "0Idle", "1Awaiting"]);
});

// catches: the busy hold dies when the poll hands the screen a brand-new object for the changed session
it("keeps Working for the hold window when reconcileSessions replaces the session object", () => {
	vi.useFakeTimers();
	const mk = (shell: "busy" | "idle"): SessionInfo => ({
		session_id: "s1",
		cwd: "/w",
		worktree_path: null,
		worktree_branch: null,
		display_name: "S1",
		state: {
			awaiting_input: false,
			rate_limited: false,
			last_activity_ms: 0,
			agent_type: "claude",
			shell_state: shell,
		},
	});
	const [list, setList] = createSignal<SessionInfo[]>([mk("busy")]);
	const view = render(() => (
		<SessionsScreen
			sessions={list()}
			loading={false}
			refreshing={false}
			error={null}
			onRefresh={() => {}}
			onSelectSession={() => {}}
		/>
	));
	setList((prev) => reconcileSessions(prev, [mk("idle")]));
	vi.advanceTimersByTime(500);
	const numbers = view.getAllByTestId("metric-number").map((n) => n.textContent);
	expect(numbers.slice(0, 2)).toEqual(["1", "0"]);
	expect(view.getByRole("button", { name: "Open session S1" }).textContent).toContain("Working");
	vi.advanceTimersByTime(1500);
	expect(header(view)).toEqual(["0Working", "1Idle", "0Awaiting"]);
	expect(view.getByRole("button", { name: "Open session S1" }).textContent).toContain("Idle");
});

// Catches: an agent running delegated work is omitted from the Working total.
it("counts Sub-tasks rows as Working and stops when delegated work ends", () => {
	const [sessions, setSessions] = createSignal([session("parent", { active_sub_tasks: 2 })]);
	const view = render(() => (
		<SessionsScreen
			sessions={sessions()}
			loading={false}
			refreshing={false}
			error={null}
			onRefresh={() => {}}
			onSelectSession={() => {}}
		/>
	));
	expect(view.getByText("Sub-tasks")).toBeTruthy();
	expect(header(view)).toEqual(["1Working", "0Idle", "0Awaiting"]);
	setSessions((previous) => reconcileSessions(previous, [session("parent", { active_sub_tasks: 0 })]));
	expect(header(view)).toEqual(["0Working", "1Idle", "0Awaiting"]);
});

// Catches: a removed session's accessor reads a missing snapshot, or a re-added id inherits the old Working hold.
it("drops a removed session's hold and starts a re-added id fresh", () => {
	vi.useFakeTimers();
	const [list, setList] = createSignal<SessionInfo[]>([
		session("a", { agent_state: "working" }),
		session("b", { agent_state: "idle" }),
	]);
	const view = render(() => (
		<SessionsScreen
			sessions={list()}
			loading={false}
			refreshing={false}
			error={null}
			onRefresh={() => {}}
			onSelectSession={() => {}}
		/>
	));
	expect(header(view)).toEqual(["1Working", "1Idle", "0Awaiting"]);
	setList((prev) => prev.filter((item) => item.session_id !== "a"));
	expect(header(view)).toEqual(["0Working", "1Idle", "0Awaiting"]);
	vi.advanceTimersByTime(3000);
	setList((prev) => [session("a", { agent_state: "idle" }), ...prev]);
	expect(header(view)).toEqual(["0Working", "2Idle", "0Awaiting"]);
	setList((prev) => [...prev].reverse());
	expect(header(view)).toEqual(["0Working", "2Idle", "0Awaiting"]);
});
