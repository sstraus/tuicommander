import { cleanup, fireEvent, render, screen } from "@solidjs/testing-library";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { ToastList } from "../../components/ToastContainer/ToastList";
import type { ProgressEntry } from "../../stores/progress";
import { progressRepliesStore } from "../../stores/progressReplies";
import { toastsStore } from "../../stores/toasts";

vi.mock("../../invoke", () => ({ invoke: vi.fn() }));
vi.mock("../../i18n", () => ({ t: (_key: string, fallback: string) => fallback }));
const entry = (id: number): ProgressEntry => ({
	id,
	project: "/origin",
	ptyId: "author",
	replyTargetAvailable: true,
	createdAtMs: 1,
	type: "done",
	text: "Same update",
});
function add(id: number) {
	return toastsStore.add("Step", "Same update", "info", false, undefined, 1000, "/origin", "author", false, {
		progress: entry(id),
	});
}
beforeEach(() => {
	vi.useFakeTimers();
	progressRepliesStore.resetForTests();
	while (toastsStore.toasts.length) toastsStore.remove(toastsStore.toasts[0].id);
});
afterEach(() => {
	cleanup();
	while (toastsStore.toasts.length) toastsStore.remove(toastsStore.toasts[0].id);
	vi.useRealTimers();
});

// Catches: collapsing two identical report texts into one card with the wrong reply origin/id.
it("keeps distinct Progress reports separate instead of grouping their reply context", () => {
	add(31);
	add(32);
	expect(toastsStore.toasts).toHaveLength(2);
	expect(toastsStore.toasts.map((toast) => toast.progress?.id)).toEqual([31, 32]);
	add(31);
	expect(toastsStore.toasts).toHaveLength(2);
});

// Catches: the card expiring/displacing while typing, or reply clicks dismissing the card.
it("retains the card while replying and resumes expiry when the editor closes", () => {
	const dismiss = vi.fn();
	const id = add(31);
	render(() => <ToastList onDismiss={dismiss} repoName={() => "Origin"} />);
	fireEvent.click(screen.getByRole("button", { name: "Reply" }));
	fireEvent.input(screen.getByRole("textbox"), { target: { value: "Typing a reply" } });
	vi.advanceTimersByTime(2000);
	expect(toastsStore.toasts.some((toast) => toast.id === id)).toBe(true);
	expect(dismiss).not.toHaveBeenCalled();
	add(32);
	add(33);
	expect(toastsStore.toasts.some((toast) => toast.id === id)).toBe(true);
	fireEvent.click(screen.getAllByRole("button", { name: "Reply" })[0]);
	vi.advanceTimersByTime(1001);
	expect(toastsStore.toasts.some((toast) => toast.id === id)).toBe(false);
});

// Catches: Reply shrinking the message column or allocating a closed composer row.
it("keeps Reply with the existing compact action buttons and only mounts the editor on click", () => {
	toastsStore.add(
		"Step",
		"Full width report",
		"info",
		false,
		{ label: "Open Progress", onClick: vi.fn() },
		0,
		"/origin",
		"author",
		false,
		{ progress: entry(38) },
	);
	render(() => <ToastList onDismiss={vi.fn()} repoName={() => "Origin"} />);
	const reply = screen.getByRole("button", { name: "Reply" });
	const open = screen.getByRole("button", { name: "Open Progress" });
	expect(reply.closest("[data-progress-reply]")?.parentElement).toBe(open.parentElement);
	expect(document.querySelector('[data-progress-reply="composer"]')).toBeNull();
	fireEvent.click(reply);
	expect(screen.getByRole("textbox")).toBeInTheDocument();
});

// Catches: remote and local report rowids colliding in deduplication or shared drafts.
it("keeps identical report paths and ids separate across backend hosts", () => {
	const local = { ...entry(39), originConnectionId: null };
	const remote = { ...entry(39), originConnectionId: "remote-host" };
	for (const report of [local, remote])
		toastsStore.add("Step", "Same update", "info", false, undefined, 0, "/origin", "author", false, {
			progress: report,
		});
	expect(toastsStore.toasts).toHaveLength(2);
	progressRepliesStore.setDraft(local, "Local draft");
	progressRepliesStore.setDraft(remote, "Remote draft");
	expect(progressRepliesStore.get(local).draft).toBe("Local draft");
	expect(progressRepliesStore.get(remote).draft).toBe("Remote draft");
});
