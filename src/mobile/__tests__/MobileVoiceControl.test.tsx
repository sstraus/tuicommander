import { cleanup, fireEvent, render, screen, waitFor, within } from "@solidjs/testing-library";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { appLogger } from "../../stores/appLogger";
import { SessionDetailScreen } from "../screens/SessionDetailScreen";
import type { SessionInfo } from "../useSessions";

const { rpc, toastAdd, voice } = vi.hoisted(() => {
	const voice = {
		state: { handsFree: null as unknown, handsFreeError: null as string | null },
		setSpokenReplies: vi.fn(),
		armHandsFree: vi.fn(),
		disarmHandsFree: vi.fn(),
		setStatus: (_status: unknown) => {},
		setError: (_message: string | null) => {},
	};
	return { rpc: vi.fn(), toastAdd: vi.fn(), voice };
});

vi.mock("../../stores/appLogger", async (importOriginal) => {
	const original = await importOriginal<typeof import("../../stores/appLogger")>();
	return { ...original, appLogger: { ...original.appLogger, warn: vi.fn(), info: vi.fn(), debug: vi.fn() } };
});

vi.mock("../../transport", async (importOriginal) => ({
	...(await importOriginal<typeof import("../../transport")>()),
	rpc,
}));
vi.mock("../../stores/toasts", () => ({ toastsStore: { add: toastAdd } }));
vi.mock("../components/OutputView", () => ({ OutputView: () => <div /> }));
vi.mock("../components/TerminalKeybar", () => ({ TerminalKeybar: () => <div /> }));
vi.mock("../../stores/dictation", async () => {
	const { createStore } = await import("solid-js/store");
	const [state, setState] = createStore({
		handsFree: null as unknown,
		handsFreeError: null as string | null,
		spokenReplies: true,
	});
	voice.state = state as typeof voice.state;
	voice.setStatus = (status) => setState("handsFree", status);
	voice.setError = (message) => setState("handsFreeError", message);
	return {
		browserAudioOwner: "browser-test",
		dictationStore: {
			state,
			refreshConfig: vi.fn().mockResolvedValue(undefined),
			setSpokenReplies: voice.setSpokenReplies,
			armHandsFree: voice.armHandsFree,
			disarmHandsFree: voice.disarmHandsFree,
		},
	};
});

const session: SessionInfo = {
	session_id: "s1",
	cwd: "/repo",
	worktree_path: null,
	worktree_branch: null,
	state: { agent_type: "claude", awaiting_input: false, rate_limited: false, last_activity_ms: 1 },
};

const armedStatus = { armed: true, phase: "waiting", sessionId: "s1", owner: "browser-test" };

function mount() {
	return render(() => (
		<SessionDetailScreen
			subscribeChatWake={() => () => {}}
			session={session}
			sessionExists={true}
			onBack={() => {}}
			onOpenFiles={() => {}}
		/>
	));
}

beforeEach(() => {
	rpc.mockResolvedValue({ armed: false });
	voice.armHandsFree.mockImplementation(async () => {
		voice.setStatus(armedStatus);
		return true;
	});
	voice.disarmHandsFree.mockImplementation(async () => {
		voice.setStatus(null);
	});
});

afterEach(() => {
	cleanup();
	localStorage.clear();
	vi.clearAllMocks();
	voice.setStatus(null);
});

it("hides the voice control when the server has no hands-free route", async () => {
	// catches: a dead mic button on headless tuic-remote, which answers 404.
	rpc.mockRejectedValue(new Error("RPC get_hands_free_status failed: 404"));
	mount();
	await waitFor(() => expect(rpc).toHaveBeenCalledWith("get_hands_free_status"));
	await Promise.resolve();
	expect(screen.queryByRole("button", { name: /voice call/ })).toBeNull();
});

it("arms synchronously inside the tap and disarms on the second tap", async () => {
	// catches: arming after an await, which spends the iOS user gesture before the AudioContext can resume.
	mount();
	const button = await screen.findByRole("button", { name: "Start voice call" });

	fireEvent.click(button);
	expect(voice.armHandsFree).toHaveBeenCalledWith("s1");

	const stop = await screen.findByRole("button", { name: "Stop voice call" });
	fireEvent.click(stop);
	await waitFor(() => expect(voice.disarmHandsFree).toHaveBeenCalledTimes(1));
	await screen.findByRole("button", { name: "Start voice call" });
});

it("shows a refused start as a readable toast without the Error prefix", async () => {
	// catches: the raw "Error: ..." string, or silence, when the mic is refused.
	voice.armHandsFree.mockImplementation(async () => {
		voice.setError("Error: Voice needs a secure page");
		return false;
	});
	mount();
	fireEvent.click(await screen.findByRole("button", { name: "Start voice call" }));
	await waitFor(() =>
		expect(toastAdd).toHaveBeenCalledWith("Voice not started", "Voice needs a secure page", "error", true),
	);
});

it("ends the conversation when the session screen is left", async () => {
	// catches: the microphone staying open on a screen the user has navigated away from.
	const { unmount } = mount();
	fireEvent.click(await screen.findByRole("button", { name: "Start voice call" }));
	await screen.findByRole("button", { name: "Stop voice call" });

	unmount();

	expect(voice.disarmHandsFree).toHaveBeenCalledTimes(1);
});

it("keeps Spoken replies out of the session screen when voice is available", async () => {
	// catches: the redundant Spoken replies row returning beneath the session header.
	mount();
	await screen.findByRole("button", { name: "Start voice call" });
	expect(screen.queryByRole("switch", { name: "Spoken replies" })).toBeNull();
	expect(screen.queryByText("Spoken replies")).toBeNull();
});

class FakeRecognition {
	static latest: FakeRecognition;
	lang = "";
	continuous = false;
	interimResults = false;
	onresult: ((event: { results: ArrayLike<{ isFinal: boolean; 0: { transcript: string } }> }) => void) | null = null;
	onerror: ((event: { error: string }) => void) | null = null;
	onend: (() => void) | null = null;
	start = vi.fn();
	stop = vi.fn(() => this.onend?.());
	abort = vi.fn();
	constructor() {
		FakeRecognition.latest = this;
	}
	result(text: string, isFinal = false) {
		this.onresult?.({ results: [{ isFinal, 0: { transcript: text } }] });
	}
}

function browserSpeech() {
	vi.stubGlobal("webkitSpeechRecognition", FakeRecognition);
}

afterEach(() => vi.unstubAllGlobals());

it("hides browser dictation when neither recognition constructor exists", async () => {
	// Catches: shipping an unusable browser mic on unsupported devices.
	vi.stubGlobal("SpeechRecognition", undefined);
	vi.stubGlobal("webkitSpeechRecognition", undefined);
	mount();
	await screen.findByRole("button", { name: "Start voice call" });
	expect(screen.queryByRole("button", { name: "Dictate into draft" })).toBeNull();
});

it("appends interim and final speech without focusing, duplicating hypotheses or auto-submitting", async () => {
	// Catches: opening the phone keyboard, replacing typed text, or sending on result.
	browserSpeech();
	const { container } = mount();
	await screen.findByRole("button", { name: "Start voice call" });
	const input = container.querySelector("textarea")!;
	const focus = vi.spyOn(input, "focus");
	fireEvent.input(input, { target: { value: "Review" } });
	rpc.mockClear();
	const mic = screen.getByRole("button", { name: "Dictate into draft" });
	// Send sits outside the composer box, right after the box that holds the mic.
	expect(mic.parentElement?.nextElementSibling).toBe(screen.getByRole("button", { name: "Send" }));
	fireEvent.click(mic);
	FakeRecognition.latest.result("the diff");
	expect(input.value).toBe("Review the diff");
	FakeRecognition.latest.result("the changes", true);
	expect(input.value).toBe("Review the changes");
	expect(focus).not.toHaveBeenCalled();
	expect(document.activeElement).not.toBe(input);
	expect(rpc.mock.calls.some(([name, args]) => name === "submit_agent_reply" || args?.data?.includes("\r"))).toBe(
		false,
	);
	fireEvent.click(screen.getByRole("button", { name: "Stop dictation" }));
	expect(FakeRecognition.latest.stop).toHaveBeenCalledTimes(1);
});

it.each([
	["not-allowed", "Microphone or speech recognition permission denied"],
	["network", "Speech service unavailable (network)"],
	["no-speech", "No speech detected"],
])("keeps the draft and toasts the %s recognition error", async (error, plainWords) => {
	// Catches: silently discarding the draft on browser/service permission failures.
	browserSpeech();
	const { container } = mount();
	await screen.findByRole("button", { name: "Start voice call" });
	const input = container.querySelector("textarea")!;
	fireEvent.input(input, { target: { value: "Keep this" } });
	fireEvent.click(screen.getByRole("button", { name: "Dictate into draft" }));
	FakeRecognition.latest.onerror?.({ error });
	FakeRecognition.latest.onend?.();
	expect(input.value).toBe("Keep this");
	expect(toastAdd).toHaveBeenCalledWith("Dictation failed", plainWords, "error", true);
	expect(appLogger.warn).toHaveBeenCalledWith(
		"dictation",
		`Browser dictation failed: ${error}`,
		expect.objectContaining({ code: error }),
	);
	expect(screen.getByRole("button", { name: "Dictate into draft" }).getAttribute("aria-pressed")).toBe("false");
});

it.each(["it", "auto"])("uses configured language %s or the navigator fallback", async (language) => {
	// Catches: ignoring the shared dictation language or passing Whisper's auto token to Web Speech.
	browserSpeech();
	rpc.mockImplementation(async (command) => (command === "get_dictation_config" ? { language } : { armed: false }));
	mount();
	await screen.findByRole("button", { name: "Start voice call" });
	await waitFor(() => expect(rpc).toHaveBeenCalledWith("get_dictation_config"));
	await Promise.resolve();
	fireEvent.click(screen.getByRole("button", { name: "Dictate into draft" }));
	expect(FakeRecognition.latest.lang).toBe(language === "auto" ? navigator.language : language);
});

it("excludes server voice while browser recognition starts and releases it on end", async () => {
	// Catches: two mic consumers during the browser permission prompt.
	browserSpeech();
	mount();
	const server = await screen.findByRole("button", { name: "Start voice call" });
	fireEvent.click(screen.getByRole("button", { name: "Dictate into draft" }));
	expect(server.hasAttribute("disabled")).toBe(true);
	fireEvent.click(server);
	expect(voice.armHandsFree).not.toHaveBeenCalled();
	FakeRecognition.latest.onend?.();
	expect(server.hasAttribute("disabled")).toBe(false);
	fireEvent.click(server);
	await screen.findByRole("button", { name: "Stop voice call" });
	expect(screen.getByRole("button", { name: "Dictate into draft" }).hasAttribute("disabled")).toBe(true);
});

// Catches: opening Ideas bypasses the session's existing microphone exclusion,
// so idea speech competes with a voice call or continues into the hidden terminal draft.
it.each(["voice call", "terminal dictation"])(
	"blocks Ideas dictation while %s already owns the microphone",
	async (owner) => {
		browserSpeech();
		mount();
		const server = await screen.findByRole("button", { name: "Start voice call" });
		if (owner === "voice call") {
			fireEvent.click(server);
			await screen.findByRole("button", { name: "Stop voice call" });
		} else {
			fireEvent.click(screen.getByRole("button", { name: "Dictate into draft" }));
		}
		fireEvent.click(screen.getByRole("button", { name: "More session actions" }));
		fireEvent.click(screen.getByRole("button", { name: /^Ideas$/ }));
		const ideaInput = screen.getByPlaceholderText("Type an idea...");
		const mic = within(ideaInput.parentElement!).getByRole("button", { name: "Dictate into draft" });
		expect(mic.hasAttribute("disabled")).toBe(true);
	},
);

it("excludes browser speech during a pending server permission prompt", async () => {
	// Catches: a race before the server has published its armed status.
	browserSpeech();
	let finish!: (ok: boolean) => void;
	voice.armHandsFree.mockImplementation(
		() =>
			new Promise<boolean>((resolve) => {
				finish = resolve;
			}),
	);
	mount();
	fireEvent.click(await screen.findByRole("button", { name: "Start voice call" }));
	expect(screen.getByRole("button", { name: "Dictate into draft" }).hasAttribute("disabled")).toBe(true);
	finish(false);
	await waitFor(() =>
		expect(screen.getByRole("button", { name: "Dictate into draft" }).hasAttribute("disabled")).toBe(false),
	);
});

it.each(["run tests in package", "replace everything", ""])(
	"returns the mic to idle after end and keeps the edited draft %j after queued speech",
	async (edited) => {
		// Catches: editing leaves recording active or a queued result overwrites the manual draft.
		browserSpeech();
		mount();
		const server = await screen.findByRole("button", { name: "Start voice call" });
		const input = screen.getByRole("textbox") as HTMLTextAreaElement;
		fireEvent.click(screen.getByRole("button", { name: "Dictate into draft" }));
		const recognition = FakeRecognition.latest;
		recognition.result("run tests", true);
		const lateResult = recognition.onresult!;
		fireEvent.input(input, { target: { value: edited } });
		expect(recognition.abort).toHaveBeenCalledTimes(1);
		expect(server.hasAttribute("disabled")).toBe(true);
		recognition.onend?.();
		expect(screen.getByRole("button", { name: "Dictate into draft" }).getAttribute("aria-pressed")).toBe("false");
		expect(server.hasAttribute("disabled")).toBe(false);
		rpc.mockClear();
		lateResult({ results: [{ isFinal: true, 0: { transcript: "run tests now" } }] });
		expect(input.value).toBe(edited);
		expect(rpc).not.toHaveBeenCalled();
		fireEvent.click(screen.getByRole("button", { name: "Dictate into draft" }));
		FakeRecognition.latest.result("continue");
		expect(input.value).toBe(edited ? `${edited} continue` : "continue");
	},
);

it("aborts recognition and ignores late results when the screen is left", async () => {
	// Catches: background recording or modifying an abandoned composer.
	browserSpeech();
	const { unmount } = mount();
	await screen.findByRole("button", { name: "Start voice call" });
	const input = screen.getByRole("textbox") as HTMLTextAreaElement;
	fireEvent.click(screen.getByRole("button", { name: "Dictate into draft" }));
	const recognition = FakeRecognition.latest;
	recognition.result("keep draft");
	const lateResult = recognition.onresult!;
	unmount();
	expect(recognition.abort).toHaveBeenCalledTimes(1);
	rpc.mockClear();
	lateResult({ results: [{ isFinal: true, 0: { transcript: "late speech" } }] });
	expect(input.value).toBe("keep draft");
	expect(rpc).not.toHaveBeenCalled();
});

it("keeps voice start and stop reachable from More without losing the tap gesture", async () => {
	// catches: moving phone voice into More loses arming, disarming, or the iOS gesture.
	mount();
	await screen.findByRole("button", { name: "Start voice call" });
	fireEvent.click(screen.getByRole("button", { name: "More session actions" }));
	const overflow = screen
		.getAllByRole("button", { name: "Start voice call" })
		.find((button) => !button.closest("header"))!;
	fireEvent.click(overflow);
	expect(voice.armHandsFree).toHaveBeenCalledWith("s1");
	await waitFor(() =>
		expect(screen.getByRole("button", { name: "More session actions" }).getAttribute("aria-expanded")).toBe("false"),
	);
	fireEvent.click(screen.getByRole("button", { name: "More session actions" }));
	const stop = screen.getAllByRole("button", { name: "Stop voice call" }).find((button) => !button.closest("header"))!;
	fireEvent.click(stop);
	await waitFor(() => expect(voice.disarmHandsFree).toHaveBeenCalledOnce());
});
