import { createSignal } from "solid-js";
import { invoke } from "../invoke";
import { appLogger } from "./appLogger";

interface ReferenceLocation {
	filePath: string;
	line: number;
	name: string;
}

const [references, setReferences] = createSignal<ReferenceLocation[]>([]);
const [querySymbol, setQuerySymbol] = createSignal<string | null>(null);
const [repoPath, setRepoPath] = createSignal<string>("");
const [fsRoot, setFsRoot] = createSignal<string>("");
const [loading, setLoading] = createSignal(false);

async function findReferences(repo: string, fs: string, symbolName: string): Promise<void> {
	setRepoPath(repo);
	setFsRoot(fs);
	setQuerySymbol(symbolName);
	setLoading(true);
	try {
		const results = await invoke<ReferenceLocation[]>("mdkb_references", {
			repoPath: repo,
			symbolName,
		});
		setReferences(results);
	} catch (e) {
		appLogger.debug("references", "mdkb_references failed", { error: String(e) });
		setReferences([]);
	} finally {
		setLoading(false);
	}
}

export const referencesStore = {
	get references() {
		return references();
	},
	get querySymbol() {
		return querySymbol();
	},
	get repoPath() {
		return repoPath();
	},
	get fsRoot() {
		return fsRoot();
	},
	get loading() {
		return loading();
	},
	findReferences,
};
