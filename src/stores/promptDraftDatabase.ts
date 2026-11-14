const PARKED_DB = "tuic-ai-chat-parked-drafts";
export const PARKED_STORE = "drafts";
let database: Promise<IDBDatabase | null> | undefined;

export function openPromptDraftDatabase(): Promise<IDBDatabase | null> {
	if (database) return database;
	if (typeof indexedDB === "undefined") return Promise.resolve(null);
	database = new Promise((resolve) => {
		try {
			const request = indexedDB.open(PARKED_DB, 1);
			request.onupgradeneeded = () => request.result.createObjectStore(PARKED_STORE);
			request.onsuccess = () => resolve(request.result);
			request.onerror = () => resolve(null);
			request.onblocked = () => resolve(null);
		} catch {
			resolve(null);
		}
	});
	return database;
}
