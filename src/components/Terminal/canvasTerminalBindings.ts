export interface CanvasTerminalBindings {
	listen<K extends keyof HTMLElementEventMap>(
		target: HTMLElement,
		type: K,
		listener: (event: HTMLElementEventMap[K]) => void,
		options?: boolean | AddEventListenerOptions,
	): void;
	listen<K extends keyof DocumentEventMap>(
		target: Document,
		type: K,
		listener: (event: DocumentEventMap[K]) => void,
		options?: boolean | AddEventListenerOptions,
	): void;
	dispose(): void;
}

export function createCanvasTerminalBindings(): CanvasTerminalBindings {
	const cleanups: (() => void)[] = [];
	let disposed = false;

	return {
		listen(target: EventTarget, type: string, listener: EventListener, options?: boolean | AddEventListenerOptions) {
			if (disposed) return;
			target.addEventListener(type, listener, options);
			cleanups.push(() => target.removeEventListener(type, listener, options));
		},
		dispose() {
			if (disposed) return;
			disposed = true;
			for (let index = cleanups.length - 1; index >= 0; index--) cleanups[index]();
			cleanups.length = 0;
		},
	};
}
