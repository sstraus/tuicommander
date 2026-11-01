/**
 * The microphone and the speaker of a browser tab, on a WebSocket.
 *
 * Transport only, deliberately. Utterance boundaries, the activation phrase,
 * the hold-back and whether a turn may be delivered are all decided in Rust,
 * exactly as they are for the desktop endpoint — this file captures samples,
 * ships them, and plays back what comes the other way. Anything here that
 * started deciding *when* someone had finished speaking would be a second
 * implementation of the segmenter, disagreeing with the first one only on
 * browsers.
 *
 * ## Wire format
 *
 * | Direction | Frame | Meaning |
 * |---|---|---|
 * | up | binary | `f32` little-endian samples, mono, 16 kHz |
 * | up | `{"type":"playback-ended","sequence":"1"}` | the reply finished playing |
 * | down | binary | `u32` little-endian sample rate, `u64` little-endian reply sequence, then `f32` samples |
 * | down | `{"type":"stop"}` | stop playing and drop the queue |
 * | down | `{"type":"pause"}` | hold the reply where it is, and any that arrives |
 * | down | `{"type":"resume"}` | carry on from where it was held |
 *
 * Resampling happens here rather than on the server: the `AudioContext` is
 * created at 16 kHz and resamples the device's native rate for free, and
 * sending 48 kHz would triple the bytes for audio the segmenter throws away.
 */

import { appLogger } from "../stores/appLogger";
import { holdScreenAwake } from "./screenWakeLock";

/** The rate Whisper wants, and therefore the rate the socket carries. */
const CAPTURE_SAMPLE_RATE = 16_000;

/**
 * How many samples are buffered before a frame is sent.
 *
 * 1024 at 16 kHz is 64 ms — short enough that the hold-back and the
 * activation phrase still feel immediate, long enough that a conversation is
 * ~16 frames a second rather than a frame per audio callback.
 */
const FRAME_SAMPLES = 1024;

/** A live browser audio session: its socket, its microphone and its playback. */
export interface BrowserVoiceSession {
	/** Close the socket, the microphone and anything still playing. */
	stop(): void;
}

/** What `connectBrowserVoice` needs, named so a test can supply its own. */
export interface BrowserVoiceDeps {
	/** Build the socket. Injected because a test has no server. */
	openSocket(url: string): WebSocket;
	/** Open the microphone. */
	getUserMedia(): Promise<MediaStream>;
	/** Build the capture context, at the rate the socket carries. */
	createContext(sampleRate: number): AudioContext;
	/** The page's audio session where the browser has one (Safari 16.4+), else undefined. */
	audioSession(): { type: string } | undefined;
}

/**
 * The microphone, or a readable reason there is none.
 *
 * Over plain `http://` the browser hides `navigator.mediaDevices` entirely, so
 * calling through it raises a bare `TypeError` that the arming error shows as
 * is. The usual cause on a phone is the page being opened by IP or an HTTP
 * Tailscale URL, and that is the one thing the user can fix.
 */
export function openMicrophone(): Promise<MediaStream> {
	if (!navigator.mediaDevices?.getUserMedia) {
		return Promise.reject(
			new Error(
				window.isSecureContext
					? "This browser cannot open the microphone."
					: "Voice needs a secure page: open TUICommander over https:// (the browser blocks the microphone on plain http).",
			),
		);
	}
	return navigator.mediaDevices.getUserMedia({
		audio: {
			// The browser's own cancellation, on top of the server's. They
			// are solving the same problem at different distances: this one
			// knows the device, and AEC3 in Rust knows what was sent to be
			// played. Neither is sufficient alone on a laptop speaker.
			echoCancellation: true,
			noiseSuppression: true,
			autoGainControl: true,
		},
	});
}

const browserDeps: BrowserVoiceDeps = {
	openSocket: (url) => new WebSocket(url),
	getUserMedia: openMicrophone,
	createContext: (sampleRate) => new AudioContext({ sampleRate }),
	audioSession: () => (navigator as Navigator & { audioSession?: { type: string } }).audioSession,
};

/** The `ws://`/`wss://` audio socket for this origin. */
export function audioSocketUrl(owner: string, origin = window.location): string {
	const scheme = origin.protocol === "https:" ? "wss:" : "ws:";
	return `${scheme}//${origin.host}/dictation/hands-free/audio?owner=${encodeURIComponent(owner)}`;
}

/**
 * Split a binary downlink frame into its rate and its samples.
 *
 * Exported for its own test: a frame that is decoded at the wrong rate plays
 * at the wrong pitch, which is the kind of failure that is obvious to a human
 * and invisible to an assertion about lengths.
 */
export function decodeReply(frame: ArrayBuffer): {
	sampleRate: number;
	sequence: string;
	samples: Float32Array;
} {
	const view = new DataView(frame);
	return {
		sampleRate: view.getUint32(0, true),
		sequence: view.getBigUint64(4, true).toString(),
		samples: new Float32Array(frame.slice(12)),
	};
}

/** Pack captured samples for the uplink. */
export function encodeCapture(samples: Float32Array): ArrayBuffer {
	// `Float32Array` is already little-endian on every platform a browser runs
	// on, so the buffer is the frame.
	return samples.buffer.slice(samples.byteOffset, samples.byteOffset + samples.byteLength) as ArrayBuffer;
}

/**
 * Open this tab's microphone and speaker for the conversation owned by
 * `owner`.
 *
 * The socket is opened **before** arming, and arming fails when it is not
 * there: Rust refuses an owner with no client rather than falling back to its
 * own microphone, so a tab that armed first would be told its own name is
 * unknown.
 *
 * `listen` false is a replies-only conversation: the socket and the speaker
 * are opened, the microphone is not.
 */
export async function connectBrowserVoice(
	owner: string,
	deps: BrowserVoiceDeps = browserDeps,
	listen = true,
): Promise<BrowserVoiceSession> {
	// Everything up to the first `await` runs inside the arming gesture, which
	// is the only place iOS lets a context leave `suspended` (and the only place
	// it lets the audio category change). `getUserMedia` resolves later, after
	// the prompt, by when the gesture no longer counts.
	const socket = deps.openSocket(audioSocketUrl(owner));
	socket.binaryType = "arraybuffer";
	const session = deps.audioSession();
	// The default category is `auto`, which on iOS plays the reply at ringer
	// volume or not at all once the microphone is open.
	if (session) session.type = listen ? "play-and-record" : "playback";
	const context = deps.createContext(CAPTURE_SAMPLE_RATE);
	const resumed = context.resume();
	// Observed below; this keeps a rejection during the mic prompt from being reported as unhandled.
	resumed.catch(() => {});

	let stream: MediaStream | undefined;
	try {
		if (listen) stream = await deps.getUserMedia();
		await resumed;
		if (context.state === "suspended") await context.resume();
	} catch (err) {
		// The socket is the conversation's claim on the owner name: a failed
		// start must not leave it open, nor a context alive, nor a granted
		// microphone running.
		stream?.getTracks().forEach((track) => track.stop());
		socket.close();
		void context.close();
		throw err;
	}
	// Held only once the microphone is open: a failed start above never took it.
	const releaseScreen = stream ? holdScreenAwake() : () => {};
	const source = stream ? context.createMediaStreamSource(stream) : undefined;
	// `ScriptProcessorNode` rather than an `AudioWorklet`: a worklet needs a
	// separate module URL, which the packaged frontend serves from a path the
	// plugin loader also owns, and this node is deprecated rather than absent.
	// The work per callback is a copy, not a computation.
	const capture = source ? context.createScriptProcessor(FRAME_SAMPLES, 1, 1) : undefined;

	if (capture && source) {
		capture.onaudioprocess = (event) => {
			if (socket.readyState !== WebSocket.OPEN) return;
			socket.send(encodeCapture(event.inputBuffer.getChannelData(0)));
		};
		source.connect(capture);
		// A `ScriptProcessorNode` only runs while it is connected to a
		// destination. Nothing is *heard* from it — it emits the silence its
		// output buffer was created with — but without this the callback above
		// never fires and the conversation hears nothing.
		capture.connect(context.destination);
	}

	// Schedule the complete queue on the audio clock. Starting the next reply
	// from onended would insert a main-thread callback delay between buffers.
	const replies: {
		buffer: AudioBuffer;
		sequence: string;
		node: AudioBufferSourceNode | null;
		startedAt: number;
		offset: number;
	}[] = [];
	let paused = false;
	const detach = (reply: (typeof replies)[number]) => {
		const node = reply.node;
		reply.node = null;
		node?.stop();
	};
	const stopPlayback = () => {
		for (const reply of replies) detach(reply);
		replies.length = 0;
		paused = false;
	};
	const startPlayback = (reply: (typeof replies)[number], when: number) => {
		const node = context.createBufferSource();
		node.buffer = reply.buffer;
		node.connect(context.destination);
		node.onended = () => {
			// stop/pause can deliver ended after this buffer has been rescheduled.
			if (reply.node !== node) return;
			reply.node = null;
			replies.splice(replies.indexOf(reply), 1);
			if (socket.readyState === WebSocket.OPEN) {
				socket.send(JSON.stringify({ type: "playback-ended", sequence: reply.sequence }));
			}
		};
		reply.startedAt = when - reply.offset;
		reply.node = node;
		node.start(when, reply.offset);
	};
	const schedule = () => {
		let when = context.currentTime;
		for (const reply of replies) {
			if (!reply.node) startPlayback(reply, when);
			when = Math.max(when, reply.startedAt + reply.buffer.duration);
		}
	};

	socket.onmessage = (event) => {
		if (typeof event.data === "string") {
			const type = JSON.parse(event.data).type;
			if (type === "stop") stopPlayback();
			if (type === "pause" && !paused) {
				paused = true;
				for (const reply of replies) {
					reply.offset = Math.min(reply.buffer.duration, Math.max(0, context.currentTime - reply.startedAt));
					detach(reply);
				}
			}
			if (type === "resume" && paused) {
				paused = false;
				schedule();
			}
			return;
		}
		const { sampleRate, sequence, samples } = decodeReply(event.data as ArrayBuffer);
		if (!sampleRate || samples.length === 0) return;
		// Its own rate, not the capture context's: the engine renders at 24 kHz
		// and the browser resamples. Reusing the capture rate would play every
		// reply 50% slow.
		const buffer = context.createBuffer(1, samples.length, sampleRate);
		buffer.getChannelData(0).set(samples);
		replies.push({ buffer, sequence, node: null, startedAt: 0, offset: 0 });
		if (!paused) schedule();
	};

	socket.onerror = () => {
		appLogger.error("dictation", "Browser voice socket failed");
	};

	return {
		stop() {
			socket.onmessage = null;
			releaseScreen();
			stopPlayback();
			if (capture) {
				capture.onaudioprocess = null;
				capture.disconnect();
			}
			source?.disconnect();
			for (const track of stream?.getTracks() ?? []) track.stop();
			void context.close();
			socket.close();
		},
	};
}
