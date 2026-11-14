import { createEffect, createRoot, createSignal, on } from "solid-js";
import { invoke } from "../invoke";
import { subscribeEvents } from "../transport";
import { handleOpenUrl } from "../utils/openUrl";
import { reconcileById, sameData } from "../utils/reconcileById";
import { appLogger } from "./appLogger";
import { repositoriesStore } from "./repositories";

/** A manual check, or a Fabbrica help request, approval or outside request;
 *  see `boss_attention.rs` and `fabbrica_attention.rs`. */
export interface WaitingItem {
	id: string;
	kind: "manual_check" | "help" | "approval" | "outside_request";
	title: string;
	detail: string;
	options: { id: string; label: string }[];
	project: string;
	planId: string | null;
	story: { id: string; title: string; status: string } | null;
}
export interface AgentHealth {
	sessionId: string;
	connectionId: string | null;
	name: string;
	kind: string;
	reason: string;
}
/** Last read of the Fabbrica control MCP upstream. Only `ready` (or no Fabbrica
 *  at all) makes its item list complete. */
export interface FabbricaStatus {
	state: "absent" | "connect" | "unknown" | "ready";
	reason?: string;
	/** The Fabbrica console, where its items are answered. */
	console: string | null;
}
export interface WaitingSnapshot {
	items: WaitingItem[];
	health: AgentHealth[];
	/** Sources that could not be read. Their items are absent, not resolved. */
	warnings: string[];
	fabbrica: FabbricaStatus;
}

const OUTSIDE_PREFIX = "fabbrica:outside:";

/** Fabbrica items belong to no local project; their ids are `fabbrica:<task>:…`. */
export function isFabbricaId(id: string): boolean {
	return id.startsWith("fabbrica:");
}

/** Answer a Fabbrica outside request once; the next read drops it from the section and the bell. */
export async function answerOutsideRequest(item: WaitingItem, decision: "accept" | "decline"): Promise<void> {
	await invoke("answer_outside_request", { requestId: item.id.slice(OUTSIDE_PREFIX.length), decision });
}

/** Whether a snapshot lists every Fabbrica item, so a missing one is resolved. */
export function fabbricaComplete(snapshot: WaitingSnapshot): boolean {
	const state = snapshot.fabbrica?.state ?? "absent";
	return state === "ready" || state === "absent";
}

/** Fabbrica items are answered in the Fabbrica console. */
export function openFabbricaConsole(): void {
	const url = snapshot()?.fabbrica?.console;
	if (url) handleOpenUrl(url);
	else appLogger.warn("panel", "Fabbrica console address is unknown");
}

const [snapshot, setSnapshot] = createSignal<WaitingSnapshot>();
const [failed, setFailed] = createSignal(false);
const [loading, setLoading] = createSignal(true);
const [liveFailed, setLiveFailed] = createSignal(false);

/** Coalesces bursts of invalidations into one backend read. */
const REFRESH_DEBOUNCE_MS = 250;

let holders = 0;
/** Holders that show agent health; only they need session events. */
let healthHolders = 0;
let generation = 0;
let timer: ReturnType<typeof setTimeout> | undefined;
let ready = false;
let running = false;
let dirty = false;
let stop: (() => void) | undefined;

/** Keep every unchanged row object, and the whole snapshot when nothing changed. */
function reconcile(prev: WaitingSnapshot | undefined, next: WaitingSnapshot): WaitingSnapshot {
	const items = reconcileById(next.items, prev?.items, (item) => item.id);
	const health = reconcileById(next.health, prev?.health, (row) => `${row.connectionId ?? ""}:${row.sessionId}`);
	const warnings = reconcileById(next.warnings, prev?.warnings, (warning) => warning);
	const fabbrica = prev && sameData(prev.fabbrica, next.fabbrica) ? prev.fabbrica : next.fabbrica;
	if (
		prev &&
		items === prev.items &&
		health === prev.health &&
		warnings === prev.warnings &&
		fabbrica === prev.fabbrica
	) {
		return prev;
	}
	return { items, health, warnings, fabbrica };
}

function schedule(): void {
	if (timer !== undefined) clearTimeout(timer);
	timer = setTimeout(() => {
		timer = undefined;
		void refresh();
	}, REFRESH_DEBOUNCE_MS);
}

function scheduleHealth(): void {
	if (healthHolders > 0) schedule();
}

// Serialize invalidations. A response overtaken by a change is never published as current.
async function refresh(): Promise<void> {
	dirty = true;
	if (!ready || running || holders === 0) return;
	const current = generation;
	running = true;
	setLoading(true);
	try {
		while (dirty && current === generation) {
			dirty = false;
			try {
				const next = await invoke<WaitingSnapshot>("boss_attention");
				if (current === generation && !dirty) {
					setSnapshot((prev) => reconcile(prev, next));
					setFailed(false);
				}
			} catch (error) {
				if (current === generation && !dirty) {
					setSnapshot(undefined);
					setFailed(true);
					appLogger.warn("panel", "Could not refresh waiting items", error);
				}
			}
		}
	} finally {
		running = false;
		if (current === generation) setLoading(false);
	}
}

function start(): () => void {
	const current = generation;
	const disposeRoot = createRoot((dispose) => {
		// Story files edited outside TUIC reach the projection only through the repository watcher.
		// The first run only subscribes: the initial read follows the event subscription.
		createEffect(
			on(
				() => {
					for (const path of Object.keys(repositoriesStore.state.repositories)) repositoriesStore.getRevision(path);
				},
				schedule,
				{ defer: true },
			),
		);
		return dispose;
	});
	let unsubscribe: (() => void) | undefined;
	void subscribeEvents(
		{
			"boss-attention-changed": schedule,
			// Repository sets are not covered by boss-attention-changed.
			"repositories-changed": schedule,
			"remote-connection-status": schedule,
			// Session events change only agent health, so they never reread stories without a health view.
			"session-state-changed": scheduleHealth,
			"session-created": scheduleHealth,
			"session-closed": scheduleHealth,
		},
		{ onResync: schedule },
	)
		.then((stopListening) => {
			if (current !== generation) {
				stopListening();
				return;
			}
			unsubscribe = stopListening;
			ready = true;
			void refresh();
		})
		.catch((error: unknown) => {
			if (current !== generation) return;
			ready = true;
			setLiveFailed(true);
			appLogger.warn("panel", "Could not subscribe to waiting items", error);
			void refresh();
		});
	return () => {
		disposeRoot();
		unsubscribe?.();
	};
}

function reset(): void {
	generation++;
	stop?.();
	stop = undefined;
	if (timer !== undefined) clearTimeout(timer);
	timer = undefined;
	holders = 0;
	healthHolders = 0;
	ready = false;
	running = false;
	dirty = false;
	setSnapshot(undefined);
	setFailed(false);
	setLiveFailed(false);
	setLoading(true);
}

/** One shared projection for the bell, Plans and Stories and the dashboard's health view. */
export const waitingItemsStore = {
	snapshot,
	failed,
	liveFailed,
	loading,
	refresh,
	/** Keep the projection live while the caller needs it. Returns the release. */
	retain(options: { health?: boolean } = {}): () => void {
		holders++;
		if (options.health) healthHolders++;
		if (holders === 1) stop = start();
		const owner = generation;
		let released = false;
		return () => {
			// A holder from before a reset no longer counts.
			if (released || owner !== generation) return;
			released = true;
			holders--;
			if (options.health) healthHolders--;
			if (holders === 0) reset();
		};
	},
	_resetForTest: reset,
};
