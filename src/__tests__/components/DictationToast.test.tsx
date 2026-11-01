import { cleanup, render } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { testInScopeAsync } from "../helpers/store";
// Must import mocks before store/component
import { mockInvoke } from "../mocks/tauri";

describe("DictationToast", () => {
	let DictationToast: typeof import("../../components/DictationToast/DictationToast").DictationToast;
	let dictationStore: typeof import("../../stores/dictation").dictationStore;
	let easeMeterLevel: typeof import("../../stores/dictation").easeMeterLevel;

	beforeEach(async () => {
		vi.resetModules();
		mockInvoke.mockReset();
		mockInvoke.mockResolvedValue(undefined);
		const storeModule = await import("../../stores/dictation");
		dictationStore = storeModule.dictationStore;
		easeMeterLevel = storeModule.easeMeterLevel;
		const component = await import("../../components/DictationToast/DictationToast");
		DictationToast = component.DictationToast;
	});

	// Catches: a test leaves app-lifetime microphone polling alive after its scope ends.
	afterEach(async () => {
		mockInvoke.mockImplementation((command: string) => {
			if (command === "disarm_hands_free_dictation") return Promise.resolve({ status: { armed: false } });
			if (command === "stop_dictation_and_transcribe") return Promise.resolve({ text: "", skip_reason: null });
			return Promise.resolve(undefined);
		});
		await dictationStore.stopRecording();
		await dictationStore.disarmHandsFree();
		cleanup();
		if (vi.isFakeTimers()) expect(vi.getTimerCount()).toBe(0);
		vi.useRealTimers();
	});

	it("is hidden by default", () => {
		const { container } = render(() => <DictationToast />);
		expect(container.querySelector(".toast")).toBeNull();
	});

	it("shows the live preview and an idle meter when recording starts", async () => {
		// Mock start_dictation to succeed
		mockInvoke.mockResolvedValueOnce(undefined);

		await testInScopeAsync(async () => {
			const { container } = render(() => <DictationToast />);

			// Start recording (sets recording=true in store)
			await dictationStore.startRecording();
			expect(dictationStore.state.recording).toBe(true);

			expect(container.querySelector(".toast")).not.toBeNull();
			expect(container.querySelector('[role="meter"]')?.getAttribute("aria-valuenow")).toBe("0");
			// Push-to-talk is a short take, so its pulse and dots stay.
			expect(container.querySelector(".indicator")).not.toBeNull();
			expect(container.querySelector(".dots")).not.toBeNull();
			expect(container.querySelector(".voiceMeter")).toBeNull();

			mockInvoke.mockResolvedValueOnce({ text: "", skip_reason: "no speech detected", duration_s: 0, truncated_s: 0 });
			await dictationStore.stopRecording();
		});
	});

	it("hides toast after recording stops", async () => {
		mockInvoke.mockResolvedValueOnce(undefined); // start_dictation

		await testInScopeAsync(async () => {
			render(() => <DictationToast />);

			await dictationStore.startRecording();
			expect(dictationStore.state.recording).toBe(true);

			// Stop recording
			mockInvoke.mockResolvedValueOnce({ text: "hello", skip_reason: null, duration_s: 1.0, truncated_s: 0 });
			await dictationStore.stopRecording();

			expect(dictationStore.state.recording).toBe(false);
			expect(dictationStore.state.partialText).toBe("");
		});
	});

	describe("spoken replies toggle", () => {
		const SAVED_CONFIG = { hands_free_spoken_replies: true, hands_free_listening: true };

		const armedStatus = {
			armed: true,
			listening: true,
			phase: "waiting",
			sessionId: "s1",
			owner: "desktop",
			generation: 1,
			pendingText: null,
			holdBackMs: 1500,
			error: null,
		};

		/** Bug caught: the toggle is shown in push-to-talk, where no reply is ever spoken and it has no effect. */
		it("is not offered during push-to-talk dictation", async () => {
			mockInvoke.mockImplementation((cmd: string) => {
				if (cmd === "get_dictation_config") return Promise.resolve({ ...SAVED_CONFIG });
				return Promise.resolve(undefined);
			});

			await testInScopeAsync(async () => {
				const { container } = render(() => <DictationToast />);
				await dictationStore.startRecording();

				expect(dictationStore.state.recording).toBe(true);
				expect(dictationStore.state.handsFree?.armed).not.toBe(true);
				expect(container.querySelector(".toast")).not.toBeNull();
				expect(container.querySelector('button[aria-label="Spoken replies"]')).toBeNull();
			});
		});

		/** Bug caught: the toast flips a local flag, so Settings and the backend never see the change. */
		it("flips the persisted setting from the toast while a hands-free conversation is armed", async () => {
			let stored = { ...SAVED_CONFIG };
			mockInvoke.mockImplementation((cmd: string, args?: { config?: typeof stored }) => {
				if (cmd === "disarm_hands_free_dictation") return Promise.resolve({ status: { ...armedStatus, armed: false } });
				if (cmd === "arm_hands_free_dictation" || cmd === "get_hands_free_status") return Promise.resolve(armedStatus);
				if (cmd === "get_dictation_config") return Promise.resolve({ ...stored });
				if (cmd === "set_dictation_config" && args?.config) {
					stored = { ...args.config };
				}
				return Promise.resolve(undefined);
			});

			await testInScopeAsync(async () => {
				const { container } = render(() => <DictationToast />);
				expect(await dictationStore.armHandsFree("s1")).toBe(true);

				const toggle = container.querySelector<HTMLButtonElement>('button[aria-label="Spoken replies"]');
				expect(toggle).not.toBeNull();
				expect(toggle?.getAttribute("aria-pressed")).toBe("true");

				toggle?.click();
				await vi.waitFor(() => expect(toggle?.getAttribute("aria-pressed")).toBe("false"));
				expect(stored.hands_free_spoken_replies).toBe(false);
				expect(dictationStore.state.spokenReplies).toBe(false);

				toggle?.click();
				await vi.waitFor(() => expect(toggle?.getAttribute("aria-pressed")).toBe("true"));
				expect(stored.hands_free_spoken_replies).toBe(true);

				await dictationStore.disarmHandsFree();
			});
		});

		/** Bug caught: a replies-only conversation keeps a permanent toast with a flat microphone meter. */
		it("stays hidden for an idle replies-only conversation and appears while recording", async () => {
			vi.useFakeTimers();
			mockInvoke.mockImplementation((cmd: string) => {
				if (cmd === "arm_hands_free_dictation" || cmd === "get_hands_free_status")
					return Promise.resolve({
						armed: true,
						listening: false,
						phase: "waiting",
						sessionId: "s1",
						owner: "desktop",
						generation: 1,
						pendingText: null,
						holdBackMs: 1500,
						error: null,
					});
				if (cmd === "get_dictation_status") return Promise.resolve({ audio_level: 0 });
				if (cmd === "get_speech_status") return Promise.resolve({ speaking: false, paused: false });
				return Promise.resolve(undefined);
			});

			await testInScopeAsync(async () => {
				const { container } = render(() => <DictationToast />);
				expect(await dictationStore.armHandsFree("s1")).toBe(true);
				await vi.advanceTimersByTimeAsync(300);
				expect(container.querySelector(".toast")).toBeNull();

				await dictationStore.startRecording();
				await vi.advanceTimersByTimeAsync(100);
				expect(container.querySelector(".toast")).not.toBeNull();
				expect(container.querySelector('button[aria-label="Spoken replies"]')).not.toBeNull();
				expect(container.textContent).toContain("Listening");
			});
		});
	});

	describe("while a hands-free conversation is armed", () => {
		const status = (armed: boolean, phase: string) => ({
			armed,
			phase,
			sessionId: armed ? "s1" : null,
			owner: armed ? "desktop" : null,
			generation: 1,
			pendingText: null,
			holdBackMs: 1500,
			error: null,
		});

		/** Answer each command the arm path and the monitor issue. */
		const backend = (hands: { armed: boolean; phase: string }, level: number, speaking = false, paused = false) =>
			mockInvoke.mockImplementation((cmd: string) => {
				if (cmd === "arm_hands_free_dictation" || cmd === "get_hands_free_status") {
					return Promise.resolve(status(hands.armed, hands.phase));
				}
				if (cmd === "disarm_hands_free_dictation") {
					return Promise.resolve({ status: status(false, "disarmed") });
				}
				if (cmd === "get_dictation_status") return Promise.resolve({ audio_level: level });
				if (cmd === "get_speech_status") return Promise.resolve({ speaking, paused });
				return Promise.resolve(undefined);
			});

		it("shows the phase and the live microphone level without push-to-talk", async () => {
			vi.useFakeTimers();
			backend({ armed: true, phase: "waiting" }, 0.5);

			await testInScopeAsync(async () => {
				const { container } = render(() => <DictationToast />);
				expect(await dictationStore.armHandsFree("s1")).toBe(true);
				expect(dictationStore.state.recording).toBe(false);

				await vi.advanceTimersByTimeAsync(100);
				expect(container.querySelector(".toast")).not.toBeNull();
				expect(container.textContent).toContain("Listening");
				// The raw 0.5 reaches the meter through the noise floor, not as 50%.
				const eased = Math.round(easeMeterLevel(0, 0.5) * 100);
				expect(eased).toBeGreaterThan(0);
				expect(eased).toBeLessThan(50);
				const meter = container.querySelector('[role="meter"]');
				expect(meter?.classList.contains("voiceMeter")).toBe(true);
				expect(meter?.getAttribute("aria-valuenow")).toBe(String(eased));
				// A conversation stays open for minutes: nothing may pulse or animate.
				expect(container.querySelector(".indicator")).toBeNull();
				expect(container.querySelector(".dots")).toBeNull();

				await dictationStore.disarmHandsFree();
			});
		});

		it("acknowledges only a recognized wake, not microphone activity", async () => {
			// catches: VAD or unrelated speech displaying a false wake acknowledgement.
			vi.useFakeTimers();
			const hands = { armed: true, phase: "capturing" };
			backend(hands, 0.4);
			await testInScopeAsync(async () => {
				const { container } = render(() => <DictationToast />);
				await dictationStore.armHandsFree("s1");
				expect(container.textContent).toContain("Listening");
				expect(container.textContent).not.toContain("Go ahead");
				for (const [phase, label] of [
					["wake_confirmed", "Go ahead"],
					["transcribing", "Transcribing"],
					["waiting", "Listening"],
				]) {
					hands.phase = phase;
					await vi.advanceTimersByTimeAsync(400);
					expect(container.textContent).toContain(label);
					if (phase !== "wake_confirmed") expect(container.textContent).not.toContain("Go ahead");
				}
				await dictationStore.disarmHandsFree();
			});
		});

		it("follows the phase and says when the reply is being spoken", async () => {
			vi.useFakeTimers();
			const hands = { armed: true, phase: "waiting" };
			backend(hands, 0.1);

			await testInScopeAsync(async () => {
				const { container } = render(() => <DictationToast />);
				await dictationStore.armHandsFree("s1");

				hands.phase = "transcribing";
				await vi.advanceTimersByTimeAsync(400);
				expect(container.textContent).toContain("Transcribing");

				backend(hands, 0.1, true);
				await vi.advanceTimersByTimeAsync(400);
				expect(container.textContent).toContain("Speaking");

				await dictationStore.disarmHandsFree();
			});
		});

		/** The playback controls; the always-present spoken-replies toggle has its own tests. */
		const labels = (container: HTMLElement) =>
			Array.from(container.querySelectorAll("button"))
				.map((b) => b.getAttribute("aria-label"))
				.filter((label) => label !== "Spoken replies");

		/** Bug caught: controls shown for the wrong state (Pause while paused), or while nothing speaks. */
		it("offers pause and stop while speaking, play and stop while paused, nothing otherwise", async () => {
			vi.useFakeTimers();
			const hands = { armed: true, phase: "waiting" };
			backend(hands, 0.1);

			await testInScopeAsync(async () => {
				const { container } = render(() => <DictationToast />);
				await dictationStore.armHandsFree("s1");
				await vi.advanceTimersByTimeAsync(400);
				expect(labels(container)).toEqual([]);

				backend(hands, 0.1, true);
				await vi.advanceTimersByTimeAsync(400);
				expect(labels(container)).toEqual(["Pause reply", "Stop reply"]);

				backend(hands, 0.1, false, true);
				await vi.advanceTimersByTimeAsync(400);
				expect(labels(container)).toEqual(["Resume reply", "Stop reply"]);
				expect(container.textContent).toContain("Paused");

				await dictationStore.disarmHandsFree();
			});
		});

		/** Bug caught: a click that does not reach the backend, or wires Stop to pause. */
		it("sends pause, resume and stop to the backend and shows the answer", async () => {
			vi.useFakeTimers();
			const hands = { armed: true, phase: "waiting" };
			backend(hands, 0.1, true);

			await testInScopeAsync(async () => {
				const { container } = render(() => <DictationToast />);
				await dictationStore.armHandsFree("s1");
				await vi.advanceTimersByTimeAsync(400);

				const click = async (label: string) => {
					(container.querySelector(`button[aria-label="${label}"]`) as HTMLButtonElement).click();
					await vi.advanceTimersByTimeAsync(0);
				};
				const answer = (command: string, speech: object) =>
					mockInvoke.mockImplementation((cmd: string) =>
						Promise.resolve(
							cmd === command
								? speech
								: cmd === "disarm_hands_free_dictation"
									? { status: status(false, "disarmed") }
									: undefined,
						),
					);

				answer("pause_speech", { speaking: false, paused: true });
				await click("Pause reply");
				expect(mockInvoke).toHaveBeenCalledWith("pause_speech");
				expect(labels(container)).toEqual(["Resume reply", "Stop reply"]);

				answer("resume_speech", { speaking: true, paused: false });
				await click("Resume reply");
				expect(mockInvoke).toHaveBeenCalledWith("resume_speech");
				expect(labels(container)).toEqual(["Pause reply", "Stop reply"]);

				answer("stop_speech", { speaking: false, paused: false });
				await click("Stop reply");
				expect(mockInvoke).toHaveBeenCalledWith("stop_speech");
				expect(labels(container)).toEqual([]);

				await dictationStore.disarmHandsFree();
			});
		});

		it("hides and stops polling once the conversation is disarmed", async () => {
			vi.useFakeTimers();
			backend({ armed: true, phase: "waiting" }, 0.5);

			await testInScopeAsync(async () => {
				const { container } = render(() => <DictationToast />);
				await dictationStore.armHandsFree("s1");
				await vi.advanceTimersByTimeAsync(100);
				expect(container.querySelector(".toast")).not.toBeNull();

				await dictationStore.disarmHandsFree();
				expect(dictationStore.state.audioLevel).toBe(0);
				await vi.advanceTimersByTimeAsync(200);
				expect(container.querySelector(".toast")).toBeNull();

				mockInvoke.mockClear();
				await vi.advanceTimersByTimeAsync(1000);
				expect(mockInvoke).not.toHaveBeenCalledWith("get_dictation_status");
			});
		});
	});
});
