// @vitest-environment jsdom
import { cleanup, render } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, expect, it } from "vitest";
import { Transcript } from "../../components/AIChatPanel/Transcript";

afterEach(cleanup);

// Catches: leaving either desktop or mobile on the legacy pulse, or retaining the orb after settlement.
it.each([false, true])("shows the default orb only while working (mobile=%s)", (mobile) => {
	const [busy, setBusy] = createSignal(false);
	const view = render(() => <Transcript entries={() => []} busy={busy} mobile={mobile} emptyMessage="" />);
	expect(view.queryByRole("status")).toBeNull();
	setBusy(true);
	const status = view.getByRole("status", { name: "Working…" });
	expect(status.querySelector('[aria-hidden="true"] > i')).not.toBeNull();
	expect(status.textContent).toBe("Working…");
	setBusy(false);
	expect(view.queryByRole("status")).toBeNull();
});

// Catches: hiding a provider failure behind the orb or failing to restore feedback after retry.
it("replaces the orb with provider retry status without changing the busy lifecycle", () => {
	const [busy, setBusy] = createSignal(true);
	const [retry, setRetry] = createSignal<string | null>(null);
	const view = render(() => <Transcript entries={() => []} busy={busy} retry={retry} emptyMessage="" />);
	expect(view.getByRole("status", { name: "Working…" })).toBeTruthy();
	setRetry("Provider unavailable; retrying in 2s");
	expect(view.queryByRole("status", { name: "Working…" })).toBeNull();
	expect(view.getByRole("status").textContent).toBe("Provider unavailable; retrying in 2s");
	setRetry(null);
	expect(view.getByRole("status", { name: "Working…" })).toBeTruthy();
	setBusy(false);
	expect(view.queryByRole("status")).toBeNull();
});

// Catches: replacing a host's accessible working label with a hard-coded visual label.
it("preserves the host working label for assistive technology", () => {
	const view = render(() => (
		<Transcript entries={() => []} busy={() => true} busyLabel="Checking changes…" emptyMessage="" />
	));
	expect(view.getByRole("status", { name: "Checking changes…" }).textContent).toBe("Checking changes…");
});
