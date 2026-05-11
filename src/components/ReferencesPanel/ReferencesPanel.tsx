import { type Component, createSignal, For, Show } from "solid-js";
import { invoke } from "../../invoke";
import { appLogger } from "../../stores/appLogger";
import { openFileAction } from "../../utils/filePreview";
import p from "../shared/panel.module.css";
import s from "./ReferencesPanel.module.css";

interface ReferenceLocation {
	filePath: string;
	line: number;
	name: string;
}

export interface ReferencesPanelProps {
	visible: boolean;
	onClose: () => void;
}

// DEFERRED (2026-05-11) — migrate to referencesStore.ts to avoid module-level singleton
// signals. Current approach works with single panel instance but breaks with detach/multi-window.
const [references, setReferences] = createSignal<ReferenceLocation[]>([]);
const [querySymbol, setQuerySymbol] = createSignal<string | null>(null);
const [repoPath, setRepoPath] = createSignal<string>("");
const [fsRoot, setFsRoot] = createSignal<string>("");
const [loading, setLoading] = createSignal(false);

export async function findReferences(repo: string, fs: string, symbolName: string): Promise<void> {
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

export const ReferencesPanel: Component<ReferencesPanelProps> = (props) => {
	const handleClick = (ref: ReferenceLocation) => {
		openFileAction(ref.filePath, repoPath(), fsRoot() || undefined, ref.line);
	};

	return (
		<div class={s.panel} data-visible={props.visible}>
			<div class={p.header}>
				<div class={p.headerLeft}>
					<span class={p.title}>References</span>
					<Show when={references().length}>
						<span class={p.fileCountBadge}>{references().length}</span>
					</Show>
				</div>
				<div class={p.headerRight}>
					<button class={p.close} onClick={props.onClose} title="Close">
						×
					</button>
				</div>
			</div>
			<Show when={querySymbol()}>
				<div class={s.queryLabel}>
					{querySymbol()}
				</div>
			</Show>
			<Show
				when={!loading()}
				fallback={<div class={s.empty}>Searching...</div>}
			>
				<Show
					when={references().length}
					fallback={
						<div class={s.empty}>
							{querySymbol() ? "No references found" : "Use Shift+F12 on a symbol to find references"}
						</div>
					}
				>
					<div class={s.resultList}>
						<For each={references()}>
							{(ref) => (
								<div class={s.resultItem} onClick={() => handleClick(ref)}>
									<span class={s.resultName}>{ref.name}</span>
									<span class={s.resultFile}>
										{ref.filePath}
										<span class={s.resultLine}>:{ref.line}</span>
									</span>
								</div>
							)}
						</For>
					</div>
				</Show>
			</Show>
		</div>
	);
};

export default ReferencesPanel;
