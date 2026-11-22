import { type Component, createSignal, onCleanup, Show } from "solid-js";
import { t } from "../../i18n";
import { type WaitingEntry, waitingAge, waitingEntries } from "../../stores/needsYou";
import { cx } from "../../utils";
import { keyFor } from "../../utils/hotkey";
import { navigateToTerminal } from "../../utils/navigateToTerminal";
import { Button, Dropdown, type DropdownItem } from "../ui";
import s from "./Toolbar.module.css";

function entryLabel(entry: WaitingEntry, now: number): string {
	const where = [entry.repoName, entry.branchName].filter(Boolean).join(" / ") || entry.id;
	const what =
		entry.question ?? (entry.kind === "error" ? t("needsYou.error", "Error") : t("needsYou.question", "Question"));
	return `${where} · ${what} · ${waitingAge(entry.since, now)}`;
}

/** Always-visible pill in the toolbar centre: how many agents wait for the user, and where. */
export const NeedsYouButton: Component = () => {
	const [open, setOpen] = createSignal(false);
	const [now, setNow] = createSignal(Date.now());
	let trigger: HTMLButtonElement | undefined;
	// Ages only move while the list is open; one coarse tick is enough.
	const tick = setInterval(() => open() && setNow(Date.now()), 30_000);
	onCleanup(() => clearInterval(tick));

	const items = (): DropdownItem[] =>
		waitingEntries().map((entry) => ({
			id: entry.id,
			label: entryLabel(entry, now()),
			icon: <span class={cx(s.needsYouDot, entry.kind === "error" && s.needsYouDotError)} />,
		}));
	const title = () =>
		`${t("needsYou.title", "{count} need you", { count: String(waitingEntries().length) })} (${keyFor("jump-waiting-terminal")})`;

	return (
		<Show when={waitingEntries().length > 0}>
			<Button
				ref={trigger}
				size="compact"
				variant="ghost"
				class={s.needsYou}
				data-testid="needs-you"
				aria-haspopup="menu"
				aria-expanded={open()}
				data-tooltip={title()}
				data-tooltip-pos="bottom"
				onClick={() => {
					setNow(Date.now());
					setOpen(!open());
				}}
			>
				<span class={s.needsYouDot} />
				{t("needsYou.label", "Needs you")}
				<span class={s.needsYouCount}>{waitingEntries().length}</span>
			</Button>
			<Dropdown
				items={items()}
				visible={open()}
				anchor={() => trigger?.getBoundingClientRect()}
				aria-label={t("needsYou.listLabel", "Sessions waiting for you")}
				onSelect={(id) => {
					setOpen(false);
					navigateToTerminal(id);
				}}
				onClose={() => setOpen(false)}
			/>
		</Show>
	);
};
