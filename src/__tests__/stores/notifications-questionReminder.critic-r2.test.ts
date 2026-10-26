import { beforeEach, describe, expect, it, vi } from "vitest";

const { showNativeNotice, state } = vi.hoisted(() => ({
	showNativeNotice: vi.fn().mockResolvedValue(undefined),
	state: {
		activeId: "term-A" as string | null,
		detached: false,
		focused: true,
		parentSession: undefined as string | undefined,
	},
}));

vi.mock("../../services/nativeNotifications", () => ({ showNativeNotice }));
vi.mock("@tauri-apps/api/window", () => ({
	getCurrentWindow: () => ({ setBadgeCount: vi.fn().mockResolvedValue(undefined) }),
}));
vi.mock("../../stores/appLogger", () => ({ appLogger: { debug: vi.fn(), warn: vi.fn() } }));
vi.mock("../../invoke", () => ({ invoke: vi.fn().mockResolvedValue(undefined) }));
vi.mock("../../stores/terminals", () => ({
	terminalsStore: {
		state: {
			get activeId() {
				return state.activeId;
			},
		},
		get: (id: string) => ({ name: `tab-${id}`, parentSession: state.parentSession }),
		isDetached: () => state.detached,
	},
}));
vi.mock("../../notifications", () => ({
	DEFAULT_NOTIFICATION_CONFIG: {
		enabled: true,
		silence_remote_completions: true,
		volume: 0.5,
		sounds: { question: true, error: true, completion: true, warning: true, info: true },
	},
	notificationManager: {
		play: vi.fn().mockResolvedValue(undefined),
		updateConfig: vi.fn(),
		setEnabled: vi.fn(),
		setVolume: vi.fn(),
		setSoundEnabled: vi.fn(),
		isAvailable: vi.fn().mockReturnValue(true),
		getConfig: vi.fn(),
	},
}));

const flush = async () => {
	for (let i = 0; i < 5; i++) await Promise.resolve();
	await new Promise((r) => setTimeout(r, 0));
};

describe("question reminder OS notice — critic round 2", () => {
	let store: typeof import("../../stores/notifications").notificationsStore;

	beforeEach(async () => {
		vi.resetModules();
		showNativeNotice.mockClear();
		state.activeId = "term-A";
		state.detached = false;
		state.focused = true;
		state.parentSession = undefined;
		vi.spyOn(document, "hasFocus").mockImplementation(() => state.focused);
		localStorage.clear();
		store = (await import("../../stores/notifications")).notificationsStore;
	});

	it("focused window, user on another tab: reminder goes out with ignoreFocus and its own dedup key", async () => {
		// Catches: reminder suppressed whenever the window has focus (the whole point of the story).
		await store.playQuestionReminder("term-B");
		await flush();
		expect(showNativeNotice).toHaveBeenCalledTimes(1);
		const notice = showNativeNotice.mock.calls[0][0];
		expect(notice.ignoreFocus).toBe(true);
		expect(notice.key).toBe("reminder:question:term-B");
		expect(notice.target).toEqual({ kind: "terminal", id: "term-B" });
	});

	it("focused window on the very terminal asking: no OS notice", async () => {
		// Catches: nagging the user with an OS banner for the tab they are looking at.
		await store.playQuestionReminder("term-A");
		await flush();
		expect(showNativeNotice).not.toHaveBeenCalled();
	});

	it("the same terminal shown in a detached window counts as not viewed in the main window", async () => {
		// Catches: isViewed ignoring detachment, hiding the reminder for a tab living in another window.
		state.detached = true;
		await store.playQuestionReminder("term-A");
		await flush();
		expect(showNativeNotice).toHaveBeenCalledTimes(1);
	});

	it("isCurrent re-evaluates viewing after the permission await", async () => {
		// Catches: isCurrent frozen at call time, so a tab the user switched to meanwhile still gets a banner.
		await store.playQuestionReminder("term-B");
		await flush();
		const { isCurrent } = showNativeNotice.mock.calls[0][0];
		expect(isCurrent()).toBe(true);
		state.activeId = "term-B";
		expect(isCurrent()).toBe(false);
	});

	it("ignoreFocus does not leak to the ordinary first question notice", async () => {
		// Catches: the reminder option being spread onto every notice, so a focused window gets banners for everything.
		await store.playQuestion("term-B");
		await flush();
		expect(showNativeNotice).not.toHaveBeenCalled();
		state.focused = false;
		await store.playQuestion("term-B");
		await flush();
		expect(showNativeNotice).toHaveBeenCalledTimes(1);
		const notice = showNativeNotice.mock.calls[0][0];
		expect(notice.ignoreFocus).toBeUndefined();
		expect(notice.key).toBe("question:term-B");
		// Catches: ordinary notices bypassing a newly applied parent-session mute.
		expect(notice.isCurrent()).toBe(true);
		state.parentSession = "parent";
		expect(notice.isCurrent()).toBe(false);
	});

	it("reminder does not bump the dock badge while the window is focused", async () => {
		// Catches: reminder treated as an unfocused notice, inflating the badge.
		await store.playQuestionReminder("term-B");
		await flush();
		expect(store.state.badgeCount).toBe(0);
	});

	it("with notifications globally disabled the reminder sends no OS notice either", async () => {
		// Catches: the OS notice bypassing the user's master notification switch.
		store.setEnabled(false);
		await store.playQuestionReminder("term-B");
		await flush();
		expect(showNativeNotice).not.toHaveBeenCalled();
	});
});
