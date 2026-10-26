import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import "../mocks/tauri";
import { notificationManager } from "../../notifications";
import { showNativeNotice } from "../../services/nativeNotifications";
import { notificationsStore } from "../../stores/notifications";
import { createProgressStore } from "../../stores/progress";
import { terminalsStore } from "../../stores/terminals";
import { toastsStore } from "../../stores/toasts";
import { makeTerminal } from "../helpers/store";

vi.mock("../../services/nativeNotifications", () => ({ showNativeNotice: vi.fn().mockResolvedValue(undefined) }));

function addFamily() {
	const parent = terminalsStore.add(makeTerminal({ sessionId: "parent-pty", tuicSession: "parent-peer" }));
	const child = terminalsStore.add(
		makeTerminal({
			sessionId: "child-pty",
			tuicSession: "child-peer",
			parentSession: "parent-peer",
		}),
	);
	return { parent, child };
}

describe("subagent notification policy", () => {
	beforeEach(() => {
		for (const id of terminalsStore.getIds()) terminalsStore.remove(id);
		notificationsStore.reset();
		vi.spyOn(notificationManager, "play").mockResolvedValue(undefined);
		vi.spyOn(document, "hasFocus").mockReturnValue(false);
		vi.mocked(showNativeNotice).mockClear();
	});

	afterEach(() => vi.restoreAllMocks());

	// Catches: only completion is muted, or the PTY/TUIC identity bypasses the tab-level gate.
	it.each(["question", "error", "completion", "attention", "warning", "info"] as const)(
		"mutes %s for a child through every live identity",
		async (sound) => {
			const { child } = addFamily();
			for (const id of [child, "child-pty", "child-peer"]) {
				await notificationsStore.play(sound, { terminalId: id });
			}
			expect(notificationManager.play).not.toHaveBeenCalled();
			expect(showNativeNotice).not.toHaveBeenCalled();
		},
	);

	// Catches: HTTP origin is mistaken for parentage and mutes user-created MCP sessions or the parent.
	it("leaves the parent, user tabs and unparented MCP questions audible", async () => {
		const { parent } = addFamily();
		const user = terminalsStore.add(makeTerminal({ sessionId: "user-pty" }));
		const external = terminalsStore.add({ ...makeTerminal({ sessionId: "external-pty" }), isExternallyCreated: true });
		for (const terminalId of [parent, user, external]) await notificationsStore.playQuestion(terminalId);
		await vi.waitFor(() => expect(showNativeNotice).toHaveBeenCalledTimes(3));
		expect(notificationManager.play).toHaveBeenCalledTimes(3);
	});

	// Catches: the saved toggle is ignored for non-completion sounds or question reminders.
	it("restores child sounds and native notices when the existing toggle is off", async () => {
		const { child } = addFamily();
		notificationsStore.setSilenceRemoteCompletions(false);
		await notificationsStore.playQuestionReminder(child);
		await vi.waitFor(() => expect(showNativeNotice).toHaveBeenCalledOnce());
		expect(notificationManager.play).toHaveBeenCalledWith("question");
	});

	// Catches: enabling silence while OS permission is pending still sends the queued notice.
	it("withdraws a pending native notice when the child becomes silenced", async () => {
		const { child } = addFamily();
		notificationsStore.setSilenceRemoteCompletions(false);
		await notificationsStore.playQuestion(child);
		await vi.waitFor(() => expect(showNativeNotice).toHaveBeenCalledOnce());
		const notice = vi.mocked(showNativeNotice).mock.calls[0][0];
		expect(notice.isCurrent?.()).toBe(true);
		notificationsStore.setSilenceRemoteCompletions(true);
		expect(notice.isCurrent?.()).toBe(false);
	});

	// Catches: Progress bypasses the session gate or silence drops the recorded outcome's card.
	it("mutes child Progress native notices without dropping outcome presentation", () => {
		addFamily();
		const addToast = vi.spyOn(toastsStore, "add").mockReturnValue(1);
		const progress = createProgressStore();
		for (const type of ["done", "blocked"] as const) {
			progress.presentLive({
				repo_path: "/repo",
				payload: { entry: { id: 1, project: "/repo", ptyId: "child-pty", createdAtMs: 1, type, text: "Outcome" } },
			});
		}
		expect(showNativeNotice).not.toHaveBeenCalled();
		expect(addToast).toHaveBeenCalledTimes(2);
	});
});
