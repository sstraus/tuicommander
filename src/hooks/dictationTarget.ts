import { EditorView } from "@codemirror/view";
import { composeSubmit } from "../components/ComposePanel/composeSubmit";
import { terminalsStore } from "../stores/terminals";
import { agentInputPayload, getShellFamily, sendCommand } from "../utils/sendCommand";

export interface DictationTarget {
	kind: "input" | "editor" | "compose" | "contenteditable" | "pty" | "none";
	terminalId: string | null;
	sessionId: string | null;
	deliver: (
		text: string,
		write: (sessionId: string, data: string) => Promise<void>,
		autoSend: boolean,
	) => Promise<void>;
}

/** Resolve the operator's destination synchronously, before recording can yield. */
export function captureDictationTarget(): DictationTarget {
	const el = document.activeElement;
	const ownerId = el?.closest<HTMLElement>("[data-terminal-id]")?.dataset.terminalId;
	const terminal = ownerId ? terminalsStore.get(ownerId) : terminalsStore.getActive();
	const terminalId = terminal?.id ?? null;
	const sessionId = terminal?.sessionId ?? null;
	const context = { terminalId, sessionId };
	function unavailable(): never {
		throw new Error("Dictation target unavailable");
	}
	const checkSession = () => {
		if (
			terminalId &&
			(terminalsStore.get(terminalId)?.sessionId !== sessionId ||
				terminalsStore.get(terminalId)?.shellState === "exited")
		)
			unavailable();
	};
	const editorTarget = (view: EditorView, kind: "editor" | "compose"): DictationTarget => {
		const { doc, selection } = view.state;
		return {
			kind,
			...context,
			async deliver(text, _write, autoSend) {
				if (ownerId || kind === "compose") checkSession();
				if (
					!view.contentDOM.isConnected ||
					EditorView.findFromDOM(view.contentDOM) !== view ||
					!view.state.doc.eq(doc) ||
					view.state.readOnly ||
					!view.state.facet(EditorView.editable)
				)
					unavailable();
				// Use the captured selection on the unchanged draft, independent of DOM
				// focus. A concurrently edited draft is retained rather than overwritten.
				const selected = view.state.update({ selection }).state;
				view.dispatch({ ...selected.replaceSelection(text), userEvent: "input.dictation" });
				// Only terminal Chat Compose opts into submit. Ordinary editors still
				// need review, and Compose owns in-flight deduplication and cleanup.
				if (kind === "compose" && autoSend) view.state.facet(composeSubmit)?.();
			},
		};
	};
	// findFromDOM searches descendants, so BODY would match any editor on the page.
	const cmDom = el instanceof HTMLElement ? el.closest<HTMLElement>(".cm-editor") : null;
	const view = cmDom ? EditorView.findFromDOM(cmDom) : null;
	if (view)
		return editorTarget(
			view,
			ownerId && terminal?.viewMode === "chat" && cmDom?.closest("[data-compose-editor]") ? "compose" : "editor",
		);

	// CanvasTerminal and xterm own hidden keyboard inputs; those are PTY focus,
	// not ordinary editors. Explicit search/form inputs still take precedence.
	const isInput = el instanceof HTMLTextAreaElement || el instanceof HTMLInputElement;
	if (isInput && !el.closest(".xterm, [data-terminal-container]")) {
		const { value, selectionStart, selectionEnd } = el;
		const from = selectionStart ?? value.length;
		const to = selectionEnd ?? from;
		return {
			kind: "input",
			...context,
			async deliver(text) {
				if (ownerId) checkSession();
				if (!el.isConnected || el.disabled || el.readOnly || el.value !== value) unavailable();
				el.value = value.slice(0, from) + text + value.slice(to);
				el.selectionStart = el.selectionEnd = from + text.length;
				el.dispatchEvent(new Event("input", { bubbles: true }));
			},
		};
	}
	const editable = el?.closest<HTMLElement>('[contenteditable="true"]');
	if (editable) {
		const selection = document.getSelection();
		const range =
			selection?.rangeCount && editable.contains(selection.anchorNode) ? selection.getRangeAt(0).cloneRange() : null;
		return {
			kind: "contenteditable",
			...context,
			async deliver(text) {
				if (ownerId) checkSession();
				if (!editable.isConnected || !range || !editable.contains(range.commonAncestorContainer)) unavailable();
				editable.focus();
				const current = document.getSelection();
				current?.removeAllRanges();
				current?.addRange(range);
				if (!document.execCommand("insertText", false, text)) unavailable();
			},
		};
	}
	if (terminal?.viewMode === "chat" && sessionId) {
		const root = Array.from(document.querySelectorAll<HTMLElement>("[data-terminal-id]")).find(
			(node) => node.dataset.terminalId === terminalId,
		);
		const dom = root?.querySelector<HTMLElement>("[data-compose-editor] .cm-editor");
		const compose = dom ? EditorView.findFromDOM(dom) : null;
		return compose
			? editorTarget(compose, "compose")
			: { kind: "compose", ...context, deliver: async () => unavailable() };
	}
	if (terminal && sessionId) {
		const agentType = terminal.agentType;
		return {
			kind: "pty",
			...context,
			async deliver(text, write, autoSend) {
				checkSession();
				const writeFn = async (data: string) => {
					checkSession();
					await write(sessionId, data);
				};
				if (autoSend) {
					const shellFamily = await getShellFamily(sessionId);
					await sendCommand(writeFn, text, agentType, shellFamily, true, sessionId);
				} else await writeFn(agentInputPayload(text, agentType));
				requestAnimationFrame(() => {
					if (terminalId && terminalsStore.getActive()?.id === terminalId) terminalsStore.get(terminalId)?.ref?.focus();
				});
			},
		};
	}
	return { kind: "none", ...context, deliver: async () => unavailable() };
}
