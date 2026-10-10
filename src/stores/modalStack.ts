import { onCleanup } from "solid-js";

// Centralized modal/dialog Escape handling.
//
// Problem: modals used to each wire their own `Escape` key handler, inconsistently
// (some at document-capture with stopPropagation, most not). When a modal did NOT
// stop propagation, Escape bubbled to the focused terminal's keyInputRef keydown
// listener, which wrote ESC to the PTY — so closing a modal with Escape ALSO sent
// ESC to the agent underneath.
//
// Fix: a single document-level CAPTURE keydown listener owns Escape whenever any
// modal is open. Capture fires before the event reaches the terminal's keyInputRef
// (a descendant), so stopPropagation there guarantees the terminal never sees the
// ESC. The top-most modal's close handler runs instead. Modals just register on
// mount and unregister on cleanup — no per-modal key handling needed.

interface ModalEntry {
	id: number;
	close: () => void;
}

let stack: ModalEntry[] = [];
let nextId = 1;
let listenerInstalled = false;

function handleCaptureKeydown(e: KeyboardEvent) {
	if (e.key !== "Escape") return;
	if (stack.length === 0) return;
	const top = stack.at(-1)!;
	// Consume the event before it can reach the terminal (or any other handler):
	// a modal is open, so Escape belongs to it and nothing else.
	e.preventDefault();
	e.stopPropagation();
	top.close();
}

function ensureListener() {
	if (listenerInstalled || typeof document === "undefined") return;
	document.addEventListener("keydown", handleCaptureKeydown, true);
	listenerInstalled = true;
}

/**
 * Register an open modal. Returns an id to unregister with. While at least one
 * modal is registered, Escape is captured globally and routed to the top-most
 * modal's `close` handler instead of propagating (e.g. to the terminal).
 */
export function pushModal(close: () => void): number {
	ensureListener();
	const id = nextId++;
	stack.push({ id, close });
	return id;
}

/** Unregister a previously-pushed modal by id. Safe to call more than once. */
export function popModal(id: number): void {
	stack = stack.filter((m) => m.id !== id);
}

/** True when at least one modal is open (Escape is being intercepted). */
export function anyModalOpen(): boolean {
	return stack.length > 0;
}

/**
 * Solid hook: register `close` as the current component's modal for as long as it
 * is mounted. Call once in a modal component's setup with its close/dismiss action.
 * Escape (via the global capture listener) will invoke the top-most open modal's
 * `close` and stop the event from reaching the terminal underneath.
 */
export function registerModal(close: () => void): void {
	const id = pushModal(close);
	onCleanup(() => popModal(id));
}

/** Test-only: reset internal state between cases. */
export function __resetModalStackForTest(): void {
	stack = [];
	nextId = 1;
}
