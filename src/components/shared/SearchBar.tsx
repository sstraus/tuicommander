import { type Component, createEffect, createSignal, For, type JSX, Show } from "solid-js";
import { cx } from "../../utils";
import { Button } from "../ui/Button";
import { ChevronIcon } from "../ui/ChevronIcon";
import { Input } from "../ui/Input";
import type { SearchOptions } from "./DomSearchEngine";
import s from "./SearchBar.module.css";

export interface ExtraToggle {
	active: boolean;
	title: string;
	icon: () => JSX.Element;
	onToggle: () => void;
}

export interface SearchBarProps {
	visible: boolean;
	/** Bumped on every re-open request. Re-focuses the input even when the bar is
	 * already visible — see `createSearchVisibility`. */
	focusToken?: number;
	/** Called when search term or options change */
	onSearch: (term: string, opts: SearchOptions) => void;
	onNext: () => void;
	onPrev: () => void;
	onClose: () => void;
	/** 0-based active match index */
	matchIndex: number;
	/** Total number of matches. A negative value means "not counted yet" (e.g. a
	 *  huge file still being counted asynchronously) — the counter is then hidden. */
	matchCount: number;
	/** Optional label override for match count (e.g. "500+" for truncated results) */
	matchLabel?: string;
	/** Additional toggle buttons after the built-in ones */
	extraToggles?: ExtraToggle[];
	/** Replace the current match. Providing this (or onReplaceAll) enables the
	 * VS Code-style expandable replace row — omit it for search-only consumers
	 * (terminal, read-only panels). */
	onReplace?: (replacement: string) => void;
	/** Replace every match at once. */
	onReplaceAll?: (replacement: string) => void;
}

/** SVG icons for toggle buttons and navigation (VS Code style) */
const CaseSensitiveIcon = () => (
	<svg viewBox="0 0 16 16" fill="currentColor">
		<path d="M8.854 11.702h-1l-.816-2.159H3.772l-.768 2.16H2L5.09 4h.76l3.004 7.702zm-2.27-3.074L5.452 5.549a1.635 1.635 0 01-.066-.252h-.02a1.674 1.674 0 01-.07.256L4.17 8.628h2.415zM13.995 11.7v-.73c-.37.47-.955.792-1.705.792-1.2 0-2.088-.797-2.088-1.836 0-1.092.855-1.792 2.156-1.867l1.636-.09v-.362c0-.788-.49-1.257-1.328-1.257-.678 0-1.174.31-1.399.778h-.91c.153-.95 1.085-1.635 2.333-1.635 1.39 0 2.227.79 2.227 2.04V11.7h-.922z" />
	</svg>
);

const WholeWordIcon = () => (
	<svg viewBox="0 0 16 16" fill="currentColor">
		<path d="M2 6h1v7H2V6zm5.38 4.534h-.022c-.248.371-.7.596-1.205.596C5.344 11.13 4.7 10.48 4.7 9.6c0-.925.604-1.46 1.703-1.522l1-.052V7.72c0-.547-.336-.87-.918-.87-.514 0-.836.22-1.002.563H4.59c.156-.734.808-1.253 1.866-1.253 1.117 0 1.825.6 1.825 1.548v3.023h-.9v-.197zM5.604 9.6c0 .37.283.64.674.64.548 0 .96-.373.96-.836v-.45l-.864.046c-.57.034-.77.256-.77.6zM10.552 6.26c.467 0 .824.186 1.078.54V4h.904v8.73h-.9v-.65c-.258.456-.66.72-1.158.72C9.546 12.8 8.8 11.88 8.8 10.52c0-1.37.74-2.26 1.752-2.26zm.18.816c-.647 0-1.038.56-1.038 1.44s.39 1.46 1.048 1.46c.647 0 1.043-.57 1.043-1.45s-.396-1.45-1.053-1.45z" />
		<path d="M1 13h14v1H1z" />
	</svg>
);

const RegexIcon = () => (
	<svg viewBox="0 0 16 16" fill="currentColor">
		<path d="M10.012 2h.976v3.113l2.56-1.557.486.885L11.47 6l2.564 1.559-.486.885-2.56-1.557V10h-.976V6.887l-2.56 1.557-.486-.885L9.53 6 6.966 4.441l.486-.885 2.56 1.557V2zM2 10h4v4H2v-4z" />
	</svg>
);

const PrevIcon = () => (
	<svg viewBox="0 0 16 16" fill="currentColor">
		<path d="M8 3.5l-5 5h3V14h4V8.5h3l-5-5z" />
	</svg>
);

const NextIcon = () => (
	<svg viewBox="0 0 16 16" fill="currentColor">
		<path d="M8 12.5l5-5h-3V2H6v5.5H3l5 5z" />
	</svg>
);

const CloseIcon = () => (
	<svg viewBox="0 0 16 16" fill="currentColor">
		<path d="M8 8.707l3.646 3.647.708-.707L8.707 8l3.647-3.646-.707-.708L8 7.293 4.354 3.646l-.708.708L7.293 8l-3.647 3.646.708.708L8 8.707z" />
	</svg>
);

const ReplaceIcon = () => (
	<svg viewBox="0 0 16 16" fill="currentColor">
		<path d="M3.221 3.739l2.261 2.269L7.7 3.784l-.7-.7-1.012 1.007-.008-1.6a.523.523 0 01.5-.526H8V1H6.48A1.482 1.482 0 005 2.489V4.1L3.927 3.033zM10 8a2 2 0 11.732 1.547l-.733.733A3 3 0 1010 8z" />
		<path d="M1 9h6v5H1z" />
	</svg>
);

const ReplaceAllIcon = () => (
	<svg viewBox="0 0 16 16" fill="currentColor">
		<path d="M3.221 3.739l2.261 2.269L7.7 3.784l-.7-.7-1.012 1.007-.008-1.6a.523.523 0 01.5-.526H8V1H6.48A1.482 1.482 0 005 2.489V4.1L3.927 3.033zM10 8a2 2 0 11.732 1.547l-.733.733A3 3 0 1010 8z" />
		<path d="M1 6h4v1H1zm0 3h4v1H1zm0 3h6v1H1z" />
	</svg>
);

/** Visibility state for a <SearchBar>, plus the re-focus signal it needs.
 *
 * Pressing the find shortcut while the bar is ALREADY open is a focus request,
 * not a state change: after clicking into the content the input has lost focus,
 * but `visible` is still true. A plain boolean signal cannot express it — Solid
 * skips notification when the value is unchanged, so the auto-focus effect never
 * re-runs. `open()` therefore bumps a monotonic token the effect also tracks.
 */
export function createSearchVisibility() {
	const [visible, setVisible] = createSignal(false);
	const [focusToken, setFocusToken] = createSignal(0);
	return {
		visible,
		focusToken,
		/** Show the bar and (re-)focus its input, open or not. */
		open: () => {
			setVisible(true);
			setFocusToken((n) => n + 1);
		},
		close: () => setVisible(false),
	};
}

export const SearchBar: Component<SearchBarProps> = (props) => {
	const [searchTerm, setSearchTerm] = createSignal("");
	const [replaceTerm, setReplaceTerm] = createSignal("");
	const [caseSensitive, setCaseSensitive] = createSignal(false);
	const [useRegex, setUseRegex] = createSignal(false);
	const [wholeWord, setWholeWord] = createSignal(false);
	const [expanded, setExpanded] = createSignal(false);

	/** Whether this consumer wired up replace ("dove serve"). */
	const hasReplace = () => Boolean(props.onReplace || props.onReplaceAll);

	let inputRef: HTMLInputElement | undefined;

	const getOptions = (): SearchOptions => ({
		caseSensitive: caseSensitive(),
		regex: useRegex(),
		wholeWord: wholeWord(),
	});

	// Auto-focus and select when becoming visible, and again on every focusToken
	// bump. Tracked unconditionally so the effect re-runs even while `visible`
	// stays true — that repeat is the whole point (see createSearchVisibility).
	createEffect(() => {
		void props.focusToken;
		if (props.visible) {
			requestAnimationFrame(() => {
				inputRef?.focus();
				inputRef?.select();
			});
		}
	});

	const fireSearch = () => {
		props.onSearch(searchTerm(), getOptions());
	};

	const handleInput = (value: string) => {
		setSearchTerm(value);
		props.onSearch(value, getOptions());
	};

	const handleKeyDown = (e: KeyboardEvent) => {
		if (e.key === "Escape") {
			e.preventDefault();
			e.stopPropagation();
			props.onClose();
		} else if (e.key === "Enter") {
			e.preventDefault();
			if (e.shiftKey) {
				props.onPrev();
			} else {
				props.onNext();
			}
		} else if (e.key === "g" && (e.metaKey || e.ctrlKey)) {
			e.preventDefault();
			if (e.shiftKey) {
				props.onPrev();
			} else {
				props.onNext();
			}
		}
	};

	const handleReplaceKeyDown = (e: KeyboardEvent) => {
		if (e.key === "Escape") {
			e.preventDefault();
			e.stopPropagation();
			props.onClose();
		} else if (e.key === "Enter") {
			e.preventDefault();
			if (e.metaKey || e.ctrlKey) {
				props.onReplaceAll?.(replaceTerm());
			} else {
				props.onReplace?.(replaceTerm());
			}
		}
	};

	const toggleCaseSensitive = () => {
		setCaseSensitive((v) => !v);
		requestAnimationFrame(fireSearch);
	};
	const toggleRegex = () => {
		setUseRegex((v) => !v);
		requestAnimationFrame(fireSearch);
	};
	const toggleWholeWord = () => {
		setWholeWord((v) => !v);
		requestAnimationFrame(fireSearch);
	};

	const counterText = () => {
		if (!searchTerm()) return "";
		// Negative count = not computed (huge file); hide the counter entirely.
		if (props.matchCount < 0) return "";
		if (props.matchCount === 0) return "No results";
		const countLabel = props.matchLabel ?? String(props.matchCount);
		if (props.matchIndex < 0) return `${countLabel} found`;
		return `${props.matchIndex + 1} of ${countLabel}`;
	};

	return (
		<Show when={props.visible}>
			<div class={s.overlay}>
				<Show when={hasReplace()}>
					<Button
						variant="ghost"
						geometry="stretch"
						class={cx(s.expandBtn, expanded() && s.expandBtnOpen)}
						aria-expanded={expanded()}
						onClick={() => setExpanded((v) => !v)}
						title="Toggle Replace"
					>
						<ChevronIcon />
					</Button>
				</Show>

				<div class={s.rows}>
					<div class={s.row}>
						<div class={s.inputWrap}>
							<Input
								ref={inputRef}
								class={s.input}
								type="text"
								placeholder="Find…"
								value={searchTerm()}
								onInput={(e) => handleInput(e.currentTarget.value)}
								onKeyDown={handleKeyDown}
								autocomplete="off"
								autocorrect="off"
								spellcheck={false}
							/>
							<Show when={counterText()} keyed>
								{(text) => <span class={s.counter}>{text}</span>}
							</Show>
						</div>

						<div class={s.separator} />

						<Button
							variant="ghost"
							class={cx(s.toggleBtn, caseSensitive() && s.toggleActive)}
							aria-pressed={caseSensitive()}
							onClick={toggleCaseSensitive}
							title="Match Case"
						>
							<CaseSensitiveIcon />
						</Button>

						<Button
							variant="ghost"
							class={cx(s.toggleBtn, wholeWord() && s.toggleActive)}
							aria-pressed={wholeWord()}
							onClick={toggleWholeWord}
							title="Match Whole Word"
						>
							<WholeWordIcon />
						</Button>

						<Button
							variant="ghost"
							class={cx(s.toggleBtn, useRegex() && s.toggleActive)}
							aria-pressed={useRegex()}
							onClick={toggleRegex}
							title="Use Regular Expression"
						>
							<RegexIcon />
						</Button>

						<For each={props.extraToggles}>
							{(toggle) => (
								<Button
									variant="ghost"
									class={cx(s.toggleBtn, toggle.active && s.toggleActive)}
									aria-pressed={toggle.active}
									onClick={toggle.onToggle}
									title={toggle.title}
								>
									{toggle.icon()}
								</Button>
							)}
						</For>

						<div class={s.separator} />

						<Button variant="ghost" class={s.btn} onClick={() => props.onPrev()} title="Previous Match (Shift+Enter)">
							<PrevIcon />
						</Button>

						<Button variant="ghost" class={s.btn} onClick={() => props.onNext()} title="Next Match (Enter)">
							<NextIcon />
						</Button>

						<div class={s.separator} />

						<Button variant="ghost" class={s.btn} onClick={() => props.onClose()} title="Close (Escape)">
							<CloseIcon />
						</Button>
					</div>

					<Show when={hasReplace() && expanded()}>
						<div class={s.row}>
							<Input
								class={s.input}
								type="text"
								placeholder="Replace…"
								value={replaceTerm()}
								onInput={(e) => setReplaceTerm(e.currentTarget.value)}
								onKeyDown={handleReplaceKeyDown}
								autocomplete="off"
								autocorrect="off"
								spellcheck={false}
							/>

							<Button
								variant="ghost"
								class={s.btn}
								onClick={() => props.onReplace?.(replaceTerm())}
								title="Replace (Enter)"
							>
								<ReplaceIcon />
							</Button>

							<Button
								variant="ghost"
								class={s.btn}
								onClick={() => props.onReplaceAll?.(replaceTerm())}
								title="Replace All (Cmd/Ctrl+Enter)"
							>
								<ReplaceAllIcon />
							</Button>
						</div>
					</Show>
				</div>
			</div>
		</Show>
	);
};
