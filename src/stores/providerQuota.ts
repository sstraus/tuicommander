import { createSignal } from "solid-js";
import { invoke, listen } from "../invoke";

export interface SubscriptionKey {
	owner: string;
	provider: string;
	credential: string;
	profile?: string | null;
}
export interface QuotaWindow {
	kind: string;
	label: string;
	used_percent: number;
	resets_at: string | null;
	window_seconds: number | null;
	model: string | null;
}
export interface QuotaView {
	key: SubscriptionKey;
	session_ids?: string[];
	snapshot: {
		detail?: { provider: "claude" | "codex"; observed_at_ms: number; response: unknown };
		windows: QuotaWindow[];
		plan: string | null;
		credits: string | null;
		account: string | null;
		observed_at_ms: number;
		source: string;
	} | null;
	state: "fresh" | "stale" | "unavailable" | "auth_invalid" | "backoff" | "contract_error";
	reason?: string;
	until_ms?: number;
	message?: string;
}
interface BindingChange {
	session_id: string;
	key: SubscriptionKey | null;
}
const keyId = (key: SubscriptionKey) => JSON.stringify([key.owner, key.provider, key.credential, key.profile ?? null]);
const [views, setViews] = createSignal<Record<string, QuotaView>>({});
const [bindings, setBindings] = createSignal<Record<string, SubscriptionKey | null>>({});
let unlisten: (() => void)[] = [];
let starting: Promise<void> | null = null;
let epoch = 0;
let changedViews = new Set<string>();
let bindingVersions = new Map<string, number>();

export const providerQuotaStore = {
	binding(sessionId: string): SubscriptionKey | null {
		return bindings()[sessionId] ?? null;
	},
	view(sessionId: string): QuotaView | null {
		const key = bindings()[sessionId];
		return key ? (views()[keyId(key)] ?? null) : null;
	},
	forOwner(owner: string): QuotaView | null {
		const keys = new Map(
			Object.values(bindings())
				.filter((key): key is SubscriptionKey => key?.owner === owner)
				.map((key) => [keyId(key), key]),
		);
		if (keys.size !== 1) return null;
		const key = keys.values().next().value;
		return key ? (views()[keyId(key)] ?? null) : null;
	},
	async refresh(sessionId: string): Promise<void> {
		const key = this.binding(sessionId);
		if (!key) return;
		const current = epoch;
		const id = keyId(key);
		const before = views()[id];
		const view = await invoke<QuotaView>("refresh_provider_quota", { key });
		if (epoch === current && views()[id] === before) setViews((previous) => ({ ...previous, [id]: view }));
	},
	start(): Promise<void> {
		if (starting) return starting;
		const current = ++epoch;
		changedViews = new Set();
		bindingVersions = new Map();
		starting = (async () => {
			const listeners = await Promise.all([
				listen<QuotaView>("provider-quota-changed", ({ payload }) => {
					changedViews.add(keyId(payload.key));
					setViews((previous) => ({ ...previous, [keyId(payload.key)]: payload }));
				}),
				listen<BindingChange>("provider-quota-binding-changed", ({ payload }) => {
					bindingVersions.set(payload.session_id, (bindingVersions.get(payload.session_id) ?? 0) + 1);
					setBindings((previous) => ({ ...previous, [payload.session_id]: payload.key }));
				}),
			]);
			if (epoch !== current) {
				listeners.forEach((fn) => fn());
				return;
			}
			unlisten = listeners;
			const initial = await invoke<QuotaView[]>("get_provider_quotas");
			if (epoch !== current) return;
			setBindings((previous) => {
				const next = { ...previous };
				for (const view of initial)
					for (const id of view.session_ids ?? []) {
						if (!bindingVersions.has(id)) next[id] = view.key;
					}
				return next;
			});
			setViews((previous) => {
				const next = { ...previous };
				for (const view of initial) if (!changedViews.has(keyId(view.key))) next[keyId(view.key)] = view;
				return next;
			});
		})();
		return starting;
	},
	stop(): void {
		++epoch;
		unlisten.forEach((fn) => fn());
		unlisten = [];
		starting = null;
		setViews({});
		setBindings({});
	},
};
