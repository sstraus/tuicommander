import { type Component, For, Show } from "solid-js";
import { t } from "../../i18n";
import { Button } from "../ui/Button";
import { Input } from "../ui/Input";
import s from "./Settings.module.css";
import type { SettingsShellTab } from "./SettingsShell";
import { entryLabel, entrySection, type SettingsSearchEntry } from "./settingsSearchIndex";

/** Search box for the Settings nav sidebar. */
export const SettingsSearchBox: Component<{ value: string; onInput: (value: string) => void }> = (props) => (
	<div class={s.searchBox}>
		<svg class={s.searchIcon} viewBox="0 0 16 16" width="12" height="12" fill="currentColor" aria-hidden="true">
			<path d="M6.5 1a5.5 5.5 0 0 1 4.38 8.83l4.15 4.14-1.06 1.06-4.14-4.15A5.5 5.5 0 1 1 6.5 1Zm0 1.5a4 4 0 1 0 0 8 4 4 0 0 0 0-8Z" />
		</svg>
		<Input
			data-non-setting="Filter settings search results"
			type="text"
			value={props.value}
			placeholder={t("settings.search.placeholder", "Search settings")}
			aria-label={t("settings.search.placeholder", "Search settings")}
			onInput={(e) => props.onInput(e.currentTarget.value)}
			autocomplete="off"
			autocorrect="off"
			spellcheck={false}
		/>
		<Show when={props.value}>
			<Button
				variant="ghost"
				data-non-setting="Search or navigate the catalogue without reading owner values"
				class={s.searchClear}
				aria-label={t("settings.search.clear", "Clear search")}
				onClick={() => props.onInput("")}
			>
				&times;
			</Button>
		</Show>
	</div>
);

/** Results list, shown in place of the active tab while a query is typed. */
export const SettingsSearchResults: Component<{
	results: SettingsSearchEntry[];
	tabs: SettingsShellTab[];
	onSelect: (entry: SettingsSearchEntry) => void;
}> = (props) => {
	const tabLabel = (key: string) => props.tabs.find((tab) => tab.key === key)?.label ?? key;

	return (
		<div class={s.section}>
			<Show
				when={props.results.length > 0}
				fallback={<p class={s.hint}>{t("settings.search.empty", "No settings match your search.")}</p>}
			>
				<div class={s.searchResults}>
					<For each={props.results}>
						{(entry) => (
							<button
								data-control-layout="row"
								data-non-setting="Search or navigate the catalogue without reading owner values"
								class={s.searchResult}
								onClick={() => props.onSelect(entry)}
							>
								<span class={s.searchResultLabel}>
									{entryLabel(entry) ?? entrySection(entry)}
									<Show when={entry.expert}>
										<span class={s.expertBadge} data-expert-badge>
											{t("settings.expert.badge", "Expert")}
										</span>
									</Show>
								</span>
								<span class={s.searchResultTrail}>
									{tabLabel(entry.tab)} › {entrySection(entry)}
								</span>
							</button>
						)}
					</For>
				</div>
			</Show>
		</div>
	);
};

/** Text of an element's own text nodes, ignoring nested elements.
 *
 * A section heading may carry a trailing info badge and a toggle label sits in
 * a `<span>` next to a checkbox — in both cases only the direct text counts. */
function ownText(el: Element): string {
	return [...el.childNodes]
		.filter((n) => n.nodeType === Node.TEXT_NODE)
		.map((n) => n.textContent ?? "")
		.join("")
		.trim();
}

function findHeading(root: ParentNode, heading: string): HTMLHeadingElement | null {
	// A page and one of its cards may share a title (for example Terminal).
	// Controls belong to the card; use the page title only when no card matches.
	for (const selector of ["h3", "h2"]) {
		for (const el of root.querySelectorAll<HTMLHeadingElement>(selector)) {
			if (ownText(el) === heading) return el;
		}
	}
	return null;
}

/** The element carrying `label`, searched only between `heading` and the next
 * `<h3>` — a label like "Terminal" occurs in more than one section. */
function findLabelInSection(root: ParentNode, heading: HTMLHeadingElement, label: string): Element | null {
	const all = [...root.querySelectorAll("h2, h3, label, span")];
	const start = all.indexOf(heading);
	if (start < 0) return null;
	for (const el of all.slice(start + 1)) {
		if (el.tagName === "H2" || el.tagName === "H3") return null;
		if (ownText(el) === label) return el;
	}
	return null;
}

/**
 * Scroll the settings content to a setting, or to the section holding it.
 *
 * Matching is on rendered text, not on an `id`: the index is derived from that
 * same text (see `settingsSearchIndex.ts`), so the drift test keeps the two in
 * step, and no tab needs an id attribute added to every section and field.
 *
 * Returns false when the section is not on screen — it can sit behind a
 * collapsed form or an `isTauri()` guard. A miss must leave the scroll position
 * alone; scrolling to an approximate target is worse than not scrolling.
 */
export function scrollToSetting(root: ParentNode, section: string, label?: string): boolean {
	const heading = findHeading(root, section);
	if (!heading) return false;
	const target = label ? findLabelInSection(root, heading, label) : null;
	(target ?? heading).scrollIntoView({ block: "start", behavior: "smooth" });
	return true;
}
