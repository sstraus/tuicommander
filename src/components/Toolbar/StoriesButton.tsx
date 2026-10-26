import { type Component, createEffect, createSignal, on, onCleanup, Show } from "solid-js";
import { t } from "../../i18n";
import { invoke } from "../../invoke";
import { storiesUi } from "../../stores/storiesUi";
import { cx } from "../../utils";
import { Button } from "../ui";
import s from "./Toolbar.module.css";

/** Counts over the active plans of one project, computed by the Rust `summary` story action. */
interface StoriesSummary {
	activePlans: number;
	inProgress: number;
	done: number;
	total: number;
}

/** Stories also move under agents and other windows, and nothing signals it: re-read slowly. */
const REFRESH_MS = 60_000;

export const StoriesButton: Component<{ repoPath: string }> = (props) => {
	const [summary, setSummary] = createSignal<StoriesSummary | null>(null);
	let request = 0;

	const refresh = async (): Promise<void> => {
		const current = ++request;
		const project = props.repoPath;
		try {
			const reply = await invoke<{ type: string; value: StoriesSummary }>("story_action_command", {
				project,
				action: { action: "summary" },
			});
			if (current === request) setSummary(reply?.type === "summary" ? reply.value : null);
		} catch {
			// No story backend, or a transient failure: keep the last counts so the button does not
			// collapse and jump. Only a repo change clears them; before any success it is the plain icon.
		}
	};

	// A new repo must never show the previous repo's counts while its own are in flight.
	createEffect(
		on(
			() => props.repoPath,
			() => {
				setSummary(null);
				void refresh();
			},
		),
	);
	// The dialog is where stories change from the UI: re-read when it closes.
	createEffect(on(storiesUi.visible, (open) => !open && void refresh(), { defer: true }));
	createEffect(() => {
		const tick = setInterval(() => !document.hidden && void refresh(), REFRESH_MS);
		const wake = () => !document.hidden && void refresh();
		window.addEventListener("focus", wake);
		document.addEventListener("visibilitychange", wake);
		onCleanup(() => {
			clearInterval(tick);
			window.removeEventListener("focus", wake);
			document.removeEventListener("visibilitychange", wake);
		});
	});

	const active = () => {
		const value = summary();
		return value && (value.activePlans > 0 || value.inProgress > 0) ? value : null;
	};
	const label = () => {
		const value = active();
		const base = t("stories.title", "Plans and Stories");
		if (!value) return base;
		return t(
			"stories.pill.label",
			"{title}: active plans {plans}, stories in progress {working}, stories done {done} of {total}",
			{
				title: base,
				plans: String(value.activePlans),
				working: String(value.inProgress),
				done: String(value.done),
				total: String(value.total),
			},
		);
	};

	return (
		<Button
			size="compact"
			variant="ghost"
			type="button"
			class={cx(s.bell, active() && s.storiesPillBtn)}
			title={label()}
			aria-label={label()}
			onClick={() => storiesUi.open(props.repoPath)}
		>
			<svg viewBox="0 0 16 16" width="15" height="15" fill="currentColor" aria-hidden="true">
				<path d="M2 2h9a2 2 0 0 1 2 2v10H4a2 2 0 0 1-2-2V2Zm2 2v8h7V4H4Zm1 2h5v1H5V6Zm0 3h5v1H5V9Zm8-5h1v10h-1V4Z" />
			</svg>
			<Show when={active()}>
				{(value) => (
					<span class={s.storiesPill} data-testid="stories-pill" aria-hidden="true">
						<span class={s.storiesCount}>{value().activePlans}</span>
						<svg class={s.storiesGlyph} viewBox="0 0 16 16" width="9" height="9" fill="currentColor">
							<path d="M4 2.5v11l9-5.5-9-5.5Z" />
						</svg>
						<span class={s.storiesCount}>{value().inProgress}</span>
						<span
							class={s.storiesTrack}
							role="progressbar"
							aria-valuemin={0}
							aria-valuenow={value().done}
							aria-valuemax={value().total}
						>
							<span
								class={s.storiesFill}
								style={{ width: `${value().total > 0 ? (100 * value().done) / value().total : 0}%` }}
							/>
						</span>
					</span>
				)}
			</Show>
		</Button>
	);
};
