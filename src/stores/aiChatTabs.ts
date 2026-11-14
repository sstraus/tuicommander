import { createSignal } from "solid-js";

interface ChatTabs {
	ids: string[];
	active: string;
	pinnedRepos?: Record<string, string>;
}

const STORAGE_KEY = "tuic-ai-chat-tabs";
const [byRoot, setByRoot] = createSignal<Record<string, ChatTabs>>({});

function read(): Record<string, ChatTabs> {
	try {
		const raw = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "{}");
		if (!raw || typeof raw !== "object" || Array.isArray(raw)) return {};
		const valid: Record<string, ChatTabs> = {};
		for (const [root, tabs] of Object.entries(raw)) {
			if (!tabs || typeof tabs !== "object") continue;
			const candidate = tabs as Partial<ChatTabs>;
			if (!Array.isArray(candidate.ids) || !candidate.ids.every((id) => typeof id === "string")) continue;
			const ids = [...new Set(candidate.ids)];
			if (ids.length && typeof candidate.active === "string" && ids.includes(candidate.active)) {
				const pinnedRepos = Object.fromEntries(
					Object.entries(candidate.pinnedRepos ?? {}).filter(
						([session, path]) => ids.includes(session) && typeof path === "string" && path.length > 0,
					),
				);
				valid[root] = { ids, active: candidate.active, pinnedRepos };
			}
		}
		return valid;
	} catch {
		return {};
	}
}

function save(root: string, tabs: ChatTabs): void {
	const pinnedRepos = Object.fromEntries(
		Object.entries(tabs.pinnedRepos ?? byRoot()[root]?.pinnedRepos ?? {}).filter(([session]) =>
			tabs.ids.includes(session),
		),
	);
	const next = { ...byRoot(), ...read(), [root]: { ...tabs, pinnedRepos } };
	setByRoot(next);
	try {
		localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
	} catch {
		// Keep the open document usable when storage is unavailable.
	}
}

export const aiChatTabs = {
	ids: (root: string) => byRoot()[root]?.ids ?? [],
	pinnedRepo: (root: string, session: string) => byRoot()[root]?.pinnedRepos?.[session] ?? null,
	pinRepo(root: string, session: string, repo: string | null): void {
		const current = byRoot()[root];
		if (!current) return;
		const pinnedRepos = { ...current.pinnedRepos };
		if (repo) pinnedRepos[session] = repo;
		else delete pinnedRepos[session];
		save(root, { ...current, pinnedRepos });
	},
	active: (root: string) => byRoot()[root]?.active ?? null,
	/** Apply the host selection without publishing another selection. A compaction successor
	 *  names the tab it `replacing`, so a follower swaps it in place like the host did. */
	adopt(root: string, session: string | null, replacing?: string | null): void {
		if (session) {
			aiChatTabs.add(root, session, replacing ?? undefined);
			return;
		}
		save(root, { ids: byRoot()[root]?.ids ?? [], active: "" });
	},
	refresh(): void {
		setByRoot(read());
	},
	ensure(root: string, session: string): void {
		if (byRoot()[root]) return;
		save(root, { ids: [session], active: session });
	},
	/** Open `session` as the active tab. With `replacing`, the same conversation continues under a
	 *  new id (compaction, a missing saved session): it takes that tab's place and repository pin. */
	add(root: string, session: string, replacing?: string): void {
		const current = byRoot()[root];
		const ids = current?.ids ?? [];
		if (!replacing || replacing === session || !ids.includes(replacing)) {
			save(root, { ids: ids.includes(session) ? ids : [...ids, session], active: session });
			return;
		}
		const pinnedRepos = { ...current?.pinnedRepos };
		if (pinnedRepos[replacing]) pinnedRepos[session] = pinnedRepos[replacing];
		delete pinnedRepos[replacing];
		save(root, {
			ids: ids.filter((id) => id !== session).map((id) => (id === replacing ? session : id)),
			active: session,
			pinnedRepos,
		});
	},
	replace(root: string, session: string): void {
		save(root, { ids: [session], active: session });
	},
	close(root: string, session: string): string | null {
		const current = byRoot()[root];
		if (!current || current.ids.length < 2 || !current.ids.includes(session)) return current?.active ?? null;
		const index = current.ids.indexOf(session);
		const ids = current.ids.filter((id) => id !== session);
		const active = current.active === session ? ids[Math.max(0, index - 1)] : current.active;
		save(root, { ids, active });
		return active;
	},
	/** A deleted conversation cannot remain as the last open tab. */
	remove(root: string, session: string): string | null {
		const current = byRoot()[root];
		if (!current) return null;
		const ids = current.ids.filter((id) => id !== session);
		const active =
			current.active === session ? (ids[Math.max(0, current.ids.indexOf(session) - 1)] ?? "") : current.active;
		save(root, { ids, active });
		return active || null;
	},
	/** Recreate a fresh document in tests; the persisted store remains. */
	resetMemory(): void {
		setByRoot({});
	},
};

if (typeof window !== "undefined") {
	aiChatTabs.refresh();
	window.addEventListener("storage", (event) => {
		if (event.key === STORAGE_KEY) aiChatTabs.refresh();
	});
}
