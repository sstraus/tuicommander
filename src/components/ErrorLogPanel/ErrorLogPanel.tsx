import { type Component, createEffect, createMemo, createSignal, For, onCleanup, Show } from "solid-js";
import {
	type AppLogAudience,
	type AppLogEntry,
	type AppLogLevel,
	type AppLogSource,
	appLogger,
} from "../../stores/appLogger";
import { errorLogStore } from "../../stores/errorLog";
import s from "./ErrorLogPanel.module.css";

/** Audience tabs. "User" (default) hides app-internal telemetry so it can't bury
 *  user-relevant signal; "Diagnostics" shows only telemetry; "All" shows both. */
const AUDIENCE_OPTIONS: Array<{ value: AppLogAudience | "all"; label: string }> = [
	{ value: "user", label: "User" },
	{ value: "diagnostic", label: "Diagnostics" },
	{ value: "all", label: "All" },
];

/** True if an entry's audience (defaulting to "user") passes the selected tab. */
export function audiencePasses(audience: AppLogAudience | undefined, filter: AppLogAudience | "all"): boolean {
	return filter === "all" || (audience ?? "user") === filter;
}

const LEVEL_OPTIONS: Array<{ value: AppLogLevel | "all"; label: string }> = [
	{ value: "all", label: "All" },
	{ value: "error", label: "Error" },
	{ value: "warn", label: "Warn" },
	{ value: "info", label: "Info" },
	{ value: "debug", label: "Debug" },
];

// Severity rank: picking a level shows that level and everything more severe
// (e.g. "Warn" intermingles Warn + Error), matching common log-viewer behaviour.
const LEVEL_SEVERITY: Record<AppLogLevel, number> = { debug: 0, info: 1, warn: 2, error: 3 };

/** True if an entry of `level` should be shown under the selected `filter`.
 *  "all" shows everything; any level shows itself and all more-severe levels. */
export function levelPassesThreshold(level: AppLogLevel, filter: AppLogLevel | "all"): boolean {
	return filter === "all" || LEVEL_SEVERITY[level] >= LEVEL_SEVERITY[filter];
}

const SOURCE_OPTIONS: Array<{ value: AppLogSource | "all"; label: string }> = [
	{ value: "all", label: "All Sources" },
	{ value: "app", label: "App" },
	{ value: "plugin", label: "Plugin" },
	{ value: "git", label: "Git" },
	{ value: "network", label: "Network" },
	{ value: "terminal", label: "Terminal" },
	{ value: "github", label: "GitHub" },
	{ value: "dictation", label: "Dictation" },
	{ value: "store", label: "Store" },
	{ value: "config", label: "Config" },
];

function formatTime(ts: number): string {
	const d = new Date(ts);
	return d.toLocaleTimeString("en-GB", { hour12: false, hour: "2-digit", minute: "2-digit", second: "2-digit" });
}

function levelClass(level: AppLogLevel): string {
	switch (level) {
		case "error":
			return s.levelError;
		case "warn":
			return s.levelWarn;
		case "info":
			return s.levelInfo;
		case "debug":
			return s.levelDebug;
	}
}

function rowClass(level: AppLogLevel): string {
	switch (level) {
		case "error":
			return s.rowError;
		case "warn":
			return s.rowWarn;
		default:
			return "";
	}
}

function formatEntryForClipboard(entry: AppLogEntry): string {
	const time = formatTime(entry.timestamp);
	const data =
		entry.data !== undefined ? ` ${typeof entry.data === "string" ? entry.data : JSON.stringify(entry.data)}` : "";
	return `[${time}] [${entry.level.toUpperCase()}] [${entry.source}] ${entry.message}${data}`;
}

export const ErrorLogPanel: Component = () => {
	const [audienceFilter, setAudienceFilter] = createSignal<AppLogAudience | "all">("user");
	const [levelFilter, setLevelFilter] = createSignal<AppLogLevel | "all">("all");
	const [sourceFilter, setSourceFilter] = createSignal<AppLogSource | "all">("all");
	const [searchText, setSearchText] = createSignal("");

	let listRef: HTMLDivElement | undefined;
	let searchRef: HTMLInputElement | undefined;

	const isOpen = () => errorLogStore.state.isOpen;

	// Mark errors as seen while panel is open (including new arrivals)
	createEffect(() => {
		if (isOpen()) {
			appLogger.entryCount(); // subscribe to new entries
			appLogger.markSeen();
		}
	});

	// Focus search input when panel opens
	createEffect(() => {
		if (isOpen()) {
			requestAnimationFrame(() => searchRef?.focus());
		}
	});

	// Keyboard navigation
	createEffect(() => {
		if (!isOpen()) return;

		const handleKeydown = (e: KeyboardEvent) => {
			if (e.key === "Escape") {
				e.preventDefault();
				e.stopPropagation();
				errorLogStore.close();
			}
		};

		document.addEventListener("keydown", handleKeydown, true);
		onCleanup(() => document.removeEventListener("keydown", handleKeydown, true));
	});

	// Auto-scroll to bottom when new entries arrive
	createEffect(() => {
		if (!isOpen()) return;
		appLogger.entryCount(); // subscribe to changes
		requestAnimationFrame(() => {
			if (listRef) {
				listRef.scrollTop = listRef.scrollHeight;
			}
		});
	});

	const filteredEntries = createMemo(() => {
		const entries = appLogger.getEntries();
		const audience = audienceFilter();
		const level = levelFilter();
		const source = sourceFilter();
		const search = searchText().toLowerCase();

		return entries.filter((entry) => {
			if (!audiencePasses(entry.audience, audience)) return false;
			if (!levelPassesThreshold(entry.level, level)) return false;
			if (source !== "all" && entry.source !== source) return false;
			if (search && !entry.message.toLowerCase().includes(search)) return false;
			return true;
		});
	});

	/** Count of diagnostic entries — surfaced on the Diagnostics tab so the user
	 *  knows telemetry exists even while the default view hides it. */
	const diagnosticCount = createMemo(() => {
		appLogger.entryCount(); // subscribe
		return appLogger.getEntries().filter((e) => (e.audience ?? "user") === "diagnostic").length;
	});

	const handleCopy = (entry: AppLogEntry) => {
		navigator.clipboard.writeText(formatEntryForClipboard(entry)).catch(() => {});
	};

	const handleCopyAll = () => {
		const text = filteredEntries().map(formatEntryForClipboard).join("\n");
		navigator.clipboard.writeText(text).catch(() => {});
	};

	return (
		<Show when={isOpen()}>
			<div class={s.overlay} onClick={() => errorLogStore.close()}>
				<div class={s.panel} onClick={(e) => e.stopPropagation()}>
					{/* Header */}
					<div class={s.header}>
						<h3>Error Log</h3>
						<button class={s.close} onClick={() => errorLogStore.close()}>
							&times;
						</button>
					</div>

					{/* Filters */}
					<div class={s.filters}>
						<div class={s.levelFilters}>
							<For each={AUDIENCE_OPTIONS}>
								{(opt) => (
									<button
										class={`${s.levelBtn} ${audienceFilter() === opt.value ? s.levelBtnActive : ""}`}
										onClick={() => setAudienceFilter(opt.value)}
										title={
											opt.value === "user"
												? "Issues relevant to you (default)"
												: opt.value === "diagnostic"
													? "App-internal telemetry (TUIC debugging)"
													: "Show every entry"
										}
									>
										{opt.label}
										{opt.value === "diagnostic" && diagnosticCount() > 0 ? ` (${diagnosticCount()})` : ""}
									</button>
								)}
							</For>
						</div>

						<div class={s.levelFilters}>
							<For each={LEVEL_OPTIONS}>
								{(opt) => (
									<button
										class={`${s.levelBtn} ${levelFilter() === opt.value ? s.levelBtnActive : ""}`}
										onClick={() => setLevelFilter(opt.value)}
										title={opt.value === "all" ? "Show all log levels" : `Show ${opt.label} and more severe levels`}
									>
										{opt.label}
									</button>
								)}
							</For>
						</div>

						<select
							class={s.sourceSelect}
							value={sourceFilter()}
							onChange={(e) => setSourceFilter(e.currentTarget.value as AppLogSource | "all")}
						>
							<For each={SOURCE_OPTIONS}>{(opt) => <option value={opt.value}>{opt.label}</option>}</For>
						</select>

						<input
							ref={searchRef}
							class={s.searchInput}
							type="text"
							placeholder="Filter messages..."
							value={searchText()}
							onInput={(e) => setSearchText(e.currentTarget.value)}
						/>

						<button class={s.clearBtn} onClick={() => appLogger.clear()}>
							Clear
						</button>
					</div>

					{/* Log list */}
					<div class={s.list} ref={listRef}>
						<Show when={filteredEntries().length === 0}>
							<div class={s.empty}>
								{appLogger.entryCount() === 0 ? "No log entries" : "No entries match the current filters"}
							</div>
						</Show>

						<For each={filteredEntries()}>
							{(entry) => (
								<div class={`${s.row} ${rowClass(entry.level)}`}>
									<span class={s.time}>{formatTime(entry.timestamp)}</span>
									<span class={`${s.level} ${levelClass(entry.level)}`}>{entry.level}</span>
									<span class={s.source}>{entry.source}</span>
									<span class={s.message}>
										{entry.message}
										{entry.data !== undefined && (
											<span style={{ color: "var(--fg-muted)" }}>
												{" "}
												{typeof entry.data === "string" ? entry.data : JSON.stringify(entry.data, null, 2)}
											</span>
										)}
									</span>
									<button class={s.copyBtn} onClick={() => handleCopy(entry)} title="Copy to clipboard">
										<svg viewBox="0 0 24 24" width="12" height="12" fill="currentColor">
											<path d="M16 1H4c-1.1 0-2 .9-2 2v14h2V3h12V1zm3 4H8c-1.1 0-2 .9-2 2v14c0 1.1.9 2 2 2h11c1.1 0 2-.9 2-2V7c0-1.1-.9-2-2-2zm0 16H8V7h11v14z" />
										</svg>
									</button>
								</div>
							)}
						</For>
					</div>

					{/* Footer */}
					<div class={s.footer}>
						<span>
							{filteredEntries().length} of {appLogger.entryCount()} entries
						</span>
						<span style={{ "margin-left": "auto", display: "flex", "align-items": "center", gap: "12px" }}>
							<button class={s.clearBtn} onClick={handleCopyAll} title="Copy all visible entries">
								Copy All
							</button>
							<span>Esc to close</span>
						</span>
					</div>
				</div>
			</div>
		</Show>
	);
};
