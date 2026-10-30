// @vitest-environment jsdom
import { cleanup, render } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, expect, it } from "vitest";
import { Transcript } from "../../components/AIChatPanel/Transcript";
import type { WorkingEffect } from "../../components/AIChatPanel/WorkingIndicator";

afterEach(cleanup);

it.each<WorkingEffect>(["shimmer", "dots", "orb", "typewriter"])(
	"%s does not retain working feedback after idle or cover a provider retry",
	(effect) => {
		const [busy, setBusy] = createSignal(true);
		const [retry, setRetry] = createSignal<string | null>(null);
		const view = render(() => (
			<Transcript
				entries={() => []}
				busy={busy}
				retry={retry}
				workingEffect={effect}
				busyLabel="Working…"
				emptyMessage=""
			/>
		));
		expect(view.getByRole("status", { name: "Working…" })).toBeTruthy();
		setRetry("Provider unavailable; retrying in 2s");
		expect(view.queryByRole("status", { name: "Working…" })).toBeNull();
		expect(view.getByRole("status").textContent).toBe("Provider unavailable; retrying in 2s");
		setRetry(null);
		expect(view.getByRole("status", { name: "Working…" })).toBeTruthy();
		setBusy(false);
		expect(view.queryByRole("status")).toBeNull();
	},
);

it("does not select a candidate or change either host's default feedback", () => {
	const terminal = render(() => (
		<Transcript entries={() => []} busy={() => true} busyLabel="Working…" emptyMessage="" />
	));
	expect(terminal.getByRole("status").textContent).toBe("Working…");
	expect(terminal.container.querySelector("[data-working-effect]")).toBeNull();
	terminal.unmount();
	const acp = render(() => <Transcript entries={() => []} busy={() => true} emptyMessage="" />);
	expect(acp.container.textContent).toBe("…");
	expect(acp.container.querySelector("[data-working-effect]")).toBeNull();
});
