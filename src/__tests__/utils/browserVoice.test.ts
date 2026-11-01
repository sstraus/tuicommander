import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
	audioSocketUrl,
	type BrowserVoiceDeps,
	connectBrowserVoice,
	decodeReply,
	encodeCapture,
	openMicrophone,
} from "../../utils/browserVoice";

vi.mock("../../stores/appLogger", () => ({
	appLogger: { error: vi.fn(), warn: vi.fn(), info: vi.fn() },
}));

/** A socket that records what was sent and can be handed messages. */
class FakeSocket {
	readyState = 1;
	binaryType = "";
	sent: (string | ArrayBuffer)[] = [];
	onmessage: ((event: { data: unknown }) => void) | null = null;
	onerror: (() => void) | null = null;
	closed = false;

	send(frame: string | ArrayBuffer) {
		this.sent.push(frame);
	}
	close() {
		this.closed = true;
		this.readyState = 3;
	}
}

/** The three `AudioContext` nodes this module builds, recorded. */
function fakeContext() {
	const destination = { id: "destination" };
	const capture = {
		onaudioprocess: null as ((event: unknown) => void) | null,
		connect: vi.fn(),
		disconnect: vi.fn(),
	};
	const played: { rate: number; samples: Float32Array; started: boolean; when?: number; from?: number }[] = [];
	const clock = { now: 0 };
	const sources: { stop: ReturnType<typeof vi.fn>; onended: (() => void) | null }[] = [];
	// iOS creates a context suspended; it runs only once resumed in a gesture.
	const lifecycle = { state: "running" as "running" | "suspended" };
	const resume = vi.fn(async () => {
		lifecycle.state = "running";
	});
	return {
		lifecycle,
		resume,
		capture,
		played,
		sources,
		clock,
		context: {
			get currentTime() {
				return clock.now;
			},
			destination,
			get state() {
				return lifecycle.state;
			},
			resume,
			createMediaStreamSource: () => ({ connect: vi.fn(), disconnect: vi.fn() }),
			createScriptProcessor: () => capture,
			createBuffer: (_channels: number, length: number, rate: number) => {
				const samples = new Float32Array(length);
				return {
					getChannelData: () => samples,
					duration: length / rate,
					__rate: rate,
					__samples: samples,
				};
			},
			createBufferSource: () => {
				const node = {
					buffer: null as { __rate: number; __samples: Float32Array } | null,
					connect: vi.fn(),
					onended: null as (() => void) | null,
					start: (when?: number, from?: number) => {
						played.push({
							rate: node.buffer?.__rate ?? 0,
							samples: node.buffer?.__samples ?? new Float32Array(),
							started: true,
							when,
							from,
						});
					},
					stop: vi.fn(),
				};
				sources.push(node);
				return node;
			},
			close: vi.fn(),
		} as unknown as AudioContext,
	};
}

describe("the browser audio wire format", () => {
	it("round-trips samples through the uplink frame", () => {
		const samples = new Float32Array([0.25, -0.5, 1]);
		expect(Array.from(new Float32Array(encodeCapture(samples)))).toEqual([0.25, -0.5, 1]);
	});

	/**
	 * The rate travels with the audio because it is not the capture rate: the
	 * engine renders at 24 kHz into a 16 kHz socket, and a reply decoded at the
	 * wrong one plays at the wrong pitch — audible to a person, invisible to an
	 * assertion about lengths.
	 */
	it("reads the reply's own sample rate off the frame", () => {
		const frame = new ArrayBuffer(12 + 8);
		new DataView(frame).setUint32(0, 24_000, true);
		new DataView(frame).setBigUint64(4, 9_007_199_254_740_993n, true);
		new Float32Array(frame, 12).set([0.5, -0.5]);

		const decoded = decodeReply(frame);

		expect(decoded.sampleRate).toBe(24_000);
		expect(decoded.sequence).toBe("9007199254740993");
		expect(Array.from(decoded.samples)).toEqual([0.5, -0.5]);
	});

	it("names the owner on the socket URL and follows the page's scheme", () => {
		expect(audioSocketUrl("browser a/b", { protocol: "https:", host: "x:9877" } as Location)).toBe(
			"wss://x:9877/dictation/hands-free/audio?owner=browser%20a%2Fb",
		);
		expect(audioSocketUrl("b1", { protocol: "http:", host: "localhost:9877" } as Location)).toBe(
			"ws://localhost:9877/dictation/hands-free/audio?owner=b1",
		);
	});
});

describe("starting a browser voice session on iOS", () => {
	it("resumes a suspended context in the arming gesture, before the microphone prompt is answered", async () => {
		// catches: iOS leaves the AudioContext suspended, onaudioprocess never
		// fires and every reply is silent while the mic light is on.
		const nodes = fakeContext();
		nodes.lifecycle.state = "suspended";
		let answerPrompt: (stream: MediaStream) => void = () => {};
		const deps: BrowserVoiceDeps = {
			openSocket: () => new FakeSocket() as unknown as WebSocket,
			getUserMedia: () =>
				new Promise<MediaStream>((resolve) => {
					answerPrompt = resolve;
				}),
			createContext: () => nodes.context,
			audioSession: () => undefined,
		};

		const started = connectBrowserVoice("b1", deps);
		// The prompt is still open: `resume` must already have been called, since
		// the gesture is gone by the time the user answers.
		expect(nodes.resume).toHaveBeenCalledTimes(1);
		answerPrompt({ getTracks: () => [] } as unknown as MediaStream);
		await started;

		expect(nodes.lifecycle.state).toBe("running");
	});

	it("resumes again when the context fell back to suspended during the prompt", async () => {
		// catches: a context suspended again by the prompt stays silent.
		const nodes = fakeContext();
		nodes.resume.mockImplementationOnce(async () => {});
		nodes.lifecycle.state = "suspended";
		const deps: BrowserVoiceDeps = {
			openSocket: () => new FakeSocket() as unknown as WebSocket,
			getUserMedia: async () => ({ getTracks: () => [] }) as unknown as MediaStream,
			createContext: () => nodes.context,
			audioSession: () => undefined,
		};

		await connectBrowserVoice("b1", deps);

		expect(nodes.resume).toHaveBeenCalledTimes(2);
		expect(nodes.lifecycle.state).toBe("running");
	});

	it("sets the audio session to play-and-record before the microphone opens", async () => {
		// catches: the page stays in the default `auto` category when the mic
		// opens, so iOS routes the reply as a ringer-volume sound.
		const session = { type: "auto" };
		const seenAtPrompt: string[] = [];
		const nodes = fakeContext();
		const deps: BrowserVoiceDeps = {
			openSocket: () => new FakeSocket() as unknown as WebSocket,
			getUserMedia: async () => {
				seenAtPrompt.push(session.type);
				return { getTracks: () => [] } as unknown as MediaStream;
			},
			createContext: () => nodes.context,
			audioSession: () => session,
		};

		await connectBrowserVoice("b1", deps);

		expect(seenAtPrompt).toEqual(["play-and-record"]);
	});

	it("closes the socket and the context when the microphone is refused", async () => {
		// catches: a refused prompt leaves the owner's socket and a live
		// AudioContext behind, so the next arm is told the owner is taken.
		const socket = new FakeSocket();
		const nodes = fakeContext();
		const deps: BrowserVoiceDeps = {
			openSocket: () => socket as unknown as WebSocket,
			getUserMedia: async () => {
				throw new Error("NotAllowedError");
			},
			createContext: () => nodes.context,
			audioSession: () => undefined,
		};

		await expect(connectBrowserVoice("b1", deps)).rejects.toThrow("NotAllowedError");

		expect(socket.closed).toBe(true);
		expect((nodes.context as unknown as { close: ReturnType<typeof vi.fn> }).close).toHaveBeenCalled();
	});
});

describe("a replies-only browser voice session", () => {
	it("opens the socket and the speaker but never the microphone", async () => {
		// catches: replies-only still asks for the microphone, so a phone prompts
		// for a device the user turned off and streams silence to the server.
		const nodes = fakeContext();
		const getUserMedia = vi.fn(async () => ({ getTracks: () => [] }) as unknown as MediaStream);
		const session = { type: "auto" };
		const deps: BrowserVoiceDeps = {
			openSocket: () => new FakeSocket() as unknown as WebSocket,
			getUserMedia,
			createContext: () => nodes.context,
			audioSession: () => session,
		};

		const voice = await connectBrowserVoice("b1", deps, false);

		expect(getUserMedia).not.toHaveBeenCalled();
		expect(nodes.capture.connect).not.toHaveBeenCalled();
		expect(session.type).toBe("playback");
		expect(() => voice.stop()).not.toThrow();
	});
});

describe("openMicrophone", () => {
	afterEach(() => vi.unstubAllGlobals());

	it("explains a non-secure origin instead of throwing a bare TypeError", async () => {
		// catches: plain http leaves navigator.mediaDevices undefined and the
		// user reads "Cannot read properties of undefined (reading 'getUserMedia')".
		vi.stubGlobal("navigator", { mediaDevices: undefined });
		vi.stubGlobal("isSecureContext", false);

		await expect(openMicrophone()).rejects.toThrow(/https/);
	});

	it("does not blame the origin when the page is secure but the browser has no microphone API", async () => {
		// catches: telling a user on https to "use https".
		vi.stubGlobal("navigator", { mediaDevices: undefined });
		vi.stubGlobal("isSecureContext", true);

		await expect(openMicrophone()).rejects.toThrow(/cannot open the microphone/);
	});
});

describe("a browser voice session", () => {
	let socket: FakeSocket;
	let nodes: ReturnType<typeof fakeContext>;
	let deps: BrowserVoiceDeps;
	let tracks: { stop: ReturnType<typeof vi.fn> }[];

	beforeEach(() => {
		socket = new FakeSocket();
		sequence = 0n;
		nodes = fakeContext();
		tracks = [{ stop: vi.fn() }];
		deps = {
			openSocket: () => socket as unknown as WebSocket,
			getUserMedia: async () => ({ getTracks: () => tracks }) as unknown as MediaStream,
			createContext: () => nodes.context,
			audioSession: () => undefined,
		};
	});

	let sequence = 0n;
	const reply = (seconds = 4, rate = 24_000) => {
		const frame = new ArrayBuffer(12 + seconds * rate * 4);
		new DataView(frame).setUint32(0, rate, true);
		new DataView(frame).setBigUint64(4, ++sequence, true);
		return frame;
	};

	// Catches: a second downlink stops the first reply instead of appending it.
	it("schedules consecutive replies without cutting active audio or waiting for ended callbacks", async () => {
		await connectBrowserVoice("b1", deps);
		socket.onmessage?.({ data: reply() });
		nodes.clock.now = 1;
		socket.onmessage?.({ data: reply(2, 48_000) });

		expect(nodes.sources[0].stop).not.toHaveBeenCalled();
		expect(nodes.played.map(({ when, from }) => [when, from])).toEqual([
			[0, 0],
			[4, 0],
		]);
		// Even if the main thread receives the ended callback late, the second
		// source is already scheduled at the first source's exact audio-clock end.
		nodes.clock.now = 4.5;
		nodes.sources[0].onended?.();
		expect(socket.sent).toEqual([JSON.stringify({ type: "playback-ended", sequence: "1" })]);
		nodes.sources[1].onended?.();
		expect(socket.sent).toEqual([
			JSON.stringify({ type: "playback-ended", sequence: "1" }),
			JSON.stringify({ type: "playback-ended", sequence: "2" }),
		]);
	});

	// Catches: holding the queue loses the active remainder or overlaps later audio.
	it("resumes the active remainder before replies appended during a hold", async () => {
		await connectBrowserVoice("b1", deps);
		socket.onmessage?.({ data: reply() });
		socket.onmessage?.({ data: reply(2) });
		nodes.clock.now = 1.5;
		socket.onmessage?.({ data: JSON.stringify({ type: "pause" }) });
		for (const source of nodes.sources) expect(source.stop).toHaveBeenCalledOnce();
		socket.onmessage?.({ data: reply(3) });
		nodes.clock.now = 10;
		socket.onmessage?.({ data: JSON.stringify({ type: "resume" }) });
		expect(nodes.played.slice(2).map(({ when, from }) => [when, from])).toEqual([
			[10, 1.5],
			[12.5, 0],
			[14.5, 0],
		]);
		// Stopped nodes can report ended after the replacement nodes started.
		nodes.sources[0].onended?.();
		nodes.sources[1].onended?.();
		expect(socket.sent).toEqual([]);
	});

	// Catches: stop/teardown leaves a future source audible or sends a stale ACK.
	it.each(["server stop", "session stop"])("drops scheduled and held replies on %s", async (kind) => {
		const voice = await connectBrowserVoice("b1", deps);
		socket.onmessage?.({ data: reply() });
		socket.onmessage?.({ data: reply(2) });
		if (kind === "server stop") socket.onmessage?.({ data: JSON.stringify({ type: "stop" }) });
		else voice.stop();
		for (const source of nodes.sources) {
			expect(source.stop).toHaveBeenCalledOnce();
			source.onended?.();
		}
		socket.onmessage?.({ data: JSON.stringify({ type: "resume" }) });
		expect(nodes.played).toHaveLength(2);
		expect(socket.sent).toEqual([]);
	});

	it("ships captured audio up the socket without deciding anything about it", async () => {
		await connectBrowserVoice("b1", deps);

		nodes.capture.onaudioprocess?.({
			inputBuffer: { getChannelData: () => new Float32Array([0.25, 0.5]) },
		});

		expect(socket.sent).toHaveLength(1);
		expect(Array.from(new Float32Array(socket.sent[0] as ArrayBuffer))).toEqual([0.25, 0.5]);
	});

	/**
	 * Silence is audio. Whether an utterance has ended is the segmenter's
	 * decision in Rust, and a frontend that dropped quiet frames would be a
	 * second segmenter that only exists in browsers.
	 */
	it("sends silence too, rather than deciding the user stopped talking", async () => {
		await connectBrowserVoice("b1", deps);

		nodes.capture.onaudioprocess?.({
			inputBuffer: { getChannelData: () => new Float32Array([0, 0, 0, 0]) },
		});

		expect(socket.sent).toHaveLength(1);
	});

	it("plays a reply at the rate it was rendered at and reports the end", async () => {
		await connectBrowserVoice("b1", deps);
		const frame = new ArrayBuffer(12 + 8);
		new DataView(frame).setUint32(0, 24_000, true);
		new Float32Array(frame, 12).set([0.5, -0.5]);

		socket.onmessage?.({ data: frame });

		expect(nodes.played).toHaveLength(1);
		expect(nodes.played[0].rate).toBe(24_000);
		expect(Array.from(nodes.played[0].samples)).toEqual([0.5, -0.5]);
	});

	/**
	 * Barge-in reaches the browser as a control frame, and the reply has to
	 * stop where the user is — the server can only stop sending.
	 */
	it("stops playback when the server says the turn is over", async () => {
		await connectBrowserVoice("b1", deps);
		const frame = new ArrayBuffer(12 + 4);
		new DataView(frame).setUint32(0, 24_000, true);
		socket.onmessage?.({ data: frame });

		socket.onmessage?.({ data: JSON.stringify({ type: "stop" }) });

		expect(nodes.sources[0].stop).toHaveBeenCalled();
		// The socket stays open: the turn ended, not the conversation.
		expect(socket.closed).toBe(false);
	});

	/**
	 * A voice over the reply that is not for us: the reply holds where it is and
	 * goes on from there. Catches: a pause that restarts the reply from the
	 * beginning, or one that suspends the context and takes the microphone with
	 * it.
	 */
	it("holds the reply on pause and continues from the same position on resume", async () => {
		await connectBrowserVoice("b1", deps);
		const frame = reply();
		socket.onmessage?.({ data: frame });
		nodes.clock.now = 2.5;

		socket.onmessage?.({ data: JSON.stringify({ type: "pause" }) });
		expect(nodes.sources[0].stop).toHaveBeenCalled();
		expect(nodes.played).toHaveLength(1);

		nodes.clock.now = 9;
		socket.onmessage?.({ data: JSON.stringify({ type: "resume" }) });

		expect(nodes.played).toHaveLength(2);
		expect(nodes.played[1].from).toBe(2.5);
	});

	/** A reply handed over while held waits instead of talking over the user. */
	it("keeps a reply that arrives during a pause until the resume", async () => {
		await connectBrowserVoice("b1", deps);
		socket.onmessage?.({ data: JSON.stringify({ type: "pause" }) });
		const frame = new ArrayBuffer(12 + 4);
		new DataView(frame).setUint32(0, 24_000, true);
		socket.onmessage?.({ data: frame });
		expect(nodes.played).toHaveLength(0);

		socket.onmessage?.({ data: JSON.stringify({ type: "resume" }) });

		expect(nodes.played).toHaveLength(1);
		expect(nodes.played[0].from).toBe(0);
	});

	/**
	 * The microphone must close with the conversation. A tab that kept its
	 * device light on after disarming is the failure a user notices and cannot
	 * explain.
	 */
	it("releases the microphone and the socket when it stops", async () => {
		const session = await connectBrowserVoice("b1", deps);

		session.stop();

		expect(tracks[0].stop).toHaveBeenCalled();
		expect(nodes.capture.disconnect).toHaveBeenCalled();
		expect(socket.closed).toBe(true);
	});
});

describe("the screen lock of a browser voice session", () => {
	const lock = { release: vi.fn(async () => {}) };
	const request = vi.fn(async () => lock);
	const deps = (listen: boolean): BrowserVoiceDeps => ({
		openSocket: () => new FakeSocket() as unknown as WebSocket,
		getUserMedia: async () => {
			if (!listen) throw new Error("replies-only must not open the microphone");
			return { getTracks: () => [] } as unknown as MediaStream;
		},
		createContext: () => fakeContext().context,
		audioSession: () => undefined,
	});
	const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

	beforeEach(() => {
		localStorage.clear();
		Object.defineProperty(navigator, "wakeLock", { configurable: true, value: { request } });
	});
	afterEach(() => {
		Reflect.deleteProperty(navigator, "wakeLock");
		vi.clearAllMocks();
	});

	// Catches: hands-free listening on a phone lets the screen lock and the conversation go deaf.
	it("holds the lock while the microphone is open and releases it on stop", async () => {
		const session = await connectBrowserVoice("b1", deps(true));
		await flush();
		expect(request).toHaveBeenCalledOnce();
		session.stop();
		expect(lock.release).toHaveBeenCalledOnce();
	});

	// Catches: a refused microphone start leaves the screen pinned awake.
	it("takes no lock when the microphone fails to open", async () => {
		const failing = { ...deps(true), getUserMedia: async () => Promise.reject(new Error("denied")) };
		await expect(connectBrowserVoice("b1", failing)).rejects.toThrow("denied");
		await flush();
		expect(request).not.toHaveBeenCalled();
	});

	// Catches: a replies-only conversation, which captures nothing, keeps the screen on.
	it("takes no lock in a replies-only conversation", async () => {
		const session = await connectBrowserVoice("b1", deps(false), false);
		await flush();
		expect(request).not.toHaveBeenCalled();
		session.stop();
	});
});
