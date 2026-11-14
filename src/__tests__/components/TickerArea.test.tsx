import { cleanup, fireEvent, render, screen } from "@solidjs/testing-library";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { TickerArea } from "../../components/StatusBar/TickerArea";
import { destroyAgentActivity, initAgentActivity } from "../../features/agentActivity";
import { activityDashboardStore } from "../../stores/activityDashboard";
import { activityStore } from "../../stores/activityStore";
import { appLogger } from "../../stores/appLogger";
import { statusBarTicker } from "../../stores/statusBarTicker";
import { terminalsStore } from "../../stores/terminals";

beforeAll(() => activityStore.hydrate());
let agentId: string;
beforeEach(() => {
	vi.spyOn(appLogger, "info").mockImplementation(() => {});
	vi.stubGlobal(
		"ResizeObserver",
		class {
			observe() {}
			disconnect() {}
		},
	);
	statusBarTicker.clear();
	activityDashboardStore.close();
	agentId = terminalsStore.add({
		sessionId: "ticker-agent",
		name: "Ticker agent",
		fontSize: 14,
		cwd: "/ticker",
		awaitingInput: null,
		agentType: "claude",
	});
	terminalsStore.update(agentId, { agentState: "working" });
	initAgentActivity();
});
afterEach(() => {
	cleanup();
	destroyAgentActivity();
	terminalsStore.remove(agentId);
	statusBarTicker.clear();
	activityDashboardStore.close();
	vi.unstubAllGlobals();
	vi.restoreAllMocks();
});

const visibleLabel = () => screen.getAllByText("Agents").find((el) => !el.closest('[aria-hidden="true"]'))!;

describe("Agents ticker navigation", () => {
	it("opens the dashboard from its label — catches an unwired ticker click", () => {
		render(() => <TickerArea />);
		fireEvent.click(visibleLabel());
		expect(activityDashboardStore.state.isOpen).toBe(true);
	});

	it("opens from counts and stays open on a second click — catches toggle instead of open", () => {
		render(() => <TickerArea />);
		const counts = screen.getAllByText("1 working · 0 idle").find((el) => !el.closest('[aria-hidden="true"]'))!;
		fireEvent.click(counts);
		expect(activityDashboardStore.state.isOpen).toBe(true);
		fireEvent.click(counts);
		expect(activityDashboardStore.state.isOpen).toBe(true);
	});

	it.each(["Enter", " "])("opens with %j — catches mouse-only activation", (key) => {
		render(() => <TickerArea />);
		const action = screen.getByRole("button", { name: /Agents/ });
		expect(action.tabIndex).toBe(0);
		fireEvent.keyDown(action, { key });
		expect(activityDashboardStore.state.isOpen).toBe(true);
	});

	it("cycles without opening and leaves probes inert — catches click bubbling from the badge", () => {
		statusBarTicker.addMessage({ id: "other", pluginId: "other", text: "Other ticker", priority: 20, ttlMs: 0 });
		const view = render(() => <TickerArea />);
		const probe = view.container.querySelector('[aria-hidden="true"]')!;
		fireEvent.click(probe.querySelector("span")!);
		expect(activityDashboardStore.state.isOpen).toBe(false);
		expect(probe.querySelector('[role="button"], [tabindex]')).toBeNull();
		fireEvent.click(screen.getByTitle("Click to cycle tickers"));
		expect(activityDashboardStore.state.isOpen).toBe(false);
		expect(screen.getAllByText("Other ticker").some((el) => !el.closest('[aria-hidden="true"]'))).toBe(true);
	});
});
