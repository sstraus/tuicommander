import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, expect, it, vi } from "vitest";
import { ComposeToggle } from "../../components/Terminal/ComposeToggle";
import type { QueuedCommand } from "../../hooks/usePty";
import { appLogger } from "../../stores/appLogger";

vi.mock("../../stores/appLogger", () => ({ appLogger: { debug: vi.fn() } }));

afterEach(cleanup);

// Catches: peer wake notices are invisible while Compose is collapsed, or a drained count sticks.
it("shows one queued peer wake while collapsed and hides the count after delivery", () => {
	const [count, setCount] = createSignal(1);
	const onOpen = vi.fn();
	const load = vi.fn(async (): Promise<QueuedCommand[]> => [{ id: 1, kind: "notice", text: "Peer mail available" }]);
	const { getByRole, queryByText } = render(() => (
		<ComposeToggle shortcut="⌘I" queuedCount={count} onOpen={onOpen} onLoadQueue={load} />
	));
	expect(queryByText("1")?.tagName).toBe("SPAN");
	expect(getByRole("button").textContent).toContain("Compose ⌘I");
	expect(load).not.toHaveBeenCalled();
	setCount(0);
	expect(queryByText("1")).toBeNull();
	fireEvent.click(getByRole("button"));
	expect(onOpen).toHaveBeenCalledOnce();
});

// Catches: a hover mislabels server notices as commands, or keeps stale details after delivery.
it("describes mixed FIFO work on hover and refreshes it when delivery changes the count", async () => {
	const [count, setCount] = createSignal(3);
	const load = vi.fn(
		async (): Promise<QueuedCommand[]> => [
			{ id: 1, kind: "user_command", text: "first" },
			{ id: 2, kind: "initial_prompt", text: "task" },
			{ id: 3, kind: "notice", text: "Peer mail available" },
		],
	);
	const { getByRole } = render(() => (
		<ComposeToggle shortcut="⌘I" queuedCount={count} onOpen={() => {}} onLoadQueue={load} />
	));
	const button = getByRole("button");
	fireEvent.mouseEnter(button);
	await waitFor(() => expect(button.title).toContain("1 command, 2 messages"));
	load.mockResolvedValue([{ id: 1, kind: "user_command", text: "first" }]);
	setCount(1);
	await waitFor(() => expect(button.title).toContain("1 command"));
	expect(button.title).not.toContain("messages");
	setCount(0);
	expect(button.title).toBe("Open compose editor (⌘I)");
});

// Catches: keyboard focus cannot discover queued work because only mouse hover loads details.
it("loads queue details on keyboard focus without changing the visible label", async () => {
	const load = vi.fn(async (): Promise<QueuedCommand[]> => [{ id: 1, kind: "notice", text: "notice" }]);
	const { getByRole } = render(() => (
		<ComposeToggle shortcut="Ctrl+I" queuedCount={() => 1} onOpen={() => {}} onLoadQueue={load} />
	));
	const button = getByRole("button");
	fireEvent.focus(button);
	await waitFor(() => expect(button.title).toContain("1 message"));
	expect(button.textContent).not.toContain("message");
});

// Catches: an old hover read overwrites newer delivery details or restores a drained queue.
it("ignores a stale queue read after delivery changes the count", async () => {
	const [count, setCount] = createSignal(2);
	let resolveOld!: (items: QueuedCommand[]) => void;
	const load = vi
		.fn()
		.mockImplementationOnce(
			() =>
				new Promise<QueuedCommand[]>((resolve) => {
					resolveOld = resolve;
				}),
		)
		.mockResolvedValue([{ id: 2, kind: "notice", text: "Peer mail available" }]);
	const { getByRole, queryByText } = render(() => (
		<ComposeToggle shortcut="⌘I" queuedCount={count} onOpen={() => {}} onLoadQueue={load} />
	));
	const button = getByRole("button");
	fireEvent.mouseEnter(button);
	setCount(1);
	await waitFor(() => expect(button.title).toContain("1 message"));
	resolveOld([
		{ id: 1, kind: "user_command", text: "old" },
		{ id: 2, kind: "notice", text: "mail" },
	]);
	await Promise.resolve();
	expect(button.title).not.toContain("command");
	setCount(0);
	expect(queryByText("1")).toBeNull();
	expect(button.title).toBe("Open compose editor (⌘I)");
});

// Catches: a failed details request hides the authoritative count or breaks Compose opening.
it("keeps the count and open action when queue details are unavailable", async () => {
	const load = vi.fn().mockRejectedValue(new Error("queue unavailable"));
	const open = vi.fn();
	const { getByRole, getByText } = render(() => (
		<ComposeToggle shortcut="⌘I" queuedCount={() => 3} onOpen={open} onLoadQueue={load} />
	));
	const button = getByRole("button");
	fireEvent.mouseEnter(button);
	await waitFor(() => expect(load).toHaveBeenCalledOnce());
	await Promise.resolve();
	expect(getByText("3")).toBeTruthy();
	expect(button.title).toContain("3 queued");
	expect(appLogger.debug).toHaveBeenCalledWith("terminal", "Failed to load compose queue details", expect.any(Error));
	fireEvent.click(button);
	expect(open).toHaveBeenCalledOnce();
});

// Catches: readable Compose text accidentally exposes the compact Button chrome at rest.
it.each(["idle", "hover", "focus"])(
	"keeps the collapsed handle discreet in %s state without fading its count",
	async (state) => {
		const { readStylesheet } = await import("../fixtures/readStylesheet");
		const style = document.createElement("style");
		// happy-dom has no pointer/keyboard modality: map only those pseudo-classes to attributes.
		style.textContent =
			`${readStylesheet("src/components/ui/FormControls.module.css")}\n${readStylesheet("src/components/Terminal/Terminal.module.css")}`
				.replaceAll(":hover", "[data-test-hover]")
				.replaceAll(":focus-visible", "[data-test-focus]");
		document.head.append(style);
		try {
			const { getByRole, getByText } = render(() => (
				<div style="--bg-tertiary: #2d2d2d; --bg-highlight: #3c3c3c; --border: #454545; --fg-primary: #d4d4d4; --fg-muted: #a6a6a6; --fg-secondary: #b5b5b5; --accent: #007acc">
					<ComposeToggle shortcut="⌘I" queuedCount={() => 1} onOpen={() => {}} onLoadQueue={async () => []} />
				</div>
			));
			const button = getByRole("button");
			if (state === "hover") button.setAttribute("data-test-hover", "");
			if (state === "focus") button.setAttribute("data-test-focus", "");
			const computed = getComputedStyle(button);
			expect(computed.backgroundColor).toBe(state === "idle" ? "transparent" : "#3c3c3c");
			expect(computed.borderTopColor).toBe(state === "idle" ? "transparent" : "#454545");
			expect(computed.color).toBe(state === "idle" ? "#a6a6a6" : "#d4d4d4");
			expect(Number(computed.opacity || "1")).toBe(1);
			const badge = getComputedStyle(getByText("1"));
			expect(badge.color).toBe("#b5b5b5");
			expect(badge.backgroundColor).toBe("#3c3c3c");
			expect(Number(badge.opacity || "1")).toBe(1);
		} finally {
			style.remove();
		}
	},
);
