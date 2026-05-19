import { type Component, createEffect, createSignal } from "solid-js";
import { appLogger } from "../../stores/appLogger";
import { SearchBar } from "../shared/SearchBar";
import type { CanvasTerminalRef } from "./CanvasTerminal";

export interface TerminalSearchProps {
	visible: boolean;
	canvasRef?: CanvasTerminalRef | undefined;
	onClose: () => void;
}

export const TerminalSearch: Component<TerminalSearchProps> = (props) => {
	const [resultIndex, setResultIndex] = createSignal(-1);
	const [resultCount, setResultCount] = createSignal(0);
	const [blockScope, setBlockScope] = createSignal(false);

	let lastTerm = "";

	createEffect(() => {
		if (!props.visible) {
			props.canvasRef?.searchClear();
			setResultIndex(-1);
			setResultCount(0);
		}
	});

	const handleSearch = (term: string) => {
		lastTerm = term;
		if (term) {
			props.canvasRef
				?.searchFind(term, blockScope())
				.then(({ index, count }) => {
					setResultIndex(index);
					setResultCount(count);
				})
				.catch((e) => {
					appLogger.warn("terminal", "searchFind failed", { error: e });
				});
		} else {
			props.canvasRef?.searchClear();
			setResultIndex(-1);
			setResultCount(0);
		}
	};

	const handleNext = () => {
		if (!lastTerm) return;
		const { index, count } = props.canvasRef?.searchNext() ?? { index: -1, count: 0 };
		setResultIndex(index);
		setResultCount(count);
	};

	const handlePrev = () => {
		if (!lastTerm) return;
		const { index, count } = props.canvasRef?.searchPrev() ?? { index: -1, count: 0 };
		setResultIndex(index);
		setResultCount(count);
	};

	const handleToggleBlockScope = () => {
		const newVal = !blockScope();
		setBlockScope(newVal);
		if (lastTerm) {
			props.canvasRef
				?.searchFind(lastTerm, newVal)
				.then(({ index, count }) => {
					setResultIndex(index);
					setResultCount(count);
				})
				.catch((e) => {
					appLogger.warn("terminal", "searchFind failed", { error: e });
				});
		}
	};

	return (
		<SearchBar
			visible={props.visible}
			onSearch={handleSearch}
			onNext={handleNext}
			onPrev={handlePrev}
			onClose={props.onClose}
			matchIndex={resultIndex()}
			matchCount={resultCount()}
			extraToggles={[
				{
					active: blockScope(),
					title: "Search in Block (Cmd+Shift+B)",
					icon: () => (
						<svg viewBox="0 0 16 16" fill="currentColor">
							<path d="M2 3h12v1H2V3zm0 4h12v1H2V7zm0 4h8v1H2v-1z" />
							<rect x="12" y="10" width="3" height="3" rx="0.5" />
						</svg>
					),
					onToggle: handleToggleBlockScope,
				},
			]}
		/>
	);
};
