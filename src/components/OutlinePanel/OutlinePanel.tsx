import { type Component, createResource, For, Show } from "solid-js";
import { invoke } from "../../invoke";
import { appLogger } from "../../stores/appLogger";
import { editorTabsStore } from "../../stores/editorTabs";
import { cx } from "../../utils";
import { openFileAction } from "../../utils/filePreview";
import p from "../shared/panel.module.css";
import s from "./OutlinePanel.module.css";

interface OutlineSymbol {
	name: string;
	kind: string;
	filePath: string;
	lineStart: number;
	lineEnd: number | null;
	signature: string | null;
	scopeContext: string | null;
}

export interface OutlinePanelProps {
	visible: boolean;
	onClose: () => void;
}

const KIND_ABBREV: Record<string, string> = {
	Function: "fn",
	Method: "fn",
	Struct: "S",
	Class: "C",
	Interface: "I",
	Enum: "E",
	Const: "c",
	Variable: "v",
	Field: "f",
	Property: "p",
	Type: "T",
	TypeAlias: "T",
	Module: "M",
	Trait: "tr",
	Impl: "im",
};

function kindClass(kind: string): string {
	const normalized = kind.replace(/\s+/g, "");
	const key = `kind${normalized}` as keyof typeof s;
	return s[key] ?? s.kindDefault;
}

function nestLevel(sym: OutlineSymbol): number {
	if (!sym.scopeContext) return 0;
	const parts = sym.scopeContext.split("::").length;
	return Math.min(parts, 2);
}

export const OutlinePanel: Component<OutlinePanelProps> = (props) => {
	const activeFile = () => {
		const tab = editorTabsStore.getActive();
		if (!tab) return null;
		return { repoPath: tab.repoPath, fsRoot: tab.fsRoot, filePath: tab.filePath };
	};

	const resourceSource = () => (props.visible ? activeFile() : null);

	const [symbols] = createResource(resourceSource, async (file) => {
		if (!file) return [];
		try {
			return await invoke<OutlineSymbol[]>("mdkb_outline", {
				repoPath: file.repoPath,
				filePath: file.filePath,
			});
		} catch (e) {
			appLogger.debug("outline", "mdkb_outline failed", { error: String(e) });
			return [];
		}
	});

	const handleClick = (sym: OutlineSymbol) => {
		const file = activeFile();
		if (!file) return;
		openFileAction(sym.filePath, file.repoPath, file.fsRoot, sym.lineStart);
	};

	return (
		<div id="outline-panel" class={s.panel} data-visible={props.visible}>
			<div class={p.header}>
				<div class={p.headerLeft}>
					<span class={p.title}>Outline</span>
					<Show when={symbols()?.length}>
						<span class={p.fileCountBadge}>{symbols()!.length}</span>
					</Show>
				</div>
				<div class={p.headerRight}>
					<button class={p.close} onClick={props.onClose} title="Close">
						×
					</button>
				</div>
			</div>
			<Show when={!symbols.loading} fallback={<div class={s.empty}>Loading symbols...</div>}>
				<Show
					when={symbols()?.length}
					fallback={<div class={s.empty}>{activeFile() ? "No symbols found" : "Open a file to see its outline"}</div>}
				>
					<div class={s.symbolList}>
						<For each={symbols()}>
							{(sym) => {
								const level = nestLevel(sym);
								const nestClass = level === 1 ? s.nested1 : level >= 2 ? s.nested2 : undefined;
								return (
									<div
										class={cx(s.symbolItem, nestClass)}
										onClick={() => handleClick(sym)}
										title={sym.signature ?? sym.name}
									>
										<span class={cx(s.kindBadge, kindClass(sym.kind))}>
											{KIND_ABBREV[sym.kind] ?? sym.kind.slice(0, 2).toLowerCase()}
										</span>
										<span class={s.symbolName}>{sym.name}</span>
										<span class={s.lineNum}>{sym.lineStart}</span>
									</div>
								);
							}}
						</For>
					</div>
				</Show>
			</Show>
		</div>
	);
};

export default OutlinePanel;
