import { cleanup, fireEvent, render } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { ContextualTip } from "../../components/TipOfTheDay/ContextualTip";
import { LongRunTip } from "../../components/TipOfTheDay/LongRunTip";

beforeEach(() => localStorage.clear());
afterEach(() => {
	cleanup();
	vi.useRealTimers();
});

// Catches: irrelevant contexts display tips, or dismissal returns after remount/reload.
it.each([
	["hooks", "More precise agent status"],
	["autoRetry", "Retry temporary API failures"],
	["preventSleep", "Keep long agent runs awake"],
] as const)("keeps %s hidden until relevant and persists its dismissal", (id, title) => {
	const [relevant, setRelevant] = createSignal(false);
	const view = render(() => <ContextualTip id={id} when={relevant()} />);
	expect(view.queryByRole("complementary", { name: title })).toBeNull();
	setRelevant(true);
	expect(view.getByRole("complementary", { name: title })).toBeTruthy();
	fireEvent.click(view.getByRole("button", { name: `Dismiss ${title}` }));
	expect(localStorage.getItem(`tuic.dismissed-tip.${id}`)).toBe("true");
	view.unmount();
	const remount = render(() => <ContextualTip id={id} when={true} />);
	expect(remount.queryByRole("complementary", { name: title })).toBeNull();
});

// Catches: another terminal's mounted copy returns a dismissed hint; dismissing one feature hides all.
it("dismisses mounted copies without dismissing a different feature", () => {
	const view = render(() => (
		<>
			<ContextualTip id="hooks" when={true} />
			<ContextualTip id="hooks" when={true} />
			<ContextualTip id="autoRetry" when={true} />
		</>
	));
	fireEvent.click(view.getAllByRole("button", { name: "Dismiss More precise agent status" })[0]);
	expect(view.queryAllByRole("complementary", { name: "More precise agent status" })).toHaveLength(0);
	expect(view.getByRole("complementary", { name: "Retry temporary API failures" })).toBeTruthy();
});

// Catches: a storage event in another window leaves a duplicate hint on screen.
it("adopts dismissal from another window", () => {
	const view = render(() => <ContextualTip id="hooks" when={true} />);
	localStorage.setItem("tuic.dismissed-tip.hooks", "true");
	window.dispatchEvent(new StorageEvent("storage", { key: "tuic.dismissed-tip.hooks", newValue: "true" }));
	expect(view.queryByRole("complementary")).toBeNull();
});

// Catches: short runs nag, ended runs keep a stale timer, or enabling prevention leaves a tip visible.
it("shows sleep guidance only after a minute of uninterrupted eligible work", () => {
	vi.useFakeTimers();
	const [eligible, setEligible] = createSignal(true);
	const view = render(() => <LongRunTip eligible={eligible()} busyDuration={0} />);
	vi.advanceTimersByTime(59_999);
	expect(view.queryByRole("complementary")).toBeNull();
	setEligible(false);
	vi.advanceTimersByTime(60_000);
	expect(view.queryByRole("complementary")).toBeNull();
	setEligible(true);
	vi.advanceTimersByTime(60_000);
	expect(view.getByRole("complementary", { name: "Keep long agent runs awake" })).toBeTruthy();
	setEligible(false);
	expect(view.queryByRole("complementary")).toBeNull();
});

// Catches: remounting a terminal during a long run delays the tip by another full minute.
it("uses the existing busy duration on mount", () => {
	vi.useFakeTimers();
	const view = render(() => <LongRunTip eligible={true} busyDuration={59_000} />);
	vi.advanceTimersByTime(1_000);
	expect(view.getByRole("complementary", { name: "Keep long agent runs awake" })).toBeTruthy();
});
