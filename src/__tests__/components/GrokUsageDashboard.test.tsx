import { render } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { formatMoney, GrokUsageDashboard } from "../../components/GrokUsageDashboard/GrokUsageDashboard";
import type { GrokUsageApiResponse } from "../../features/grokUsage";
import { providerQuotaStore } from "../../stores/providerQuota";
import { mockInvoke } from "../mocks/tauri";

const grokFixture: GrokUsageApiResponse = {
	config: {
		credit_usage_percent: 26,
		current_period: { period_type: "USAGE_PERIOD_TYPE_WEEKLY", start: "2026-09-15", end: "2026-09-22" },
		on_demand_cap: { val: 10 },
		on_demand_used: { val: 2.5 },
		prepaid_balance: null,
		is_unified_billing_user: true,
		billing_period_start: "2026-09-15",
		billing_period_end: "2026-09-22",
	},
	on_demand_enabled: true,
	subscription_tier: "X Premium+",
};

describe("GrokUsageDashboard", () => {
	beforeEach(() => {
		vi.useFakeTimers();
		mockInvoke.mockReset();
		mockInvoke.mockResolvedValue(grokFixture);
	});

	afterEach(() => {
		providerQuotaStore.stop();
		vi.useRealTimers();
	});

	it("renders provider usage and keeps missing amounts unavailable", async () => {
		const { container, unmount } = render(() => GrokUsageDashboard({}));
		await vi.advanceTimersByTimeAsync(0);

		expect(mockInvoke).toHaveBeenCalledWith("get_grok_usage_api");
		expect(container.textContent).toContain("26%");
		expect(container.textContent).toContain("X Premium+");
		expect(container.textContent).toContain("2.50");
		expect(container.textContent).toContain("--");
		unmount();
	});

	// Catches: the session billing branch returning no data even when its matching registry detail exists.
	it("a_session_dashboard_renders_its_registry_billing_detail", async () => {
		mockInvoke.mockImplementation(async (command) =>
			command === "get_provider_quotas"
				? [
						{
							key: { owner: "grok-cli", provider: "xai", credential: "/private-grok" },
							session_ids: ["private-session"],
							state: "fresh",
							snapshot: {
								windows: [],
								plan: null,
								credits: null,
								account: null,
								observed_at_ms: 1,
								source: "active",
								detail: { provider: "grok", observed_at_ms: 1, response: grokFixture },
							},
						},
					]
				: null,
		);
		const { container, unmount } = render(() => GrokUsageDashboard({ sessionId: "private-session" }));
		await vi.advanceTimersByTimeAsync(0);
		expect(container.textContent).toContain("26%");
		expect(container.textContent).toContain("X Premium+");
		expect(mockInvoke.mock.calls.map(([command]) => command)).toEqual(["get_provider_quotas"]);
		unmount();
	});

	// Catches: a session-bound click reading host billing instead of the opening session's registry detail.
	it("a_session_dashboard_never_reads_the_host_grok_account", async () => {
		mockInvoke.mockImplementation(async (command) => (command === "get_provider_quotas" ? [] : null));
		const { container, unmount } = render(() => GrokUsageDashboard({ sessionId: "unbound-session" }));
		await vi.advanceTimersByTimeAsync(0);
		expect(container.textContent).not.toContain("26%");
		expect(container.textContent).toContain("No billing data available");
		expect(mockInvoke.mock.calls.map(([command]) => command)).toEqual(["get_provider_quotas"]);
		unmount();
		providerQuotaStore.stop();
	});

	// Catches: a reused singleton waits for the timer on session-to-host changes,
	// reuses old host billing on a later open, or shows a late reply after switching.
	it("switching_session_and_host_refreshes_without_reusing_account_detail", async () => {
		let resolveHost!: (value: typeof grokFixture) => void;
		mockInvoke.mockImplementation(async (command) =>
			command === "get_provider_quotas"
				? []
				: new Promise<typeof grokFixture>((resolve) => {
						resolveHost = resolve;
					}),
		);
		const [sessionId, setSessionId] = createSignal<string | undefined>("private-session");
		const { container, unmount } = render(() => GrokUsageDashboard({ sessionId }));
		await vi.advanceTimersByTimeAsync(0);
		setSessionId(undefined);
		await vi.advanceTimersByTimeAsync(0);
		expect(mockInvoke.mock.calls.map(([command]) => command)).toEqual(["get_provider_quotas", "get_grok_usage_api"]);
		resolveHost(grokFixture);
		await vi.advanceTimersByTimeAsync(0);
		expect(container.textContent).toContain("X Premium+");
		setSessionId("other-session");
		await vi.advanceTimersByTimeAsync(0);
		expect(container.textContent).not.toContain("X Premium+");
		setSessionId(undefined);
		await vi.advanceTimersByTimeAsync(0);
		expect(container.textContent).not.toContain("X Premium+");
		const lateReply = resolveHost;
		setSessionId("third-session");
		await vi.advanceTimersByTimeAsync(0);
		lateReply(grokFixture);
		await vi.advanceTimersByTimeAsync(0);
		setSessionId(undefined);
		await vi.advanceTimersByTimeAsync(0);
		expect(container.textContent).not.toContain("X Premium+");
		resolveHost({
			...grokFixture,
			subscription_tier: "Host billing",
			config: { ...grokFixture.config, credit_usage_percent: null },
		});
		await vi.advanceTimersByTimeAsync(0);
		expect(container.textContent).toContain("Host billing");
		expect(container.textContent).toContain("Provider did not report a usage percentage.");
		expect(container.textContent).toContain("2.50");
		expect(container.textContent).not.toContain("26%");
		unmount();
	});

	it("does not turn an absent amount into zero", () => {
		expect(formatMoney(null)).toBe("--");
		expect(formatMoney(0)).toBe("0.00");
	});
});
