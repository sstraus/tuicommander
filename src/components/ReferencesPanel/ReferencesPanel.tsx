import { type Component, For, Show } from "solid-js";
import { referencesStore } from "../../stores/references";
import { onClickKeyDown } from "../../utils/a11y";
import { openFileAction } from "../../utils/filePreview";
import p from "../shared/panel.module.css";
import s from "./ReferencesPanel.module.css";

export interface ReferencesPanelProps {
	visible: boolean;
	onClose: () => void;
}

export const ReferencesPanel: Component<ReferencesPanelProps> = (props) => {
	const handleClick = (ref: { filePath: string; line: number }) => {
		openFileAction(ref.filePath, referencesStore.repoPath, referencesStore.fsRoot || undefined, ref.line);
	};

	return (
		<div id="references-panel" class={s.panel} data-visible={props.visible}>
			<div class={p.header}>
				<div class={p.headerLeft}>
					<span class={p.title}>References</span>
					<Show when={referencesStore.references.length}>
						<span class={p.fileCountBadge}>{referencesStore.references.length}</span>
					</Show>
				</div>
				<div class={p.headerRight}>
					<button class={p.close} onClick={props.onClose} title="Close">
						×
					</button>
				</div>
			</div>
			<Show when={referencesStore.querySymbol}>
				<div class={s.queryLabel}>{referencesStore.querySymbol}</div>
			</Show>
			<Show when={!referencesStore.loading} fallback={<div class={s.empty}>Searching...</div>}>
				<Show
					when={referencesStore.references.length}
					fallback={
						<div class={s.empty}>
							{referencesStore.querySymbol ? "No references found" : "Use Shift+F12 on a symbol to find references"}
						</div>
					}
				>
					<div class={s.resultList}>
						<For each={referencesStore.references}>
							{(ref) => (
								<div
									class={s.resultItem}
									role="button"
									tabIndex={0}
									onClick={() => handleClick(ref)}
									onKeyDown={onClickKeyDown(() => handleClick(ref))}
								>
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
