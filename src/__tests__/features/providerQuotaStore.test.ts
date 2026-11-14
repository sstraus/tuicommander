import { expect, it, vi } from "vitest";

const handlers = new Map<string, (event: { payload: unknown }) => void>();
const invoke = vi.fn(async (command: string): Promise<unknown> => (command === "get_provider_quotas" ? [] : null));
vi.mock("../../invoke", () => ({
	invoke: (...args: [string]) => invoke(...args),
	listen: async (name: string, handler: (event: { payload: unknown }) => void) => {
		handlers.set(name, handler);
		return () => handlers.delete(name);
	},
}));

// Catches a binding created after startup remaining invisible until a session is queried.
it("binding_event_updates_store_without_a_get", async () => {
	const { providerQuotaStore } = await import("../../stores/providerQuota");
	await providerQuotaStore.start();
	invoke.mockClear();
	const key = { owner: "claude-code", provider: "anthropic", credential: "/profile", profile: null };
	handlers.get("provider-quota-binding-changed")?.({ payload: { session_id: "one", key } });
	expect(providerQuotaStore.binding("one")).toEqual(key);
	expect(invoke).not.toHaveBeenCalled();
	providerQuotaStore.stop();
});

// Catches a dashboard without a session silently choosing one of two accounts.
it("owner_selection_refuses_ambiguous_subscriptions", async () => {
	const { providerQuotaStore } = await import("../../stores/providerQuota");
	invoke.mockResolvedValueOnce(
		["one", "two"].map((id) => ({
			key: { owner: "codex-cli", provider: "openai-codex", credential: `/${id}` },
			session_ids: [id],
			state: "unavailable",
			snapshot: null,
		})),
	);
	await providerQuotaStore.start();
	expect(providerQuotaStore.binding("one")?.credential).toBe("/one");
	expect(providerQuotaStore.forOwner("codex-cli")).toBeNull();
	providerQuotaStore.stop();
});

// Catches a late explicit refresh replacing a newer registry push.
it("quota_push_wins_over_an_in_flight_refresh", async () => {
	const { providerQuotaStore } = await import("../../stores/providerQuota");
	const key = { owner: "claude-code", provider: "anthropic", credential: "/refresh" };
	const initial = { key, session_ids: ["refresh"], snapshot: null, state: "unavailable" };
	invoke.mockResolvedValueOnce([initial]);
	await providerQuotaStore.start();
	let resolve!: (value: unknown) => void;
	invoke.mockImplementationOnce(
		() =>
			new Promise((done) => {
				resolve = done;
			}),
	);
	const pending = providerQuotaStore.refresh("refresh");
	const pushed = { ...initial, state: "auth_invalid" };
	handlers.get("provider-quota-changed")?.({ payload: pushed });
	resolve(initial);
	await pending;
	expect(providerQuotaStore.view("refresh")).toEqual(pushed);
	providerQuotaStore.stop();
});
