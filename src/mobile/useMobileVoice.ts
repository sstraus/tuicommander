import { type Accessor, createEffect, createSignal, onCleanup } from "solid-js";
import { appLogger } from "../stores/appLogger";
import { toastsStore } from "../stores/toasts";
import { rpc } from "../transport";

type DictationModule = typeof import("../stores/dictation");
type DictationStore = DictationModule["dictationStore"];

/**
 * Tap-to-talk for one agent session, over the browser voice path.
 *
 * The dictation store is loaded ahead of the tap, not on it: `armHandsFree`
 * must run synchronously inside the click, because that is the only moment
 * iOS lets the audio context resume, and a dynamic `import()` awaited in the
 * handler would spend the gesture before the first line of it ran. It is also
 * kept out of the entry chunk for the phones that never use voice.
 */
export function useMobileVoice(sessionId: Accessor<string>, enabled: Accessor<boolean>) {
	const [store, setStore] = createSignal<DictationStore | null>(null);
	const [available, setAvailable] = createSignal(false);
	let module: DictationModule | null = null;
	let arming = false;
	let disposed = false;

	createEffect(() => {
		if (!enabled() || available()) return;
		let cancelled = false;
		onCleanup(() => {
			cancelled = true;
		});
		// A 404 is the headless `tuic-remote`, which does not compile the voice
		// routes. Any other failure hides the control as well: a mic button that
		// cannot work is worse than none.
		rpc("get_hands_free_status")
			.then(() => import("../stores/dictation"))
			.then((loaded) => {
				if (cancelled) return;
				module = loaded;
				setStore(loaded.dictationStore);
				void loaded.dictationStore.refreshConfig();
				setAvailable(true);
			})
			.catch((err) => appLogger.info("dictation", "Voice control hidden: hands-free unavailable", err));
	});

	const armed = () => {
		const status = store()?.state.handsFree;
		return !!status?.armed && status.sessionId === sessionId() && status.owner === module?.browserAudioOwner;
	};
	const phase = () => (armed() ? store()?.state.handsFree?.phase : undefined);

	async function toggle(): Promise<void> {
		const current = store();
		if (!current) return;
		if (armed()) {
			await current.disarmHandsFree();
			return;
		}
		// A second tap while the mic prompt is open must not open a second conversation.
		if (arming) return;
		arming = true;
		let ok: boolean;
		try {
			// No `await` before this call: see the note on the hook.
			ok = await current.armHandsFree(sessionId());
		} finally {
			arming = false;
		}
		// The screen was left while the permission prompt was open.
		if (ok && disposed) {
			await current.disarmHandsFree();
			return;
		}
		if (!ok) {
			const message = (current.state.handsFreeError ?? "Voice could not start").replace(/^Error: /, "");
			toastsStore.add("Voice not started", message, "error", true);
		}
	}

	// Leaving the screen ends the conversation: the mic must not stay open on
	// a page the user cannot see.
	onCleanup(() => {
		disposed = true;
		if (armed()) void store()?.disarmHandsFree();
	});

	return {
		available,
		armed,
		phase,
		toggle,
		spokenReplies: () => store()?.state.spokenReplies ?? true,
		setSpokenReplies: (value: boolean) => store()?.setSpokenReplies(value),
	};
}
