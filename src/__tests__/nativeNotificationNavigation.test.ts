import { describe, expect, it, vi } from "vitest";

const terminal = vi.fn();
const progress = vi.fn();
const listen = vi.fn();
const aiChat = vi.fn();

vi.mock("../utils/navigateToTerminal", () => ({ navigateToTerminal: terminal }));
vi.mock("../stores/progress", () => ({ progressStore: { open: progress } }));
vi.mock("../stores/ui", () => ({ uiStore: { setAiChatPanelVisible: aiChat } }));
vi.mock("../invoke", () => ({ listen }));
const conversation = vi.fn();
vi.mock("../services/taskNavigation", () => ({ openTaskConversation: conversation }));

describe("native notification click", () => {
	it("opens the named terminal after a question", async () => {
		const { navigateFromNativeNotice } = await import("../services/nativeNotificationNavigation");
		navigateFromNativeNotice({ kind: "terminal", id: "question-terminal" });
		expect(terminal).toHaveBeenCalledWith("question-terminal");
		expect(progress).not.toHaveBeenCalled();
	});

	it("opens the matching project and terminal scope after a progress outcome", async () => {
		const { navigateFromNativeNotice } = await import("../services/nativeNotificationNavigation");
		navigateFromNativeNotice({ kind: "progress", project: "/other-project", ptyId: "agent-2" });
		expect(progress).toHaveBeenCalledWith("/other-project", "agent-2");
		expect(terminal).toHaveBeenCalledTimes(0);
	});

	it("opens AI Chat after an ACP interaction notice", async () => {
		const { navigateFromNativeNotice } = await import("../services/nativeNotificationNavigation");
		navigateFromNativeNotice({ kind: "aichat", id: "connection-1:permission-1" });
		expect(aiChat).toHaveBeenCalledWith(true);
		expect(terminal).not.toHaveBeenCalled();
		expect(progress).not.toHaveBeenCalled();
	});

	it("routes a PR notice through Tasks with a GitHub fallback", async () => {
		const { navigateFromNativeNotice } = await import("../services/nativeNotificationNavigation");
		navigateFromNativeNotice({ kind: "pr", url: "https://github.com/acme/api/pull/12" });
		expect(conversation).toHaveBeenCalledWith("https://github.com/acme/api/pull/12");
		expect(terminal).not.toHaveBeenCalled();
	});

	it("routes a native click event to its recorded target", async () => {
		listen.mockImplementation((_name, handler) => {
			handler({ payload: { kind: "terminal", id: "from-notification" } });
			return Promise.resolve(() => {});
		});
		const { listenForNativeNoticeClicks } = await import("../services/nativeNotificationNavigation");
		await listenForNativeNoticeClicks();
		expect(listen).toHaveBeenCalledWith("native-notification-click", expect.any(Function));
		expect(terminal).toHaveBeenCalledWith("from-notification");
	});
});
